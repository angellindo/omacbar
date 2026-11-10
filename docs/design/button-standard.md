# Button design standard

The rules every Touch Bar button follows. The numbers are implemented in
`crates/tb-core/src/spacing.rs` (`SpacingRules::STANDARD`) and enforced by the layout engine; change
them there and here together, in one ticket.

## Canvas

- Bar: **2170 x 60 px**. Work in device pixels, 1:1. Never scale the bar.
- Grid: **4 px**. Every spacing token and preset fixed width is a multiple of 4. Shared
  widths use whole pixels; derived touch slop is half the in-group gap.
- Coordinates: x from the left edge (ESC side), y from the top.

```
|<88>|[ esc ]|<28>|[ F1 ]<12>[ F2 ]<12> ... <12>[ F12 ]|<88>|      x: 2170 px
 ^ edge      ^ group gap    ^ gap                   ^ edge
 y: 8 px inset │ 44 px button │ 8 px inset            y: 60 px
```

## Spacing tokens

| Token | px | Rule |
|---|---|---|
| `edge_padding` | 88 | Minimum space at each bar edge; photographed edges are about 88–96 px. |
| `gap` | 12 | Between buttons in one group; photographed gaps are about 13–14 px. |
| `group_gap` | 88 | Between groups; photographed media gaps are about 91 px. A layer can override it with `group-gap`; Default uses 28 px for the photographed ESC → F1 gap of about 29 px. |
| `vertical_inset` | 8 | Above and below every button, so buttons are **44 px** tall. |
| `min_button_width` | 64 | No button is narrower. Below this a fingertip covers two buttons. |
| `label_padding` | 12 | Minimum space between label text and the button edge. |
| touch slop | `gap / 2` = 6 | A touch in an in-group gap within 6 px of a button still hits it. Group gaps, edges and trailing space are dead. |

The horizontal values above come from the approximate photo measurements in
[TB-38](https://app.notion.com/414934e84f7f814d8ae2dbc39971c853). The photos have perspective and
assume the glass spans the display; these values match proportions, not a calibrated panel.
Vertical inset, minimum width and label padding retain the previous standard; the photos do not
establish different values for them.

## Width

A button's width is one of:

- `auto` (= `span = 1`): an equal share of the free space.
- `span = n`: n shares. Use 2 for the primary action of a group; never above 4.
- `px = n`: fixed. Use for ESC (`116`) and for position-stable controls. The Fn media preset uses
  fixed **132 px** buttons, matching the photographed natural width of about 132 px.

When a layer has shared buttons, the engine fills its available width: free space =
2170 - 2 x 88 - gaps - fixed widths, split by shares, with leftover pixels one each to the
leftmost shared buttons. An all-fixed layer stays left-aligned and leaves unused width empty
on the right. A layer that cannot give every button 64 px is rejected with the width it needed.

The `group-gap` field is optional per layer. Omitted, it inherits `group_gap`; an explicit value
controls both width preflight and placement. Empty groups add no gaps. Version 1 files without
this field remain readable, and saving preserves an explicit override.

Capacity check: at 64 px and 12 px gaps a single-group layer holds at most **26** buttons.
The default F-key layer uses ESC at 116 px and a 28 px group gap: its first two F-buttons are
144 px and the remaining ten 143 px, with 88 px at both edges. Their widths differ by at most
one pixel because the 2170 px canvas cannot divide the remaining width evenly.
Aim for **at most 16 buttons per layer**: a layer with more still validates, with a warning
(`tb_core::config::warnings`), when it fits.

Fn has five groups: ESC, display brightness (2), keyboard light (2), playback (3), volume (3).
Its ten 132 px controls, 12 px in-group gaps and 88 px group gaps end at x=1948, leaving
**222 px** empty to the right (the photo suggests roughly 200 px). Fixed widths are intentional
here: the controls must retain their natural size instead of sharing the unused width.

## Escape

Every preset layer starts with an `esc` button, `px = 116`, alone in the first group, matching
the photographed 116–117 px width. macOS treats Escape as its own element outside app content
(research 0006); keeping it fixed means the user's thumb finds it at x=88 whatever chord is held.

## Label

- One line, never wrapped. Lowercase words or short symbols (`vol+`, `play`, `F5`).
- Font: the system monospace font (Omarchy: JetBrainsMono Nerd Font), **20 px**, semibold where
  the face has it (`tb_render::view::label_style`, `docs/design/theming.md` § Label font).
- Must fit its room, `width - 2 x label_padding`, measured with Rune's text system at the
  theme's font size (`tb_render::label_width`); otherwise validation warns
  (`tb_core::config::warnings`) and the renderer ellipsizes it inside the room.
- Nerd Font glyphs are allowed as labels until icons land. A glyph comes from the system
  monospace font like any other label: JetBrainsMono Nerd Font draws it (one cell wide in layout,
  but its ink can reach about two cells to the right, so it sits a few px right of centre), and a
  font without it draws its missing-glyph box in the label room. Either way nothing leaves the
  button (`tb-render` test `glyph_label`; snapshot `out/glyph-label.png`).

## States

| State | Fill | Text | Border |
|---|---|---|---|
| Idle | `foreground` at 4% | `foreground` | `foreground` at 40%, 1 px |
| Pressed | `accent` | `background` | `accent` |
| Overridden (idle) | style `fill` | style `text` | style `border` |

Pressed always wins over a style override, so feedback is consistent.

Press feedback is presented within one frame of touch down: the daemon presents only when the
scene changes (ADR-0006), so its frames are its presents, and the first present after a touch
lands on a button already shows that button pressed, in full (no transition), and nothing else
changed. A touch that lands in a dead zone presents nothing. How soon the panel shows that
present is the display backend's part, checked on the bar.

## Style overrides

Use theme token names (`accent`, `red`, `green`, `bright_blue`, ...) so buttons follow theme
changes. Hex (`#rrggbb[aa]`) is allowed but frozen across themes; the configurator marks it.
Reserve `red` fills for destructive actions (lock, power, kill).

## Omarchy look

Square corners (radius 0), 1 px borders, flat fills, no shadows or gradients, mono type. A theme
may set a radius; buttons then use it uniformly.

## Checklist for a new button or preset

- [ ] id is kebab-case and unique in its layer
- [ ] width is `auto`/`span` unless position must be stable or the fixed-width media preset applies
- [ ] label fits at 20 px inside the width minus 24 px
- [ ] colours are theme tokens
- [ ] the layer still validates (`cargo test -p tb-core`) and the preview snapshot looks right
