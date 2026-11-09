# Button design standard

The rules every Touch Bar button follows. The numbers are implemented in
`crates/tb-core/src/spacing.rs` (`SpacingRules::STANDARD`) and enforced by the layout engine; change
them there and here together, in one ticket.

## Canvas

- Bar: **2170 x 60 px**. Work in device pixels, 1:1. Never scale the bar.
- Grid: **4 px**. Every spacing value and every fixed width is a multiple of 4.
- Coordinates: x from the left edge (ESC side), y from the top.

```
|<8>|[ esc ]|<24>|[ F1 ]<8>[ F2 ]<8> ... <8>[ F12 ]|<8>|      x: 2170 px
 ^ edge      ^ group gap    ^ gap                   ^ edge
 y: 8 px inset │ 44 px button │ 8 px inset            y: 60 px
```

## Spacing tokens

| Token | px | Rule |
|---|---|---|
| `edge_padding` | 8 | Space between the bar edge and the first/last button. |
| `gap` | 8 | Between buttons in one group. |
| `group_gap` | 24 | Between groups. Use groups to show meaning (ESC / brightness / media / volume). |
| `vertical_inset` | 8 | Above and below every button, so buttons are **44 px** tall. |
| `min_button_width` | 64 | No button is narrower. Below this a fingertip covers two buttons. |
| `label_padding` | 12 | Minimum space between label text and the button edge. |
| touch slop | `gap / 2` = 4 | A touch in a gap within 4 px of a button still hits it. Group gaps and edges are dead. |

## Width

A button's width is one of:

- `auto` (= `span = 1`): an equal share of the free space.
- `span = n`: n shares. Use 2 for the primary action of a group; never above 4.
- `px = n`: fixed. Use for ESC (`120`) and for buttons that must not move when others are added.

The engine fills the bar exactly: free space = 2170 - 2 x 8 - gaps - fixed widths, split by
shares, leftover pixels one each to the leftmost shared buttons. A layer that cannot give every
button 64 px is rejected with the width it needed.

Capacity check: at 64 px and 8 px gaps a single-group layer holds at most **30** buttons. The
default F-key layer (ESC at 120 px + 12 F-keys) gets 160 px per F-key. Aim for **at most 16 buttons per layer**: a layer with more still validates, with a warning
(`tb_core::config::warnings`).

## Escape

Every layer starts with an `esc` button, `px = 120`, alone in the first group. macOS treats Escape
as its own element outside app content (research 0006); keeping it fixed means the user's thumb
finds it in the same place whatever chord is held.

## Label

- One line, never wrapped. Lowercase words or short symbols (`vol+`, `play`, `F5`).
- Font: the system monospace font (Omarchy: JetBrainsMono Nerd Font), **20 px**, semibold where
  the face has it (`tb_render::view::label_style`, `docs/design/theming.md` § Label font).
- Must fit its room, `width - 2 x label_padding`, measured with Rune's text system at the
  theme's font size (`tb_render::label_width`); otherwise validation warns
  (`tb_core::config::warnings`) and the renderer ellipsizes it inside the room.
- Nerd Font glyphs are allowed as labels until icons land.

## States

| State | Fill | Text | Border |
|---|---|---|---|
| Idle | `foreground` at 4% | `foreground` | `foreground` at 40%, 1 px |
| Pressed | `accent` | `background` | `accent` |
| Overridden (idle) | style `fill` | style `text` | style `border` |

Pressed always wins over a style override, so feedback is consistent.

## Style overrides

Use theme token names (`accent`, `red`, `green`, `bright_blue`, ...) so buttons follow theme
changes. Hex (`#rrggbb[aa]`) is allowed but frozen across themes; the configurator marks it.
Reserve `red` fills for destructive actions (lock, power, kill).

## Omarchy look

Square corners (radius 0), 1 px borders, flat fills, no shadows or gradients, mono type. A theme
may set a radius; buttons then use it uniformly.

## Checklist for a new button or preset

- [ ] id is kebab-case and unique in its layer
- [ ] width is `auto`/`span` unless position must be stable
- [ ] label fits at 20 px inside the width minus 24 px
- [ ] colours are theme tokens
- [ ] the layer still validates (`cargo test -p tb-core`) and the preview snapshot looks right
