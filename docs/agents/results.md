# `### RESULT:` payloads

Every subagent ends its final message with exactly one block below (single source, ADR-0005).
The orchestrator reads it, checks the required fields, and decides the next step. A missing field
means the payload goes back to the agent; a missing block means the run failed (retry once with a
shorter brief, then escalate). Agents never call each other.

## bouncer

```markdown
### RESULT: ticket-approved
- Ticket: TB-n · <title> · <Notion url>
- Type / Area / Lane proposed: <Feature|Bug|…> / <areas> / <standard|express|research|hardware>
- Digest: goal; in / out of scope; Acceptance normalised as testable statements; blockers and their Status
- Minor gaps (to settle in planning): <none | …>
```

```markdown
### RESULT: ticket-bounced
- Ticket: TB-n · <title>
- Missing hard minimums: <each, with the ticket text that shows it>
- To resubmit: <what the human must add>
```

## planner

```markdown
### RESULT: plan-ready
- Plan: .scratch/TB-n/plan.md (mirrored to Notion § Plan)
- Slices: <N> · seams: <module: item, …>
- Acceptance → slice map complete: yes
- Questions for the human: none
- Risks: <…>
```

```markdown
### RESULT: plan-blocked
- Plan: .scratch/TB-n/plan.md (draft)
- Blocking questions: <each, with why the plan cannot choose>
```

`research-done` (Research tickets): `- Doc: docs/research/NNNN-…md` · `- [RESEARCH] comments: <n>` ·
`- Claims without a primary source: none`.

## qa (spec mode)

```markdown
### RESULT: spec-ready
- Slice: N · mode: behavior | text | tooling · manifest .scratch/TB-n/contract/slice-N.json
- Seal: sha256:<first 12>… · cases: <required k, preserve m> · checks: <ids>
- Red evidence: <each required case → RED_BEHAVIOR | RED_MISSING_SYMBOL> (behavior only)
- Acceptance → case map: <item → QA-xx, …>
```

```markdown
### RESULT: spec-blocked
- Slice: N
- Unresolved: <Acceptance item that is not testable, or two items that contradict (quote both)>
- Next: human decides; no implementation
```

## qa (functional mode)

```markdown
### RESULT: qa-functional-results
- Cases: PASS x / FAIL y / BLOCKED z
- Evidence: <command → result | file path> per Acceptance item
- HIL items for the human: <none | list>
- Verdict: APPROVED | REQUIRES FIXES
- Defects: <id, severity, repro, expected vs actual | none>
```

## implementer

```markdown
### RESULT: issues-done
- Branch / HEAD: tb-n-slug @ <sha>
- pipeline-gate --final: exit 0 per slice · .scratch/TB-n/contract/slice-N.final.json
- seen_red / never_red: <ids> / (none)
- Files changed: <paths>
- Deviations from plan: <none | …> (.scratch/TB-n/deviations.md)
- Standards self-check: R1–R12 read against the diff: <clean | fixed: …>
```

```markdown
### RESULT: corrections-applied
- Review cycle: k · findings closed: <BLOCKER/MAJOR ids → commit>
- Delta since verdict: <files, behaviour touched>
- pipeline-gate --final: exit 0 · HEAD <sha>
```

`defects-fixed` has the same shape with `- QA defects closed:` instead of findings.

```markdown
### RESULT: contract-stuck
- Slice: N · gate runs: 5/5 non-green
- Still red: <ids and statuses>
- Tried: <summary> · Suspect: <the code | the contract (why)>
- Next: orchestrator (re-spec with QA, more budget, or human). No review.
```

```markdown
### RESULT: spec-missing
- Slice: N · reason: no sealed manifest | SEAL_BROKEN | missing .ack
- Next: orchestrator runs QA spec (or intake)
```

## reviewer (Codex, via `scripts/codex-review.sh`)

Not a subagent payload: the script's exit code (0 approved, 2 changes requested, other = did not
run) and `.scratch/TB-n/review-k.md` in the shape of `docs/agents/review-rubric.md`.
