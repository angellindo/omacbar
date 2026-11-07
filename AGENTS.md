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
| `docs/research/` | Cited findings (hardware, input, Rune, testing, Omarchy) |
| `docs/design/` | Button standard and theming |
| `docs/standards/` | Coding (R1-R12) and testing standards |
| `docs/adr/` | Decisions |
| `docs/agents/` | Tracker contract, workflow states, review rubric |
| `.claude/skills/` | Project skills (`tb-*`) + vendored Matt Pocock skills; `.agents/skills` links here for Codex |

## Commands

- Toolchain: Rust 1.99.1 via `mise` (`mise install`), pinned in `rust-toolchain.toml`.
- All gates: `scripts/check.sh` (fmt, clippy `-D warnings`, tests, file size). Must exit 0 before review.
- Fast loop: `cargo test -p tb-core`.
- Look at it: `TB_WRITE_SNAPSHOTS=1 cargo test -p tb-render --test functional` → `out/default-layer.png`;
  `cargo run -p tb-app -- --snap out/app.png [--scene fn|fn-ctrl]`; `cargo run -p tb-app` for the window.
- Review stage: `scripts/codex-review.sh TB-<n> [base]`.

## Ticket pipeline

Every change belongs to a Notion ticket `TB-<n>` and runs `/tb-ticket TB-<n>`:
plan (agent `planner`, Opus high) → human approves plan → implement (agent `implementer`,
Opus medium) → review (Codex `gpt-6.1-sol`, high, `scripts/codex-review.sh`) → functional QA → Done.
States, arrows and comment templates: `docs/agents/workflow.md`. Why fixed models: ADR-0004.

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
