# Touch Bar Customizer

A user-arranged, themed Touch Bar for a T1 MacBook Pro on Omarchy: what the bar shows, how a
touch becomes an action, and how pixels reach the panel.

## Bar and layout

**Bar**:
The physical Touch Bar panel, 2170 x 60 device pixels, landscape.
_Avoid_: strip, touchbar screen, DFR (except when citing drivers)

**Layer**:
The set of buttons the bar shows while one chord is held. The default layer is the F-key row.
_Avoid_: page, mode, view, profile

**Chord**:
The held Fn/modifier combination that selects a layer: `default`, `fn`, `fn-ctrl`, `fn-alt`,
`fn-shift`, `fn-super`.
_Avoid_: shortcut, combo, modifier mode

**Group**:
An ordered run of buttons inside a layer, separated from other groups by the group gap.
_Avoid_: section, cluster, segment

**Button**:
One tappable region with an id, label, width, style and action.
_Avoid_: key (a key is what an action presses), item, control

**Span**:
A button's share of the free width (`auto` is span 1). Fixed widths are `px`.
_Avoid_: stretch, flex, weight

**Spacing rules**:
The pixel tokens (edge padding, gap, group gap, vertical inset, minimum width) every layer obeys.
_Avoid_: margins, paddings (as a collective name)

**Placement**:
The pixel rect the layout engine computed for a button.
_Avoid_: frame (reserved), bounds

**Action**:
What a tap does: press a key chord (`key`), run a command (`command`) or nothing.
_Avoid_: binding, handler, command (for the general concept)

## Drawing

**Theme**:
The roles, geometry and colour tokens buttons are drawn with, derived from Omarchy `colors.toml`.
_Avoid_: skin, palette (a palette is only the tokens)

**Token**:
A named colour in the theme (`accent`, `red`); style overrides refer to tokens.

**Scene**:
A layer resolved against a theme and pressed state: everything the view needs, nothing more.
_Avoid_: model, render state

**Frame**:
One rendered image of the bar in RGBA, before conversion for a display backend.
_Avoid_: buffer, image, screenshot

**Present**:
Hand a frame to a display backend.
_Avoid_: blit, flush, push (except in driver docs)

**Display backend**:
Where frames go: the Touch Bar via DRM, `/dev/dfr0`, or the simulator (PNG/window).
_Avoid_: output, sink, driver

**Simulator**:
The display backend that shows frames on screen or writes PNGs, for work without the bar.

## Hardware

**Key mode**:
iBridge USB configuration 1: the T1 firmware draws fixed layouts and no custom pixels exist.

**Pixel mode**:
iBridge USB configuration 2: the host draws every pixel and receives multitouch.

**Modifier leak**:
A physically held modifier combining with a key the daemon emits from a modifier layer.
