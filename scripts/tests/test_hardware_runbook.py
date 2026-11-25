"""Structure of `docs/hardware-runbook.md`: every stage names its owner, authorized target, entry
state, expected changes, stop criterion, return and evidence; physical stages are human-only; the
three different "returns" are kept apart; the driver compile evidence is pinned with its limits;
nothing the repo has not verified is stated as done; the service manifest preserved in 2.0 is part
of the removal's expected changes and evidence and is put back, metadata included, on the way
B → A. This checks the document's content markers, not any hardware fact, and not the exact
wording of column labels."""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RUNBOOK = ROOT / "docs/hardware-runbook.md"
# One pattern per field a stage must name; a header cell matches by meaning, in any order.
FIELDS = {
    "owner": r"owner",
    "target": r"target",
    "entry": r"entry|start",
    "changes": r"change",
    "stop": r"stop",
    "return": r"return|back|rollback",
    "evidence": r"evidence|compar",
}
MANIFEST = r"(?i)\bmanifest\b|installed\.json"
STAGES = ["S0", "S1", "S2", "S3", "S4", "B → A"]
PINNED = {
    "Seborider T1 stack commit": "3a00d8dceb2cfeb75c3e6b9f501ead095c1d9924",
    "Linux stable commit of the stock sources": "497791a0a96b866d90add64041c47acdff49cae6",
    "stock appletbdrm.c": "44d7ed7d8d0fc7ee86c479f9123dd1640cf68066201a35922d783abd6f4e4d29",
    "stock hid-appletb-bl.c": "e1126767e0668f8b82d23e7904b4f180b5b5f6b51aa0a144339e601dce31dd25",
    "appletbdrm-t1.patch": "6621a2669b66afdd695c819e42deefd7b17f447f186ee393f1c2440deecede8e",
    "hid-appletb-bl-t1.patch": "932aec1bb0a4358179b81f90a17e84deec4362f7d7134d4b7ed982a7e1b9e642",
    "patched appletbdrm.c": "bd3348ed92234ea22dc7bc2f30ec521df1b4606549767e0dedf445fae2d9626b",
    "patched hid-appletb-bl.c": "761816cc6dd3c0e2fa04a056b51bdc3248385094e3857bea40589aa5c132756d",
}


def sections(text):
    """Heading line → body, for `##`/`###` headings."""
    parts = re.split(r"^(#{2,3} .+)$", text, flags=re.M)
    return {parts[i].strip(): parts[i + 1] for i in range(1, len(parts) - 1, 2)}


def first_table(body):
    rows = [line for line in body.splitlines() if line.strip().startswith("|")]
    cells = [[c.strip() for c in r.strip().strip("|").split("|")] for r in rows]
    return [r for r in cells if not all(re.fullmatch(r":?-{3,}:?", c) for c in r)]


class HardwareRunbook(unittest.TestCase):
    def setUp(self):
        self.assertTrue(RUNBOOK.is_file(), "docs/hardware-runbook.md is missing")
        self.text = RUNBOOK.read_text()
        self.sections = sections(self.text)

    def stage(self, name):
        found = [h for h in self.sections if re.search(rf"(^|\W){re.escape(name)}(\W|$)", h)]
        self.assertTrue(found, f"no heading for stage {name}")
        return self.sections[found[0]]

    def columns(self, header):
        """Field → column index; each field matches exactly one header cell."""
        out = {}
        for field, pattern in FIELDS.items():
            hits = [i for i, cell in enumerate(header) if re.search(pattern, cell, re.I)]
            self.assertTrue(hits, f"no column for {field} in {header}")
            out[field] = hits[0]
        self.assertEqual(len(set(out.values())), len(FIELDS), f"one column per field: {header}")
        return out

    def test_every_stage_names_the_seven_fields_with_no_empty_cell(self):
        for name in STAGES:
            with self.subTest(stage=name):
                table = first_table(self.stage(name))
                self.assertGreaterEqual(len(table), 2, "a header and at least one row")
                self.columns(table[0])
                for row in table[1:]:
                    self.assertEqual(len(row), len(table[0]), row)
                    self.assertTrue(all(row), f"empty cell in {row}")

    def test_physical_stages_are_owned_by_the_human(self):
        for name in STAGES[1:]:
            with self.subTest(stage=name):
                table = first_table(self.stage(name))
                owner = self.columns(table[0])["owner"]
                for row in table[1:]:
                    self.assertRegex(row[owner].lower(), r"\bhuman\b")
                    self.assertNotRegex(row[owner].lower(), r"\bagent\b")

    def row(self, stage, step):
        """The first row of `stage`'s table whose first cell starts with `step`, by field."""
        table = first_table(self.stage(stage))
        cols = self.columns(table[0])
        rows = [r for r in table[1:] if r[0].startswith(step)] if step else table[1:]
        self.assertTrue(rows, f"no row {step!r} in {stage}")
        return {field: rows[0][i] for field, i in cols.items()}

    def test_the_removal_stage_lists_the_manifest_change_in_its_changes_and_evidence(self):
        removal = self.row("S2", "2.4")
        for field in ("changes", "evidence"):
            self.assertRegex(removal[field], MANIFEST, f"2.4 {field} names the service manifest")

    def test_b_to_a_puts_back_the_preserved_service_manifest_with_its_metadata(self):
        back = self.row("B → A", "")
        for field in ("changes", "evidence"):
            self.assertRegex(back[field], MANIFEST, f"B → A {field} names the service manifest")
        body = self.stage("B → A")
        lines = body.splitlines()
        self.assertTrue(any(re.search(r"\binstalled\.json\b", l) and re.search(r"\b(cp|install|rsync)\b", l)
                            for l in lines), "a command puts installed.json back from the preserved copy")
        self.assertTrue(any(re.search(r"\binstalled\.json\b", l) and re.search(r"\b(cmp|sha256sum|diff)\b", l)
                            for l in lines), "a command compares it with the copy preserved in 2.0")
        self.assertTrue(any(re.search(r"\bstat\b", l) and re.search(r"\binstalled\.json\b", l) for l in lines),
                        "its metadata (stat) is compared with the copy preserved in 2.0")

    def test_baselines_a_and_b_are_separate_and_re_recorded(self):
        lower = self.text.lower()
        self.assertRegex(self.text, r"\bA\b.*TB-28")
        self.assertIn("baseline b", lower)
        self.assertRegex(lower, r"re-record")

    def test_unresearched_driver_steps_are_no_go_blanks(self):
        body = self.stage("S3")
        self.assertIn("TBD by TB-9", body)
        self.assertIn("no-go", body.lower())

    def test_reset_personal_restore_and_key_mode_are_three_things(self):
        found = {}
        for phrase in ("factory-default", "personal", "physical return to key mode"):
            hits = [h for h in self.sections if phrase in h.lower()]
            self.assertTrue(hits, f"heading for {phrase}")
            found[phrase] = self.sections[hits[0]].lower()
        reset, personal, physical = found["factory-default"], found["personal"], found["physical return to key mode"]
        # The factory reset ships (TB-48): stated as the configurator's own draft-only action.
        self.assertIn("load default buttons", reset, "the configurator action is named")
        for word in ("draft", "preview", "cancel", "recoverable copy", "compare-and-swap"):
            self.assertIn(word, reset)
        self.assertNotRegex(reset, r"not (yet )?(implemented|built)|follow-up", "it is no longer future work")
        # Restoring a personal copy ships too: a verified generation, the terminal tool, on a profile.
        for word in ("generation", "touchbar-recover", "rehearsal.py restore"):
            self.assertIn(word, personal)
        self.assertRegex(personal, r"physical[^\n]*\bs2\b[^\n]*\bs3\b", "the physical check stays in S2/S3")
        # The physical return stays apart from both and unverified: no config tool stands in for it.
        for word in ("generation", "touchbar-recover", "load default", "restore"):
            self.assertNotIn(word, physical, f"{word} is not a physical return")
        self.assertRegex(physical, r"firmware key row on the panel")
        self.assertRegex(self.text.lower(), r"physical return to config(uration)? 1[^\n]*unverified")
        resumen = self.sections["## Resumen para el usuario"].lower()
        self.assertRegex(resumen, r"retorno físico[^.]*no está comprobado")

    def test_physical_facts_are_cited_and_nothing_private_is_inlined(self):
        for research in ("0007", "0010"):
            self.assertIn(research, self.text)
        self.assertNotRegex(self.text, r"/home/[a-z]")
        self.assertIn("$SERVICE_SOURCES", self.text)

    def test_the_driver_compile_experiment_is_pinned_and_reproducible_from_a_checkout(self):
        for what, value in PINNED.items():
            with self.subTest(pinned=what):
                self.assertIn(value, self.text)
        self.assertRegex(self.text, r"make\b[^\n]*-C[^\n]*\bM=\S+[^\n]*\bmodules\b")
        self.assertNotIn(".scratch/", self.text, "a fresh checkout has no .scratch")

    def test_the_limits_of_the_evidence_are_stated(self):
        lower = self.text.lower()
        for word in ("lockdown", "sig_enforce", "modules_disabled"):
            self.assertIn(word, lower, "historical kernel policy, to recheck")
        self.assertRegex(lower, r"recheck|re-check")
        self.assertIn("clear_display", self.text, "binding can blank the current frame")
        self.assertIn("seat", lower, "seat/owner isolation before the DRM card appears")
        self.assertRegex(lower, r"config(uration)? 1[^\n]*unverified|unverified[^\n]*config(uration)? 1")
        for word in ("service files", "module tree", "firmware"):
            self.assertIn(word, lower, "the three recoveries are named apart")
        for key in ("Esc", "F12"):
            self.assertIn(key, self.text, "a return is shown on the physical key row")

    def test_research_0001_no_longer_says_both_drivers_choose_config_2_at_enumeration(self):
        text = (ROOT / "docs/research/0001-t1-touch-bar-hardware.md").read_text()
        self.assertNotIn("Both select config 2 at enumeration", text)
        self.assertIn("3a00d8d", text, "the correction cites the pinned Seborider revision")

    def test_linked_from_the_checklist_glossary_and_summarized_for_the_user(self):
        self.assertIn("hardware-runbook.md", (ROOT / "docs/hil-checklist.md").read_text())
        self.assertIn("Rehearsal profile", (ROOT / "GLOSSARY.md").read_text())
        self.assertIn("## Resumen para el usuario", self.text)


if __name__ == "__main__":
    unittest.main()
