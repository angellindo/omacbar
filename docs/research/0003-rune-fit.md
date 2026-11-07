# 0003 · Rune as the UI framework

Status: resolved (TB-3) · pinned rev `f73f72cdc563534e4f7de9f5a79bed007ac42384` (dev, 2026-11-06)

## Question

Can Rune draw the bar (a non-window surface) and the configurator, and can agents test both?

## Findings

- Rune is a wgpu + winit Rust UI framework (views are plain functions `fn(&S, &mut Cx<S>) -> El`,
  Taffy layout, own text stack). Pre-release: **always pin a `dev` rev**.
- **Offscreen rendering works and is public API**: `rune::render::{Gpu::headless, Renderer,
  Offscreen}` + `Runtime::new(state, view, size, Env)` → `Offscreen::read()` gives RGBA8. This
  is the bar path: render a 2170 x 60 scene at scale 1, convert, push to the device.
  Verified in this repo: `crates/tb-render/src/offscreen.rs`, test
  `the_offscreen_frame_is_the_panel_size_and_paints_theme_colours` (Intel Vulkan, ~60 ms GPU init,
  ~16 ms first frame in debug).
- **Headless agent tooling is built in**: any `rune::run` binary accepts `--snap out.png
  [--scene NAME]`, `--tree tree.json`, `--script events.json` (taps, drags, keys on a virtual
  clock), `--video`. Unit tests drive `Runtime` directly (`pointer_down/up`, `find`, `presented`,
  `frame(dt)`).
- Linux needs `libxkbcommon` + `wayland` (installed) and a Vulkan driver (`vulkan-intel`
  installed; lavapipe for CI). Headless `--snap` needs no display.
- Features: we build with `default-features = false, features = ["embedded-fonts"]` (no D-Bus,
  no accessibility bridge) to keep the daemon light. Revisit if the configurator needs AT-SPI.
- Gaps found: `TextStyle::mono()` falls back to the sans face (no mono font embedded with our
  feature set). Load the Omarchy font (JetBrainsMono Nerd Font) explicitly (TB-14).

## Sources

- [anomalyco/rune](https://github.com/anomalyco/rune) `README.md`, `AGENTS.md`, `crates/rune/README.md` (§ Agent tooling, § Testing stories, § Linux)
- `crates/rune/src/runtime/pixel_snap_tests.rs` (offscreen recipe)
