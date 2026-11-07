#!/usr/bin/env bash
# Review stage of the ticket pipeline (ADR-0004): Codex gpt-6.1-sol, high effort, read-only sandbox.
# Usage: scripts/codex-review.sh TB-<n> [base-ref]     (base defaults to main)
# Needs: .scratch/TB-<n>/ticket.md and plan.md (written by the planner / tb-ticket skill).
# Writes: .scratch/TB-<n>/review-<cycle>.md and prints its path. Exit 0 = APPROVED, 2 = CHANGES REQUESTED.
set -euo pipefail
cd "$(dirname "$0")/.."

ticket="${1:?usage: scripts/codex-review.sh TB-<n> [base-ref]}"
base="${2:-main}"
dir=".scratch/$ticket"
[[ -f "$dir/ticket.md" && -f "$dir/plan.md" ]] || { echo "missing $dir/ticket.md or $dir/plan.md" >&2; exit 1; }

cycle=1
while [[ -e "$dir/review-$cycle.md" ]]; do cycle=$((cycle + 1)); done
out="$dir/review-$cycle.md"
gates="$dir/gates-$cycle.log"

scripts/check.sh >"$gates" 2>&1 && gate_status="PASSED" || gate_status="FAILED"

model="${TB_REVIEW_MODEL:-gpt-6.1-sol}"
effort="${TB_REVIEW_EFFORT:-high}"

{
  echo "You are the review stage for ticket $ticket in this repository. Follow docs/agents/review-rubric.md exactly."
  echo "Base ref: $base. Inspect the change with: git diff $base...HEAD (and git log $base..HEAD)."
  echo "Uncommitted changes, if any, are part of the change: also inspect git diff and git status."
  echo "Mechanical gates (scripts/check.sh): $gate_status. Full log: $gates"
  echo
  echo "=== TICKET ($dir/ticket.md) ==="; cat "$dir/ticket.md"
  echo
  echo "=== PLAN ($dir/plan.md) ==="; cat "$dir/plan.md"
  echo
  echo "Output only the review in the rubric's output shape."
} | codex exec -m "$model" -c "model_reasoning_effort=\"$effort\"" -s read-only -o "$out" - >/dev/null

echo "$out"
if grep -q '^VERDICT: APPROVED' "$out" && [[ "$gate_status" == "PASSED" ]]; then exit 0; else exit 2; fi
