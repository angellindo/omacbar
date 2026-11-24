"""Restore never loses a version: external and locked editors make it a conflict with both
versions kept, swapped-in symlinks and parents are never followed, failures and interrupts keep a
recoverable original, and cleanup removes only the test's own unchanged artifacts. Settle always
carries the expectation of the test's own save."""

from __future__ import annotations

import fcntl
import os
import signal
import subprocess
import sys
import time
import unittest

from rehearsal_fixture import Fixture, replace_bytes, sha256, state, store_holds, tree

ORIGINAL = b'# original layout\nlabel = "Mine"\n'
SAVED = b'label = "Edited by the test"\n'
EXTERNAL = b'label = "Edited by someone else"\n'


class Settled(Fixture):
    """Original captured, the test's save in place and settled."""

    def setUp(self):
        super().setUp()
        self.config.parent.mkdir(parents=True)
        self.config.write_bytes(ORIGINAL)
        os.chmod(self.config, 0o640)
        self.original = state(self.config)
        self.capture()
        replace_bytes(self.config, SAVED)
        self.settle(sha256(SAVED))

    def lock_dir(self):
        """Hold the configurator's lock: exclusive flock on the config directory."""
        fd = os.open(self.config.parent, os.O_RDONLY | os.O_DIRECTORY)
        fcntl.flock(fd, fcntl.LOCK_EX)
        return fd

    def start_restore(self):
        p = subprocess.Popen([sys.executable, "-B", str(self.tool_path), "restore", "--manifest",
                              str(self.manifest)], cwd=self.repo, env=self.env,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.addCleanup(lambda: p.poll() is None and p.kill())
        time.sleep(0.5)
        return p

    def names(self):
        return sorted(os.listdir(self.config.parent))


class Conflicts(Settled):
    def test_an_external_edit_after_the_test_is_a_conflict_that_keeps_both_versions(self):
        replace_bytes(self.config, EXTERNAL)
        self.restore(code=3)
        self.assertEqual(self.config.read_bytes(), EXTERNAL, "the external version stays in place")
        self.assertTrue(store_holds(self.store, ORIGINAL), "the original stays in the store")

    def test_a_symlink_swapped_in_for_the_config_is_never_followed(self):
        outside = self.tmp / "outside"
        outside.mkdir()
        (outside / "victim.toml").write_bytes(SAVED)  # same bytes the test left
        before = tree(outside)
        self.config.unlink()
        os.symlink(outside / "victim.toml", self.config)
        self.assertNotEqual(self.tool("restore", "--manifest", self.manifest).returncode, 0)
        self.assertEqual(tree(outside), before)
        self.assertTrue(store_holds(self.store, ORIGINAL))

    def test_a_parent_swapped_for_a_symlink_is_never_written_through(self):
        outside = self.tmp / "outside"
        outside.mkdir()
        (outside / "config.toml").write_bytes(SAVED)
        before = tree(outside)
        parent = self.config.parent
        os.rename(parent, self.tmp / "moved away")
        os.symlink(outside, parent)
        self.assertNotEqual(self.tool("restore", "--manifest", self.manifest).returncode, 0)
        self.assertEqual(tree(outside), before)
        self.assertOutsideUntouched()

    def test_a_restore_waits_for_a_locked_editor_and_then_keeps_its_change(self):
        fd = self.lock_dir()
        p = self.start_restore()
        self.assertEqual(self.config.read_bytes(), SAVED, "no write while another editor holds the lock")
        replace_bytes(self.config, EXTERNAL)  # the second editor saves under its lock
        os.close(fd)
        out, err = p.communicate(timeout=30)
        self.assertNotEqual(p.returncode, 0, out + err)
        self.assertEqual(self.config.read_bytes(), EXTERNAL, "both versions survive: current here")
        self.assertTrue(store_holds(self.store, ORIGINAL), "and the original in the store")


class Failures(Settled):
    def test_an_interrupt_during_restore_keeps_a_whole_file_and_the_store(self):
        fd = self.lock_dir()
        names = self.names()
        p = self.start_restore()
        p.send_signal(signal.SIGINT)
        p.communicate(timeout=30)
        os.close(fd)
        self.assertNotEqual(p.returncode, 0)
        self.assertEqual(self.config.read_bytes(), SAVED)
        self.assertEqual(self.names(), names, "no temp file is left behind")
        self.assertTrue(store_holds(self.store, ORIGINAL))
        self.restore()
        self.assertEqual(state(self.config), self.original, "the original is still recoverable")

    @unittest.skipIf(os.geteuid() == 0, "root writes into mode 0555 directories")
    def test_a_failed_write_keeps_the_current_file_and_a_recoverable_original(self):
        names = self.names()
        os.chmod(self.config.parent, 0o555)
        p = self.tool("restore", "--manifest", self.manifest)
        os.chmod(self.config.parent, 0o755)
        self.assertNotIn(p.returncode, (0, 3), p.stdout + p.stderr)
        self.assertEqual(self.config.read_bytes(), SAVED)
        self.assertEqual(self.names(), names)
        self.restore()
        self.assertEqual(state(self.config), self.original)


class Cleanup(Fixture):
    """Own artifacts are declared to `settle` with the hash the producing step expects; nothing
    becomes the test's own just because it appeared in a directory listing."""

    def setUp(self):
        super().setUp()
        parent = self.config.parent
        parent.mkdir(parents=True)
        self.config.write_bytes(ORIGINAL)
        self.old_bak = parent / "older.toml.bak"
        self.old_bak.write_bytes(b"an unrelated older backup\n")
        self.capture()
        replace_bytes(self.config, SAVED)
        self.own_bak = parent / "config.toml.bak"  # an acknowledged overwrite made one
        self.own_bak.write_bytes(ORIGINAL)
        self.mark = self.home / "rehearsal-mark"
        self.mark.write_bytes(b"x")
        self.own = ("--artifact", self.own_bak, sha256(ORIGINAL), "--artifact", self.mark, sha256(b"x"))

    def test_cleanup_refuses_while_a_recorded_process_is_alive(self):
        proc = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
        self.addCleanup(lambda: proc.poll() is None and proc.kill())
        self.settle(sha256(SAVED), "--pid", proc.pid, *self.own)
        self.restore()
        self.assertNotEqual(self.tool("cleanup", "--manifest", self.manifest).returncode, 0)
        self.assertTrue(self.own_bak.exists() and self.mark.exists(), "nothing removed while running")
        proc.kill()
        proc.wait()
        self.ok("cleanup", "--manifest", self.manifest)
        self.assertFalse(self.own_bak.exists())
        self.assertFalse(self.mark.exists())

    def test_cleanup_removes_only_unchanged_own_artifacts(self):
        early = self.config.parent / "config.toml.bak.1"  # app-like name, but nobody declared it
        early.write_bytes(b"appeared before settle\n")
        self.settle(sha256(SAVED), *self.own)
        self.mark.write_bytes(b"xx")  # changed after the test left it
        late = self.home / "late.txt"
        late.write_bytes(b"unknown to the test\n")
        self.restore()
        self.tool("cleanup", "--manifest", self.manifest)
        self.assertFalse(self.own_bak.exists(), "own unchanged backup is removed")
        self.assertEqual(self.mark.read_bytes(), b"xx", "a changed artifact is kept")
        self.assertEqual(late.read_bytes(), b"unknown to the test\n", "an unknown file is kept")
        self.assertEqual(early.read_bytes(), b"appeared before settle\n",
                         "a listing difference never makes a file the test's own")
        self.assertEqual(self.old_bak.read_bytes(), b"an unrelated older backup\n")
        self.assertEqual(self.config.read_bytes(), ORIGINAL)

    def test_an_artifact_that_does_not_match_its_expected_hash_is_never_owned(self):
        self.tool("settle", "--manifest", self.manifest, "--expect-config", sha256(SAVED),
                  "--artifact", self.mark, sha256(b"y"))
        self.tool("restore", "--manifest", self.manifest)
        self.tool("cleanup", "--manifest", self.manifest)
        self.assertTrue(self.own_bak.exists(), "an undeclared artifact is not removed either")
        self.assertEqual(self.mark.read_bytes(), b"x", "a mismatch keeps the file")

    def test_a_symlink_swapped_in_for_an_own_artifact_is_kept_with_its_target(self):
        self.settle(sha256(SAVED), *self.own)
        elsewhere = self.tmp / "elsewhere"
        elsewhere.mkdir()
        (elsewhere / "mark").write_bytes(b"x")  # same bytes behind the link
        self.mark.unlink()
        os.symlink(elsewhere / "mark", self.mark)
        self.restore()
        self.assertEqual(self.config.read_bytes(), ORIGINAL)
        self.tool("cleanup", "--manifest", self.manifest)
        self.assertEqual(state(self.mark), {"type": "symlink", "link": str(elsewhere / "mark")})
        self.assertEqual((elsewhere / "mark").read_bytes(), b"x")

    def test_cleanup_after_a_conflict_keeps_the_store(self):
        self.settle(sha256(SAVED), *self.own)
        replace_bytes(self.config, EXTERNAL)
        self.restore(code=3)
        self.tool("cleanup", "--manifest", self.manifest)
        self.assertEqual(self.config.read_bytes(), EXTERNAL)
        self.assertTrue(store_holds(self.store, ORIGINAL), "no passing verify: the store stays")


if __name__ == "__main__":
    unittest.main()
