---
name: tb-plan
description: "Plan one Touch Bar Customizer ticket (planner stage, Opus high): read the ticket, glossary, ADRs and standards, choose seams and slices, write .scratch/TB-n/plan.md, return plan-ready or plan-blocked. Use when the orchestrator dispatches the plan step."
---

# Plan a ticket

Input: `.scratch/TB-n/ticket.md` and the intake brief in `.scratch/TB-n/.ack`. Output:
`.scratch/TB-n/plan.md` and a payload from `docs/agents/results.md` § planner. You write no product
code and no tests (QA writes the acceptance contract from your slices), and nothing in Notion.

1. **Read** the ticket, `GLOSSARY.md`, `docs/scope.md`, ADRs in the area, `docs/standards/*.md`,
   and `docs/design/button-standard.md` if anything visible changes. Read the code the ticket
   touches. Done when you can name every module the change crosses.
2. **Decide the type.** Research or Spike → follow skill `tb-research` instead and stop here.
3. **Shape the change** with skill `codebase-design` (deep modules, seams). Prefer extending the
   existing crates per R1; a new crate or dependency arrow needs an ADR (skill `domain-modeling`).
4. **Slice vertically**: each slice delivers one observable behaviour end to end and maps to
   Acceptance items. Typically 1–3 slices; more than 4 means the ticket should be split.
5. **Pick seams and tests** with skill `tdd`: for each slice, the public item or Rune key where QA
   can observe it, and what a failing test there would catch.
6. **Write `plan.md`**:

   ```
   # TB-n plan
   Goal: <one sentence>
   Lane: <standard | express>
   Seams: <module: public function/type>, ...
   Slices (in order):
   1. <behaviour> · observe at <seam / Rune key> · Acceptance: <items> · files: <...>
   Contract mode per slice: <behavior | text | tooling>
   Functional check: <Rune headless test / script / snapshot>
   Risks & unknowns: <...>
   Out of scope: <...>
   Questions for the human: <none | ...>
   ```
   Done when every Acceptance item maps to a slice and every slice fits one red→green cycle.
7. **Return** `plan-ready`, or `plan-blocked` if a question stops you from choosing.
