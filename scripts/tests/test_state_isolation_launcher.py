"""`scripts/try-app.sh` keeps the configurator's and the simulator's generation store private to
the persistent manual-test session: both windows resolve the same state directory (what the real
binaries do: `$XDG_STATE_HOME` when it is absolute, else `$HOME/.local/state`), it lies inside the
session and outside the config directory a restore would replace, it survives a relaunch, and the
caller's own state directory (a temp decoy set as `XDG_STATE_HOME`, or left unset, or relative)
and HOME stay untouched. The launcher names the session's store, never the caller's. Fake Cargo
and fake windows; no Rust build, no desktop, never the real `.scratch/manual-test`."""

from __future__ import annotations

import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
LAUNCHER = ROOT / "scripts/try-app.sh"
STORE = Path("touchbar-customizer/backups")

FAKE_CARGO = r'''
import os, shutil, sys
from pathlib import Path
root = Path(os.environ["FAKE_ROOT"])
target = root / "target/debug"
target.mkdir(parents=True, exist_ok=True)
for name in ("touchbar-customizer", "touchbard"):
    shutil.copy2(root / "fake-window.py", target / name)
'''

# A window resolves its state directory as the real binaries do and keeps a marker generation there.
FAKE_WINDOW = r'''
import json, os, signal, sys, time
from pathlib import Path
root = Path(os.environ["FAKE_ROOT"])
name = Path(sys.argv[0]).name
signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
home = os.environ.get("HOME")
xdg = os.environ.get("XDG_STATE_HOME")
state = Path(xdg) if xdg and os.path.isabs(xdg) else Path(home) / ".local/state"
store = state / "touchbar-customizer/backups"
store.mkdir(parents=True, exist_ok=True)
marker = store / ("marker-" + name)
existed = marker.exists()
marker.write_text(name)
with (root / "events.jsonl").open("a") as f:
    f.write(json.dumps(dict(name=name, home=home, xdg=xdg, store=str(store), existed=existed,
                            pid=os.getpid())) + "\n")
while not (root / "exit-all").exists():
    time.sleep(0.01)
'''


def tree(root: Path) -> dict:
    out = {}
    if not root.exists():
        return out
    for path in [root, *root.rglob("*")]:
        st = path.lstat()
        out[str(path.relative_to(root))] = (st.st_mode, st.st_size, st.st_mtime_ns,
                                             path.read_bytes() if path.is_file() and not path.is_symlink() else None)
    return out


class LauncherState(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="tb-launcher-state-")
        self.addCleanup(self.temp.cleanup)
        tmp = Path(self.temp.name)
        self.root = tmp / "repo copy"
        (self.root / "scripts").mkdir(parents=True)
        shutil.copy2(LAUNCHER, self.root / "scripts/try-app.sh")
        self.session = self.root / ".scratch/manual-test"
        self.caller_home = tmp / "caller home"
        (self.caller_home / ".local/state/touchbar-customizer").mkdir(parents=True)
        (self.caller_home / ".local/state/touchbar-customizer/theme-set").write_text("real\n")
        self.decoy = tmp / "caller state"
        (self.decoy / STORE / "1").mkdir(parents=True)
        (self.decoy / STORE / "1/manifest.toml").write_text('kind = "absent"\n')
        self.env = dict(os.environ, HOME=str(self.caller_home), XDG_STATE_HOME=str(self.decoy),
                        FAKE_ROOT=str(self.root))
        self.env["PATH"] = str(self.root / "bin") + os.pathsep + self.env["PATH"]
        self.program("bin/cargo", FAKE_CARGO)
        self.program("fake-window.py", FAKE_WINDOW)
        self.before = {"home": tree(self.caller_home), "decoy": tree(self.decoy)}
        self.procs = []
        self.addCleanup(self.stop_all)

    def program(self, rel, code):
        p = self.root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(f"#!{sys.executable}\n" + code)
        p.chmod(0o755)

    def stop_all(self):
        for proc in self.procs:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            proc.communicate(timeout=5)

    def events(self):
        path = self.root / "events.jsonl"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def launch(self, **env):
        """One launch until both windows are up, then both close; returns (windows, output)."""
        (self.root / "exit-all").unlink(missing_ok=True)
        seen = len(self.events())
        proc = subprocess.Popen([str(self.root / "scripts/try-app.sh")], cwd=self.temp.name,
                                env={k: v for k, v in dict(self.env, **env).items() if v is not None},
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                                start_new_session=True)
        self.procs.append(proc)
        deadline = time.monotonic() + 10
        while len(self.events()) < seen + 2:
            if proc.poll() is not None:
                self.fail("launcher ended before both windows started: " + proc.communicate()[0])
            if time.monotonic() > deadline:
                self.fail("both windows did not start")
            time.sleep(0.01)
        (self.root / "exit-all").write_text("0")
        output = proc.communicate(timeout=10)[0]
        return self.events()[seen:], output

    def assertCallerUntouched(self):
        self.assertEqual(tree(self.decoy), self.before["decoy"], "the caller's XDG_STATE_HOME was touched")
        self.assertEqual(tree(self.caller_home), self.before["home"], "the caller's HOME was touched")

    def assertPrivateShared(self, windows):
        stores = {w["store"] for w in windows}
        self.assertEqual(len(stores), 1, f"both windows share one store: {windows}")
        store = Path(stores.pop())
        self.assertTrue(store.is_absolute())
        self.assertTrue(store.resolve().is_relative_to(self.session.resolve()), f"{store} is in the session")
        config_dir = Path(windows[0]["home"]) / ".config/touchbar-customizer"
        self.assertFalse(store.resolve().is_relative_to(config_dir.resolve()),
                         f"{store} is outside the config directory a restore replaces")
        return store

    def test_both_windows_share_a_private_store_and_the_callers_state_is_untouched(self):
        windows, _ = self.launch()
        self.assertEqual({w["name"] for w in windows}, {"touchbar-customizer", "touchbard"})
        self.assertPrivateShared(windows)
        self.assertCallerUntouched()

    def test_the_store_is_private_whether_the_caller_sets_xdg_state_home_or_not(self):
        for xdg in (None, "relative/state", ""):
            with self.subTest(xdg=xdg):
                windows, _ = self.launch(XDG_STATE_HOME=xdg)
                self.assertPrivateShared(windows)
                self.assertCallerUntouched()
                self.assertFalse((Path(self.temp.name) / "relative").exists())

    def test_the_session_store_persists_across_a_relaunch(self):
        first, _ = self.launch()
        second, _ = self.launch()
        self.assertEqual(self.assertPrivateShared(first), self.assertPrivateShared(second))
        self.assertTrue(all(w["existed"] for w in second), f"the first launch's generations are still there: {second}")
        self.assertCallerUntouched()

    def test_the_launcher_names_the_session_store_not_the_callers(self):
        windows, output = self.launch()
        store = self.assertPrivateShared(windows)
        self.assertIn(str(store), output, "the launcher tells where the test generations are kept")
        self.assertNotIn(str(self.decoy), output)


if __name__ == "__main__":
    unittest.main()
