"""`pipeline-gate.py --final` judges the committed tree it was asked about.

Black box: the current gate is copied into a throwaway git repo with a stub `scripts/check.sh`
and a `tooling` manifest, so nothing compiles and the real repository is never touched.
"""

from __future__ import annotations

import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GATE = ROOT / "scripts" / "pipeline-gate.py"
TICKET = "TB-1"


def load_gate():
    spec = importlib.util.spec_from_file_location("pipeline_gate", GATE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def git_env() -> dict[str, str]:
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env.update(
        GIT_CONFIG_GLOBAL=os.devnull,
        GIT_CONFIG_NOSYSTEM="1",
        GIT_AUTHOR_NAME="t",
        GIT_AUTHOR_EMAIL="t@t",
        GIT_COMMITTER_NAME="t",
        GIT_COMMITTER_EMAIL="t@t",
    )
    return env


def flag_check() -> list[str]:
    """A `tests` check that is green while src/flag.txt holds 0."""
    return [sys.executable, "-c", "import sys; sys.exit(int(open('src/flag.txt').read().strip()))"]


class GateRepo:
    """A committed temp repo with one sealed tooling slice."""

    def __init__(
        self,
        check_cmd: list[str] | None = None,
        check_sh: str = "exit 0",
        extra_checks: list[dict] | None = None,
    ):
        self.tmp = tempfile.TemporaryDirectory(prefix="tb-gate-")
        self.root = Path(self.tmp.name)
        self.env = git_env()
        self.git("init", "-q", "-b", "main")
        (self.root / "scripts").mkdir()
        shutil.copy2(GATE, self.root / "scripts" / "pipeline-gate.py")
        self.write_check_sh(check_sh)
        self.write(".gitignore", ".scratch/*/gate/\n")
        self.write("src/app.txt", "app\n")
        self.write("src/flag.txt", "0\n")
        self.write("contract-tests/frozen.txt", "frozen contract\n")
        self.tdir = self.root / ".scratch" / TICKET
        self.cdir = self.tdir / "contract"
        self.write(f".scratch/{TICKET}/.ack", json.dumps({"ticket": TICKET, "confirmed": True, "lane": "express"}))
        manifest = {
            "version": 1,
            "mode": "tooling",
            "reason": "fixture slice",
            "files": ["contract-tests/frozen.txt"],
            "cases": [],
            "checks": [{"id": "tests", "command": check_cmd or flag_check()}, *(extra_checks or [])],
        }
        self.write(f".scratch/{TICKET}/contract/slice-1.json", json.dumps(manifest, indent=2))
        self.commit_all("fixture")
        stamp = self.gate("--stamp")
        assert stamp.returncode == 0, stamp.stderr

    def close(self) -> None:
        self.tmp.cleanup()

    def write(self, rel: str, text: str) -> None:
        path = self.root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def write_check_sh(self, body: str) -> None:
        path = self.root / "scripts" / "check.sh"
        path.write_text("#!/bin/sh\n" + body + "\n")
        path.chmod(0o755)

    def git(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["git", "-c", "commit.gpgsign=false", *args],
            cwd=self.root, env=self.env, capture_output=True, text=True, check=True,
        )

    def commit_all(self, msg: str) -> None:
        self.git("add", "-A")
        self.git("commit", "-q", "-m", msg)

    def head(self) -> str:
        return self.git("rev-parse", "HEAD").stdout.strip()

    def gate(self, *flags: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, "scripts/pipeline-gate.py", TICKET, "--slice", "1", *flags],
            cwd=self.root, env=self.env, capture_output=True, text=True,
        )

    @property
    def final_record(self) -> Path:
        return self.cdir / "slice-1.final.json"


class DirtyOutsideScratch(unittest.TestCase):
    """`dirty_outside_scratch(porcelain)` reads raw `git status --porcelain` text."""

    def setUp(self):
        self.dirty = load_gate().dirty_outside_scratch

    def test_a_modified_tracked_scratch_file_on_the_first_line_is_not_dirty(self):
        self.assertEqual(self.dirty(" M .scratch/TB-7/contract/slice-1.evidence.json\n"), [])

    def test_a_modified_tracked_source_file_is_dirty(self):
        found = self.dirty(" M crates/x.rs\n")
        self.assertEqual(len(found), 1)
        self.assertIn("crates/x.rs", found[0])

    def test_an_untracked_file_outside_scratch_is_dirty(self):
        found = self.dirty("?? notes.txt\n")
        self.assertEqual(len(found), 1)
        self.assertIn("notes.txt", found[0])

    def test_an_untracked_scratch_directory_is_not_dirty(self):
        self.assertEqual(self.dirty("?? .scratch/\n"), [])

    def test_only_the_entries_outside_scratch_are_returned(self):
        porcelain = " M .scratch/TB-7/contract/slice-1.evidence.json\n M crates/x.rs\n?? .scratch/TB-7/gate/\n"
        found = self.dirty(porcelain)
        self.assertEqual(len(found), 1)
        self.assertIn("crates/x.rs", found[0])

    def test_a_rename_out_of_source_into_scratch_is_dirty(self):
        found = self.dirty("R  crates/a.rs -> .scratch/a.rs\n")
        self.assertEqual(len(found), 1)
        self.assertIn("crates/a.rs", found[0])

    def test_a_rename_from_scratch_into_source_is_dirty(self):
        found = self.dirty("R  .scratch/a.rs -> crates/a.rs\n")
        self.assertEqual(len(found), 1)
        self.assertIn("crates/a.rs", found[0])

    def test_an_empty_status_is_clean(self):
        self.assertEqual(self.dirty(""), [])


class FinalOnACommittedTree(unittest.TestCase):
    def setUp(self):
        self.repos: list[GateRepo] = []

    def tearDown(self):
        for repo in self.repos:
            repo.close()

    def repo(self, **kw) -> GateRepo:
        repo = GateRepo(**kw)
        self.repos.append(repo)
        return repo

    def assert_no_final_record(self, repo: GateRepo, run: subprocess.CompletedProcess) -> None:
        self.assertFalse(repo.final_record.exists(), f"a final record was written\n{run.stdout}\n{run.stderr}")

    def test_a_clean_committed_tree_passes_and_records_pass(self):
        repo = self.repo()
        run = repo.gate("--final")
        self.assertEqual(run.returncode, 0, run.stderr)
        record = json.loads(repo.final_record.read_text())
        self.assertEqual(record["result"], "PASS")
        self.assertEqual(record["head"], repo.head())

    def test_a_modified_tracked_scratch_file_does_not_block_final(self):
        repo = self.repo()
        self.assertEqual(repo.gate().returncode, 0)  # evidence.json now exists
        repo.write(f".scratch/{TICKET}/index.md", "Phase: IMPLEMENT\n")
        repo.commit_all("track scratch state")  # seal, evidence and index.md are tracked
        repo.write(f".scratch/{TICKET}/index.md", "Phase: REVIEW\n")
        run = repo.gate("--final")
        self.assertEqual(run.returncode, 0, run.stderr)
        self.assertEqual(json.loads(repo.final_record.read_text())["result"], "PASS")

    def test_a_dirty_source_file_blocks_final(self):
        repo = self.repo()
        repo.write("src/app.txt", "uncommitted\n")
        run = repo.gate("--final")
        self.assertEqual(run.returncode, 1, run.stderr)
        self.assert_no_final_record(repo, run)

    def test_a_check_that_edits_a_tracked_source_file_fails_final(self):
        edit = [sys.executable, "-c", "open('src/app.txt', 'a').write('changed by the check\\n')"]
        repo = self.repo(check_cmd=edit)
        run = repo.gate("--final")
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        self.assert_no_final_record(repo, run)

    def test_a_check_script_that_edits_a_tracked_source_file_fails_final(self):
        repo = self.repo(check_sh="echo changed >> src/app.txt\nexit 0")
        run = repo.gate("--final")
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        self.assert_no_final_record(repo, run)

    def test_a_check_script_that_moves_head_fails_final(self):
        repo = self.repo(check_sh="git -c commit.gpgsign=false commit -q --allow-empty -m moved\nexit 0")
        run = repo.gate("--final")
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        self.assert_no_final_record(repo, run)

    def test_a_check_script_that_edits_a_frozen_file_fails_final(self):
        repo = self.repo(check_sh="echo tampered >> contract-tests/frozen.txt\nexit 0")
        run = repo.gate("--final")
        # The tree is dirty and the seal is broken: either refusal (1 or 2) is correct.
        self.assertIn(run.returncode, (1, 2), run.stdout + run.stderr)
        self.assert_no_final_record(repo, run)


def py(code: str) -> list[str]:
    return [sys.executable, "-c", code]


EDIT_APP = py("open('src/app.txt', 'a').write('changed by the check\\n')")
RESTORE_APP = ["git", "checkout", "-q", "--", "src/app.txt"]


class FinalAfterAMutationThatIsUndone(unittest.TestCase):
    """A subprocess changes the tree, HEAD or a frozen file and a later one puts it back."""

    def setUp(self):
        self.repos: list[GateRepo] = []

    def tearDown(self):
        for repo in self.repos:
            repo.close()

    def repo(self, **kw) -> GateRepo:
        repo = GateRepo(**kw)
        self.repos.append(repo)
        return repo

    def assert_refused(self, repo: GateRepo, run: subprocess.CompletedProcess, codes=(1,)) -> None:
        self.assertIn(run.returncode, codes, run.stdout + run.stderr)
        self.assertFalse(repo.final_record.exists(), f"a final record was written\n{run.stdout}\n{run.stderr}")

    def test_a_check_edit_restored_by_the_check_script_fails_final(self):
        repo = self.repo(check_cmd=EDIT_APP, check_sh="git checkout -q -- src/app.txt\nexit 0")
        self.assert_refused(repo, repo.gate("--final"))

    def test_a_check_edit_restored_by_a_later_check_fails_final(self):
        repo = self.repo(check_cmd=EDIT_APP, extra_checks=[{"id": "restore", "command": RESTORE_APP}])
        self.assert_refused(repo, repo.gate("--final"))

    def test_a_check_that_moves_head_and_a_check_script_that_moves_it_back_fails_final(self):
        commit = ["git", "-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", "moved"]
        repo = self.repo(check_cmd=commit, check_sh="git reset -q --soft HEAD~1\nexit 0")
        self.assert_refused(repo, repo.gate("--final"))

    def test_a_check_frozen_file_edit_restored_by_a_later_check_fails_final(self):
        tamper = py("open('contract-tests/frozen.txt', 'a').write('tampered\\n')")
        restore = ["git", "checkout", "-q", "--", "contract-tests/frozen.txt"]
        repo = self.repo(check_cmd=tamper, extra_checks=[{"id": "restore", "command": restore}])
        # Dirty tree or broken seal: either refusal (1 or 2) is correct.
        self.assert_refused(repo, repo.gate("--final"), codes=(1, 2))


SEAL = f".scratch/{TICKET}/contract/slice-1.seal"
SEAL_BACKUP = f".scratch/{TICKET}/seal.bak"
REWRITE_SEAL = py(f"open({SEAL!r}, 'w').write('sha256:' + '0' * 64 + '\\n')")
DELETE_SEAL = py(f"import os; os.remove({SEAL!r})")
BACKUP_AND_REWRITE_SEAL = py(
    f"import shutil; shutil.copyfile({SEAL!r}, {SEAL_BACKUP!r}); "
    f"open({SEAL!r}, 'w').write('sha256:' + '0' * 64 + '\\n')"
)
RESTORE_SEAL = py(f"import shutil; shutil.copyfile({SEAL_BACKUP!r}, {SEAL!r})")


class FinalWhileTheSealFileChanges(unittest.TestCase):
    """The slice's `.seal` lives in `.scratch/`, where edits are allowed, yet --final must not
    pass if a subprocess rewrites or deletes it, even when a later one puts it back."""

    def setUp(self):
        self.repos: list[GateRepo] = []

    def tearDown(self):
        for repo in self.repos:
            repo.close()

    def repo(self, **kw) -> GateRepo:
        repo = GateRepo(**kw)
        self.repos.append(repo)
        return repo

    def assert_refused(self, repo: GateRepo, run: subprocess.CompletedProcess) -> None:
        # Seal broken (2) or contract invalid (1): either refusal is correct.
        self.assertIn(run.returncode, (1, 2), run.stdout + run.stderr)
        self.assertFalse(repo.final_record.exists(), f"a final record was written\n{run.stdout}\n{run.stderr}")

    def test_a_check_that_rewrites_the_seal_file_fails_final(self):
        repo = self.repo(extra_checks=[{"id": "reseal", "command": REWRITE_SEAL}])
        self.assert_refused(repo, repo.gate("--final"))

    def test_a_check_that_deletes_the_seal_file_fails_final(self):
        repo = self.repo(extra_checks=[{"id": "unseal", "command": DELETE_SEAL}])
        self.assert_refused(repo, repo.gate("--final"))

    def test_a_seal_file_rewrite_restored_by_a_later_check_fails_final(self):
        repo = self.repo(extra_checks=[
            {"id": "reseal", "command": BACKUP_AND_REWRITE_SEAL},
            {"id": "restore", "command": RESTORE_SEAL},
        ])
        self.assert_refused(repo, repo.gate("--final"))

    def test_a_check_script_that_rewrites_the_seal_file_fails_final(self):
        repo = self.repo(check_sh=f"echo sha256:tampered > {SEAL}\nexit 0")
        self.assert_refused(repo, repo.gate("--final"))

    def test_a_check_script_that_deletes_the_seal_file_fails_final(self):
        repo = self.repo(check_sh=f"rm -f {SEAL}\nexit 0")
        self.assert_refused(repo, repo.gate("--final"))

    def test_a_check_that_edits_another_tracked_scratch_file_still_passes_final(self):
        note = py(f"open('.scratch/{TICKET}/index.md', 'w').write('Phase: REVIEW\\n')")
        repo = self.repo(extra_checks=[{"id": "note", "command": note}])
        repo.write(f".scratch/{TICKET}/index.md", "Phase: IMPLEMENT\n")
        repo.commit_all("track scratch state")  # seal and index.md are tracked
        run = repo.gate("--final")
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        record = json.loads(repo.final_record.read_text())
        self.assertEqual(record["result"], "PASS")
        self.assertEqual(record["seal"], (repo.root / SEAL).read_text().strip())


if __name__ == "__main__":
    unittest.main()
