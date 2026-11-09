# Coding standards

What reviewers (human, Claude or Codex) flag. Mechanical rules are enforced by `scripts/check.sh`;
the rest is judgement, and a review cites the rule number.

## Gates (must pass before review)

`scripts/check.sh` = `cargo fmt --check` + `cargo clippy --workspace --all-targets -D warnings` +
`cargo test --workspace` + file-size check. Toolchain pinned to Rust 1.99.1 (`rust-toolchain.toml`,
`mise.toml`). Lints live in the root `Cargo.toml` `[workspace.lints]`; crates opt in with
`[lints] workspace = true`.

## Rules

**R1. Crate boundaries are seams.**
- `tb-core`: pure domain logic. No IO, no GPU, no Rune, no clocks, no env. Takes text, returns values.
- `tb-render`: Rune views and offscreen rendering. No device or input access.
- `tb-app`: the configurator binary.
- `tb-config` (ADR-0007, ADR-0008): the files the bar follows on disk: the config file (load,
  atomic save, change watch) and the active Omarchy theme (load with fallback, switch watch). The
  only crate that reads or writes them; binaries use it.
- `tb-daemon` (binary `touchbard`, ADR-0006): the only crate that touches devices (DRM, evdev, uinput),
  each behind a trait with a fake for tests.
A new dependency arrow between crates needs an ADR.

**R2. Domain language.** Names come from `GLOSSARY.md` (Layer, Chord, Group, Span, Scene, Frame,
Present ...). New concept → add it to the glossary in the same change.

**R3. Errors.** Libraries return `Result<_, ThisError>` with typed variants (`thiserror`); binaries may
use `anyhow`-style context at the edge. No `unwrap`/`expect`/`panic!` outside tests (clippy
denies `unwrap_used` and `panic`; `expect` only with a message stating the invariant). Validation
collects all problems (`Vec<Issue>`), not just the first.

**R4. No silent fallbacks.** Falling back (theme, layer, font) is allowed only when the fallback is
documented on the function and tested.

**R5. Integers for pixels.** Geometry is `u16`/`u32` device pixels. Float only at the Rune boundary.
Saturating or checked arithmetic where overflow is possible; no `as` casts that can truncate
(use `try_from`).

**R6. Serde shapes are contracts.** Config types use `deny_unknown_fields` and kebab-case. Any change
to the config shape bumps `CONFIG_VERSION` or stays backward compatible, with a round-trip test.

**R7. Unsafe is denied** workspace-wide. A device crate that needs it gets a scoped `#[allow]`, a
`// SAFETY:` comment per block and an ADR.

**R8. Rune usage.** Pin a `dev` rev in the root `Cargo.toml`; bump it in its own ticket. Views are
pure functions of state. Key every element a test or script needs (`.key("f1")`). Do not reach
into Rune internals beyond `rune::render::{Gpu, Renderer, Offscreen}`.

**R9. Size.** Files under 500 non-test lines (hard cap 800); functions under ~60 lines. Tests
live in a sibling `tests.rs` (`mod tests;`) once a module's tests pass ~40 lines.

**R10. Comments** explain why, not what. Every public item in `tb-core` has a one-line doc.

**R11. Logging and output.** No `println!` in libraries (clippy warns). Binaries log through one
module.

**R12. Config and environment** are read only at a binary's startup (clippy disallows
`std::env::var` elsewhere). Clocks are injected (`SystemTime::now` is disallowed).

## Commits

Conventional commits with the ticket: `feat(layout): split free space by span (TB-12)`.
One ticket per branch: `tb-<n>-<slug>`. End with the agent attribution trailer when an agent wrote it.
