---
name: tb-implement
description: "Implement or fix one planned Touch Bar Customizer ticket (implementer stage, Opus medium): make QA's sealed contract pass with TDD, run scripts/pipeline-gate.py --final, commit, return a RESULT payload. Use when a ticket is In Progress or Changes Requested."
---

# Implement a ticket

Input: `.scratch/TB-n/ticket.md`, `plan.md`, the sealed `contract/slice-N.json`, and for a fix the
latest `review-k.md` or QA defects. Output: commits on `tb-n-slug`, a passing
`scripts/pipeline-gate.py TB-n --slice N --final`, and a payload from `docs/agents/results.md`.

1. **Preconditions**: on branch `tb-n-slug` (the orchestrator made it); `contract/slice-N.seal`
   exists; `scripts/pipeline-gate.py TB-n --slice N` exits 1 with every `required` case already in
   `seen_red`. Otherwise return `spec-missing` with the reason.
2. **Read** the plan, `docs/standards/coding-standards.md`, `docs/standards/testing.md`,
   `GLOSSARY.md`, and the contract tests (read-only). For a fix, list every BLOCKER and MAJOR
   finding or QA defect; each becomes a slice.
3. **Slices**: for each plan slice in order, skill `tdd`: your own failing unit test where the
   contract does not reach (in non-frozen files), minimal code, green. Run the gate after each
   slice. Budget: 5 non-green gate runs per slice; at 5, return `contract-stuck`.
4. **Deviations**: if the plan is wrong, record why in `.scratch/TB-n/deviations.md`; a changed
   seam or scope goes back to the orchestrator.
5. **Visual change** → snapshots per skill `tb-functional-test`, saved under `.scratch/TB-n/`.
6. **Self-check** the diff against R1–R12 and fix what you find before committing.
7. **Commit**: conventional message with the key, e.g. `feat(layout): split free width by span (TB-12)`,
   plus the attribution trailer. Never push.
8. **Final gate**: with a committed tree, `scripts/pipeline-gate.py TB-n --slice N --final` for
   every slice (it also runs `scripts/check.sh`). Exit 0 for all, then return `issues-done`
   (or `corrections-applied` / `defects-fixed` on a fix cycle).
