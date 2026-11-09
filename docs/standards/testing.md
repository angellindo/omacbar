# Testing standard

## Layers

| Layer | Where | Runs | Asserts |
|---|---|---|---|
| Unit | `tb-core` modules (`mod tests;`) | `cargo test -p tb-core`, no GPU | Domain behaviour through the public function: layout rects, chord resolution, validation issues, theme mapping |
| Render (scene) | `tb-render` unit tests | no GPU | Resolved colours/states per button |
| Functional | `crates/*/tests/*.rs`, app `#[cfg(test)]` | Rune `Runtime` headless, virtual clock; pixel tests need a Vulkan adapter | Element tree (`presented`, `find`), taps via `pointer_down/up`, state after N frames; pixels with tolerance |
| Daemon functional | `tb-daemon/tests` (TB-21) | fake `Display` + uinput/evdev fakes | touch x → button → emitted action; Fn+mod → layer |
| Hardware (HIL) | `docs/hil-checklist.md` (TB-21) | manual, on the MacBookPro14,3 | orientation, colours, latency, theme switch, recovery |

## Rules

1. **TDD at the seams** (Matt Pocock `/tdd`): one failing test per behaviour, then the code. Test
   names state behaviour in domain words: `a_span_two_button_is_twice_a_span_one_plus_at_most_one_pixel`.
2. **Test through the public interface.** No testing private helpers; no mocks of our own pure code.
   Fakes only at device boundaries (`Display`, `TouchSource`, `ModifierSource`, `KeyEmitter`).
3. **Element tree before pixels.** Pixel checks compare single sampled pixels with a tolerance
   (±2 per channel); no full-image goldens in `cargo test` (GPU drivers differ).
4. **Snapshots are for humans.** `TB_WRITE_SNAPSHOTS=1 cargo test -p tb-render` writes
   `out/*.png`; `cargo run -p tb-app -- --snap out/app.png [--scene fn-ctrl]` renders the
   configurator. Attach them to the ticket when visuals change.
5. **Scripts for flows.** Multi-step UI flows (drag to reorder) get a Rune `--script` JSON under
   `crates/tb-app/scripts/` and a test that runs the same events through `Runtime`.
6. Every bug fix starts with a test that fails on the bug.
7. Integration test files may `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`.
