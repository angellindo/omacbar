# Touch Bar Customizer

Arrange, size and theme a T1 MacBook Pro Touch Bar on [Omarchy](https://omarchy.org), using
Rust and [Rune](https://github.com/anomalyco/rune). F-keys are the default layer; Fn and
Fn + Ctrl/Alt/Shift/Super select other layers.

**Proof of concept:** the configurator, layout engine, themed renderer and PNG/window simulator
run today. Physical pixel-mode display/touch support is **pending**. Building or passing doctor
does not enable the physical bar. See [hardware findings](docs/research/0001-t1-touch-bar-hardware.md)
and the [human checklist](docs/hil-checklist.md). Other machines can use the simulator with a
usable Linux desktop/GPU; physical compatibility is not promised.

## Fresh Arch/Omarchy checkout

Run installation commands as a human on the supported Arch-family environment. Python 3.11+
(`python` on Arch) runs the bootstrap before Rust binaries exist. System prerequisites:

```sh
sudo pacman -S --needed base-devel git python pkgconf bash coreutils util-linux \
  libxkbcommon wayland fontconfig ttf-dejavu vulkan-icd-loader
```

Install the Vulkan driver suitable for the machine; the development laptop uses `vulkan-intel`.
For headless software Vulkan, install `vulkan-swrast` instead/as available. Select the intended
ICD with `VK_ICD_FILENAMES` if multiple drivers cause trouble (inspect the installed manifest
under `/usr/share/vulkan/icd.d/`; filenames vary). X11 windows additionally need `libx11 libxcb
libxkbcommon-x11`; Wayland sessions use the Wayland libraries above. A resolvable monospace font
is sufficient for basic labels; a Nerd Font supplies the presets' optional icon glyphs.

Install [mise](https://mise.jdx.dev/getting-started.html) if it is absent (`sudo pacman -S mise`
on Arch). Enable it for your shell (`eval "$(mise activate bash)"` for the current Bash shell,
or follow mise's shell instructions). Then:

```sh
git clone https://github.com/angellindo/omacbar.git
cd omacbar
mise install                                   # pinned Rust 1.99.1
rustup component add rustfmt clippy --toolchain 1.99.1
scripts/doctor.py --profile build
nice -n 19 env CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 cargo build --workspace --locked
scripts/doctor.py --profile runtime
```

The toolchain/components are pinned by `rust-toolchain.toml` and `mise.toml`. Installation and
build may fetch dependencies; doctor never installs, builds, downloads or authenticates. If mise
asks you to trust this checkout, inspect `mise.toml` first. Doctor uses existing concrete rustup
toolchain executables, bypassing install-capable PATH shims. When using a nonstandard Rust
installation, set `RUSTUP_HOME` to its existing rustup root. Set `CARGO_TARGET_DIR` consistently
for both build and doctor if you customize Cargo's target directory; the probe is expected at
`$CARGO_TARGET_DIR/debug/touchbar-doctor-probe` (default `target/debug/`). A missing probe's build
remedy uses the same environment. Cargo config `build.target-dir` is not automatically inspected;
export the matching `CARGO_TARGET_DIR` or build with `--target-dir target` and unset that variable.
Cross-compiled targets are not the supported native recipe.

A successful build creates `target/debug/touchbar-customizer`, `touchbard` and
`touchbar-doctor-probe`. Build doctor exit 0 means prerequisites passed. Runtime doctor additionally
requires native config/theme validation, a Rune GPU render/readback, and a reachable local window
session. Run runtime doctor from your desktop terminal; a headless session can render successfully
and still exit 1 because it cannot reach a desktop socket. Socket reachability never proves GUI
creation/presentation. Ubuntu CI uses Mesa software Vulkan; a fresh Arch OS package installation
has not been reproduced in an isolated OS. Same-host fresh-checkout checks reuse installed host
dependencies/caches and cannot establish clean-OS or physical/GUI operation.

## Open the app

```sh
scripts/try-app.sh
```

This builds and opens the configurator and simulator together. Save config edits to update the
simulator; close either window or press Ctrl+C to stop both. The launcher persists an isolated
config at `.scratch/manual-test/home/.config/touchbar-customizer/config.toml`; current-run logs
are `.scratch/manual-test/logs/{configurator,simulator}.log`, frames are
`.scratch/manual-test/frames/`, and emitted key records are `.scratch/manual-test/keys.bin`.
There are no prerequisite scratch files: the launcher creates its session. Its isolated HOME
uses bundled Tokyo Night unless you configure a test theme. Saved configs keep their arrangement
across updates; back up a test config before deliberately resetting it.

The configurator follows [variant B's layer → group → button workflow](docs/design/configurator-b.md).
Choose any of the six layers in the sidebar, add a button within its group, select a card or
preview button, and edit the panel at right. Drag cards or preview buttons to reorder; Alt+Left
and Alt+Right move the selected button. The bottom preview stays at 50% of 2170×60; small windows
scroll to keep every control reachable. Physical Fn keys affect the daemon's layer, while the
configurator's editing layer stays selected manually.

Width drafts stay with each button and layer; invalid drafts block Save and Ctrl+S. Key and
command text have separate buffers. Modifier layers offer command/none for new actions because
held modifiers combine with emitted keys. Existing keyed buttons, including preset ESC, remain
readable/saveable with warnings; ESC does not promise modifier isolation. Empty or whitespace-only
commands now fail shared validation even in existing version 1 files: choose `none` for a no-op.

Pending changes compare with the loaded or last successfully saved config, so reverting edits
clears them. Save/Ctrl+S validates every layer and draft, then writes atomically. A failed write
keeps edits and the previous successful save time. Startup shows “not saved this session”; only a
successful explicit save records a timestamp. “Saved config/file” describes the file the daemon
watches; simulator acknowledgement is unavailable and the configurator does not claim the
simulator is connected/current or that it consumed a particular save.

The launcher follows the Fn keyboard with `--keys auto`, selected by KEY_FN capability rather
than event number, independent of window focus. The process must already have read access to
that keyboard's event node. Hold Fn or Fn+Ctrl, then release to return to the default layer.
Unavailable Fn access keeps the default layer usable and is logged in `simulator.log`. Agents
inspect capability/access metadata only; a human performs actual input tests and permission setup.
Key actions in the launcher record to a file; **command actions execute their configured commands**.
Real key emission requires explicit `--emit uinput` and existing `/dev/uinput` write access. Doctor
opens neither input device and never emits keys or changes permissions.

Standalone commands from the repo root:

```sh
cargo run -p tb-app                             # configurator window; normal HOME
cargo run -p tb-daemon --bin touchbard -- --sim-window --sim out/touchbard
# Headless output: GPU required, desktop session not required.
cargo run -p tb-app -- --snap out/app.png
printf 'down 906\nup\n' | cargo run -p tb-daemon --bin touchbard -- --sim out/touchbard --emit out/keys.bin
TB_WRITE_SNAPSHOTS=1 cargo test -p tb-render --test functional
```

The screenshot is `out/app.png`; simulator commands produce `out/touchbard/frame-*.png` at
2170×60. `--emit out/keys.bin` writes input-event records, not real keys. Omit `--emit` to log key
actions without emission. `--keys auto` explicitly enables real Fn event reading in the daemon;
without it stdin touch flows do not read a keyboard. Only a human should run real-input examples.

For an isolated headless config, build first, then run the executable with a temporary HOME so
Cargo retains access to your real toolchain/cache:

```sh
existing_rustup_home="${RUSTUP_HOME:-$HOME/.rustup}"
test_home=$(mktemp -d)
HOME="$test_home" target/debug/touchbar-customizer --snap out/isolated-app.png
HOME="$test_home" RUSTUP_HOME="$existing_rustup_home" scripts/doctor.py --profile runtime --json
```

Normal config is `~/.config/touchbar-customizer/config.toml`. App startup writes first-run defaults
when absent; doctor validates in-memory defaults without creating files. Active Omarchy colors
are `~/.local/state/omarchy/current/theme/colors.toml`; absent/malformed active colors use bundled
Tokyo Night with a warning. An explicitly selected invalid/missing theme file rejects the config.
The daemon follows valid config saves and theme switches; invalid saves retain the last accepted
config. The configurator preview currently follows Omarchy regardless of the saved theme source.
`contrib/omarchy/` contains the optional theme hook a human installs. Standalone diagnostic output
uses stdout/stderr; only the launcher redirects its current-run logs to the scratch paths above.

## Doctor and local triage

```sh
scripts/doctor.py                              # default build profile, human report
scripts/doctor.py --profile build --json
scripts/doctor.py --profile runtime --json
scripts/triage.py --prepare-only               # runtime evidence; no agent needed
scripts/triage.py --agent codex --profile build
scripts/triage.py                              # runtime evidence, installed Codex handoff
```

JSON stdout is one `schema_version: 1` object with ordered stable `checks` and a counted
`summary`. Each row has `id`, `status`, `required`, `message`, `evidence`, and actionable `next`.
`ok` means the stated check succeeded; `warning` means a usable fallback/optional degradation;
`failed` means a demonstrated fault; `skipped` means outside scope or an unmet prerequisite.
Required failed **or skipped** rows block readiness. Build scope emits optional skipped runtime
rows. Runtime requires the native probe/config/theme/GPU and local window session; Fn/uinput
metadata are optional, physical hardware is always skipped. The native probe uses the same Rust
config/domain/theme logic as the app and actually renders a 2170×60 RGBA frame in memory for GPU
validation. Package presence cannot substitute for rendering. Missing HOME, an unavailable probe,
invalid JSON, process failure, timeout and oversized output cannot count as healthy.

Doctor exits 0 when all required checks are ready, 1 for required blockers, 2 for usage/internal
report failure. Checks are offline and read-only: tool subprocess deadlines are 5 seconds (native
GPU 20 seconds), captured output is capped at 16 KiB (native 64 KiB), and socket reachability has
a 1-second deadline. Triage's doctor aggregate has a 110-second deadline and 64 KiB cap. Neither
command executes config actions, opens event devices, loads modules or writes system settings.

Triage writes unique private `.scratch/triage/<session>/` directories (0700), with `doctor.json`,
`context.json`, and `prompt.md` (0600), and prints their path. Context contains the sanitized doctor,
revision/tracked-status count, OS/architecture and relevant path descriptions; no raw config,
logs, environment dumps, remote URLs or filenames. Prepare-only exits 0 even with doctor blockers.
An invalid doctor report/preparation error exits 2. Only `--agent codex` is supported: a missing
installed CLI exits 1 after preserving artifacts. Launch passes a prompt-file reference as one
argument with repo cwd, preserves normal Codex authentication/approvals, inherits interactive
stdio, and returns its exit status. It never installs or silently switches CLIs. The interactive
agent can use its normal tools after explicit launch; offline collection does not make that
session an offline agent. See the [local playbook](docs/agents/install-triage.md).

For self-review: read failed/skipped rows and their `next` commands, fix only within your
authorization, rerun build doctor and the locked build, then runtime doctor and applicable tests.
Use prepare-only to inspect evidence before sharing anything. Fn/uinput warnings need human
verification if those capabilities matter; physical setup remains separate pending work.

```sh
nice -n 19 env CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 scripts/check.sh
```

On this shared laptop, run heavy jobs one at a time and pause above 85°C (`sensors`). The check
script covers Python fixtures, formatting, clippy, tests and size/mirror gates. GPU tests need a
working Vulkan adapter, including software Vulkan. Use the [human checklist](docs/hil-checklist.md)
for GUI/Fn/hardware checks the automated evidence cannot establish.

## Source and local files

Versioned source, fixtures, scripts, Cargo lockfile and docs are public. `.scratch/` includes
intentionally versioned sealed acceptance contracts and gate evidence; its raw plans, photos,
test sessions and logs remain local and may contain private evidence. Triage sessions are ignored
explicitly.
`target/`, `out/`, Python caches and Cargo/mise caches are local/generated and are not prerequisites
for another checkout. Never publish raw scratch/config/logs or hardware identifiers.

See [scope](docs/scope.md), [research](docs/research/), [button standard](docs/design/button-standard.md),
[theming](docs/design/theming.md), [agent workflow](AGENTS.md) and [standards](docs/standards/).
Vendored [Matt Pocock skills](https://github.com/mattpocock/skills) are MIT-licensed in `.claude/skills/`.
