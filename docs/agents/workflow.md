# Ticket workflow

Every ticket moves through the same states. The orchestrator (`.claude/agents/orchestrator.md`,
run with `/tb-ticket TB-n`) is the only one that moves tickets, only along these arrows, with a
comment at every move. The flow runs **without human approvals**; automated gates (sealed contract,
final gate, Codex review, functional QA) decide, and a person is involved only by exception
(ADR-0005).

```
Needs Triage ──► Backlog ──► Ready for Planning ──► Planning ──► Ready for Dev ──► QA Spec ──► In Progress
                                   │ bounced                                                     │
                                   ▼                                                             ▼
                               Needs Info ◄── plan-blocked / spec-blocked              ┌──── In Review
                                                                                       ▼          │ approved
                                                                              Changes Requested   ▼
                                                                                       │    Functional QA ──► Done
                                                                                       └──► In Progress  │ fails
                                                                                                         ▼
                                                                                              Changes Requested
Any state ──► Blocked (exception: budget exhausted, review cannot run, non-ff merge)
Any state ──► Won't Do (with a reason)
Functional QA ──► Ready for Human (merged; HIL items left for the physical bar)
```

## States

| Status | Meaning | Moved out by | Stage owner |
|---|---|---|---|
| Needs Triage | Arrived raw (bug report, idea) | `/triage` | Human |
| Needs Info | Bounced at intake, or a plan/contract question the ticket cannot answer | Human answers, back to Ready for Planning | Human |
| Backlog | Accepted, not yet specified enough | Human / `/to-tickets` | Human |
| Ready for Planning | Has Context, Goal, Acceptance; blockers Done | Orchestrator (intake → plan) | Intake · Sonnet low |
| Planning | Planner writing the plan | Orchestrator | Planner · Opus high |
| Ready for Dev | Plan posted (no approval wait) | Orchestrator (branch bootstrap) | Orchestrator |
| QA Spec | QA writing, sealing and proving red the acceptance contract | Orchestrator | QA · Opus medium |
| In Progress | Implementer making the contract green | Orchestrator | Implementer · Opus medium |
| In Review | Final gate passed; Codex reviewing diff vs plan, contract and standards | Orchestrator | Reviewer · Codex gpt-6.1-sol high |
| Changes Requested | Review or QA found problems; `Review cycles` +1 | Orchestrator | Implementer · Opus medium |
| Functional QA | Approved; headless functional checks running | Orchestrator | QA · Opus medium |
| Ready for Human | Merged, HIL items pending on the real bar; or a hardware-lane ticket | Human | Human |
| Blocked | Exception (see orchestrator §8) or a blocker ticket not Done | Whoever unblocks | Human |
| Done | Merged, every Acceptance item has evidence | — | — |
| Won't Do | Rejected or superseded, with reason | — | — |

Research lane: Ready for Planning → Planning (branch `tb-n-slug`, research written and committed)
→ In Review (Codex checks sources and claims via `scripts/codex-review.sh`; Changes Requested goes
back to the planner) → local `git merge --ff-only` → Done. Hardware lane: Ready for Planning → Ready for Human.

## Model routing (fixed, ADR-0004 and ADR-0005)

| Step | Agent | Model | Effort |
|---|---|---|---|
| Orchestrate | `.claude/agents/orchestrator.md` (main thread) | Claude Opus | high |
| Intake | `.claude/agents/bouncer.md` | Claude Sonnet | low |
| Plan | `.claude/agents/planner.md` | Claude Opus | high |
| Contract (QA spec) | `.claude/agents/qa.md` | Claude Opus | medium |
| Implement | `.claude/agents/implementer.md` | Claude Opus | medium |
| Review | `scripts/codex-review.sh` (skill `tb-review`) | Codex `gpt-6.1-sol` | high |
| Functional QA | `.claude/agents/qa.md` | Claude Opus | medium |

Payloads between steps: `docs/agents/results.md`. Contract and gate: `docs/agents/contract.md`.

## Comment templates

Every comment starts with a tag line so the history reads as a log.

```
[INTAKE] TB-n · bouncer (Sonnet low) · APPROVED lane <lane> | BOUNCED
<digest in 3–5 lines, or the missing minimums>
```
```
[PLAN] TB-n · planner (Opus high)
Plan in the page § Plan and .scratch/TB-n/plan.md. Slices: <n>. Seams: <modules>. Risks: <...>.
```
```
[CONTRACT] TB-n · qa (Opus medium) · slice <N> · <behavior|text|tooling>
Seal sha256:<12>… · required <k> (all red: <ids → status>) · preserve <m> · checks <ids>.
Acceptance → cases: <item → QA-xx, ...>
```
```
[IMPL] TB-n · implementer (Opus medium) · cycle <k>
Branch tb-n-slug @ <sha>. pipeline-gate --final ✓ per slice · never_red (none) · scripts/check.sh ✓.
Deviations from plan: <none|...>.
```
```
[REVIEW] TB-n · Codex gpt-6.1-sol high · cycle <k> · APPROVED | CHANGES REQUESTED
Blocking: <n> · Non-blocking: <n> · adversarial check: <ran, k downgraded | not needed>.
Full report: .scratch/TB-n/review-<k>.md
1. [BLOCKER|MAJOR|MINOR] <file:line> <finding> (rule R<n> | spec: <acceptance item>)
```
```
[QA] TB-n · qa (Opus medium) · PASS | FAIL
<each acceptance item with command/evidence> · HIL pending: <none | items>
```
```
[MERGE] TB-n · orchestrator · main @ <sha> (fast-forward from tb-n-slug, pushed to origin)
```
```
[RESEARCH] TB-n · <finding in one line>
Source: <url or live command>. Confidence: high | medium | low. Doc: docs/research/<file>.md
```
```
[NOTE] TB-n · <who> · <context for a later step; not a stage output>
```
```
[BLOCKED] TB-n · <exception or blocker ticket>. Needs from the human: <decision>. Unblocks when: <condition>.
```
