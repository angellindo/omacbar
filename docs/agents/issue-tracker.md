# Issue tracker: Notion

Tickets for this repo live in the Notion database **Touchbar Customizer — Tickets**.

- Board: https://app.notion.com/p/411934e84f7f80db976df4729663cf51
- Data source (use with the Notion MCP tools): `collection://411934e8-4f7f-8043-a1ed-000b3b1404ce`
- Parent page: "Touchbar customize" (`411934e84f7f80a5aff9e9ac2678c870`)
- Ticket key: the `ID` property, shown as `TB-<n>`. Refer to tickets by key everywhere (branches,
  commits, `.scratch/` folders).

The Notion MCP server (`claude.ai Notion`) must be connected. Always `notion-fetch` the data source
before writing, and use the exact property names below.

## Properties

| Property | Type | Values / use |
|---|---|---|
| `Name` | title | Imperative, domain words: "Layout engine: split free width by span" |
| `ID` | unique id | `TB-<n>`, automatic |
| `Status` | status | See `workflow.md`. Default `Backlog` |
| `Type` | select | Research, Spike, Feature, Bug, Chore, Docs, Epic |
| `Priority` | select | P0 (blocks the POC) .. P3 |
| `Area` | multi-select | hardware, daemon, render, layout-engine, configurator, theme, input, testing, tooling, agents, docs |
| `Milestone` | select | M0 Setup, M1 Research, M2 Render POC, M3 Hardware POC, M4 MVP |
| `Stage owner` | select | Who holds the ticket now: Orchestrator, Intake · Sonnet low, Planner · Opus high, QA · Opus medium, Implementer · Opus medium, Reviewer · Codex gpt-6.1-sol high, Human |
| `Blocked by` / `Blocks` | relation (self, two-way) | Blocking edges. A ticket is ready only when every `Blocked by` ticket is Done |
| `Review verdict` | select | Not reviewed, Approved, Changes requested |
| `Review cycles` | number | Incremented on each Changes requested; at 3 the ticket goes to Human |
| `Estimate` | select | S (< half a session), M (one session), L (split it) |
| `Branch` | text | `tb-<n>-<slug>` |
| `PR` | url | When a remote exists |
| `Repo refs` | text | Paths the ticket touches or produced (docs, crates) |
| `Order` | number | Execution order (gaps of 10). Agents take the lowest open, unblocked, agent-runnable one; humans the lowest human-owned one. View: **Execution order** |

## Page body template

```
## Context        why this ticket exists, links to research / ADRs
## Goal           one sentence, verifiable
## Acceptance     checklist of observable outcomes (tests, snapshots, HIL steps)
## Out of scope
## Plan           written by the planner (mirrored in .scratch/TB-<n>/plan.md)
## Notes
```

Conversation history goes in **comments**, never edits to earlier sections. Each comment starts
with its stage tag (see `workflow.md` § Comment templates).

## When a skill says "publish to the issue tracker"

Create a page in the data source with `Name`, `Type`, `Priority`, `Area`, `Milestone`,
`Status = Backlog` (or `Ready for Planning` when it came from `/to-tickets` and is fully specified),
and the body template. Add `Blocked by` relations for every blocking edge.

## When a skill says "fetch the relevant ticket"

Query the data source by `ID` (`SELECT url FROM "collection://…" WHERE "userDefined:ID" = <n>`),
then `notion-fetch` the page with `include_discussions: true` to read comments.

## Local mirror

`.scratch/TB-<n>/` holds `ticket.md`, `index.md` and `.ack` (orchestrator), `plan.md` (planner),
`contract/` (QA's sealed manifest and the gate's evidence, `docs/agents/contract.md`),
`review-<cycle>.md` (Codex) and any evidence (snapshots). Notion is the source of truth for status; the mirror exists because Codex
reads the repo, not Notion.

## Wayfinding operations

Map: a ticket of `Type = Epic`; child decision tickets link to it in `Repo refs` and use
`Blocked by` for order. Claim = set `Status = Planning`; resolve = answer in a comment, set `Done`.
