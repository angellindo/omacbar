# Touch Bar Customizer

A Rust + [Rune](https://github.com/anomalyco/rune) app that lets the user arrange, size and theme
the Touch Bar of a T1 MacBook Pro (`MacBookPro14,3`) on Omarchy: the F-keys by default, other
buttons per Fn + modifier layer. This is a POC/MVP to prove viability; scope is `docs/scope.md`.

## How to think here

- **Facts before code.** Hardware facts come from `docs/research/` (cite them); nothing about the
  Touch Bar is assumed. Unknown → a Research ticket, not a guess.
- **Smallest change that meets the ticket's Acceptance.** No unrequested features, flags or abstractions.
- **Words from `GLOSSARY.md`** in code, tests, tickets and commits.
- **Hardware is the human's.** Agents never load kernel modules, edit udev/systemd/ESP, or write to
  `/dev` on this machine. The simulator and headless Rune tests are the agent's world.

## Layout

| Path | What |
|---|---|
| `crates/tb-core` | Pure domain: model, chords, spacing rules, layout engine, hit-test, reorder, theme, config |
| `crates/tb-render` | Rune view of a bar scene + offscreen render to RGBA frames |
| `crates/tb-app` | `touchbar-customizer` configurator window (Rune) |
| `contrib/` | Files a human installs outside the repo (the Omarchy `theme-set.d` hook) |
| `fixtures/` | Test data shared by crates (Omarchy `colors.toml` per theme) |
| `docs/research/` | Cited findings (hardware, input, Rune, testing, Omarchy) |
| `docs/design/` | Button standard and theming |
| `docs/standards/` | Coding (R1-R12) and testing standards |
| `docs/adr/` | Decisions |
| `docs/agents/` | Tracker contract, workflow states, payloads, acceptance contract, review rubric |
| `.claude/agents/`, `.claude/skills/` | Agents (`orchestrator`, `bouncer`, `planner`, `qa`, `implementer`) and skills (`tb-*` + vendored Matt Pocock); mirrored to `.agents/` |

## Commands

- Toolchain: Rust 1.99.1 via `mise` (`mise install`), pinned in `rust-toolchain.toml`.
- All gates: `scripts/check.sh` (fmt, clippy `-D warnings`, tests, file size). Must exit 0 before review.
- Fast loop: `cargo test -p tb-core`.
- Look at it: `TB_WRITE_SNAPSHOTS=1 cargo test -p tb-render --test functional` → `out/default-layer.png`;
  `HOME=$(mktemp -d) cargo run -p tb-app -- --snap out/app.png [--scene <name>]` (startup writes a
  default config into `HOME`); `cargo run -p tb-app` for the window.
- Contract gate: `scripts/pipeline-gate.py TB-<n> --slice N [--stamp|--final]`.
- Shared laptop (another heavy job running): prefix heavy runs with
  `nice -n 19 env CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2`, one at a time; pause above 85 °C (`sensors`).
- Review stage: `scripts/codex-review.sh TB-<n> [base]`.
- Remote: `origin` = `git@github.com:angellindo/omacbar.git`, **public**. Only the orchestrator
  pushes (`main` after a merge, ticket branches as backup); never force-push. Nothing private goes
  in a commit: no serials, ECID, firmware tickets, tokens, or files copied from `~/Documents/mac`.

## Ticket pipeline

Every change belongs to a Notion ticket `TB-<n>` and runs `/tb-ticket TB-<n>` (or `/tb-ticket next`,
which picks the lowest open `Order` an agent can run), which makes the
main thread the **orchestrator** (`.claude/agents/orchestrator.md`):
intake (`bouncer`, Sonnet low) → plan (`planner`, Opus high) → sealed acceptance contract (`qa`,
Opus medium) → implement (`implementer`, Opus medium) until `scripts/pipeline-gate.py --final`
passes → review (Codex `gpt-6.1-sol`, high, `scripts/codex-review.sh`) → functional QA (`qa`) →
local fast-forward merge → Done. No human approvals in the normal flow; a person is involved only
by exception (HIL, unanswerable questions, conflicting Acceptance, exhausted budgets).
Agents never call each other; they return `### RESULT:` payloads (`docs/agents/results.md`).
States and comments: `docs/agents/workflow.md`. Contract: `docs/agents/contract.md`.
Why: ADR-0004 (fixed models), ADR-0005 (orchestrator, low HITL).

## Per tool

The instructions above are the same for every agent. Only how each role is launched differs.

- **Claude Code** orchestrates, plans, writes the contract and implements. The orchestrator runs
  in the main thread (`/tb-ticket` or `claude --agent orchestrator`); `bouncer`, `planner`, `qa`
  and `implementer` are dispatched by it with a fresh context, never inline.
  Notion is reached through the `claude.ai Notion` MCP server; if it is not connected, stop and
  tell the user instead of tracking work elsewhere.
- **Codex** reviews, and only reviews (ADR-0004). `scripts/codex-review.sh` runs it with
  `gpt-6.1-sol`, effort high, in a read-only sandbox; it reads `.scratch/TB-<n>/`, not Notion.
  A Codex session never plans or implements a ticket, even though it can see the agent files.
- **Permissions are per tool and not mirrored.** `.claude/settings.json` binds Claude Code only;
  Codex is bounded by the read-only sandbox set in `codex-review.sh`.

## Mirrors

`CLAUDE.md` = `AGENTS.md`, `.claude/agents/` = `.agents/agents/`, `.claude/skills/` =
`.agents/skills/`, byte for byte. Edit the `CLAUDE.md` / `.claude/` side, then run
`scripts/sync-agents.sh`. `scripts/check.sh` fails when they drift.

## Agent skills

### Issue tracker

Notion database "Touchbar Customizer — Tickets", keys `TB-<n>`. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical roles map to Notion `Status` values. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` + `docs/adr/` at the root. See `docs/agents/domain.md`.

### Which skill when

- New idea or feature → `/grill-with-docs`, then `/to-spec`, `/to-tickets` (publishes to Notion).
- A ticket → `/tb-ticket TB-<n>` (dispatches `tb-plan`, `tb-implement`, `tb-review`, `tb-functional-test`).
- Anything a user sees on the bar → skill `tb-button-design` first.
- A hard bug → `/diagnosing-bugs`. After a session that went sideways → `/retro`.
- Not sure → `/ask-matt` (router over the vendored skills).
