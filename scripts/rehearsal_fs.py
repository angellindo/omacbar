"""Shared pieces of the rehearsal tool: errors, hashes, the manifest file, path guards, process
identities and no-follow directory walks. Python stdlib only."""

from __future__ import annotations

import hashlib
import json
import os
import re
import stat
from collections.abc import Callable
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SESSION = ROOT / ".scratch/manual-test"
CONFIG_REL = Path(".config/touchbar-customizer/config.toml")
THEME_REL = Path(".local/state/omarchy/current/theme/colors.toml")
OK, FAILED, CONFLICT = 0, 1, 3


class Refused(Exception):
    """A precondition does not hold; nothing was created, replaced or removed."""


class Conflict(Exception):
    """The path is no longer what the test left; nothing was changed."""


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def caller_home() -> Path | None:
    home = os.environ.get("HOME")
    return Path(home).resolve() if home else None


def inside(path: Path, root: Path | None) -> bool:
    """`path` is `root` or below it, both resolved (missing tails resolve lexically)."""
    return root is not None and path.resolve().is_relative_to(root.resolve())


def refuse_shared(path: Path, what: str, allow_caller_home: bool = False) -> None:
    """Refuse the caller's HOME (or anything in it), the persistent session and the repo."""
    if inside(path, SESSION):
        raise Refused(f"{what} {path} is inside the persistent session .scratch/manual-test")
    if inside(path, ROOT):
        raise Refused(f"{what} {path} is inside the repository")
    if not allow_caller_home and inside(path, caller_home()):
        raise Refused(f"{what} {path} is the caller's HOME or inside it")


def write_json(path: Path, data: dict) -> None:
    """Write `path` whole: private temp file, fsync, rename."""
    tmp = path.with_name(f".{path.name}.tmp-{os.getpid()}")
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        with os.fdopen(fd, "w") as f:
            json.dump(data, f, indent=2, sort_keys=True)
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp, path)
    except BaseException:
        tmp.unlink(missing_ok=True)
        raise


def read_json(path: Path) -> dict:
    try:
        return json.loads(Path(path).read_text())
    except (OSError, ValueError) as e:
        raise Refused(f"cannot read manifest {path}: {e}") from e


def process_identity(pid: int) -> dict | None:
    """pid plus its start time, so a reused pid is never taken for the recorded process."""
    try:
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    except (OSError, IndexError):
        return None
    if fields[0] == "Z":
        return None
    return {"pid": pid, "start": fields[19]}


def alive(identity: dict) -> bool:
    return process_identity(identity["pid"]) == identity


def lstat_state(path: Path, dir_fd: int | None = None, name: str | None = None) -> dict:
    """Type, mode, sha256 and link text of one path without following a final symlink."""
    where = name if dir_fd is not None else path
    try:
        st = os.stat(where, dir_fd=dir_fd, follow_symlinks=False)
    except FileNotFoundError:
        return {"type": "absent"}
    if stat.S_ISLNK(st.st_mode):
        return {"type": "symlink", "link": os.readlink(where, dir_fd=dir_fd)}
    if stat.S_ISDIR(st.st_mode):
        return {"type": "dir", "mode": stat.S_IMODE(st.st_mode)}
    if not stat.S_ISREG(st.st_mode):
        return {"type": "other"}
    return {"type": "file", "mode": stat.S_IMODE(st.st_mode),
            "sha256": sha256(read_nofollow(where, dir_fd))}


def read_nofollow(where, dir_fd: int | None = None) -> bytes:
    fd = os.open(where, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=dir_fd)
    with os.fdopen(fd, "rb") as f:
        return f.read()


def open_dirs(base: Path, rel: Path) -> list[int]:
    """fds of `base` and each directory of `rel` that exists, never following a symlink.
    A component that is a symlink or not a directory raises Refused."""
    flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
    try:
        fds = [os.open(base, flags)]
    except OSError as e:
        raise Refused(f"{base} is not a plain directory: {e}") from e
    for part in rel.parts:
        try:
            fds.append(os.open(part, flags, dir_fd=fds[-1]))
        except FileNotFoundError:
            break
        except OSError as e:
            close_all(fds)
            raise Refused(f"{part} under {base} is not a plain directory (symlink swapped in?)") from e
    return fds


def open_parent(path: Path) -> int:
    """fd of `path`'s parent, reached from / one component at a time without following any
    symlink. Recorded paths are fully resolved, so a symlink on the way means a directory was
    swapped after it was recorded: Refused, nothing opened. A missing directory raises
    FileNotFoundError."""
    path = Path(path)
    if not path.is_absolute() or any(part in (".", "..") for part in path.parts):
        raise Refused(f"{path} is not an absolute, resolved path")
    flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
    fd = os.open("/", flags)
    try:
        for part in path.parent.parts[1:]:
            nxt = os.open(part, flags, dir_fd=fd)
            os.close(fd)
            fd = nxt
    except FileNotFoundError:
        os.close(fd)
        raise
    except OSError as e:
        os.close(fd)
        raise Refused(f"{path.parent} is no longer a plain directory chain (symlink swapped in?)") from e
    return fd


def open_dir(path: Path) -> int:
    """fd of the directory `path` itself, validated like `open_parent`."""
    parent = open_parent(path)
    try:
        return os.open(Path(path).name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent)
    except FileNotFoundError:
        raise
    except OSError as e:
        raise Refused(f"{path} is not a plain directory (symlink swapped in?)") from e
    finally:
        os.close(parent)


HASH = re.compile(r"[0-9a-fA-F]{64}")


def parse_expectation(text: str, what: str) -> dict:
    """`absent`, the SHA-256 of a regular file's bytes, or `symlink:<link text>`."""
    if text == "absent":
        return {"type": "absent"}
    if text.startswith("symlink:") and len(text) > len("symlink:"):
        return {"type": "symlink", "link": text[len("symlink:"):]}
    if HASH.fullmatch(text):
        return {"type": "file", "sha256": text.lower()}
    raise Refused(f"{what} {text!r} is not `absent`, a 64-hex SHA-256 or `symlink:<link text>`")


def matches(state: dict, expected: dict) -> bool:
    """`state` (from lstat_state) is what `expected` says; a file's mode is not part of it."""
    return all(state.get(k) == v for k, v in expected.items())


def close_all(fds: list[int]) -> None:
    for fd in fds:
        os.close(fd)


def write_whole(dir_fd: int, name: str, data: bytes, mode: int,
                still: Callable[[], None] = lambda: None) -> None:
    """Replace `name` in `dir_fd` with `data`: temp file, fsync, chmod, `still()` (the last
    comparison: it raises if `name` is no longer what may be replaced), rename, fsync the dir.
    On any failure (or interrupt) the temp file is removed and `name` is untouched."""
    tmp = f".{name}.rehearsal-{os.getpid()}"
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=dir_fd)
    try:
        try:
            os.write(fd, data)
            os.fsync(fd)
            os.fchmod(fd, mode)
        finally:
            os.close(fd)
        still()
        os.replace(tmp, name, src_dir_fd=dir_fd, dst_dir_fd=dir_fd)
        os.fsync(dir_fd)
    except BaseException:
        try:
            os.unlink(tmp, dir_fd=dir_fd)
        except OSError:
            pass
        raise


def link_whole(dir_fd: int, name: str, link: str, still: Callable[[], None] = lambda: None) -> None:
    """Replace `name` with a symlink to `link` through a temp link, `still()`, and a rename."""
    tmp = f".{name}.rehearsal-link-{os.getpid()}"
    os.symlink(link, tmp, dir_fd=dir_fd)
    try:
        still()
        os.replace(tmp, name, src_dir_fd=dir_fd, dst_dir_fd=dir_fd)
        os.fsync(dir_fd)
    except BaseException:
        try:
            os.unlink(tmp, dir_fd=dir_fd)
        except OSError:
            pass
        raise
