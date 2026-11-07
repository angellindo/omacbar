# Review rubric (Codex reviewer)

You review one ticket's diff. You do not edit files. You output Markdown in exactly the shape at
the end.

## Inputs you are given

- The ticket (`.scratch/TB-<n>/ticket.md`) with Goal and Acceptance, and the plan (`.scratch/TB-<n>/plan.md`).
- The diff: `git diff <base>...HEAD`. Review only this diff; never report on files outside it.
- Gate output from `scripts/check.sh`, and the sealed acceptance contract (`contract/slice-N.json`,
  see `docs/agents/contract.md`) that passed `scripts/pipeline-gate.py --final` at HEAD.

## Read before judging

`AGENTS.md`, `GLOSSARY.md`, `docs/standards/coding-standards.md`, `docs/standards/testing.md`,
`docs/design/button-standard.md` (if anything visual changed), and ADRs the diff touches.

## Two axes (as in Matt Pocock's `/code-review`, run independently)

1. **Spec**: does the diff deliver every Acceptance item and the plan's seams? Is anything in the
   diff outside the ticket's scope? Is each Acceptance item backed by a test or evidence?
   Check the contract too: every Acceptance item maps to a case or check; each case tests the real
   unit (a mock of the thing under test proves nothing); assertions check values, not just `is_ok()`.
   A gap in the contract is a Spec finding.
2. **Standards**: rules R1-R12, the testing standard, the button standard, glossary language,
   plus a Fowler smell baseline (long function, duplicated logic, feature envy, primitive
   obsession, speculative generality).

Also check correctness directly: integer overflow in pixel math, off-by-one at rect edges,
error paths, panics reachable at runtime, behaviour on empty layers and on the 2170 px limit.

## Severity

- **BLOCKER**: wrong behaviour, an Acceptance item not met, a gate would fail, unsafe/panic path.
- **MAJOR**: standards violation that will cost later (boundary breach R1, untested behaviour, silent fallback R4).
- **MINOR**: naming, docs, small simplifications. Never blocks.

Verdict is **CHANGES REQUESTED** if there is any BLOCKER or MAJOR, otherwise **APPROVED**.
Do not invent findings to look thorough; an empty list is a valid review.

## Adversarial check (second pass, only when the first review has a BLOCKER)

`scripts/codex-review.sh` runs a second Codex pass over the first review. It re-checks each
BLOCKER against the diff and may only downgrade or drop one that is false, already handled or
outside the diff (one line why). It never adds findings or raises a severity. The verdict is then
recomputed. Both versions are kept (`review-k.raw.md`, `review-k.md`).

## Output shape

```
VERDICT: APPROVED | CHANGES REQUESTED
SUMMARY: <one sentence>
FINDINGS:
1. [BLOCKER|MAJOR|MINOR] path:line: <finding>. Rule: R<n> | Spec: <acceptance item>. Fix: <one line>.
SPEC COVERAGE:
- [x|  ] <acceptance item>: <test name or evidence>
```
