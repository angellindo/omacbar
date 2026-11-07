---
name: tb-ticket
description: "Run one Touch Bar Customizer ticket through its pipeline stage(s): plan (Opus high), implement (Opus medium), review (Codex gpt-6.1-sol high), QA, with Notion updated at every move. Use as /tb-ticket TB-<n> [plan|implement|review|qa|next]."
disable-model-invocation: true
---

# Run a ticket

You are the **orchestrator**. You move the ticket and dispatch stages; you never plan or write
product code yourself. The stage owners and their models are fixed (ADR-0004):

| Stage | How you run it |
|---|---|
| Plan | Agent tool, `subagent_type: planner` (Opus, high effort), fresh context |
| Implement / fix | Agent tool, `subagent_type: implementer` (Opus, medium effort), fresh context |
| Review | skill `tb-review` (runs `scripts/codex-review.sh`: Codex `gpt-6.1-sol`, high effort) |
| QA | skill `tb-functional-test`; hardware items go to the human |

## Steps

1. **Load the ticket.** Skill `tb-notion` → fetch `TB-<n>` with comments. State its title, Status,
   Type and Blocked-by. Done when you know the current Status and every blocker's Status.
2. **Pick the stage** from the argument, or with `next`/no argument from Status:
   `Ready for Planning` → Plan · `Plan Review` → stop, ask the human to approve (comment
   `[PLAN-APPROVED]` or `[PLAN-CHANGES]`) · `Ready for Dev` or `Changes Requested` → Implement ·
   `In Review` → Review · `Functional QA` → QA · `Ready for Human`, `Blocked`, `Done` → report and stop.
   A blocker that is not Done → set `Blocked`, comment `[BLOCKED]`, stop.
3. **Mirror the ticket**: write `.scratch/TB-<n>/ticket.md` (title, Context, Goal, Acceptance, Out
   of scope, and every comment) so agents and Codex read the same text.
4. **Run the stage** (table above). Brief the subagent with only: the ticket key, the path
   `.scratch/TB-<n>/ticket.md`, the stage, and for fixes the review file path. Its skill holds the
   procedure; do not restate it.
5. **Move the ticket** per `docs/agents/workflow.md`: set `Status` and `Stage owner`, post the
   stage's comment template. After Implement, continue straight to Review. After an APPROVED review,
   continue to QA. After CHANGES REQUESTED, increment `Review cycles`; at 3, set `Blocked`,
   `Stage owner = Human` and stop.
6. **Report** to the user: stage(s) run, new Status, comment posted, files under `.scratch/TB-<n>/`,
   and what the human must do next (if anything).

Stop at every human gate (Plan Review, Ready for Human, HIL items). Never mark Done without every
Acceptance item checked with evidence in a `[QA]` comment.
