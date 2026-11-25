"""Capture, settle and verify one rehearsal profile's config path.

capture: records the config path (absent / file / symlink and its target), which parent directories
existed and the active theme's provenance, copies the bytes into a private store outside the tree
being replaced, and writes the manifest; any failure leaves nothing behind. settle: compares the
config (and a captured link's target) with the expectation of the step that produced it and only
then records it as what the test left, with the test's own artifacts (declared with the hash the
producing step expects) and processes; what it reads never certifies itself. verify: compares the
current state with the capture."""

from __future__ import annotations

import os
import shutil
import tomllib
from pathlib import Path

from rehearsal_fs import (CONFIG_REL, THEME_REL, Conflict, Refused, close_all, inside, lstat_state,
                          matches, open_dirs, parse_expectation, process_identity, read_json,
                          read_nofollow, refuse_shared, sha256, write_json)


def config_state(home: Path) -> tuple[dict, int]:
    """State of the config path and how many of its parent directories exist (no symlinks)."""
    fds = open_dirs(home, CONFIG_REL.parent)
    try:
        depth = len(fds) - 1
        if depth < len(CONFIG_REL.parent.parts):
            return {"type": "absent"}, depth
        return lstat_state(home / CONFIG_REL, fds[-1], CONFIG_REL.name), depth
    finally:
        close_all(fds)


def target_state(target: str | None) -> dict | None:
    return None if target is None else lstat_state(Path(target))


def theme_provenance(home: Path, config_bytes: bytes | None) -> dict:
    """Which colors file the profile resolves to (or the bundled fallback) and the config's
    `theme` source."""
    try:
        source = tomllib.loads(config_bytes.decode()).get("theme", {}).get("source")
    except (AttributeError, UnicodeDecodeError, tomllib.TOMLDecodeError):
        source = None
    path = home / THEME_REL
    if not path.is_file():
        return {"colors": "bundled fallback", "config_source": source}
    real = path.resolve()
    shown = str(real.relative_to(home.resolve())) if inside(real, home) else str(real)
    return {"colors": shown, "sha256": sha256(real.read_bytes()), "config_source": source}


def capture(home: Path, store: Path, manifest: Path, allow_caller_home: bool = False) -> dict:
    home = home.resolve()
    if not home.is_dir():
        raise Refused(f"profile HOME {home} is not an existing directory")
    refuse_shared(home, "profile HOME", allow_caller_home)
    if os.path.lexists(store) or os.path.lexists(manifest):
        raise Refused(f"store {store} or manifest {manifest} already exists")
    store = Path(os.path.realpath(store))  # recorded resolved: cleanup walks it without symlinks
    if inside(store, home):
        raise Refused(f"store {store} is inside the profile tree being replaced")
    refuse_shared(store, "store", allow_caller_home=True)
    original, depth = config_state(home)
    if original["type"] not in ("absent", "file", "symlink"):
        raise Refused(f"config path is a {original['type']}, not a file or symlink")
    config = home / CONFIG_REL
    copies = {}
    target = None
    try:
        if original["type"] == "file":
            copies["original"] = read_nofollow(config)
        elif original["type"] == "symlink":
            target = os.path.realpath(config)
            if inside(store, Path(target).parent):
                raise Refused(f"store {store} is inside the symlink target's directory")
            if os.path.isfile(target):
                copies["target"] = Path(target).read_bytes()
    except OSError as e:
        raise Refused(f"cannot read {config}: {e}; nothing was modified") from e
    theme = theme_provenance(home, copies.get("original") or copies.get("target"))
    data = {
        "kind": "capture", "home": str(home), "config": str(config), "store": str(store),
        "original": original, "target": target, "target_state": target_state(target),
        "ancestors": [{"rel": str(Path(*CONFIG_REL.parent.parts[:i + 1])), "existed": i < depth}
                      for i in range(len(CONFIG_REL.parent.parts))],
        "theme": theme, "left": None, "left_target": None, "pids": [], "artifacts": [],
        "dirs": [], "stores": [{"dir": str(store), "files": {k: sha256(v) for k, v in copies.items()}}],
        "restored": False, "verified": False,
    }
    for a in data["ancestors"]:
        if a["existed"]:
            a["mode"] = lstat_state(home / a["rel"])["mode"]
    _write_store(store, copies, manifest, data)
    return data


def _write_store(store: Path, copies: dict, manifest: Path, data: dict) -> None:
    """Copies first (fsynced), manifest last; on any failure remove what this call created."""
    try:
        store.mkdir(mode=0o700)
        for name, blob in copies.items():
            fd = os.open(store / name, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(fd, "wb") as f:
                f.write(blob)
                f.flush()
                os.fsync(f.fileno())
        write_json(manifest, data)
    except BaseException:
        shutil.rmtree(store, ignore_errors=True)
        raise


def settle(manifest: Path, pids: list[int], artifacts: list[tuple[str, str]],
           expect_config: str | None, expect_target: str | None = None) -> list[str]:
    """Record what the test left, only if it is what the producing step expects. Returns the
    problems (artifacts not owned, gone processes). A missing, malformed or different expectation
    raises and leaves nothing restore could act on."""
    data = read_json(manifest)
    if data.get("kind") not in ("capture",):
        raise Refused(f"{manifest} is not a capture manifest")
    try:
        want, want_target = _expectations(data, expect_config, expect_target)
        home = Path(data["home"])
        now, _ = config_state(home)
        kept = f"current kept in place, original in {data['store']}"
        if not matches(now, want):
            raise Conflict(f"config is {_shown(now)}, the producing step expects {_shown(want)}; {kept}")
        now_target = target_state(data["target"])
        if want_target is not None and not matches(now_target, want_target):
            raise Conflict(f"symlink target {data['target']} is {_shown(now_target)}, the producing step "
                           f"expects {_shown(want_target)}; {kept}")
    except (Conflict, Refused):
        _unsettle(manifest, data)
        raise
    data["left"], data["left_target"] = now, now_target
    data["expected"] = {"config": expect_config, "target": expect_target}
    problems = []
    for pid in pids:
        identity = process_identity(pid)
        if identity:
            data["pids"].append(identity)
        else:
            problems.append(f"process {pid} is not running; not recorded")
    for path, expected in artifacts:
        p = Path(path)
        state = lstat_state(p)
        if not inside(p.parent, home):
            problems.append(f"{p}: not inside the profile HOME; not owned")
        elif state.get("sha256") != expected.lower():
            problems.append(f"{p}: is not the expected file ({state['type']}); kept, not owned")
        else:
            data["artifacts"].append({"path": str(p.parent.resolve() / p.name), **state})
    write_json(manifest, data)
    return problems


def _expectations(data: dict, expect_config: str | None, expect_target: str | None):
    if expect_config is None:
        raise Refused("settle needs --expect-config from the step that produced the config; "
                      "nothing recorded")
    want = parse_expectation(expect_config, "--expect-config")
    if data["original"]["type"] == "symlink" and expect_target is None:
        raise Refused("the captured config is a symlink: settle needs --expect-target too; nothing recorded")
    if data["original"]["type"] != "symlink" and expect_target is not None:
        raise Refused("--expect-target is only for a captured symlink; nothing recorded")
    return want, None if expect_target is None else parse_expectation(expect_target, "--expect-target")


def _shown(state: dict) -> str:
    return state["type"] + "".join(f" {k}={state[k]}" for k in ("sha256", "link") if k in state)


def _unsettle(manifest: Path, data: dict) -> None:
    """A refused settle also withdraws an earlier one: restore needs a settle that held."""
    if data.get("left") is not None:
        data["left"] = data["left_target"] = None
        write_json(manifest, data)


def differences(data: dict) -> list[str]:
    """What differs between the current profile and the capture."""
    home = Path(data["home"])
    out = []
    try:
        now, depth = config_state(home)
    except Refused as e:
        return [str(e)]
    if now != data["original"]:
        out.append(f"config: captured {data['original']}, now {now}")
    if target_state(data["target"]) != data["target_state"]:
        out.append(f"symlink target {data['target']} changed")
    for i, a in enumerate(data["ancestors"]):
        state = lstat_state(home / a["rel"]) if i < depth else {"type": "absent"}
        want = {"type": "dir", "mode": a["mode"]} if a["existed"] else {"type": "absent"}
        if state != want:
            out.append(f"{a['rel']}: captured {want}, now {state}")
    blob = None
    if now["type"] == "file":
        blob = read_nofollow(home / CONFIG_REL)
    elif data["target"] and os.path.isfile(data["target"]):
        blob = Path(data["target"]).read_bytes()
    theme = theme_provenance(home, blob)
    if theme != data["theme"]:
        out.append(f"theme provenance: captured {data['theme']}, now {theme}")
    return out


def verify(manifest: Path) -> list[str]:
    data = read_json(manifest)
    if data.get("kind") != "capture":
        raise Refused(f"{manifest} is not a capture manifest")
    diffs = differences(data)
    data["verified"] = not diffs
    write_json(manifest, data)
    return diffs
