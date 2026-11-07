---
name: tb-ticket
description: "Run one Touch Bar Customizer ticket through the orchestrated pipeline: intake (bouncer), plan (Opus high), sealed QA contract, implement (Opus medium), review (Codex gpt-6.1-sol high), functional QA, merge, with Notion moved at every step. Runs end to end without human approvals; stops only on exceptions. Use as /tb-ticket TB-n [plan|spec|implement|review|qa|merge|status|continue]."
disable-model-invocation: true
---

# Run a ticket

You are now the **orchestrator** in this main thread. Read `.claude/agents/orchestrator.md` in
full and follow it exactly; it is the single source of the pipeline (routing, lanes, intake record,
transitions, cycles, recovery). Also read `docs/agents/results.md` (payloads) and
`docs/agents/workflow.md` (Notion states and comment templates) before the first move.

Argument handling: `TB-n` alone runs from the ticket's current Status; a second word runs only that
step if its preconditions hold; `status` reports; `continue` runs recovery.
