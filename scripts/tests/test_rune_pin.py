"""Rune and the crates Rune asks its apps to patch resolve from the public Rune repository at one
exact revision, recorded in the versioned lockfile, never from a local path.

Pure: reads the real `Cargo.toml` and `Cargo.lock` with `tomllib`; no cargo, no network. The rule
functions are checked first against small inline fixtures (the shapes that must be rejected), then
against the repository's own files.

Scope: only Rune's dependency, the `[patch]` tables and the packages that come from the Rune
repository; other dependencies keep whatever policy the project already has.
"""

from __future__ import annotations

import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

RUNE_GIT = "https://github.com/anomalyco/rune"
RUNE_REV = "22ebd7e7f5ceb189e1829f066b5ba0fe31ed5598"
RUNE_SOURCE = f"git+{RUNE_GIT}?rev={RUNE_REV}#{RUNE_REV}"
# The crates.io crates Rune's own workspace patches with its `vendor/` copies at RUNE_REV and asks
# an app that builds Rune to patch too (Rune `Cargo.toml` `[patch.crates-io]`).
RUNE_VENDORED = {
    "wgpu-hal",
    "naga",
    "winit",
    "sctk-adwaita",
    "parley_engine",
    "accesskit_windows",
    "accesskit_atspi_common",
    "accesskit_macos",
}


def is_rune_source(source: str) -> bool:
    return source.startswith(f"git+{RUNE_GIT}?") or source.startswith(f"git+{RUNE_GIT}#") or source == f"git+{RUNE_GIT}"


# ---------- rules (each returns the list of problems, empty when the rule holds) ----------

def rune_dependency_problems(manifest: dict) -> list[str]:
    dep = manifest.get("workspace", {}).get("dependencies", {}).get("rune")
    if not isinstance(dep, dict):
        return ["[workspace.dependencies] has no rune table"]
    problems = []
    if dep.get("git") != RUNE_GIT:
        problems.append(f"rune git is {dep.get('git')!r}, expected {RUNE_GIT!r}")
    if dep.get("rev") != RUNE_REV:
        problems.append(f"rune rev is {dep.get('rev')!r}, expected {RUNE_REV!r}")
    for key in ("branch", "tag", "path", "version"):
        if key in dep:
            problems.append(f"rune must not set {key!r} (exact public rev only)")
    if dep.get("default-features") is not False:
        problems.append("rune keeps default-features = false")
    if "embedded-fonts" not in dep.get("features", []):
        problems.append("rune keeps the embedded-fonts feature")
    return problems


def patch_problems(manifest: dict) -> list[str]:
    problems = []
    for table, entries in manifest.get("patch", {}).items():
        for name, spec in entries.items():
            where = f"[patch.{table!r}] {name}"
            if table != "crates-io":
                problems.append(f"{where}: only [patch.crates-io] is allowed (no override of the Rune git source)")
            if name not in RUNE_VENDORED:
                problems.append(f"{where}: not one of the crates Rune vendors")
            if not isinstance(spec, dict) or spec.get("git") != RUNE_GIT or spec.get("rev") != RUNE_REV:
                problems.append(f"{where}: must be {{ git = {RUNE_GIT!r}, rev = {RUNE_REV!r} }}, got {spec!r}")
                continue
            for key in ("path", "branch", "tag"):
                if key in spec:
                    problems.append(f"{where}: must not set {key!r}")
    return problems


def workspace_member_names(manifest: dict, read_member) -> set[str]:
    """Package names of the workspace members; `read_member(path)` returns a member's manifest."""
    return {read_member(m)["package"]["name"] for m in manifest.get("workspace", {}).get("members", [])}


def lock_problems(lock: dict, manifest: dict, members: set[str]) -> list[str]:
    problems = []
    packages = lock.get("package", [])
    rune = [p for p in packages if p["name"] == "rune"]
    if not rune:
        problems.append("the lock has no rune package")
    for p in packages:
        source = p.get("source")
        if source is None:
            if p["name"] not in members:
                problems.append(f"{p['name']} {p.get('version')} has no source: a local path outside the workspace")
        elif is_rune_source(source) and source != RUNE_SOURCE:
            problems.append(f"{p['name']} resolves from {source}, expected {RUNE_SOURCE}")
    for p in rune:
        if p.get("source") != RUNE_SOURCE:
            problems.append(f"rune resolves from {p.get('source')}, expected {RUNE_SOURCE}")
    declared = {name for entries in manifest.get("patch", {}).values() for name in entries}
    for name in sorted(declared):
        if not any(p["name"] == name and p.get("source") == RUNE_SOURCE for p in packages):
            problems.append(f"patched {name} does not resolve from {RUNE_SOURCE}")
    for unused in lock.get("patch", {}).get("unused", []):
        problems.append(f"the lock records an unused patch: {unused.get('name')}")
    return problems


def cargo_config_problems(root: Path) -> list[str]:
    return [f"{p.relative_to(root)} can override sources outside Cargo.toml"
            for p in (root / ".cargo" / "config", root / ".cargo" / "config.toml") if p.exists()]


# ---------- fixtures ----------

def manifest_fixture(rune: str, patch: str = "") -> dict:
    return tomllib.loads(
        '[workspace]\nmembers = ["crates/a"]\n[workspace.dependencies]\n'
        f"rune = {rune}\n{patch}"
    )


GOOD_RUNE = f'{{ git = "{RUNE_GIT}", rev = "{RUNE_REV}", default-features = false, features = ["embedded-fonts"] }}'


def lock_fixture(*packages: str, extra: str = "") -> dict:
    return tomllib.loads("version = 4\n" + "".join(f"[[package]]\n{p}\n" for p in packages) + extra)


MEMBER = 'name = "a"\nversion = "0.1.0"'
RUNE_AT_PIN = f'name = "rune"\nversion = "0.1.0"\nsource = "{RUNE_SOURCE}"'
SERDE = 'name = "serde"\nversion = "1.0.0"\nsource = "registry+https://github.com/rust-lang/crates.io-index"'


class RulesRejectWhatMustNotShip(unittest.TestCase):
    def test_a_public_pin_at_the_revision_passes_the_dependency_rule(self):
        self.assertEqual(rune_dependency_problems(manifest_fixture(GOOD_RUNE)), [])

    def test_a_rune_pin_at_another_revision_is_rejected_naming_both(self):
        other = GOOD_RUNE.replace(RUNE_REV, "f73f72cdc563534e4f7de9f5a79bed007ac42384")
        problems = rune_dependency_problems(manifest_fixture(other))
        self.assertEqual(len(problems), 1)
        self.assertIn("f73f72cdc563534e4f7de9f5a79bed007ac42384", problems[0])
        self.assertIn(RUNE_REV, problems[0])

    def test_a_rune_branch_or_local_path_is_rejected(self):
        branch = f'{{ git = "{RUNE_GIT}", branch = "dev", default-features = false, features = ["embedded-fonts"] }}'
        path = '{ path = "../../src/rune-head/crates/rune", default-features = false, features = ["embedded-fonts"] }'
        self.assertTrue(any("'branch'" in p for p in rune_dependency_problems(manifest_fixture(branch))))
        self.assertTrue(any("'path'" in p for p in rune_dependency_problems(manifest_fixture(path))))

    def test_a_vendored_crate_patched_from_the_pinned_rune_repo_passes(self):
        m = manifest_fixture(GOOD_RUNE, f'[patch.crates-io]\nnaga = {{ git = "{RUNE_GIT}", rev = "{RUNE_REV}" }}\n')
        self.assertEqual(patch_problems(m), [])

    def test_a_patch_to_a_relative_prototype_path_is_rejected(self):
        m = manifest_fixture(GOOD_RUNE, '[patch.crates-io]\nnaga = { path = "../../src/rune-head/vendor/naga" }\n')
        self.assertTrue(patch_problems(m))

    def test_a_patch_to_an_absolute_machine_path_is_rejected(self):
        m = manifest_fixture(GOOD_RUNE, '[patch.crates-io]\nwinit = { path = "/home/someone/rune/vendor/winit" }\n')
        self.assertTrue(patch_problems(m))

    def test_overriding_the_rune_git_source_with_a_path_is_rejected(self):
        m = manifest_fixture(
            GOOD_RUNE, f'[patch."{RUNE_GIT}"]\nrune = {{ path = "../../src/rune-head/crates/rune" }}\n'
        )
        self.assertTrue(any("only [patch.crates-io]" in p for p in patch_problems(m)))

    def test_a_patch_at_another_rune_revision_or_a_crate_rune_does_not_vendor_is_rejected(self):
        other_rev = manifest_fixture(GOOD_RUNE, f'[patch.crates-io]\nnaga = {{ git = "{RUNE_GIT}", rev = "f73f72c" }}\n')
        stranger = manifest_fixture(GOOD_RUNE, f'[patch.crates-io]\nserde = {{ git = "{RUNE_GIT}", rev = "{RUNE_REV}" }}\n')
        self.assertTrue(patch_problems(other_rev))
        self.assertTrue(any("not one of the crates Rune vendors" in p for p in patch_problems(stranger)))

    def test_a_lock_with_rune_at_the_pin_and_registry_crates_passes(self):
        self.assertEqual(lock_problems(lock_fixture(MEMBER, RUNE_AT_PIN, SERDE), manifest_fixture(GOOD_RUNE), {"a"}), [])

    def test_a_lock_with_a_rune_package_at_another_revision_is_rejected(self):
        old = "f73f72cdc563534e4f7de9f5a79bed007ac42384"
        staging = f'name = "rune-staging"\nversion = "0.1.0"\nsource = "git+{RUNE_GIT}?rev={old}#{old}"'
        problems = lock_problems(lock_fixture(MEMBER, RUNE_AT_PIN, staging), manifest_fixture(GOOD_RUNE), {"a"})
        self.assertTrue(any("rune-staging" in p and old in p for p in problems))

    def test_a_sourceless_package_that_is_not_a_workspace_member_is_rejected(self):
        local = 'name = "naga"\nversion = "30.0.1"'
        problems = lock_problems(lock_fixture(MEMBER, RUNE_AT_PIN, local), manifest_fixture(GOOD_RUNE), {"a"})
        self.assertTrue(any("naga" in p and "local path" in p for p in problems))

    def test_an_unused_patch_recorded_in_the_lock_is_rejected(self):
        unused = f'[[patch.unused]]\nname = "accesskit_macos"\nversion = "0.27.2"\nsource = "{RUNE_SOURCE}"\n'
        problems = lock_problems(lock_fixture(MEMBER, RUNE_AT_PIN, extra=unused), manifest_fixture(GOOD_RUNE), {"a"})
        self.assertTrue(any("unused patch" in p for p in problems))

    def test_a_declared_patch_missing_from_the_lock_is_rejected(self):
        m = manifest_fixture(GOOD_RUNE, f'[patch.crates-io]\nnaga = {{ git = "{RUNE_GIT}", rev = "{RUNE_REV}" }}\n')
        self.assertTrue(any("patched naga" in p for p in lock_problems(lock_fixture(MEMBER, RUNE_AT_PIN), m, {"a"})))


class TheRepositoryPinsRuneAtThePublicRevision(unittest.TestCase):
    maxDiff = None

    @classmethod
    def setUpClass(cls):
        cls.manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
        cls.lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
        cls.members = workspace_member_names(
            cls.manifest, lambda m: tomllib.loads((ROOT / m / "Cargo.toml").read_text())
        )

    def test_the_workspace_depends_on_rune_at_the_public_revision(self):
        self.assertEqual(rune_dependency_problems(self.manifest), [])

    def test_patches_take_rune_vendored_crates_from_the_pinned_rune_repository(self):
        self.assertEqual(patch_problems(self.manifest), [])

    def test_the_versioned_lock_resolves_rune_from_the_pinned_revision_and_nothing_from_a_local_path(self):
        self.assertEqual(lock_problems(self.lock, self.manifest, self.members), [])

    def test_no_cargo_config_in_the_repository_overrides_sources(self):
        self.assertEqual(cargo_config_problems(ROOT), [])


if __name__ == "__main__":
    unittest.main()
