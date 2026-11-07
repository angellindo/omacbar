---
name: tb-functional-test
description: "Functionally test Touch Bar Customizer behaviour headless with Rune (Runtime, presented/find, taps, scripts, snapshots), or run the hardware checklist. Use for QA of a ticket and when writing tests that go beyond pure tb-core logic."
---

# Functional testing

Rules live in `docs/standards/testing.md`; research behind them in `docs/research/0004-functional-testing.md`.

## Headless (agents run these)

- **Runtime test** (preferred): build `rune::Runtime::new(state, view, size, Env::new(true, 1.0))`,
  `rt.frame(0.0)`, then assert with `rt.presented("<key>")` (rect, opacity), `rt.find("<key>")`
  (centre point; skips exiting elements), drive with `rt.pointer_down/up(x, y)` and advance with
  `rt.frame(1.0 / 60.0)`. Examples: `crates/tb-render/tests/functional.rs`, `crates/tb-app/src/app.rs` tests.
- **Pixels**: `tb_render::render_frame(scene)` → sample `frame.pixel(x, y)`, tolerance ±2.
  Needs a Vulkan adapter; CI uses lavapipe (`VK_ICD_FILENAMES=.../lvp_icd.json`).
- **App snapshot**: `cargo run -p tb-app -- --snap out/<name>.png [--scene fn|fn-ctrl] [--size WxH]`;
  `--tree out/<name>.json` dumps keys, rects, text and colours for exact checks.
- **Flows**: a Rune script JSON (`{"t": 0.1, "type": "tap", "key": "tab-fnctrl"}` ...) run with
  `--script <file> --snap out.png --at mark:end`, plus the same events in a Runtime test.

QA output: for each Acceptance item, the command run and its result or the evidence path, in the
`qa-functional-results` payload (`docs/agents/results.md`); the orchestrator posts the `[QA]` comment.
The acceptance contract itself (sealed, red-before-green) is `docs/agents/contract.md`.

## Hardware (human)

Items marked HIL in a ticket's Acceptance are run by the human on the MacBookPro14,3 using
`docs/hil-checklist.md` (once TB-21 creates it). QA lists them in its payload; after the merge the
orchestrator sets Status `Ready for Human`, `Stage owner = Human`.
