"""`settle` takes the expectation of the step that produced the config (and of a captured link's
target) and compares it with what it finds. A missing, malformed or different expectation records
nothing restore could act on: the current bytes stay in place, the original stays in the store,
and restore refuses. What settle reads after a save never certifies itself."""

from __future__ import annotations

import os
import unittest

from rehearsal_fixture import ABSENT, Fixture, expect_link, replace_bytes, sha256, state, store_holds

ORIGINAL = b'# original layout\nlabel = "Mine"\n'
SAVED = b'label = "Edited by the test"\n'
EXTERNAL = b'label = "Edited by someone else before settle"\n'


class Expectation(Fixture):
    """Original captured; the test's own save (SAVED) is what the producing step expects."""

    def setUp(self):
        super().setUp()
        self.config.parent.mkdir(parents=True)
        self.config.write_bytes(ORIGINAL)
        os.chmod(self.config, 0o640)
        self.original = state(self.config)
        self.capture()
        replace_bytes(self.config, SAVED)

    def assertNotAuthorized(self, p, current: bytes):
        """A settle that must not authorize restore: refused (1) or a conflict (3), never usage (2)."""
        self.assertIn(p.returncode, (1, 3), f"settle must understand and refuse: {p.stdout}{p.stderr}")
        r = self.tool("restore", "--manifest", self.manifest)
        self.assertNotEqual(r.returncode, 0, f"restore after an unauthorized settle: {r.stdout}{r.stderr}")
        self.assertEqual(self.config.read_bytes(), current, "the current bytes stay in place")
        self.assertTrue(store_holds(self.store, ORIGINAL), "and the original stays in the store")

    def test_an_edit_landing_between_the_save_and_settle_is_never_adopted(self):
        replace_bytes(self.config, EXTERNAL)  # someone else saves before the test settles
        p = self.tool("settle", "--manifest", self.manifest, "--expect-config", sha256(SAVED))
        self.assertNotAuthorized(p, EXTERNAL)
        self.tool("cleanup", "--manifest", self.manifest)
        self.assertEqual(self.config.read_bytes(), EXTERNAL, "cleanup keeps the external version")
        self.assertTrue(store_holds(self.store, ORIGINAL), "no passing verify: the store stays")

    def test_a_settle_without_an_expectation_authorizes_nothing(self):
        p = self.tool("settle", "--manifest", self.manifest)
        self.assertNotEqual(p.returncode, 0, "the state read after the save does not certify itself")
        r = self.tool("restore", "--manifest", self.manifest)
        self.assertNotEqual(r.returncode, 0, f"nothing settled, nothing to restore: {r.stdout}{r.stderr}")
        self.assertEqual(self.config.read_bytes(), SAVED)
        self.assertTrue(store_holds(self.store, ORIGINAL))

    def test_a_wrong_kind_or_a_malformed_expectation_is_refused(self):
        for expect in (ABSENT, expect_link("elsewhere.toml"), sha256(b"other bytes"), "not-a-hash",
                       sha256(SAVED)[:-1]):
            with self.subTest(expect=expect):
                p = self.tool("settle", "--manifest", self.manifest, "--expect-config", expect)
                self.assertNotAuthorized(p, SAVED)

    def test_the_expected_save_settles_and_restores_the_original(self):
        self.settle(sha256(SAVED))  # control: the producer's expectation matches
        self.restore()
        self.assertEqual(state(self.config), self.original)
        self.ok("verify", "--manifest", self.manifest)


class AbsentExpectation(Fixture):
    def test_an_expected_absence_is_compared_like_any_other_state(self):
        self.capture()
        self.config.parent.mkdir(parents=True)
        self.config.write_bytes(b'label = "F1"\n')  # a default written by someone else
        p = self.tool("settle", "--manifest", self.manifest, "--expect-config", ABSENT)
        self.assertIn(p.returncode, (1, 3), p.stdout + p.stderr)
        self.assertNotEqual(self.tool("restore", "--manifest", self.manifest).returncode, 0)
        self.assertEqual(self.config.read_bytes(), b'label = "F1"\n', "the unexpected file is kept")
        self.config.unlink()
        self.settle(ABSENT)  # control: once the path is what was expected, settle records it
        self.restore()
        self.assertEqual(state(self.home / ".config"), {"type": "absent"})


class TargetExpectation(Fixture):
    """A symlinked config: settle needs the expectation of the link target as well."""

    def setUp(self):
        super().setUp()
        linked = self.tmp / "linked dir"
        linked.mkdir(mode=0o700)
        self.target = linked / "real.toml"
        self.target.write_bytes(ORIGINAL)
        os.chmod(self.target, 0o600)
        self.config.parent.mkdir(parents=True)
        self.link_text = os.path.relpath(self.target, self.config.parent)
        os.symlink(self.link_text, self.config)
        self.capture()

    def test_a_captured_link_needs_an_expectation_for_its_target(self):
        replace_bytes(self.config, SAVED)  # the save renames over the link
        p = self.tool("settle", "--manifest", self.manifest, "--expect-config", sha256(SAVED))
        self.assertNotAuthorized(p)

    def test_a_target_edited_before_settle_is_never_adopted(self):
        replace_bytes(self.config, SAVED)  # the save renames over the link; the target is not ours
        replace_bytes(self.target, EXTERNAL, 0o600)  # someone edits the target before settle
        p = self.tool("settle", "--manifest", self.manifest, "--expect-config", sha256(SAVED),
                      "--expect-target", sha256(ORIGINAL))
        self.assertNotAuthorized(p)
        self.assertEqual(self.target.read_bytes(), EXTERNAL, "the external target version stays")

    def test_a_write_through_the_link_settles_with_both_expectations(self):
        self.target.write_bytes(SAVED)  # the test writes through the link
        self.settle(expect_link(self.link_text), expect_target=sha256(SAVED))
        self.restore()
        self.assertEqual(os.readlink(self.config), self.link_text)
        self.assertEqual(self.target.read_bytes(), ORIGINAL)
        self.ok("verify", "--manifest", self.manifest)

    def assertNotAuthorized(self, p):
        self.assertIn(p.returncode, (1, 3), f"settle must understand and refuse: {p.stdout}{p.stderr}")
        r = self.tool("restore", "--manifest", self.manifest)
        self.assertNotEqual(r.returncode, 0, f"restore after an unauthorized settle: {r.stdout}{r.stderr}")
        self.assertEqual(self.config.read_bytes(), SAVED, "the test's save stays in place")
        self.assertTrue(store_holds(self.store, ORIGINAL), "the original target stays in the store")


if __name__ == "__main__":
    unittest.main()
