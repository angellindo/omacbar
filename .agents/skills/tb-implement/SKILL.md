---
name: tb-implement
description: "Implement or fix one planned Touch Bar Customizer ticket (implementer stage, Opus medium): branch, red-green slices from the plan, gates, evidence, commit. Use when a ticket is Ready for Dev or Changes Requested."
---

# Implement a ticket

Input: `.scratch/TB-<n>/ticket.md`, `.scratch/TB-<n>/plan.md`, and for a fix the latest
`.scratch/TB-<n>/review-<k>.md`. Output: commits on `tb-<n>-<slug>`, green gates, an `[IMPL]` comment.

1. **Branch**: `git switch -c tb-<n>-<slug>` from `main` (or stay on it for a fix cycle).
2. **Read** the plan, `docs/standards/coding-standards.md`, `docs/standards/testing.md`,
   `GLOSSARY.md`. For a fix, list every BLOCKER and MAJOR finding; each becomes a slice.
3. **Slices**: for each plan slice in order, run skill `tdd`: write the failing test, run it and
   see it fail for the right reason, write the minimal code, see it pass. Run
   `cargo test -p <crate>` per slice. Done when every slice's test exists and passes.
4. **Deviations**: if the plan is wrong, stop and record why in `.scratch/TB-<n>/deviations.md`;
   small fixes continue, a changed seam or scope goes back to the orchestrator.
5. **Gates**: `scripts/check.sh` exits 0. Visual change → snapshots per skill `tb-functional-test`,
   saved under `.scratch/TB-<n>/`.
6. **Commit**: conventional message with the key, e.g. `feat(layout): split free width by span (TB-12)`,
   plus the attribution trailer. Never push or open PRs unless the human asked.
7. **Return** to the orchestrator: branch, sha, gate summary, Acceptance status, evidence paths,
   deviations. The orchestrator posts `[IMPL]` and runs the review.
