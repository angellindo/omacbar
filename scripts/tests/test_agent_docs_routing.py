"""The agent docs route every lane the way the scripts do.

Coarse text checks (no prose snapshots): the old wrong rule is gone and each section holds the
tokens that carry the rule.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def read(rel: str) -> str:
    return (ROOT / rel).read_text()


def step(text: str, n: int) -> str:
    """The body of numbered step `n.` up to step `n+1.` (or the end)."""
    m = re.search(rf"^{n}\. (.*?)(?=^{n + 1}\. |\Z)", text, re.M | re.S)
    if not m:
        raise AssertionError(f"no step {n}.")
    return m.group(1)


def description(text: str) -> str:
    m = re.search(r"^description: (.*)$", text, re.M)
    if not m:
        raise AssertionError("no description in front matter")
    return m.group(1)


def section(text: str, heading: str) -> str:
    m = re.search(rf"^#+ {re.escape(heading)}\s*$(.*?)(?=^#+ |\Z)", text, re.M | re.S)
    if not m:
        raise AssertionError(f"no section {heading!r}")
    return m.group(1)


class ImplementAcceptsAGreenStartContract(unittest.TestCase):
    doc = ".claude/skills/tb-implement/SKILL.md"

    def test_the_first_run_is_no_longer_required_to_exit_1(self):
        self.assertNotIn("exits 1 with every", read(self.doc))

    def test_the_preconditions_check_the_seal_and_the_red_history(self):
        pre = step(read(self.doc), 1)
        self.assertIn("seal", pre)
        self.assertIn("never_red", pre)

    def test_never_red_at_start_is_a_spec_missing_reason(self):
        block = re.search(r"### RESULT: spec-missing\n(.*?)```", read("docs/agents/results.md"), re.S)
        self.assertIsNotNone(block)
        self.assertIn("NEVER_RED", block.group(1))


class PlanRoutesByTheRecordedLane(unittest.TestCase):
    def test_the_plan_skill_no_longer_routes_by_type(self):
        self.assertNotIn("Research or Spike", read(".claude/skills/tb-plan/SKILL.md"))

    def test_the_plan_skill_reads_the_lane_from_the_intake_record(self):
        routing = step(read(".claude/skills/tb-plan/SKILL.md"), 2)
        self.assertIn(".ack", routing)
        self.assertIn("lane", routing)
        self.assertIn("tb-research", routing)

    def test_the_research_skill_is_for_research_lane_tickets(self):
        desc = description(read(".claude/skills/tb-research/SKILL.md"))
        self.assertRegex(desc.lower(), r"research[- ]lane")
        self.assertNotIn("Research-type", desc)

    def test_the_planner_resolves_research_lane_tickets_not_a_type(self):
        desc = description(read(".claude/agents/planner.md"))
        self.assertNotIn("Research/Spike", desc)
        self.assertRegex(desc.lower(), r"research[- ]lane")


class ResearchLaneEndToEnd(unittest.TestCase):
    def research_row(self) -> str:
        rows = [l for l in read(".claude/agents/orchestrator.md").splitlines() if l.startswith("| research |")]
        self.assertEqual(len(rows), 1)
        return rows[0]

    def test_the_orchestrator_research_lane_has_branch_commit_review_and_merge(self):
        row = self.research_row()
        self.assertTrue("git switch -c" in row or "bootstrap" in row, row)
        self.assertIn("commit", row)
        self.assertTrue("codex-review.sh" in row or "Codex" in row, row)
        self.assertIn("merge", row)

    def test_the_workflow_research_lane_ends_in_a_merge(self):
        line = next(l for l in read("docs/agents/workflow.md").split("\n\n") if l.startswith("Research lane:"))
        self.assertIn("merge", line)

    def test_the_research_skill_commits_its_doc(self):
        text = read(".claude/skills/tb-research/SKILL.md")
        self.assertIn("docs(research)", text)
        self.assertIn("commit", text.lower())

    def test_the_planner_commits_its_doc_in_the_research_lane(self):
        paragraphs = read(".claude/agents/planner.md").lower().split("\n\n")
        self.assertTrue(any("commit" in p and "research" in p for p in paragraphs))

    def test_research_done_reports_the_commit(self):
        results = read("docs/agents/results.md")
        start = results.index("research-done")
        block = results[start : results.index("## qa", start)]
        self.assertIn("Commit:", block)

    def test_the_review_skill_needs_no_plan_in_the_research_lane(self):
        pre = step(read(".claude/skills/tb-review/SKILL.md"), 1)
        self.assertIn("plan.md", pre)
        self.assertIn("research lane", pre)

    def test_the_review_rubric_needs_no_plan_in_the_research_lane(self):
        inputs = section(read("docs/agents/review-rubric.md"), "Inputs you are given")
        plan_lines = [l for l in inputs.splitlines() if "plan.md" in l]
        self.assertTrue(plan_lines)
        self.assertTrue(any("research" in l for l in plan_lines), plan_lines)


class ScriptTestsAreWiredIntoTheGates(unittest.TestCase):
    def test_check_runs_the_script_test_suite(self):
        self.assertIn("unittest discover -s scripts/tests", read("scripts/check.sh"))


class MirrorsMatch(unittest.TestCase):
    def test_every_routed_agent_file_matches_its_mirror(self):
        for rel in (
            "skills/tb-implement/SKILL.md",
            "skills/tb-plan/SKILL.md",
            "skills/tb-research/SKILL.md",
            "skills/tb-review/SKILL.md",
            "agents/orchestrator.md",
            "agents/planner.md",
        ):
            with self.subTest(rel=rel):
                self.assertEqual(read(f".claude/{rel}"), read(f".agents/{rel}"))


if __name__ == "__main__":
    unittest.main()
