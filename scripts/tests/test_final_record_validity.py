"""A final record is valid only for the current seal and HEAD.

Black box: the current `pipeline-gate.py` and `codex-review.sh` are copied into a throwaway git
repo with a stub `scripts/check.sh` and a stub `codex` first on PATH, so nothing compiles, no
model is called and the real repository is never touched.
"""

from __future__ import annotations

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
REVIEW = ROOT / "scripts" / "codex-review.sh"
TICKET = "TB-1"

STUB_CODEX = """#!/bin/sh
# Stub reviewer: records that it ran, swallows the prompt, approves.
echo called >> "$STUB_CODEX_CALLS"
out=""
while [ $# -gt 0 ]; do
  if [ "$1" = "-o" ]; then out="$2"; shift; fi
  shift
done
cat >/dev/null
printf 'VERDICT: APPROVED\\n\\nSUMMARY: stub\\n' > "$out"
"""


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


class ReviewRepo:
    """A committed temp repo with one sealed tooling slice, ready for --final and review."""

    def __init__(self, lane: str = "express", with_plan: bool = True, with_contract: bool = True):
        self.tmp = tempfile.TemporaryDirectory(prefix="tb-review-")
        self.root = Path(self.tmp.name) / "repo"
        self.root.mkdir()
        bin_dir = Path(self.tmp.name) / "bin"
        bin_dir.mkdir()
        codex = bin_dir / "codex"
        codex.write_text(STUB_CODEX)
        codex.chmod(0o755)
        self.calls = Path(self.tmp.name) / "codex-calls"
        self.env = git_env()
        self.env["PATH"] = f"{bin_dir}{os.pathsep}{self.env.get('PATH', '')}"
        self.env["STUB_CODEX_CALLS"] = str(self.calls)

        self.git("init", "-q", "-b", "main")
        (self.root / "scripts").mkdir()
        shutil.copy2(GATE, self.root / "scripts" / "pipeline-gate.py")
        shutil.copy2(REVIEW, self.root / "scripts" / "codex-review.sh")
        self.write_check_sh("exit 0")
        self.write(".gitignore", ".scratch/*/gate/\n.scratch/*/gates-*.log\n")
        self.write("src/app.txt", "app\n")
        self.write("src/flag.txt", "0\n")
        self.write("contract-tests/frozen.txt", "frozen contract\n")
        self.tdir = self.root / ".scratch" / TICKET
        self.cdir = self.tdir / "contract"
        self.write(f".scratch/{TICKET}/.ack", json.dumps({"ticket": TICKET, "confirmed": True, "lane": lane}))
        self.write(f".scratch/{TICKET}/ticket.md", "# TB-1 fixture ticket\n")
        if with_plan:
            self.write(f".scratch/{TICKET}/plan.md", "# TB-1 plan\n")
        if with_contract:
            self.write_manifest("fixture slice")
        self.commit_all("fixture")
        if with_contract:
            self.must(self.gate("--stamp"))

    def close(self) -> None:
        self.tmp.cleanup()

    # --- files and git ---

    def write(self, rel: str, text: str) -> None:
        path = self.root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def write_check_sh(self, body: str) -> None:
        path = self.root / "scripts" / "check.sh"
        path.write_text("#!/bin/sh\n" + body + "\n")
        path.chmod(0o755)

    def write_manifest(self, reason: str) -> None:
        check = [sys.executable, "-c", "import sys; sys.exit(int(open('src/flag.txt').read().strip()))"]
        manifest = {
            "version": 1,
            "mode": "tooling",
            "reason": reason,
            "files": ["contract-tests/frozen.txt"],
            "cases": [],
            "checks": [{"id": "tests", "command": check}],
        }
        self.write(f".scratch/{TICKET}/contract/slice-1.json", json.dumps(manifest, indent=2))

    def git(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["git", "-c", "commit.gpgsign=false", *args],
            cwd=self.root, env=self.env, capture_output=True, text=True, check=True,
        )

    def commit_all(self, msg: str) -> None:
        self.git("add", "-A")
        self.git("commit", "-q", "--allow-empty", "-m", msg)

    # --- the two scripts ---

    def gate(self, *flags: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, "scripts/pipeline-gate.py", TICKET, "--slice", "1", *flags],
            cwd=self.root, env=self.env, capture_output=True, text=True,
        )

    def review(self) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["bash", "scripts/codex-review.sh", TICKET, "main"],
            cwd=self.root, env=self.env, capture_output=True, text=True, timeout=120,
        )

    @staticmethod
    def must(run: subprocess.CompletedProcess) -> subprocess.CompletedProcess:
        assert run.returncode == 0, run.stdout + run.stderr
        return run

    # --- records ---

    @property
    def final_path(self) -> Path:
        return self.cdir / "slice-1.final.json"

    @property
    def seal_path(self) -> Path:
        return self.cdir / "slice-1.seal"

    def final(self) -> dict:
        return json.loads(self.final_path.read_text())

    def edit_final(self, **fields) -> None:
        record = self.final()
        for key, value in fields.items():
            if value is None:
                record.pop(key, None)
            else:
                record[key] = value
        self.final_path.write_text(json.dumps(record, indent=2) + "\n")

    def codex_called(self) -> bool:
        return self.calls.exists()


class Fixture(unittest.TestCase):
    def setUp(self):
        self.repos: list[ReviewRepo] = []

    def tearDown(self):
        for repo in self.repos:
            repo.close()

    def repo(self, **kw) -> ReviewRepo:
        repo = ReviewRepo(**kw)
        self.repos.append(repo)
        return repo

    def passed_repo(self) -> ReviewRepo:
        repo = self.repo()
        repo.must(repo.gate("--final"))
        self.assertEqual(repo.final()["result"], "PASS")
        return repo


class FinalRecordLifecycle(Fixture):
    def test_a_pass_record_stores_the_current_seal(self):
        repo = self.passed_repo()
        self.assertEqual(repo.final()["seal"], repo.seal_path.read_text().strip())

    def test_restamping_a_slice_deletes_its_final_record(self):
        repo = self.passed_repo()
        repo.write_manifest("contract changed by QA")
        repo.must(repo.gate("--stamp"))
        self.assertFalse(repo.final_path.exists(), "a stale final record survived a re-seal")

    def test_a_final_run_whose_check_is_not_green_leaves_no_record(self):
        repo = self.passed_repo()
        repo.write("src/flag.txt", "1\n")
        repo.commit_all("check goes red")
        run = repo.gate("--final")
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        self.assertFalse(repo.final_path.exists(), "a non-PASS --final left a final record")

    def test_a_final_run_whose_check_script_fails_leaves_no_record(self):
        repo = self.passed_repo()
        repo.write_check_sh("exit 1")
        repo.commit_all("check.sh goes red")
        run = repo.gate("--final")
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        self.assertFalse(repo.final_path.exists(), "a non-PASS --final left a final record")

    def test_a_final_run_on_a_dirty_tree_leaves_no_record(self):
        repo = self.passed_repo()
        repo.write("src/app.txt", "uncommitted\n")
        run = repo.gate("--final")
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        self.assertFalse(repo.final_path.exists(), "a non-PASS --final left a final record")

    def test_a_final_run_with_a_broken_seal_leaves_no_record(self):
        repo = self.passed_repo()
        repo.write("contract-tests/frozen.txt", "edited without a re-seal\n")
        repo.commit_all("frozen file edited")
        run = repo.gate("--final")
        self.assertEqual(run.returncode, 2, run.stdout + run.stderr)
        self.assertIn("SEAL_BROKEN", run.stderr)
        self.assertFalse(repo.final_path.exists(), "a non-PASS --final left a final record")


class ReviewAcceptsOnlyAValidRecord(Fixture):
    def assert_refused(self, repo: ReviewRepo) -> subprocess.CompletedProcess:
        run = repo.review()
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        self.assertFalse(repo.codex_called(), "codex ran although the final record is not valid")
        return run

    def test_a_pass_record_for_head_and_the_current_seal_is_reviewed(self):
        repo = self.passed_repo()
        run = repo.review()
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertTrue(repo.codex_called())

    def test_a_record_whose_result_is_not_pass_is_refused(self):
        repo = self.passed_repo()
        repo.edit_final(result="FAIL")
        self.assert_refused(repo)

    def test_a_record_for_another_head_is_refused(self):
        repo = self.passed_repo()
        repo.commit_all("a later commit")
        self.assert_refused(repo)

    def test_a_record_for_another_seal_is_refused(self):
        repo = self.passed_repo()
        repo.edit_final(seal="sha256:" + "0" * 64)
        self.assert_refused(repo)

    def test_a_record_without_a_seal_is_refused(self):
        repo = self.passed_repo()
        repo.edit_final(seal=None)
        self.assert_refused(repo)

    def test_a_record_without_a_seal_file_is_refused(self):
        repo = self.passed_repo()
        repo.seal_path.unlink()
        self.assert_refused(repo)

    def test_a_stale_pass_after_a_reseal_under_the_same_head_is_refused(self):
        repo = self.passed_repo()
        repo.write_manifest("contract changed by QA")
        repo.must(repo.gate("--stamp"))
        self.assert_refused(repo)


class ReviewPreconditionsPerLane(Fixture):
    def test_the_research_lane_is_reviewed_without_plan_or_contract(self):
        repo = self.repo(lane="research", with_plan=False, with_contract=False)
        run = repo.review()
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertTrue(repo.codex_called())

    def test_the_express_lane_without_a_plan_is_refused(self):
        repo = self.repo(lane="express", with_plan=False)
        repo.must(repo.gate("--final"))
        run = repo.review()
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        self.assertIn("missing", run.stderr)
        self.assertIn("plan.md", run.stderr)
        self.assertFalse(repo.codex_called())


if __name__ == "__main__":
    unittest.main()
