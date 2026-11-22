# 0010 · ewweberlin.touchbar: control path and applicability to T1

Status: resolved as source research (TB-41) · Checked 2026-11-22 UTC · No hardware tested.

## Question

How does the exact Omarchy marketplace plugin control its Touch Bar, and which parts can inform
our Rust + Rune T1 backend without assuming T2 compatibility?

## Findings

### Identity and evidence boundary

The [marketplace catalog](https://plugins.omarchy.org/catalog.json), entry `ewweberlin.touchbar`,
resolves to **ewweberlin/Omarchy-Touchbar**, version **0.1.4**. A fresh
[GitHub API request](https://api.github.com/repos/ewweberlin/Omarchy-Touchbar/commits/main)
returned **`d790c71b9fbefe436808c32de4c3547375fa9efc`**, matching the catalog's observed commit.
The pinned [manifest](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/manifest.json#L1-L19)
confirms id, version, MIT license and service entry point. This is not a similarly named T1 plugin.
Confidence: **high** for identity at observation time.

The catalog's listing verification baseline is **`e4fb34c76dded1e9648aef22086a0b6b026d3467`**,
not the current commit. It says `verificationCoverage: update-unverified`,
`verificationStatus: unverified`, and `installAvailable: false` / manual setup. A separately recorded
`upstreamValidatedCommit` does not make the baseline equal to HEAD. These are catalog metadata,
not an independent hardware or security certification. Snapshot: `.scratch/TB-41/catalog-entry.json`.
Confidence: **high**, time-dependent.

The author reports a **MacBookPro15,2 (2018, T2), Omarchy/Hyprland, arch-mact2**. Other T2 support is
an expectation, not a test matrix; no T1 test is reported. Source: pinned
[README hardware and requirements](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/README.md#L30-L77).
Confidence: **high for the author's report; unverified independently**.

### Rendering and transport: three owners

```text
Omarchy shell → Service.qml → Python daemon → SVG + tiny-dfr config files
                                             ↓ config inotify reload
                                    tiny-dfr (Cairo/librsvg)
                                             ↓ DRM dumb framebuffer + dirty clips
                                    appletbdrm kernel driver → USB → panel

Touch digitizer → evdev → Python hit-test → commands / Hyprland / audio / brightness
               ↘ libinput → tiny-dfr → uinput (Escape and Fn-held F-keys)
```

| Component | Actual responsibility and evidence | Confidence |
|---|---|---|
| Quickshell | `Service.qml` launches `python3 -B … --managed`, forwards logs, stops the child and restarts failures with backoff. It does not paint the panel. [Service.qml:21–75](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/Service.qml#L21-L75) | high |
| Python | Builds one SVG; atomically replaces `/etc/tiny-dfr/omarchy-touchbar.svg`, then `/etc/tiny-dfr/config.toml`. Each replacement is atomic; the pair is not a transaction. Identical SVGs are skipped and redraws capped at the configured rate (default 12 fps). [daemon.py:313–345](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/daemon/daemon.py#L313-L345), [config.py:63–79](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/daemon/config.py#L63-L79) | high |
| tiny-dfr | Opens DRM, acquires master, allocates an XRGB8888 dumb buffer, commits the mode, maps memory and calls `dirty_framebuffer`. Cairo rotates drawing 90°. [display.rs:69–181](https://github.com/AsahiLinux/tiny-dfr/blob/a79f2f43e6c6a9b855968e54eb243c20642ebc89/src/display.rs#L69-L181), [main.rs:580–605](https://github.com/AsahiLinux/tiny-dfr/blob/a79f2f43e6c6a9b855968e54eb243c20642ebc89/src/main.rs#L580-L605), [main.rs:958–974](https://github.com/AsahiLinux/tiny-dfr/blob/a79f2f43e6c6a9b855968e54eb243c20642ebc89/src/main.rs#L958-L974) | high for pinned upstream |
| Kernel | Linux v6.18 `appletbdrm` converts emulated XRGB8888 to BGR888, packages damage rectangles and uses USB bulk messages. Its device table matches `05ac:8302`, not T1 `05ac:8600`. [appletbdrm.c:150–190](https://github.com/torvalds/linux/blob/7d0a66e4bb9081d75c82ec4957c50034cb0ea449/drivers/gpu/drm/tiny/appletbdrm.c#L150-L190), [372–456](https://github.com/torvalds/linux/blob/7d0a66e4bb9081d75c82ec4957c50034cb0ea449/drivers/gpu/drm/tiny/appletbdrm.c#L372-L456), [817–834](https://github.com/torvalds/linux/blob/7d0a66e4bb9081d75c82ec4957c50034cb0ea449/drivers/gpu/drm/tiny/appletbdrm.c#L817-L834) | high for this kernel reference; author's installed build unverified |

The 2170×60 panel is not one freely owned Python framebuffer. Python's canvas is **2079×60 at
x=91**. Its generated config has one `Stretch=23` media button, `IconWidth=2079`, `IconHeight=60`,
no action, and pixel shift disabled. tiny-dfr inserts Escape into both layers when width ≥2170.
The 91-pixel offset includes tiny-dfr's spacing; it is not a universal hardware reservation.
[Plugin geometry/config](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/daemon/config.py#L15-L21),
[generated config](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/daemon/config.py#L99-L142),
[tiny-dfr Escape insertion](https://github.com/AsahiLinux/tiny-dfr/blob/a79f2f43e6c6a9b855968e54eb243c20642ebc89/src/config.rs#L119-L146),
[width calculation](https://github.com/AsahiLinux/tiny-dfr/blob/a79f2f43e6c6a9b855968e54eb243c20642ebc89/src/main.rs#L602-L635).
Confidence: **high** for code, not independently measured dimensions.

### Reloads, Fn and actions

The publisher sends **no reload signal to tiny-dfr**. Upstream `ConfigManager` watches the config
path with `IN_MOVED_TO | IN_CLOSE | IN_ONESHOT`, reloads and rearms after an event. This is more
specific than the plugin comment saying only `IN_MOVED_TO`: do not infer that every in-place write
is universally ignored. On reload, tiny-dfr replaces both layers, clears its touch map and resets
`active_layer=0`. It does not release each old touch's action in that reset branch. Hence the
plugin's actionless stretched button avoids a key press whose release would be forgotten.
Sources: [config.rs:158–218](https://github.com/AsahiLinux/tiny-dfr/blob/a79f2f43e6c6a9b855968e54eb243c20642ebc89/src/config.rs#L158-L217),
[main.rs:918–922](https://github.com/AsahiLinux/tiny-dfr/blob/a79f2f43e6c6a9b855968e54eb243c20642ebc89/src/main.rs#L918-L922).
Confidence: **high** for reset; safety under every event ordering remains **unverified**.

`MediaLayerDefault=true` gives the custom strip normally and F1–F12 while Fn is held. Python reads
Fn to suppress publishing **and its own touch actions** during Fn; Fn release marks the image dirty.
Startup forces an initial publication before discovery, so this is not a synchronization guarantee
for startup with Fn already held. tiny-dfr independently receives Fn from libinput on both
`seat0` and `seat-touchbar`; its Touch Bar name check selects the digitizer, not the only Fn source.
This updates the narrower tiny-dfr description in [research 0002](0002-input-fn-modifiers.md).
Sources: [daemon.py:161–186](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/daemon/daemon.py#L161-L186),
[313–357](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/daemon/daemon.py#L313-L357),
[tiny-dfr input loop](https://github.com/AsahiLinux/tiny-dfr/blob/a79f2f43e6c6a9b855968e54eb243c20642ebc89/src/main.rs#L865-L873),
[990–1070](https://github.com/AsahiLinux/tiny-dfr/blob/a79f2f43e6c6a9b855968e54eb243c20642ebc89/src/main.rs#L990-L1059).
Confidence: **high** for code; cross-process race behavior **unverified**.

Python opens evdev read-only/nonblocking, without an `EVIOCGRAB` call. Digitizer selection uses a
Touch Bar name plus ABS X capability. Keyboard selection scores KEY_FN capability and an internal
keyboard name fallback. It tracks one contact, accepts single-touch and MT axes, uses y=30 when Y
is absent, subtracts the canvas offset, and supports scale/inversion/offset calibration. Its T2
single-row comments are not proof that T1 emits identical events.
Sources: [input.py:151–237](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/daemon/input.py#L151-L237),
[267–401](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/daemon/input.py#L267-L401).
Confidence: **high** for implementation; T1 calibration **unverified**.

Custom actions launch commands, not Python-generated uinput keys: `omarchy`, `brightnessctl`,
`pactl`, and Hyprland Lua through `hyprctl eval`. Hyprland events arrive on `.socket2.sock` with
polling fallback. tiny-dfr still creates the virtual keyboard and emits Escape/F-keys.
[actions.py:55–124](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/daemon/actions.py#L55-L124),
[audio](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/daemon/actions.py#L202-L216),
[hypr.py:54–133](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/daemon/hypr.py#L54-L133),
[tiny-dfr uinput](https://github.com/AsahiLinux/tiny-dfr/blob/a79f2f43e6c6a9b855968e54eb243c20642ebc89/src/main.rs#L840-L919).
Confidence: **high**. This does not resolve held-modifier key semantics: [research 0008](0008-modifier-leak-hyprland.md) still applies.

### Installation, packaging and power

| Dependency / side effect | Evidence | Confidence |
|---|---|---|
| Python ≥3.11, Quickshell/Omarchy, tiny-dfr, rsvg-convert, brightnessctl, pactl, hyprctl | Installer checks Python, warns for missing helpers/package. These are requirements, not all hard preflight gates. [install:79–100](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/bin/install#L79-L100) | high |
| User-owned `/etc/tiny-dfr` directory; 0644 published files; udev OWNER/user, GROUP/input, MODE/0660 for exact T2 digitizer/internal keyboard names; service enable/start | [install:308–343](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/bin/install#L308-L343), [rules:1–11](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/udev/90-omarchy-touchbar.rules#L1-L11). These names do not grant access to the T1 SPI keyboard by themselves. | high |
| Separate seat and system privileges | tiny-dfr udev assigns DRM/input to `seat-touchbar`; its startup opens DRM/uinput/backlight then drops to nobody with input/video groups. [seat rules](https://github.com/AsahiLinux/tiny-dfr/blob/a79f2f43e6c6a9b855968e54eb243c20642ebc89/etc/udev/rules.d/99-touchbar-seat.rules#L1-L5), [startup:840–873](https://github.com/AsahiLinux/tiny-dfr/blob/a79f2f43e6c6a9b855968e54eb243c20642ebc89/src/main.rs#L840-L873) | high |
| Theme integration | Reads Omarchy current `colors.toml`; hook touches a runtime refresh file. `SIGUSR1` is an optional Python theme-refresh signal, not tiny-dfr reload. [paths](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/daemon/config.py#L46-L58), [theme loop](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/daemon/daemon.py#L243-L259), [hook](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/hooks/10-omarchy-touchbar#L1-L11) | high |
| Power policy | Every 5 seconds Python attempts to restore `appletb_backlight` to maximum while lid appears open; missing lid state defaults to open. This competes with tiny-dfr's 30/60-second dim/off policy, and stopping statistics after 300 seconds is not panel sleep. [keepalive](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/daemon/daemon.py#L269-L309), [backlight](https://github.com/AsahiLinux/tiny-dfr/blob/a79f2f43e6c6a9b855968e54eb243c20642ebc89/src/backlight.rs#L119-L145) | high for attempts; runtime success unverified |

The README links `WhatAmISupposedToPutHere/tiny-dfr`; the
[GitHub repository API](https://api.github.com/repos/WhatAmISupposedToPutHere/tiny-dfr)
redirects that historical owner to **AsahiLinux/tiny-dfr** (same reviewed HEAD above), rather than
a distinct plugin-specific fork. Confidence: **high**, observed redirect and code revision.

The plugin does not pin a tiny-dfr build. A relevant current Arch T2
[packaging recipe](https://github.com/NoaHimesaka1873/tiny-dfr-arch/blob/95e8a3a7f030f22da21e49e3287fd4aee8b6261e/PKGBUILD#L1-L51)
fetches **AsahiLinux/tiny-dfr** without a source revision, depends on `linux-t2`, installs config,
udev and a T2 systemd drop-in, and declares a conflict with `touchbard`. It does not patch the Rust
source. The drop-in clears BindsTo and adds graphical-target ordering. This is packaging evidence,
not proof of which binary the author installed. The
[archiso pacman configuration](https://github.com/t2linux/archiso-t2/blob/f20577d5a432616b15ee7acc9618ae3226719cb4/archiso/pacman.conf#L102-L104)
identifies the arch-mact2 mirror; its database fetch failed during research. Exact installed package
revision and arch-mact2 kernel differences remain **unverified**. Consequently upstream behavior
above is explicitly pinned reference behavior, not an assertion of byte-identical installation.

### T1 and Rune decision input

1. **Hardware enablement remains a prerequisite.** Our T1 research distinguishes config 1 fixed
   firmware keys from config 2 pixels with patched `appletbdrm` and touch support. Equal 2170×60
   dimensions do not establish USB protocol compatibility. The plugin supplies neither those T1
   patches nor firmware restoration. Existing human gates stay in force; no current-device probe
   was performed. Confidence **high**, [research 0001](0001-t1-touch-bar-hardware.md),
   [scope](../scope.md), [macOS comparison](0011-macos-touchbar-driver-reference.md).
2. **A file-based Rune bridge is plausible, not implemented.** Pinned tiny-dfr loads PNG as well
   as SVG and accepts image dimensions. Rune RGBA could be encoded as PNG and published as a
   stretched icon; remove any higher-priority same-name SVG. This still pays encoding/reload cost,
   reserves Escape, resets tiny-dfr's layers/touches, and is not a raw-frame or shared-memory API.
   Confidence **high for PNG support; feasibility/performance unverified**,
   [image loading](https://github.com/AsahiLinux/tiny-dfr/blob/a79f2f43e6c6a9b855968e54eb243c20642ebc89/src/main.rs#L109-L194).
3. **Prefer investigating direct DRM behind the existing Display seam.** It preserves Rune
   rendering, full-panel geometry and our default F-keys plus Fn/modifier layers. Only one process
   may own DRM master; tiny-dfr and a future daemon need an explicit handoff. Competing evdev readers
   can also duplicate actions even without a grab; `/etc/tiny-dfr` writers and backlight keepalive
   are additional conflicts. Confidence **high for conflicts, design inference for preference**,
   [ADR-0002](../adr/0002-display-backend-is-a-seam.md) and DRM acquisition cited above.
4. **Reuse ideas, retain our product scope.** Shared render/hit geometry, render-on-change,
   capability-based Fn discovery, theme notification and command actions are useful patterns.
   Drawers/sliders/vitals, the inverse default layer and always-lit policy are outside or conflict
   with our MVP. Confidence **high**, [strip geometry](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/daemon/strip.py#L1-L16)
   and [scope](../scope.md).
5. **License boundaries differ by component.** Plugin and tiny-dfr source are MIT (preserve their
   copyright/permission notices for copied substantial portions); tiny-dfr's embedded Material
   icons carry Apache-2.0 notices. Linux `appletbdrm` declares GPL. Calling a separate daemon and
   copying driver code are different reuse choices; this research grants no blanket relicensing.
   Apple references remain proprietary and are not reusable assets. Confidence **high for declared
   licenses**, [plugin LICENSE](https://github.com/ewweberlin/Omarchy-Touchbar/blob/d790c71b9fbefe436808c32de4c3547375fa9efc/LICENSE#L1-L21),
   [tiny-dfr LICENSE](https://github.com/AsahiLinux/tiny-dfr/blob/a79f2f43e6c6a9b855968e54eb243c20642ebc89/LICENSE#L1-L21),
   [Material license](https://github.com/AsahiLinux/tiny-dfr/blob/a79f2f43e6c6a9b855968e54eb243c20642ebc89/LICENSE.material#L1-L15), kernel license cited above.

Proposed follow-ups: (a) after TB-9's human hardware gates, compare PNG/tiny-dfr and direct DRM
presentation on T1, including Fn held during startup/reload and touch release; blocks a backend
choice, not simulator work; (b) before physical-daemon rollout, specify exclusive display/input/
backlight ownership and restoration, preserving current layer and idle semantics; blocks safe
coexistence claims. Neither is an instruction to install this plugin.

## Sources

All code links pin the reviewed revisions. Local public-source copies are under
`.scratch/TB-41/upstream/`; no upstream scripts were run. Reviewed project base:
`7bbc4dd0e62c46916dc9056ba0f744a6ebcd1d34`. No app changes, builds, tests, device opens or system
configuration changes were needed. Installed-package equivalence, timing, suspend/resume and T1
operation remain unverified.

## Resumen para el usuario

El camino es **Omarchy → supervisor QML → Python → SVG/configuración → tiny-dfr → DRM →
controlador del kernel → USB → pantalla**. El tacto vuelve por evdev; Python calcula el botón y
lanza comandos, mientras tiny-dfr conserva Escape y las F al mantener Fn. No está probado en T1:
primero necesitamos el modo de píxeles y los controladores T1 ya investigados. Podemos evaluar una
salida PNG desde Rune o un backend DRM propio; este último conserva mejor nuestras capas y
geometría. No conviene copiar sus permisos, su política de mantener la barra encendida ni instalar
ambos propietarios de pantalla a la vez.
