---
name: tb-button-design
description: "Design or change Touch Bar buttons, layers, presets, spacing or theme roles so they follow the project's button standard (2170x60 px, 4 px grid, Omarchy look). Use before touching presets.rs, spacing.rs, theme.rs, the bar view, or any button the user will see."
---

# Button design

The standard is `docs/design/button-standard.md` (spacing tokens, widths, labels, states, Omarchy
look, checklist) and theming is `docs/design/theming.md`. Read both first.

1. State the layer, its groups and why each group exists (meaning, not looks).
2. Choose widths: `auto`/`span` by default, `px` only for position-stable buttons (ESC). Check the
   layer against the capacity rule (≤ 16 buttons; every button ≥ 64 px) with `tb_core::place`.
3. Write labels that fit (20 px mono, width − 24 px). Colours are theme tokens; `red` only for
   destructive actions.
4. On a `fn-*` modifier layer, prefer `command` actions (modifier leak, `docs/research/0002`).
5. Render and look: `TB_WRITE_SNAPSHOTS=1 cargo test -p tb-render --test functional` and
   `cargo run -p tb-app -- --snap out/app.png --scene <scene>`; read the PNGs.
   Done when every checklist item in the standard is ticked and you have looked at the render.

Changing a spacing token is a standard change: update `spacing.rs` and the standard's table in the
same commit, and say so in the ticket.
