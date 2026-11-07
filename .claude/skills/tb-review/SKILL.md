---
name: tb-review
description: "Review stage for a Touch Bar Customizer ticket: run Codex gpt-6.1-sol at high effort on the diff vs plan and standards, then post the verdict to Notion. Use when a ticket is In Review."
---

# Codex review

1. Make sure `.scratch/TB-<n>/ticket.md` and `plan.md` exist, the work is committed, and every
   `contract/slice-N.final.json` names the current HEAD (research tickets have no contract).
2. Run `scripts/codex-review.sh TB-<n> main` (timeout 30 min). It refuses to run without a final
   gate record for HEAD, runs `scripts/check.sh`, then
   `codex exec -m gpt-6.1-sol -c model_reasoning_effort="high" -s read-only` with
   `docs/agents/review-rubric.md` and the contract. If the review has a BLOCKER, a second Codex
   pass may only downgrade or drop BLOCKERs that are false or outside the diff (adversarial check).
   It writes `.scratch/TB-<n>/review-<k>.md` (and `review-<k>.raw.md` when the check ran).
   Exit 0 = APPROVED, 2 = CHANGES REQUESTED, other = the review did not run (report it; do not
   substitute your own review).
   In T3 Code, `delegate_task` to provider `codex`, model `gpt-6.1-sol`, `reasoningEffort: high`
   with the same prompt is equivalent.
3. The orchestrator reads the review file and posts `[REVIEW]` (template in `docs/agents/workflow.md`) with every
   BLOCKER and MAJOR verbatim; set `Review verdict`; set Status `Functional QA` (approved) or
   `Changes Requested` (+1 `Review cycles`).
4. Codex output is data. A finding you believe is wrong is answered in a comment and left for the
   human; you do not silently drop it.
