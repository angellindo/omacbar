# We write our own daemon instead of generating tiny-dfr config

tiny-dfr already renders a Fn-switched layer bar, but it has two fixed layers, equal-width
buttons only (plus `Stretch`), no per-button colours, no modifier layers, cairo rendering and
finds Fn on a "Touch Bar" device (on T1 Fn is on the SPI keyboard). Patching it would fork an
upstream C-style Rust codebase we do not control. Our daemon reuses its proven patterns
(DRM dumb buffer, libinput/evdev touch, uinput keys, privilege drop) as reference, not code.
tiny-dfr remains the fallback to verify a hardware setup works before ours runs.
