"""Rehearsal profile capture → settle → restore → verify on throwaway trees: regular, absent,
invalid and symlinked configs come back with their bytes, type, mode, link and parents; paths
outside the owned profile are refused before anything changes."""

from __future__ import annotations

import os
import unittest

from rehearsal_fixture import CONFIG_REL, THEME_REL, Fixture, replace_bytes, state, store_holds, tree

COMMENTED = b'# my layout, keep this comment\n[[layers]]\nlabel = "Mine"\n'
SAVED = b'# rewritten by a save\nlabel = "Edited"\n'
INVALID = b'not = [ valid toml \xff\n'


class RestoreScenarios(Fixture):
    def edit_and_restore(self):
        """The test's edit (a save replaces the file), then settle and restore."""
        self.config.parent.mkdir(parents=True, exist_ok=True)
        replace_bytes(self.config, SAVED)
        self.settle()
        self.restore()

    def test_a_commented_file_with_mode_0640_comes_back_byte_for_byte(self):
        self.config.parent.mkdir(parents=True)
        self.config.write_bytes(COMMENTED)
        os.chmod(self.config, 0o640)
        before = state(self.config)
        self.capture()
        self.edit_and_restore()
        self.assertEqual(state(self.config), before)
        self.ok("verify", "--manifest", self.manifest)
        self.assertOutsideUntouched()

    def test_an_invalid_file_gets_its_exact_bytes_and_mode_back(self):
        self.config.parent.mkdir(parents=True)
        self.config.write_bytes(INVALID)
        os.chmod(self.config, 0o600)
        before = state(self.config)
        self.capture()
        self.edit_and_restore()
        self.assertEqual(state(self.config), before)
        self.ok("verify", "--manifest", self.manifest)

    def test_an_absent_config_with_absent_parents_leaves_no_trace_but_the_home(self):
        self.capture()
        self.edit_and_restore()  # first run creates .config/touchbar-customizer/config.toml
        self.assertEqual(state(self.home / ".config"), {"type": "absent"})
        self.assertEqual(state(self.home)["type"], "dir")
        self.ok("verify", "--manifest", self.manifest)

    def test_an_unknown_file_in_a_parent_the_test_created_survives_restore(self):
        self.capture()
        self.config.parent.mkdir(parents=True)
        replace_bytes(self.config, SAVED)
        stranger = self.home / ".config" / "other-app.toml"
        stranger.write_bytes(b"written by someone else\n")
        self.settle()
        self.restore()
        self.assertEqual(state(self.config), {"type": "absent"})
        self.assertEqual(stranger.read_bytes(), b"written by someone else\n",
                         "a new directory is removed only when empty; other files are never ours")

    def test_verify_sees_a_default_written_over_a_restored_absence(self):
        self.capture()
        self.edit_and_restore()
        self.ok("verify", "--manifest", self.manifest)
        self.config.parent.mkdir(parents=True, exist_ok=True)
        self.config.write_bytes(b'label = "F1"\n')  # what a reopen that writes defaults leaves
        self.assertNotEqual(self.tool("verify", "--manifest", self.manifest).returncode, 0,
                            "absence is checked before any reopen can recreate the file")

    def test_an_absent_config_keeps_existing_parents_and_unrelated_files(self):
        parent = self.config.parent
        parent.mkdir(parents=True)
        (parent / "notes.txt").write_bytes(b"unrelated\n")
        (parent / "config.toml.bak").write_bytes(b"an older backup\n")
        os.chmod(parent, 0o750)
        before = tree(self.home)
        self.capture()
        self.edit_and_restore()
        self.assertEqual(state(self.config), {"type": "absent"})
        self.assertEqual(tree(self.home), before, "parents, notes and the old backup are unchanged")

    def test_a_symlinked_config_gets_its_link_text_and_target_back(self):
        linked = self.tmp / "linked dir"
        linked.mkdir(mode=0o700)
        target = linked / "real.toml"
        target.write_bytes(COMMENTED)
        os.chmod(target, 0o600)
        self.config.parent.mkdir(parents=True)
        link_text = os.path.relpath(target, self.config.parent)
        os.symlink(link_text, self.config)
        before_link, before_target = state(self.config), state(target)
        self.capture()
        self.edit_and_restore()  # the save renames over the link, as a temp+rename save does
        self.assertEqual(state(self.config), before_link)
        self.assertEqual(os.readlink(self.config), link_text)
        self.assertEqual(state(target), before_target)
        self.ok("verify", "--manifest", self.manifest)

    def test_verify_fails_before_restore_and_on_theme_drift(self):
        theme = self.seed_theme()
        self.config.parent.mkdir(parents=True)
        self.config.write_bytes(COMMENTED)
        self.capture()
        replace_bytes(self.config, SAVED)
        self.settle()
        self.assertNotEqual(self.tool("verify", "--manifest", self.manifest).returncode, 0,
                            "verify must fail while the edit is still in place")
        self.restore()
        self.ok("verify", "--manifest", self.manifest)
        (self.home / THEME_REL).write_bytes(theme + b"\n# drift\n")
        self.assertNotEqual(self.tool("verify", "--manifest", self.manifest).returncode, 0,
                            "a changed active theme is a provenance difference")


class RefusedBeforeAnyChange(Fixture):
    def assertRefused(self, *args):
        p = self.tool(*args)
        self.assertNotEqual(p.returncode, 0, f"{args} must be refused: {p.stdout}{p.stderr}")
        self.assertNotIn(p.returncode, (3,), "a refusal is not a conflict")
        self.assertFalse(self.store.exists(), "nothing is stored on a refusal")
        self.assertFalse(self.manifest.exists(), "no manifest on a refusal")
        self.assertOutsideUntouched()

    def test_the_caller_home_the_session_and_traversals_into_them_are_refused(self):
        for home in (self.real_home, self.real_home / "Documents" / "..",
                     self.home / ".." / ".." / "caller home", self.session / "home",
                     self.real_home / "Documents"):
            with self.subTest(home=str(home)):
                self.assertRefused("capture", "--home", home, "--store", self.store,
                                   "--manifest", self.manifest)
        self.capture()  # control: the private profile itself is accepted

    def test_a_store_inside_the_tree_being_replaced_is_refused(self):
        self.config.parent.mkdir(parents=True)
        self.config.write_bytes(COMMENTED)
        before = tree(self.home)
        for store in (self.config.parent / "keep", self.home / ".config" / "keep"):
            with self.subTest(store=str(store)):
                p = self.tool("capture", "--home", self.home, "--store", store, "--manifest", self.manifest)
                self.assertNotEqual(p.returncode, 0)
                self.assertEqual(tree(self.home), before)
                self.assertFalse(self.manifest.exists())
        self.capture()  # control: a store outside the replaced tree is accepted

    def test_a_store_inside_the_symlink_target_directory_is_refused(self):
        linked = self.tmp / "linked dir"
        linked.mkdir()
        (linked / "real.toml").write_bytes(COMMENTED)
        self.config.parent.mkdir(parents=True)
        os.symlink(linked / "real.toml", self.config)
        p = self.tool("capture", "--home", self.home, "--store", linked / "keep", "--manifest", self.manifest)
        self.assertNotEqual(p.returncode, 0)
        self.assertEqual(sorted(x.name for x in linked.iterdir()), ["real.toml"])
        self.capture()  # control

    @unittest.skipIf(os.geteuid() == 0, "root reads mode 000 files")
    def test_an_unreadable_config_aborts_capture_with_nothing_modified(self):
        self.config.parent.mkdir(parents=True)
        self.config.write_bytes(COMMENTED)
        os.chmod(self.config, 0o000)
        meta = lambda: (sorted(os.listdir(self.config.parent)), self.config.lstat().st_mode,
                        self.config.lstat().st_size, self.config.lstat().st_mtime_ns)
        before = meta()
        p = self.tool("capture", "--home", self.home, "--store", self.store, "--manifest", self.manifest)
        self.assertNotEqual(p.returncode, 0)
        self.assertEqual(meta(), before, "nothing in the config directory changed")
        os.chmod(self.config, 0o600)
        self.assertEqual(self.config.read_bytes(), COMMENTED)
        self.assertNotEqual(self.tool("restore", "--manifest", self.manifest).returncode, 0,
                            "an aborted capture leaves nothing restore could act on")
        self.capture()  # control: once readable, the same capture works


if __name__ == "__main__":
    unittest.main()
