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
The pixel tokens (edge padding, gap, group gap, vertical inset, minimum width, label padding)
every layer obeys.
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

**Active theme**:
The Omarchy theme the desktop shows now. The bar's theme is derived from its `colors.toml`, or is
the bundled Tokyo Night when that file cannot be read.
_Avoid_: current theme, system theme

**Theme switch**:
Omarchy replacing the active theme (`omarchy theme set`). The bar shows the new theme within 1 s.
_Avoid_: theme change event, reload

**Scene**:
A layer resolved against a theme and pressed state: everything the view needs, nothing more.
_Avoid_: model, render state

**Frame**:
One rendered image of the bar in RGBA, before conversion for a display backend.
_Avoid_: buffer, image, screenshot

**Present**:
Hand a frame and its damage to a display backend.
_Avoid_: blit, flush, push (except in driver docs)

**Damage**:
The rects of the bar, in bar pixels, whose pixels changed since the last present. Only damaged
rects reach the panel; the whole bar is one rect.
_Avoid_: dirty rect (except citing macOS), invalidation, update region

**Display backend**:
Where frames go: the Touch Bar via DRM, `/dev/dfr0`, or the simulator (PNG/window).
_Avoid_: output, sink, driver

**Simulator**:
The display backend that shows frames on screen or writes PNGs, for work without the bar.

**Daemon**:
`touchbard`, the background process that owns the bar: it renders the shown layer, presents
it, and turns touches into pressed buttons and actions.
_Avoid_: server, service (except the systemd unit), compositor

## Input

**Touch**:
One finger on the bar, known by its x position in bar pixels from the moment it lands until it
lifts. A touch presses the button it hits.
_Avoid_: tap (a touch that lands and lifts on one button), click, pointer

**Fn keyboard**:
The keyboard that reports the Fn key; its held Fn and modifier keys pick the layer. It is found by
its Fn capability, never by its event number (on the MacBookPro14,3, the Apple SPI keyboard).
_Avoid_: internal keyboard, SPI keyboard (except citing drivers), event5

**Modifier source**:
Where the daemon learns which Fn and modifier keys are held: the Fn keyboard, or recorded key
events in tests.
_Avoid_: key listener, keyboard reader, key hook

## Config

**Config**:
The user's file of layers and theme source, `~/.config/touchbar-customizer/config.toml`. The bar
uses the last valid config read; an invalid file never replaces it.
_Avoid_: settings, preferences, profile

**Issue**:
One problem found when checking a config. A check reports every issue, not only the first.
_Avoid_: error (for config problems), warning

**Warning**:
A problem found when checking a config that does not reject it: the bar still uses and draws the
config. A layer with more than 16 buttons and a label wider than its button's room are warnings.
_Avoid_: issue (an issue rejects the config), error, lint

**Label room**:
The width a button's label may take: the button's placement width minus the label padding on
both sides. A wider label is ellipsized on the bar.
_Avoid_: text area, content width

## Configurator

**Configurator**:
The `touchbar-customizer` window where the user arranges the buttons of each layer and saves the
config. It previews each layer exactly as the bar draws it.
_Avoid_: editor, settings app, GUI

**Selected button**:
The one button of the shown layer that the edit panel shows and keyboard reorder moves. Pressing a
button in the preview or the button list selects it.
_Avoid_: focused button (focus belongs to text fields), active button, current button

**Edit panel**:
The configurator's fields for the selected button (label, width, colours, action) and the list of
every issue in the config, updated as the user types.
_Avoid_: inspector, properties, form

## Hardware

**Key mode**:
iBridge USB configuration 1: the T1 firmware draws fixed layouts and no custom pixels exist.

**Pixel mode**:
iBridge USB configuration 2: the host draws every pixel and receives multitouch.

**Modifier leak**:
A physically held modifier combining with a key the daemon emits from a modifier layer.
