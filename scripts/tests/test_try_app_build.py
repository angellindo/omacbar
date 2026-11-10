"""Cancel an actual launcher while its fake build and compiler are running."""

import os
import signal
import subprocess
import time
import unittest

import test_try_app as fixture


BLOCKING_BUILD = r'''
import os, signal, subprocess, sys, time
from pathlib import Path
root = Path(os.environ["FAKE_ROOT"])
child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
def stop(*_):
    child.terminate()
    child.wait()
    sys.exit(0)
signal.signal(signal.SIGTERM, stop)
(root / "compiler.pid").write_text(str(child.pid))
(root / "build.pid").write_text(str(os.getpid()))
while True:
    time.sleep(0.01)
'''


class BuildCancellation(unittest.TestCase):
    def test_signals_during_build_stop_cargo_and_compiler_and_release_lock(self):
        for sig, expected in ((signal.SIGINT, 130), (signal.SIGTERM, 143)):
            with self.subTest(signal=sig):
                run = fixture.Launcher()
                run.setUp()
                self.addCleanup(run.doCleanups)
                self.addCleanup(self.stop_build, run)
                run.program("bin/cargo", BLOCKING_BUILD)
                proc = run.start()
                deadline = time.monotonic() + 5
                while not (run.root / "build.pid").exists():
                    self.assertIsNone(proc.poll(), "launcher exited before build readiness")
                    self.assertLess(time.monotonic(), deadline, "build did not become ready")
                    time.sleep(0.01)
                proc.send_signal(sig)
                self.assertEqual(proc.wait(timeout=5), expected)
                for name in ("build.pid", "compiler.pid"):
                    pid = int((run.root / name).read_text())
                    with self.assertRaises(ProcessLookupError, msg=f"{name} still running"):
                        os.kill(pid, 0)
                self.assertFalse((run.root / "built").exists())
                locked = subprocess.run(["flock", "-n", str(run.session / "launch.lock"), "true"],
                                        check=False)
                self.assertEqual(locked.returncode, 0, "cancelled build retained session lock")

    @staticmethod
    def stop_build(run):
        for name in ("compiler.pid", "build.pid"):
            path = run.root / name
            if path.exists():
                try:
                    os.kill(int(path.read_text()), signal.SIGTERM)
                except ProcessLookupError:
                    pass


if __name__ == "__main__":
    unittest.main()
