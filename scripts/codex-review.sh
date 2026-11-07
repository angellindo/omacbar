#!/usr/bin/env bash
# Review stage of the ticket pipeline (ADR-0004, ADR-0005): Codex gpt-6.1-sol, high effort, read-only sandbox.
# Usage: scripts/codex-review.sh TB-<n> [base-ref]     (base defaults to main)
# Needs: .scratch/TB-<n>/ticket.md, .ack, plan.md (not in the research lane), and for every contract slice a
#        contract/slice-N.final.json with result PASS, head = HEAD and seal = contract/slice-N.seal
#        (research lane has no contract).
# Writes: .scratch/TB-<n>/review-<cycle>.md (+ review-<cycle>.raw.md when the adversarial check ran).
# Exit 0 = APPROVED, 2 = CHANGES REQUESTED, 1 = the review did not run.
set -euo pipefail
cd "$(dirname "$0")/.."

ticket="${1:?usage: scripts/codex-review.sh TB-<n> [base-ref]}"
base="${2:-main}"
dir=".scratch/$ticket"
for f in ticket.md .ack; do
  [[ -f "$dir/$f" ]] || { echo "missing $dir/$f" >&2; exit 1; }
done

head=$(git rev-parse HEAD)
lane=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1])).get("lane",""))' "$dir/.ack")
[[ "$lane" == "research" || -f "$dir/plan.md" ]] || { echo "missing $dir/plan.md" >&2; exit 1; }
shopt -s nullglob
contract=()
for m in "$dir"/contract/slice-*.json; do
  case "$m" in *.evidence.json | *.final.json) continue ;; esac
  contract+=("$m")
done
if (( ${#contract[@]} == 0 )); then
  [[ "$lane" == "research" ]] || { echo "no contract under $dir/contract (only the research lane may skip it)" >&2; exit 1; }
fi
# A final record counts only if it is a PASS for this HEAD under the slice's current seal.
valid_final='
import json, sys
final, seal_file, head = sys.argv[1:]
try:
    seal = open(seal_file).read().strip()
except OSError:
    sys.exit(f"missing {seal_file}: the slice is not sealed")
try:
    rec = json.load(open(final))
except OSError:
    sys.exit(f"missing {final}: run scripts/pipeline-gate.py --final first")
except ValueError:
    sys.exit(f"{final} is not valid JSON: rerun --final")
if rec.get("result") != "PASS":
    sys.exit(f"{final} is not a PASS: rerun --final")
fhead = str(rec.get("head"))
if fhead != head:
    sys.exit(f"{final} is for {fhead[:8]}, HEAD is {head[:8]}: rerun --final")
if rec.get("seal") != seal:
    sys.exit(f"{final} is for another seal than {seal_file}: rerun --final")
'
for m in "${contract[@]}"; do
  python3 -I -c "$valid_final" "${m%.json}.final.json" "${m%.json}.seal" "$head" || exit 1
done

cycle=1
while [[ -e "$dir/review-$cycle.md" ]]; do cycle=$((cycle + 1)); done
out="$dir/review-$cycle.md"
gates="$dir/gates-$cycle.log"

scripts/check.sh >"$gates" 2>&1 && gate_status="PASSED" || gate_status="FAILED"

model="${TB_REVIEW_MODEL:-gpt-6.1-sol}"
effort="${TB_REVIEW_EFFORT:-high}"
codex_run() { codex exec -m "$model" -c "model_reasoning_effort=\"$effort\"" -s read-only -o "$1" - >/dev/null; }

{
  echo "You are the review stage for ticket $ticket in this repository. Follow docs/agents/review-rubric.md exactly."
  echo "Base ref: $base. HEAD: $head. Inspect the change with: git diff $base...HEAD (and git log $base..HEAD)."
  echo "Review only that diff; never report on files outside it."
  echo "Mechanical gates (scripts/check.sh): $gate_status. Full log: $gates"
  echo "Acceptance contract (docs/agents/contract.md): every slice passed scripts/pipeline-gate.py --final at HEAD."
  echo
  echo "=== TICKET ($dir/ticket.md) ==="; cat "$dir/ticket.md"
  echo
  [[ -f "$dir/plan.md" ]] && { echo "=== PLAN ($dir/plan.md) ==="; cat "$dir/plan.md"; }
  for m in "${contract[@]}"; do echo; echo "=== CONTRACT ($m) ==="; cat "$m"; done
  echo
  echo "Output only the review in the rubric's output shape."
} | codex_run "$out"

if grep -qE '^[0-9]+\. \[BLOCKER\]' "$out"; then
  raw="$dir/review-$cycle.raw.md"
  mv "$out" "$raw"
  {
    echo "Adversarial check of a code review for ticket $ticket (docs/agents/review-rubric.md § Adversarial check)."
    echo "Diff: git diff $base...HEAD. For each BLOCKER below, verify it against the diff and the code."
    echo "A BLOCKER that is false, already handled, or outside the diff is downgraded (MAJOR/MINOR) or dropped,"
    echo "with one line saying why. You may NOT add findings or raise any severity. Keep every other finding as is."
    echo "Recompute VERDICT with the rubric's rule. Output the whole corrected review in the same shape."
    echo
    cat "$raw"
  } | codex_run "$out"
fi

echo "$out"
if grep -q '^VERDICT: APPROVED' "$out" && [[ "$gate_status" == "PASSED" ]]; then exit 0; else exit 2; fi
