# The daemon is its own crate and presents damage, not whole frames

`tb-daemon` (library `tb_daemon`, binary `touchbard`) is the only crate that will touch devices
(R1). It depends on `tb-core` (layout, hit-test, chords) and `tb-render` (scene, frame,
offscreen render); it does not depend on `rune` directly, and nothing depends on it. It is a
library plus a thin binary so tests reach its seams (`Display`, `TouchSource`, the scene
renderer) without spawning a process.

Every present carries the full frame **and its damage**: the rects, in landscape bar pixels,
whose pixels changed since the last present. A backend copies only the damaged rects; rotation
and pixel-format conversion stay in the backend (ADR-0002). The daemon computes damage by
comparing the new scene with the last presented one: a button whose rect is unchanged but whose
look changed (pressed, label, colour) damages exactly its placement; anything else (another
layer, a new background, moved buttons, the first present) damages the whole bar. No change, no
render and no present.

macOS does the same: TouchBarServer draws with `DFRDisplayDrawRectsWithIOSurfaceAsync` and turns
sparse pixel regions into tiles (research 0006). TB-10 maps damage to the DRM device.

Rendering sits behind a trait in the daemon so the event loop is tested with a CPU fake; the GPU
path (`GpuRenderer`, which keeps one `tb_render::Renderer`, so one GPU device, for the daemon's
life; TB-31) is exercised by functional tests, as in `tb-render`.

## Considered options

- **Whole frames only, damage later.** Rejected: every backend would be written against full
  frames and then changed, and a press would send 2170 x 60 px to change one button.
- **Hand the backend only the damaged sub-images.** Rejected: backends that need a whole image
  (a window, a PNG of the panel) would have to keep and stitch their own copy; with the full
  frame plus rects, each backend picks what it needs.
- **Put the loop in `tb-render` or `tb-app`.** Rejected by R1: device access stays in one crate.
