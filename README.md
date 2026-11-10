# Touch Bar Customizer

Customize the Touch Bar of a T1 MacBook Pro (2016-2017) on [Omarchy](https://omarchy.org) Linux:
F-keys by default, your own buttons on **Fn**, **Fn + Ctrl/Alt/Shift/Super**, with widths, labels
and colours that follow your Omarchy theme. Built in Rust with [Rune](https://github.com/anomalyco/rune).

> **Status: proof of concept.** The layout engine, theming, renderer and configurator preview run
> today. Driving the physical bar needs the T1 "pixel mode" kernel stack (see
> `docs/research/0001-t1-touch-bar-hardware.md`), which is not installed by this project yet.

## Try it

To open the configurator and simulator together with one persistent, isolated test config:

```sh
scripts/try-app.sh
```

The launcher builds both binaries, then opens the two windows. Edit buttons and press **Save**
to update the simulator; closing either window or pressing **Ctrl+C** stops both. Test config
is reused under `.scratch/manual-test/home`; frames, recorded keys and current-run logs are in
`.scratch/manual-test/`. Keys are recorded to a file rather than sent to other apps. The isolated
HOME uses the bundled theme unless a test theme is configured. Your normal config is unchanged.

```sh
mise install                                   # Rust 1.99.1
cargo run -p tb-app                            # configurator window
cargo run -p tb-daemon -- --sim-window          # live simulator; click buttons to tap
cargo run -p tb-app -- --snap out/app.png      # headless screenshot
TB_WRITE_SNAPSHOTS=1 cargo test -p tb-render --test functional   # out/default-layer.png, 2170x60
scripts/check.sh                               # all gates
```

`touchbard` loads `~/.config/touchbar-customizer/config.toml`, creating the default config
when it is missing. Saving in the configurator updates the running simulator without a restart.
Invalid saves are reported and leave the last valid config running. Without `HOME`, the daemon
uses the default layout and bundled Tokyo Night without loading or following a config file.

The saved theme source selects live Omarchy colours or a fixed `colors.toml`. A fixed file is
read at startup and each config save; a missing or invalid theme rejects that config together
with its layout. The configurator preview currently follows Omarchy regardless of this field.
Key actions need the existing `--emit` option; without it they are reported without pressing keys.

## Docs

- What and why: `docs/scope.md`, `docs/research/`
- How buttons look: `docs/design/button-standard.md`, `docs/design/theming.md`
- How we work: `AGENTS.md`, `docs/standards/`, `docs/agents/workflow.md`

Vendored agent skills from [mattpocock/skills](https://github.com/mattpocock/skills) (MIT) live in `.claude/skills/`.
