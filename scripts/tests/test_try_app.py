"""Exercise the actual launcher with fake Cargo/windows; no Rust build or desktop needed."""

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

FAKE_CARGO = r'''
import json, os, shutil, sys
from pathlib import Path
root = Path(os.environ["FAKE_ROOT"])
with (root / "events.jsonl").open("a") as f:
    f.write(json.dumps(dict(kind="build", home=os.environ.get("HOME"),
        jobs=os.environ.get("CARGO_BUILD_JOBS"), args=sys.argv[1:],
        nice=os.getpriority(os.PRIO_PROCESS, 0))) + "\n")
code = int(os.environ.get("FAKE_BUILD_EXIT", "0"))
if code:
    sys.exit(code)
target = root / "target/debug"
target.mkdir(parents=True, exist_ok=True)
for name in ("touchbar-customizer", "touchbard"):
    if name == "touchbard" and os.environ.get("FAKE_MISSING_DAEMON"):
        continue
    shutil.copy2(root / "fake-window.py", target / name)
(root / "built").touch()
'''

FAKE_WINDOW = r'''
import json, os, signal, sys, time
from pathlib import Path
root = Path(os.environ["FAKE_ROOT"])
name = Path(sys.argv[0]).name
home = Path(os.environ["HOME"])
signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
with (root / "events.jsonl").open("a") as f:
    f.write(json.dumps(dict(kind="window", name=name, home=str(home),
        args=sys.argv[1:], pid=os.getpid(), built=(root / "built").exists())) + "\n")
print("window-log:" + name, flush=True)
if name == "touchbar-customizer":
    config = home / ".config/touchbar-customizer/config.toml"
    config.parent.mkdir(parents=True, exist_ok=True)
    if not config.exists():
        config.write_text("initial-test-config")
while True:
    control = root / ("exit-" + name)
    if control.exists():
        sys.exit(int(control.read_text()))
    time.sleep(0.01)
'''


class Launcher(unittest.TestCase):
    def setUp(self):
        self.assertTrue(LAUNCHER.is_file(), "the one-command launcher is missing")
        self.assertTrue(os.access(LAUNCHER, os.X_OK), "launcher must be executable")
        self.temp = tempfile.TemporaryDirectory(prefix="tb-launcher-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "repo with spaces"
        (self.root / "scripts").mkdir(parents=True)
        shutil.copy2(LAUNCHER, self.root / "scripts/try-app.sh")
        self.original_home = Path(self.temp.name) / "original home"
        self.original_home.mkdir()
        self.session = self.root / ".scratch/manual-test"
        self.config = self.session / "home/.config/touchbar-customizer/config.toml"
        self.env = dict(os.environ, HOME=str(self.original_home), FAKE_ROOT=str(self.root))
        self.env["PATH"] = str(self.root / "bin") + os.pathsep + self.env["PATH"]
        self.program("bin/cargo", FAKE_CARGO)
        self.program("fake-window.py", FAKE_WINDOW)
        self.processes = []
        self.addCleanup(self.stop_all)

    def program(self, path, code):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(f"#!{sys.executable}\n" + code)
        target.chmod(0o755)

    def start(self, **env):
        proc = subprocess.Popen(
            [str(self.root / "scripts/try-app.sh")], cwd=self.temp.name,
            env=dict(self.env, **env), stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, start_new_session=True,
        )
        self.processes.append(proc)
        return proc

    def stop_all(self):
        for proc in self.processes:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            proc.communicate(timeout=5)

    def events(self):
        path = self.root / "events.jsonl"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def ready(self, proc, window_count=2):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            windows = [e for e in self.events() if e["kind"] == "window"]
            if len(windows) >= window_count:
                return windows[-2:]
            if proc.poll() is not None:
                self.fail("launcher exited before both windows started: " + proc.communicate()[0])
            time.sleep(0.01)
        self.fail("both windows did not become ready")

    def close_window(self, name, code=0):
        (self.root / ("exit-" + name)).write_text(str(code))

    def finished(self, proc, windows, expected=0):
        output = proc.communicate(timeout=5)[0]
        self.assertEqual(proc.returncode, expected, output)
        for window in windows:
            with self.assertRaises(ProcessLookupError, msg="launcher left a window running"):
                os.kill(window["pid"], 0)
        return output

    def test_shared_isolated_home_build_order_and_saved_config_survive_relaunch(self):
        self.config.parent.mkdir(parents=True)
        self.config.write_text("saved-test-config")
        for run in range(2):
            proc = self.start()
            windows = self.ready(proc, 2 * (run + 1))
            for window in windows:
                self.assertEqual(window["home"], str(self.session / "home"))
                self.assertTrue(window["built"], "a window started before Cargo completed")
            daemon = next(w for w in windows if w["name"] == "touchbard")
            self.assertEqual(daemon["args"], ["--sim-window", "--sim", str(self.session / "frames"),
                "--keys", "auto", "--emit", str(self.session / "keys.bin")])
            self.close_window("touchbar-customizer")
            output = self.finished(proc, windows)
            (self.root / "exit-touchbar-customizer").unlink()
            self.assertIn(str(self.config), output)
            self.assertIn(str(self.session / "logs"), output)
            self.assertIn("Ctrl+C", output)
            self.assertEqual(self.config.read_text(), "saved-test-config")
        builds = [e for e in self.events() if e["kind"] == "build"]
        self.assertEqual(len(builds), 2)
        self.assertEqual(self.events()[0]["kind"], "build")
        for build in builds:
            self.assertEqual(build["home"], str(self.original_home))
            self.assertEqual(build["jobs"], "2")
            self.assertGreaterEqual(build["nice"], 19)
            self.assertEqual(build["args"], ["build", "--locked", "-p", "tb-app", "-p",
                "tb-daemon", "--target-dir", str(self.root / "target")])
        logs = "\n".join(p.read_text() for p in (self.session / "logs").iterdir() if p.is_file())
        self.assertIn("window-log:touchbard", logs)
        self.assertIn("window-log:touchbar-customizer", logs)
        self.assertFalse((self.original_home / ".config/touchbar-customizer").exists())
        self.assertIn("scripts/try-app.sh", (ROOT / "README.md").read_text())

    def test_closing_simulator_stops_configurator(self):
        proc = self.start()
        windows = self.ready(proc)
        self.close_window("touchbard")
        self.finished(proc, windows)

    def test_window_failure_is_propagated_and_sibling_is_reaped(self):
        proc = self.start()
        windows = self.ready(proc)
        self.close_window("touchbard", 23)
        self.finished(proc, windows, expected=23)

    def test_interrupt_and_termination_stop_both_windows(self):
        for index, (sig, expected) in enumerate(((signal.SIGINT, 130), (signal.SIGTERM, 143))):
            with self.subTest(signal=sig):
                proc = self.start()
                windows = self.ready(proc, 2 * (index + 1))
                proc.send_signal(sig)
                self.finished(proc, windows, expected)

    def test_build_failure_launches_nothing(self):
        proc = self.start(FAKE_BUILD_EXIT="17")
        self.finished(proc, [], expected=17)
        self.assertEqual([e["kind"] for e in self.events()], ["build"])

    def test_missing_simulator_executable_leaves_no_window_running(self):
        proc = self.start(FAKE_MISSING_DAEMON="1")
        output = proc.communicate(timeout=5)[0]
        self.assertNotEqual(proc.returncode, 0, output)
        self.assertIn("touchbard", output)
        for window in (e for e in self.events() if e["kind"] == "window"):
            with self.assertRaises(ProcessLookupError):
                os.kill(window["pid"], 0)

    def test_overlapping_session_is_rejected_before_building_again(self):
        proc = self.start()
        windows = self.ready(proc)
        other = self.start()
        output = other.communicate(timeout=5)[0]
        self.assertNotEqual(other.returncode, 0, output)
        self.assertIsNone(proc.poll(), "second invocation stopped the first session")
        self.assertEqual(len([e for e in self.events() if e["kind"] == "build"]), 1)
        self.close_window("touchbard")
        self.finished(proc, windows)


if __name__ == "__main__":
    unittest.main()
