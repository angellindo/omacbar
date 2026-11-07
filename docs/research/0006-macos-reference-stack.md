# 0006 · macOS Touch Bar stack and this machine's T1 history (local reference)

Status: resolved (TB-23) · Source: `~/Documents/mac/` on this machine (macOS 13.7 binaries
extracted from this Mac's own disk on 2026-10-14, plus the T1 repair notes of 2026-09).

These binaries are Apple's proprietary code. Use them **only to read facts** (plists, symbol and
string names). Never copy code, data tables or binaries into this repo.

## What is there

| Path under `~/Documents/mac/externos/macos-bins-2026-10-14/` | Role in macOS |
|---|---|
| `System/Library/Extensions/AppleUSBiBridge.kext` (Info.plist) | USB driver + IOKit matching for the T1 iBridge |
| `System/Library/Extensions/AppleEmbeddedOSSupportHost.kext` | ACPI `APP7777` host support (same node Linux `apple-ibridge` binds) |
| `usr/libexec/TouchBarServer` (x86_64 + arm64e Mach-O) | The compositor that owns the bar: renders, presents, brightness, dimming |
| `System/Library/CoreServices/ControlStrip.app` | The system Control Strip and the Escape key UI |
| `System/Library/HIDPlugins/ServiceFilters/DFRTouchServiceFilter.plugin` | Touch filtering for the bar's digitizer |
| `System/Library/PrivateFrameworks/DFRDisplay.framework`, `DFRFoundation.framework` | Display and presentation-mode APIs (code is in the dyld shared cache, not extracted; plists only) |
| `carved/AppleUSBiBridge-*.bin` | 16 KB carved from the kernel cache (partial) |

## Findings

### macOS runs the T1 in USB configuration 2 (primary source)

`AppleUSBiBridge.kext` Info.plist, personality "iBridge1,1 Driver": `idVendor 1452` (0x05ac),
`idProduct 34304` (0x8600), `bcdDevice 257`, **`kUSBPreferredConfiguration = 2`**,
`kUSBPreferredRecoveryConfiguration = 3`. Personality "iBridge1,1 Display IOUSBInterface":
**`bConfigurationValue 2`, `bInterfaceNumber 3`**, guarded by entitlement `com.apple.appledfr.client`.

So pixel mode is not an exotic Linux trick: it is the configuration Apple uses every boot. Config 1
(today's Linux key mode) is the firmware fallback. This matches the live descriptors in research 0001
(config 2, interface 3, class 0x10, bulk OUT 0x02).

### HID functions on the iBridge (from the same plist)

| Function | HID usage page / usage | Notes |
|---|---|---|
| Digitizer | 0xFF00 / 0x16 (vendor) | `kUSBHIDDeviceIdlePolicy 1000` ms |
| Ambient light sensor | 0x20 / 0x41 | idle policy 150 ms |
| Touch service filter (`DFRTouchServiceFilter`) | 0x0D / 0x04 (touch screen) | MultitouchSoftware-6440.7 |

### How macOS drives the panel (TouchBarServer and ControlStrip symbol names)

- Draws with `DFRDisplayDrawRectsWithIOSurfaceAsync` and converts sparse pixel regions into
  tiles (`convertSparsePixelRegions:toTileRegions:withTileSize:`): **partial, dirty-rect
  updates**, not full frames. It counts dropped frames (`DFRDisplay: dropped frame`).
  → Our daemon should present only damaged rects (a pressed button), not 2170x60 every time.
- Brightness has a delegate and steps (`DFRBrightness`, `BrightnessStep`), and idle dimming:
  `secondsUntilDim`, `dimToStep:`, `SetDimmingStep(... step, duration)`, `disableDimming`,
  `idleTimeout`. → Dim, then off, after inactivity; wake on any input.
- Presentation modes: `DFRPresentationMode`, `DefaultFNMode`, `GetFNBehavior`, user-visible
  strings `functionKeyShows`, `spaces`, `spaces with control strip`. → macOS also has "what
  the bar shows" + "what Fn shows" as the core model, the same shape as our Default/Fn layers.
- **Escape is its own element** in ControlStrip (`_TtC12ControlStrip6Escape`), separate from app
  content. → Keep Esc as a fixed-width, first-group button on every layer (button standard).
- Placement roles (`DFRPlacement`, `DFRPlacementTouchID`, `DFRSystemAppModal`) and a
  `DFRForceScreenLock` path exist: the system reserves the bar for lock/modal states.

### This machine's T1 history (from `~/Documents/mac/macbook-t1-handoff.md`, `recovery/README.md`)

- The ESP once had **no `EFI/APPLE/EMBEDDEDOS`**; the T1 enumerated as `05ac:1281` (recovery).
  It was restored with a staged `idevicerestore-t1` run (`FlashNOR=true`: it rewrote T1 boot/SEP
  flash). Tooling and assets are kept in `~/Documents/mac/recovery/` as the last resort.
- The restored firmware set is backed up and checked by `t1-maintenance`, but **only on the same
  internal SSD**. No external copy existed at the last check.
- Installed today: `apple-ib-drv-dkms 0.3.0-5` (key mode, `fnmode=2`) and a local
  `macbook-touchbar-local.service` (2026-09-21) that turns the bar off when idle via a config-1
  HID display report. It **refuses to run** if the USB configuration or driver version changes,
  so it will stop itself (by design) when pixel mode is enabled; retire it with its own
  `manage.py remove` first.
- The recovery helper refuses to run while `apple_ibridge`, `apple_touchbar`, `t1_cfgsel`,
  `apple_dfr_cfgsel` or `appletbdrm` are loaded: unload pixel-mode modules before any recovery.

## Decision input

1. TB-9 gains two hard gates: an **external** copy of the EFI/T1 set, and retiring
   `macbook-touchbar-local.service` + the key-mode driver cleanly.
2. Daemon design: dirty-rect presents (TB-10/TB-17), idle dim then off with wake on input (TB-24).
3. Esc stays a fixed first-group button on every layer.

## Sources (all local)

- `externos/macos-bins-2026-10-14/System/Library/Extensions/AppleUSBiBridge.kext/Contents/Info.plist` (decoded with `plistlib`)
- `strings` over `usr/libexec/TouchBarServer`, `ControlStrip.app/Contents/MacOS/ControlStrip`, `DFRTouchServiceFilter`
- `macbook-t1-handoff.md`, `recovery/README.md`, `experiments/touchbar-display-local/README.md`
