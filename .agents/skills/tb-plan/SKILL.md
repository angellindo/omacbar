---
name: tb-plan
description: "Plan one Touch Bar Customizer ticket (planner stage, Opus high): read the ticket, glossary, ADRs and standards, choose seams and tests, write .scratch/TB-<n>/plan.md and the Notion § Plan. Use when a ticket is Ready for Planning."
---

# Plan a ticket

Input: `.scratch/TB-<n>/ticket.md`. Output: `.scratch/TB-<n>/plan.md`, the same text in the
ticket's `## Plan` section, and a `[PLAN]` comment. You write no product code.

1. **Read** the ticket, `GLOSSARY.md`, `docs/scope.md`, ADRs in the area, `docs/standards/*.md`,
   and `docs/design/button-standard.md` if anything visible changes. Read the code the ticket
   touches. Done when you can name every module the change crosses.
2. **Decide the type.** Research ticket → follow skill `tb-research` instead and stop here.
   Spike → the plan is a question, an experiment and a stop condition.
3. **Shape the change** with skill `codebase-design` (deep modules, seams). Prefer extending the
   existing crates per R1; a new crate or dependency arrow needs an ADR (skill `domain-modeling`).
4. **Pick seams and tests** with skill `tdd`: list each seam, the first failing test at it (name
   in domain words), and what it catches. Functional tests per `docs/standards/testing.md`.
5. **Write `plan.md`** in this shape:

   ```
   # TB-<n> plan
   Goal: <one sentence>
   Seams: <module: public function/type>, ...
   Slices (in order, each one red→green):
   1. <test name> → <minimal change> (files)
   Functional check: <Rune headless test / script / snapshot>
   Acceptance mapping: <each Acceptance item → test or evidence>
   Risks & unknowns: <...>
   Out of scope: <...>
   Questions for the human: <none | ...>
   ```
   Done when every Acceptance item maps to a test or evidence and every slice is small enough for
   one red→green cycle.
6. **Publish** with skill `tb-notion`: `## Plan` section, `[PLAN]` comment, Status `Plan Review`,
   Stage owner `Human`. If questions block the plan, Status `Needs Info` instead.
