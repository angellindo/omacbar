# An orchestrator and a sealed acceptance contract replace human approvals

The ticket pipeline is run by an **orchestrator** agent in the main thread
(`.claude/agents/orchestrator.md`, entered with `/tb-ticket TB-n`), adapted from hismed-central's
`orquestador`. Stage agents (`bouncer`, `planner`, `qa`, `implementer`) never call each other: each
returns a `### RESULT:` payload (`docs/agents/results.md`) and the orchestrator decides the next
step, owns every Notion move, counts cycles and keeps `.scratch/TB-n/index.md` and `.session-id`
for recovery.

This project runs with **little human-in-the-loop**, so there is no start confirmation, no plan
approval and no merge confirmation (hismed has all three). Their job moves to automated gates:

- **Intake** (`bouncer`, Sonnet low) bounces tickets missing hard minimums; its approval is the
  `.ack` record, without which `scripts/pipeline-gate.py` will not run.
- **Sealed acceptance contract** (`qa`, Opus medium; `docs/agents/contract.md`): QA writes the
  acceptance tests before any code, seals them (SHA-256), and must show each `required` case
  failing for a valid reason. The implementer cannot edit them; `--final` refuses a broken seal or
  a case never seen red. This is what stops "tests written to pass".
- **Codex review** requires a `--final` record for HEAD and, when it raises a BLOCKER, runs an
  adversarial second pass that may only downgrade.
- **Functional QA** (`qa`) then a local fast-forward merge; never a push.

A person is involved only by exception: physical-bar (HIL) checks, questions the ticket cannot
answer, conflicting Acceptance items (a product decision the pipeline must not take), or an
exhausted budget (5 gate runs per slice, 3 review or 3 QA cycles).

## Considered options

- **Keep hismed's human gates** (start «Confirmo», plan approval, merge confirmation). Rejected for
  this POC: the human is a single developer who wants tickets to run unattended, and the gates
  above catch what those approvals were for.
- **No contract, trust the implementer's TDD.** Rejected: the author choosing their own tests is
  the failure mode hismed's gate was built to stop.

## Consequences

ADR-0004's routing stands (plan Opus high, implement Opus medium, review Codex `gpt-6.1-sol`
high) and gains three fixed roles: orchestrator Opus high, intake Sonnet low, QA Opus medium.
Notion gains the `QA Spec` status; `Plan Review` is retired.
