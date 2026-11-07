# Touch Bar Customizer

Customize the Touch Bar of a T1 MacBook Pro (2016-2017) on [Omarchy](https://omarchy.org) Linux:
F-keys by default, your own buttons on **Fn**, **Fn + Ctrl/Alt/Shift/Super**, with widths, labels
and colours that follow your Omarchy theme. Built in Rust with [Rune](https://github.com/anomalyco/rune).

> **Status: proof of concept.** The layout engine, theming, renderer and configurator preview run
> today. Driving the physical bar needs the T1 "pixel mode" kernel stack (see
> `docs/research/0001-t1-touch-bar-hardware.md`), which is not installed by this project yet.

## Try it

```sh
mise install                                   # Rust 1.99.1
cargo run -p tb-app                            # configurator window
cargo run -p tb-app -- --snap out/app.png      # headless screenshot
TB_WRITE_SNAPSHOTS=1 cargo test -p tb-render --test functional   # out/default-layer.png, 2170x60
scripts/check.sh                               # all gates
```

## Docs

- What and why: `docs/scope.md`, `docs/research/`
- How buttons look: `docs/design/button-standard.md`, `docs/design/theming.md`
- How we work: `AGENTS.md`, `docs/standards/`, `docs/agents/workflow.md`

Vendored agent skills from [mattpocock/skills](https://github.com/mattpocock/skills) (MIT) live in `.claude/skills/`.
