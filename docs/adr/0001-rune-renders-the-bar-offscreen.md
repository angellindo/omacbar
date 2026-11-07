# Rune renders the bar offscreen; the same view drives the configurator preview

The bar is not a window, so it cannot be a Rune app. We render each layer with Rune's headless
`Gpu` + `Renderer` + `Offscreen` at scale 1 (2170 x 60) and read back RGBA, then hand that frame to
a display backend. The configurator is a normal Rune window that reuses the same `Scene` (and, at
preview scale, the same placements), so what you edit is what the bar shows. Rejected: cairo
(tiny-dfr's approach), because it would mean two renderers and no Rune headless test tooling;
direct DRM rendering from wgpu, because the bar is not a KMS output a compositor exposes.

Consequence: the daemon needs a Vulkan adapter (Intel `vulkan-intel` here, lavapipe in CI). We
depend on `rune::render::{Gpu, Renderer, Offscreen}` and `Runtime.text.atlas/instances`, which are
public but not documented as stable: rune bumps are their own tickets.
