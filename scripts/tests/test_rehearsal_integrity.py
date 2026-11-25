"""Restore and cleanup act only on what the test owns, through the directories they validated.

An external editor that saves while restore is still preparing the file it will put back keeps its
version (a last comparison right before the replacement), for the config and for a captured link's
target. cleanup never walks a parent or a store that was swapped for a symlink, and changes
nothing but the test's own unchanged artifacts. The editor is a cooperating in-process hook on
the tool's descriptor syncs (see `rehearsal_fixture.EDITOR_HOOK`); no test reproduces an attack."""

from __future__ import annotations

import os
import shutil
import unittest

from rehearsal_fixture import Fixture, expect_link, replace_bytes, sha256, state, store_holds, tree

ORIGINAL = b'# original layout\nlabel = "Mine"\n'
SAVED = b'label = "Edited by the test"\n'
EXTERNAL = b'label = "Saved by another editor during restore"\n'


class EditDuringPreparation(Fixture):
    def test_an_edit_while_the_original_is_being_prepared_is_kept(self):
        self.config.parent.mkdir(parents=True)
        self.config.write_bytes(ORIGINAL)
        os.chmod(self.config, 0o640)
        self.capture()
        replace_bytes(self.config, SAVED)
        self.settle(sha256(SAVED))
        names = sorted(os.listdir(self.config.parent))
        p, saved = self.tool_with_editor("restore", "--manifest", self.manifest, watched=self.config,
                                         payload=ORIGINAL, external=EXTERNAL)
        self.assertTrue(saved, f"restore prepares the original in a file it syncs: {p.stdout}{p.stderr}")
        self.assertNotEqual(p.returncode, 0, "a change during preparation is a conflict, not a restore")
        self.assertEqual(self.config.read_bytes(), EXTERNAL, "the other editor's version stays in place")
        self.assertTrue(store_holds(self.store, ORIGINAL), "and the original stays in the store")
        self.assertEqual(sorted(os.listdir(self.config.parent)), names, "no temp file is left behind")

    def test_an_edit_of_the_link_target_while_it_is_being_prepared_is_kept(self):
        linked = self.tmp / "linked dir"
        linked.mkdir(mode=0o700)
        target = linked / "real.toml"
        target.write_bytes(ORIGINAL)
        os.chmod(target, 0o600)
        self.config.parent.mkdir(parents=True)
        link_text = os.path.relpath(target, self.config.parent)
        os.symlink(link_text, self.config)
        self.capture()
        target.write_bytes(SAVED)  # the test writes through the link
        self.settle(expect_link(link_text), expect_target=sha256(SAVED))
        p, saved = self.tool_with_editor("restore", "--manifest", self.manifest, watched=target,
                                         payload=ORIGINAL, external=EXTERNAL)
        self.assertTrue(saved, f"restore prepares the target in a file it syncs: {p.stdout}{p.stderr}")
        self.assertNotEqual(p.returncode, 0)
        self.assertEqual(target.read_bytes(), EXTERNAL, "the other editor's target version stays")
        self.assertEqual(os.readlink(self.config), link_text)
        self.assertTrue(store_holds(self.store, ORIGINAL))
        self.assertEqual(sorted(x.name for x in linked.iterdir()), ["real.toml"])


class CleanupConfinement(Fixture):
    """Restored and verified; one own artifact (an acknowledged overwrite's backup) declared."""

    def setUp(self):
        super().setUp()
        self.parent = self.config.parent
        self.parent.mkdir(parents=True)
        self.config.write_bytes(ORIGINAL)
        self.capture()
        replace_bytes(self.config, SAVED)
        self.own_bak = self.parent / "config.toml.bak"
        self.own_bak.write_bytes(ORIGINAL)
        self.settle(sha256(SAVED), "--artifact", self.own_bak, sha256(ORIGINAL))
        self.restore()
        self.ok("verify", "--manifest", self.manifest)

    def swap_for_link(self, directory, name):
        """Same content elsewhere, and `directory` replaced by a symlink to it."""
        outside = self.tmp / name
        shutil.copytree(directory, outside, symlinks=True)
        os.rename(directory, self.tmp / f"{name} (moved away)")
        os.symlink(outside, directory)
        return outside

    def test_a_parent_swapped_for_a_symlink_is_never_walked_by_cleanup(self):
        outside = self.swap_for_link(self.parent, "outside")
        before = tree(outside)
        self.tool("cleanup", "--manifest", self.manifest)
        self.assertEqual(tree(outside), before, "same names and bytes behind the link are not ours")
        self.assertOutsideUntouched()

    def test_a_store_swapped_for_a_symlink_is_never_emptied_through_it(self):
        outside = self.swap_for_link(self.store, "outside store")
        before = tree(outside)
        self.tool("cleanup", "--manifest", self.manifest)
        self.assertEqual(tree(outside), before, "a store reached through a link is not the store")

    def test_cleanup_changes_nothing_but_the_own_unchanged_artifacts(self):
        foreign = {self.home / "foreign.txt": b"someone else's\n",
                   self.parent / "config.toml.bak.2": ORIGINAL,  # same bytes as the own backup
                   self.store / "foreign-in-store": ORIGINAL}  # an unknown file in the store
        for path, data in foreign.items():
            path.write_bytes(data)
        skip = {str(self.manifest.relative_to(self.tmp))}
        before = {k: v for k, v in tree(self.tmp).items() if k not in skip}
        self.ok("cleanup", "--manifest", self.manifest)
        after = {k: v for k, v in tree(self.tmp).items() if k not in skip}
        self.assertFalse(self.own_bak.exists(), "the own unchanged backup is removed")
        for path, data in foreign.items():
            self.assertEqual(state(path)["bytes"], data, f"{path.name} is not the test's own")
        self.assertEqual(self.config.read_bytes(), ORIGINAL)
        self.assertEqual(set(after) - set(before), set(), "cleanup creates nothing")
        for rel, s in after.items():
            self.assertEqual(s, before[rel], f"{rel} changed")
        store_rel = str(self.store.relative_to(self.tmp))
        own_rel = str(self.own_bak.relative_to(self.tmp))
        for rel in set(before) - set(after):
            self.assertTrue(rel == own_rel or rel.startswith(store_rel + os.sep),
                            f"{rel} removed: only own artifacts and the tool's store copies go")


if __name__ == "__main__":
    unittest.main()
