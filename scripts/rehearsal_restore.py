"""Restore a captured config path and clean up a rehearsal's own artifacts.

restore takes TB-43's lock (exclusive flock on the config directory, ADR-0013) and swaps the
original back only if the path is still exactly what the settled test left (compare-and-swap);
otherwise it is a conflict and nothing changes. The comparison is repeated after the file to put
back is prepared (written, synced, chmodded) and right before each rename or unlink, for the config
and for a captured link's target. Only the window after that last comparison remains for a writer
that does not take the lock (ADR-0013's accepted residual). Parent directories are walked without
following symlinks, writes go through a temp file and a rename, and the store is never deleted
here. cleanup reaches every path through directories opened from / without following symlinks, and
removes only artifacts recorded as the test's own whose state is unchanged, and the store's own
copies only after a passing verify; it refuses while a recorded process is alive."""

from __future__ import annotations

import fcntl
import os
from pathlib import Path

from rehearsal_fs import (CONFIG_REL, Conflict, Refused, alive, close_all, link_whole, lstat_state,
                          open_dir, open_dirs, open_parent, read_json, read_nofollow, sha256,
                          write_json, write_whole)

PARTS = CONFIG_REL.parent.parts


def _store_copies(data: dict) -> dict:
    """The store's copies, read through the validated store directory and checked against the hash
    recorded at capture."""
    store = data["stores"][0]
    fd = open_dir(Path(store["dir"]))
    try:
        copies = {}
        for name, digest in store["files"].items():
            blob = read_nofollow(name, fd)
            if sha256(blob) != digest:
                raise Refused(f"store copy {store['dir']}/{name} does not match its capture hash")
            copies[name] = blob
        return copies
    finally:
        os.close(fd)


def _still(dir_fd: int, name: str, want: dict, what: str, where: str):
    """The last comparison: raises Conflict when `name` is no longer `want`."""
    def check() -> None:
        now = lstat_state(Path(name), dir_fd, name)
        if now != want:
            raise Conflict(f"{what} changed while the restore was being prepared ({now['type']}); {where}")
    return check


def restore(manifest: Path) -> None:
    data = read_json(manifest)
    if data.get("kind") != "capture" or data.get("left") is None:
        raise Refused(f"{manifest} is not a settled capture manifest; run settle first")
    home, original, left = Path(data["home"]), data["original"], data["left"]
    copies = _store_copies(data)
    fds = open_dirs(home, CONFIG_REL.parent)
    try:
        complete = len(fds) - 1 == len(PARTS)
        if complete:
            fcntl.flock(fds[-1], fcntl.LOCK_EX)  # waits for any other editor holding it
        now = lstat_state(home / CONFIG_REL, fds[-1], CONFIG_REL.name) if complete else {"type": "absent"}
        where = f"current kept at {home / CONFIG_REL}, original in {data['store']}"
        if now != left:
            raise Conflict(f"config is {now['type']} but not what the test left; {where}")
        if not complete and original["type"] != "absent":
            raise Conflict(f"a parent directory of the config is missing; {where}")
        still = _still(fds[-1], CONFIG_REL.name, left, "config", where) if complete else lambda: None
        if original["type"] == "file":
            write_whole(fds[-1], CONFIG_REL.name, copies["original"], original["mode"], still)
        elif original["type"] == "symlink":
            _restore_target(data, copies)
            if now != original:
                link_whole(fds[-1], CONFIG_REL.name, original["link"], still)
        else:
            _remove_created(data, fds, now, still)
    finally:
        close_all(fds)
    data["restored"] = True
    write_json(manifest, data)


def _restore_target(data: dict, copies: dict) -> None:
    """A symlinked config's target gets its bytes and mode back if the test wrote through it."""
    target, want, left = Path(data["target"]), data["target_state"], data["left_target"]
    where = f"kept, original in {data['store']}"
    fd = open_parent(target)  # the target was recorded resolved: no symlink on its way
    try:
        now = lstat_state(target, fd, target.name)
        if now == want:
            return
        if now != left or want["type"] != "file":
            raise Conflict(f"symlink target {target} changed after the test; {where}")
        write_whole(fd, target.name, copies["target"], want["mode"],
                    _still(fd, target.name, left, f"symlink target {target}", where))
    finally:
        os.close(fd)


def _remove_created(data: dict, fds: list[int], now: dict, still) -> None:
    """An originally absent config is removed, then each parent the test created, deepest
    first, only while it is empty."""
    depth = len(fds) - 1
    if depth == len(PARTS) and now["type"] != "absent":
        still()
        os.unlink(CONFIG_REL.name, dir_fd=fds[-1])
    for i in reversed(range(depth)):
        if data["ancestors"][i]["existed"]:
            return
        try:
            os.rmdir(PARTS[i], dir_fd=fds[i])
        except OSError:
            return


def cleanup(manifest: Path) -> tuple[list[str], list[str]]:
    """Returns (removed, kept). Refuses while a recorded process is alive."""
    data = read_json(manifest)
    live = [p["pid"] for p in data.get("pids", []) if alive(p)]
    if live:
        raise Refused(f"recorded processes still running: {live}; nothing removed")
    removed, kept = [], []
    for a in reversed(data.get("artifacts", [])):
        want = {k: a[k] for k in ("type", "mode", "sha256", "link") if k in a}
        _remove_own(Path(a["path"]), want, removed, kept)
    for store in data.get("stores", []):
        if not store.get("verified", data.get("verified")):
            kept.append(f"{store['dir']} (store kept: no passing verify)")
            continue
        _empty_store(Path(store["dir"]), store["files"], kept)
        _rmdir(Path(store["dir"]), removed, kept)
    for d in sorted(data.get("dirs", []), key=len, reverse=True):
        _rmdir(Path(d), removed, kept)
    return removed, kept


def _remove_own(path: Path, want: dict, removed: list[str], kept: list[str]) -> None:
    """Unlink `path` through its validated parent only while it is exactly what was recorded."""
    try:
        fd = open_parent(path)
    except FileNotFoundError:
        return
    except Refused as e:
        kept.append(f"{path} ({e})")
        return
    try:
        now = lstat_state(path, fd, path.name)
        if now["type"] == "absent":
            return
        if now == want:
            os.unlink(path.name, dir_fd=fd)
            removed.append(str(path))
        else:
            kept.append(f"{path} (changed since the test left it)")
    finally:
        os.close(fd)


def _empty_store(store: Path, files: dict, kept: list[str]) -> None:
    """Remove the store's own copies (by name and capture hash) through the validated store
    directory; anything else in it stays."""
    try:
        fd = open_dir(store)
    except FileNotFoundError:
        return
    except Refused as e:
        kept.append(f"{store} ({e})")
        return
    try:
        for name, digest in files.items():
            if lstat_state(store / name, fd, name).get("sha256") == digest:
                os.unlink(name, dir_fd=fd)
    finally:
        os.close(fd)


def _rmdir(path: Path, removed: list[str], kept: list[str]) -> None:
    try:
        fd = open_parent(path)
    except FileNotFoundError:
        return
    except Refused as e:
        kept.append(f"{path} ({e})")
        return
    try:
        os.rmdir(path.name, dir_fd=fd)
        removed.append(str(path))
    except FileNotFoundError:
        pass
    except OSError:
        kept.append(f"{path} (not empty or not a plain directory: not removed)")
    finally:
        os.close(fd)
