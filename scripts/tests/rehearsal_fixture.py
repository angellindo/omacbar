"""Shared fixture for the rehearsal tool tests: a throwaway copy of the repo with a fake persistent
session, a fake caller HOME and a private profile. Never reads the real `.scratch/manual-test` or
the caller's real HOME.

`settle` always carries the expectation of the step that produced the config (`--expect-config`,
and `--expect-target` for a captured symlink): `absent`, the SHA-256 of a regular file's bytes, or
`symlink:<link text>`. The tool compares it with what it finds; reading the current state after a
save never stands in for it."""

from __future__ import annotations

import hashlib
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CONFIG_REL = Path(".config/touchbar-customizer/config.toml")
THEME_REL = Path(".local/state/omarchy/current/theme/colors.toml")
SESSION_BYTES = b"# fake persistent layout of the launcher session\nlabel = \"keep me\"\n"


def state(path: Path) -> dict:
    """Type, mode, bytes and link text of one path, without following a final symlink."""
    try:
        st = path.lstat()
    except FileNotFoundError:
        return {"type": "absent"}
    if stat.S_ISLNK(st.st_mode):
        return {"type": "symlink", "link": os.readlink(path)}
    if stat.S_ISDIR(st.st_mode):
        return {"type": "dir", "mode": stat.S_IMODE(st.st_mode)}
    return {"type": "file", "mode": stat.S_IMODE(st.st_mode), "bytes": path.read_bytes()}


def tree(root: Path) -> dict:
    """`state` of every path under a fixture-owned directory, plus file mtimes."""
    out = {}
    for dirpath, dirs, files in os.walk(root):
        for name in dirs + files:
            p = Path(dirpath) / name
            s = state(p)
            if s["type"] == "file":
                s["mtime_ns"] = p.lstat().st_mtime_ns
            out[str(p.relative_to(root))] = s
    return out


def replace_bytes(path: Path, data: bytes, mode: int = 0o644) -> None:
    """Save the way an editor or the configurator does: temp file + rename over `path`."""
    tmp = path.with_name(f".{path.name}.editor-tmp")
    tmp.write_bytes(data)
    os.chmod(tmp, mode)
    os.replace(tmp, path)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def expect_link(text: str) -> str:
    """Expectation for a config left as a symlink with this link text."""
    return f"symlink:{text}"


ABSENT = "absent"

# Run by `Fixture.tool_with_editor`: the real CLI in a process where an external editor (one that
# does not take the configurator's lock) saves `watched` by temp + rename the first time the tool
# syncs or chmods a descriptor of another file that holds exactly `payload` (the bytes being put
# back, i.e. the tool's temp file still in preparation). Nothing else in the tool is replaced.
EDITOR_HOOK = r'''
import os, runpy, sys
tool, watched, external, payload, log, *args = sys.argv[1:]
external, payload = bytes.fromhex(external), bytes.fromhex(payload)
real = {n: getattr(os, n) for n in ("fsync", "fdatasync", "fchmod")}
fired = []


def preparing(fd):
    try:
        path = os.readlink(f"/proc/self/fd/{fd}")
        if path == os.path.realpath(watched) or not os.path.isfile(path):
            return False
        with open(path, "rb") as f:
            return f.read() == payload
    except OSError:
        return False


def edit():
    tmp = os.path.join(os.path.dirname(watched), "." + os.path.basename(watched) + ".external-editor")
    with open(tmp, "wb") as f:
        f.write(external)
    os.rename(tmp, watched)
    with open(log, "w") as f:
        f.write("saved during preparation\n")


def hooked(name):
    def call(fd, *rest):
        result = real[name](fd, *rest)
        if not fired and preparing(fd):
            fired.append(name)
            edit()
        return result
    return call


for name in real:
    setattr(os, name, hooked(name))
sys.path.insert(0, os.path.dirname(tool))
sys.argv = [tool, *args]
runpy.run_path(tool, run_name="__main__")
'''


def store_holds(store: Path, data: bytes) -> bool:
    """Some regular file in the store has exactly `data` (layout of the store is the tool's)."""
    return store.is_dir() and any(
        p.is_file() and not p.is_symlink() and p.read_bytes() == data for p in store.rglob("*")
    )


class Fixture(unittest.TestCase):
    """A temp repo copy with the rehearsal scripts and theme fixtures, a fake session, a fake
    caller HOME (`self.real_home`) and an empty private profile home (`self.home`, 0700)."""

    def setUp(self):
        self.old_umask = os.umask(0o077)  # restore must set modes itself, not rely on umask
        self.addCleanup(os.umask, self.old_umask)
        self.temp = tempfile.TemporaryDirectory(prefix="tb-rehearsal-")
        self.addCleanup(self._cleanup)
        self.tmp = Path(self.temp.name)
        self.repo = self.tmp / "repo copy"
        (self.repo / "scripts").mkdir(parents=True)
        for src in sorted((ROOT / "scripts").glob("rehearsal*")):
            dest = self.repo / "scripts" / src.name
            (shutil.copytree if src.is_dir() else shutil.copy2)(src, dest)
        shutil.copytree(ROOT / "fixtures/omarchy-themes", self.repo / "fixtures/omarchy-themes")
        self.tool_path = self.repo / "scripts/rehearsal.py"
        self.session = self.repo / ".scratch/manual-test"
        for rel, data in (("home" / CONFIG_REL, SESSION_BYTES), ("keys.bin", b"\x01\x02"),
                          ("launch.lock", b""), ("logs/simulator.log", b"log\n"),
                          ("frames/frame.rgba", b"\x00" * 16)):
            p = self.session / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(data)
        self.real_home = self.tmp / "caller home"
        (self.real_home / "Documents").mkdir(parents=True)
        (self.real_home / "Documents/sentinel.txt").write_text("caller data\n")
        self.home = self.tmp / "profile" / "home"
        self.home.mkdir(parents=True, mode=0o700)
        self.config = self.home / CONFIG_REL
        self.store = self.tmp / "store"
        self.manifest = self.tmp / "manifest.json"
        self.env = dict(os.environ, HOME=str(self.real_home))
        self.before_real = tree(self.real_home)
        self.before_session = tree(self.session)

    def _cleanup(self):
        for dirpath, dirs, _ in os.walk(self.tmp):
            for d in dirs:
                p = Path(dirpath) / d
                if not p.is_symlink():
                    os.chmod(p, 0o700)
        self.temp.cleanup()

    def tool(self, *args, timeout=60, **kw) -> subprocess.CompletedProcess:
        self.assertTrue(self.tool_path.is_file(), "scripts/rehearsal.py is missing")
        return subprocess.run([sys.executable, "-B", str(self.tool_path), *map(str, args)],
                              cwd=self.repo, env=self.env, capture_output=True, text=True,
                              timeout=timeout, **kw)

    def ok(self, *args, code=0) -> subprocess.CompletedProcess:
        p = self.tool(*args)
        self.assertEqual(p.returncode, code, f"rehearsal {args}: {p.stdout}{p.stderr}")
        return p

    def capture(self):
        return self.ok("capture", "--home", self.home, "--store", self.store, "--manifest", self.manifest)

    def settle(self, expect_config: str, *extra, expect_target: str | None = None):
        """Settle with the expectation of the step that produced the config (and its link target)."""
        target = ("--expect-target", expect_target) if expect_target is not None else ()
        return self.ok("settle", "--manifest", self.manifest, "--expect-config", expect_config,
                       *target, *extra)

    def tool_with_editor(self, *args, watched: Path, payload: bytes, external: bytes):
        """Run the CLI with an external editor saving `watched` while the tool prepares the file
        that will put `payload` back. Returns (process, whether the editor saved)."""
        hook, log = self.tmp / "editor-hook.py", self.tmp / "editor-hook.log"
        hook.write_text(EDITOR_HOOK)
        log.unlink(missing_ok=True)
        p = subprocess.run([sys.executable, "-B", str(hook), str(self.tool_path), str(watched),
                            external.hex(), payload.hex(), str(log), *map(str, args)],
                           cwd=self.repo, env=self.env, capture_output=True, text=True, timeout=60)
        return p, log.is_file()

    def restore(self, code=0):
        return self.ok("restore", "--manifest", self.manifest, code=code)

    def seed_theme(self, name: str = "tokyo-night") -> bytes:
        themes = sorted((self.repo / "fixtures/omarchy-themes").glob("*/colors.toml"))
        src = next((t for t in themes if t.parent.name == name), themes[0])
        dest = self.home / THEME_REL
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dest)
        return dest.read_bytes()

    def assertOutsideUntouched(self):
        self.assertEqual(tree(self.real_home), self.before_real, "the caller's HOME was touched")
        self.assertEqual(tree(self.session), self.before_session, "the persistent session was touched")
