"""Restore a captured config path and clean up a rehearsal's own artifacts.

restore takes TB-43's lock (exclusive flock on the config directory, ADR-0013) and swaps the
original back only if the path is still exactly what the test left (compare-and-swap); otherwise it
is a conflict and nothing changes. Parent directories are walked without following symlinks, writes
go through a temp file and a rename, and the store is never deleted here. cleanup removes only
artifacts recorded as the test's own whose state is unchanged, and the store only after a passing
verify; it refuses while a recorded process is alive."""

from __future__ import annotations

import fcntl
import os
from pathlib import Path

from rehearsal_fs import (CONFIG_REL, Conflict, Refused, alive, close_all, link_whole, lstat_state,
                          open_dirs, read_json, sha256, write_json, write_whole)

PARTS = CONFIG_REL.parent.parts


def _store_copies(data: dict) -> dict:
    """The store's copies, each checked against the hash recorded at capture."""
    store = data["stores"][0]
    copies = {}
    for name, digest in store["files"].items():
        blob = (Path(store["dir"]) / name).read_bytes()
        if sha256(blob) != digest:
            raise Refused(f"store copy {store['dir']}/{name} does not match its capture hash")
        copies[name] = blob
    return copies


def restore(manifest: Path) -> None:
    data = read_json(manifest)
    if data.get("kind") != "capture" or data.get("left") is None:
        raise Refused(f"{manifest} is not a settled capture manifest; run settle first")
    home, original = Path(data["home"]), data["original"]
    copies = _store_copies(data)
    fds = open_dirs(home, CONFIG_REL.parent)
    try:
        complete = len(fds) - 1 == len(PARTS)
        if complete:
            fcntl.flock(fds[-1], fcntl.LOCK_EX)  # waits for any other editor holding it
        now = lstat_state(home / CONFIG_REL, fds[-1], CONFIG_REL.name) if complete else {"type": "absent"}
        where = f"current kept at {home / CONFIG_REL}, original in {data['store']}"
        if now != data["left"]:
            raise Conflict(f"config is {now['type']} but not what the test left; {where}")
        if not complete and original["type"] != "absent":
            raise Conflict(f"a parent directory of the config is missing; {where}")
        if original["type"] == "file":
            write_whole(fds[-1], CONFIG_REL.name, copies["original"], original["mode"])
        elif original["type"] == "symlink":
            _restore_target(data, copies)
            if now != original:
                link_whole(fds[-1], CONFIG_REL.name, original["link"])
        else:
            _remove_created(data, fds, now)
    finally:
        close_all(fds)
    data["restored"] = True
    write_json(manifest, data)


def _restore_target(data: dict, copies: dict) -> None:
    """A symlinked config's target gets its bytes and mode back if the test wrote through it."""
    target, want = Path(data["target"]), data["target_state"]
    now = lstat_state(target)
    if now == want:
        return
    if now != data["left_target"] or want["type"] != "file":
        raise Conflict(f"symlink target {target} changed after the test; kept, original in {data['store']}")
    fd = os.open(target.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        write_whole(fd, target.name, copies["target"], want["mode"])
    finally:
        os.close(fd)


def _remove_created(data: dict, fds: list[int], now: dict) -> None:
    """An originally absent config is removed, then each parent the test created, deepest
    first, only while it is empty."""
    depth = len(fds) - 1
    if depth == len(PARTS) and now["type"] != "absent":
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
        path = Path(a["path"])
        want = {k: a[k] for k in ("type", "mode", "sha256", "link") if k in a}
        now = lstat_state(path)
        if now["type"] == "absent":
            continue
        if now == want:
            os.unlink(path)
            removed.append(str(path))
        else:
            kept.append(f"{path} (changed since the test left it)")
    for store in data.get("stores", []):
        if not store.get("verified", data.get("verified")):
            kept.append(f"{store['dir']} (store kept: no passing verify)")
            continue
        for name, digest in store["files"].items():
            p = Path(store["dir"]) / name
            if lstat_state(p).get("sha256") == digest:
                p.unlink()
        _rmdir(Path(store["dir"]), removed, kept)
    for d in sorted(data.get("dirs", []), key=len, reverse=True):
        _rmdir(Path(d), removed, kept)
    return removed, kept


def _rmdir(path: Path, removed: list[str], kept: list[str]) -> None:
    try:
        path.rmdir()
        removed.append(str(path))
    except FileNotFoundError:
        pass
    except OSError:
        kept.append(f"{path} (not empty: holds files the test does not own)")
