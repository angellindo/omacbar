"""Shared fixture for the rehearsal tool tests: a throwaway copy of the repo with a fake persistent
session, a fake caller HOME and a private profile. Never reads the real `.scratch/manual-test` or
the caller's real HOME."""

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

    def settle(self, *extra):
        return self.ok("settle", "--manifest", self.manifest, *extra)

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
