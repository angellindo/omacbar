# Ticket workflow

Every ticket moves through the same states. Agents move tickets only along these arrows and leave
a comment at every move.

```
Needs Triage ──► Needs Info ──┐
     │                        ▼
Backlog ──► Ready for Planning ──► Planning ──► Plan Review ──► Ready for Dev ──► In Progress
                  ▲                    │              │                               │
                  └──── (plan rejected)┘◄─────────────┘                               ▼
                                                                    ┌──────────── In Review
                                                                    ▼                 │ approved
                                                           Changes Requested          ▼
                                                                    │           Functional QA ──► Done
                                                                    └──► In Progress   │ fails
                                                                                       ▼
                                                                               Changes Requested
Any state ──► Blocked (with a comment naming the blocker) ──► back to where it was
Any state ──► Won't Do (with a reason)
Ready for Human: work only a person can do (hardware, credentials, firmware backup)
```

## States

| Status | Meaning | Who moves it out | Stage owner |
|---|---|---|---|
| Needs Triage | Arrived raw (bug report, idea) | `/triage` | Human |
| Needs Info | Waiting on the reporter | Human | Human |
| Backlog | Accepted, not yet specified enough to plan | Human / `/to-tickets` | Human |
| Ready for Planning | Has Context, Goal, Acceptance; blockers Done or planned | Planner picks it up | Planner · Opus high |
| Planning | Planner is reading and writing the plan | Planner | Planner · Opus high |
| Plan Review | Plan posted; human approves or comments | Human | Human |
| Ready for Dev | Plan approved, blockers Done | Implementer picks it up | Implementer · Opus medium |
| Ready for Human | Only a person can do it | Human | Human |
| In Progress | Implementer is coding (TDD), gates running | Implementer | Implementer · Opus medium |
| In Review | Gates green; Codex reviewing the diff vs plan and standards | Reviewer | Reviewer · Codex gpt-6.1-sol high |
| Changes Requested | Review or QA found problems; `Review cycles` +1 | Implementer | Implementer · Opus medium |
| Functional QA | Approved; functional/HIL checks from Acceptance run | Implementer (headless) or Human (HIL) | Implementer / Human |
| Blocked | Cannot proceed; comment names the blocker ticket or fact | Whoever unblocks | unchanged |
| Done | All Acceptance boxes checked with evidence | — | — |
| Won't Do | Rejected or superseded, with reason | — | — |

Research tickets skip Plan Review → Ready for Dev: Planning → (research written) → In Review
(Codex checks sources and claims) → Done.

## Model routing (fixed, see ADR-0004)

| Stage | Agent | Model | Effort |
|---|---|---|---|
| Plan | `.claude/agents/planner.md` | Claude Opus | high |
| Implement | `.claude/agents/implementer.md` | Claude Opus | medium |
| Review | `scripts/codex-review.sh` (skill `tb-review`) | Codex `gpt-6.1-sol` | high |

Three Changes Requested cycles on one ticket → `Stage owner = Human`, status `Blocked`, comment
summarising the disagreement.

## Comment templates

Every comment starts with a tag line so the history reads as a log.

```
[PLAN] TB-<n> · planner (Opus high)
Plan posted in the page § Plan and .scratch/TB-<n>/plan.md.
Seams: <modules>. Tests first: <names>. Risks: <...>. Questions for the human: <...|none>.
```
```
[PLAN-APPROVED] TB-<n> · <human>       (or [PLAN-CHANGES] with the requested changes)
```
```
[IMPL] TB-<n> · implementer (Opus medium) · cycle <k>
Branch tb-<n>-<slug> @ <sha>. Gates: fmt ✓ clippy ✓ tests <passed>/<total> ✓.
Acceptance: [x] ... [ ] ... Evidence: <snapshot paths>. Deviations from plan: <none|...>.
```
```
[REVIEW] TB-<n> · Codex gpt-6.1-sol high · cycle <k> · APPROVED | CHANGES REQUESTED
Blocking: <n> · Non-blocking: <n>. Full report: .scratch/TB-<n>/review-<k>.md
1. [BLOCKER|MAJOR|MINOR] <file:line> <finding> (rule R<n> | spec: <acceptance item>)
```
```
[QA] TB-<n> · <who> · PASS | FAIL
<each acceptance item with evidence>
```
```
[RESEARCH] TB-<n> · <finding in one line>
Source: <url or live command>. Confidence: high | medium | low. Doc: docs/research/<file>.md
```
```
[BLOCKED] TB-<n> · blocked by TB-<m> | <fact>. Unblocks when: <condition>.
```
