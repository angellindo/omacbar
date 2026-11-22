# Theming

Buttons are drawn from a `BarTheme` (`crates/tb-core/src/theme.rs`). A theme is a set of
**roles** (background, button fill/border/text, pressed fill/text, accent), geometry (radius,
border width, font size) and a table of named colour **tokens** for per-button overrides.

## Omarchy adapter (default)

| Role | From `colors.toml` |
|---|---|
| background | `background` |
| button text | `foreground` |
| button fill | `foreground` at alpha 10/255 (Omarchy `normal-fill-alpha = 0.04`) |
| button border | `foreground` at alpha 102/255 (`normal-border-alpha = 0.4`) |
| pressed fill / border | `accent` |
| pressed text | `background` |
| tokens | every `#hex` key in the file |

A missing `background`, `foreground` or `accent` is an error, not a guess.

## The active theme and its fallback

The bar follows the **active theme**: `<home>/.local/state/omarchy/current/theme/colors.toml`
(Omarchy writes this path from `$HOME`, not `$XDG_STATE_HOME`). `tb_config::ActiveTheme` reads it
(ADR-0008); the binaries take `<home>` from `$HOME` at startup, and tests pass a temp directory.

When that file is missing, unreadable or invalid, the bar uses the bundled Tokyo Night
(`BarTheme::omarchy_fallback`) and the reason, naming the file, is returned with it; `touchbard`
logs it. The same rule applies at startup and after a theme switch. An unset `$HOME` means no
active theme: the fallback, logged.

## Live switching

`touchbard` re-reads the active theme and re-renders within 1 s of a switch. Two triggers, one
inotify watch (`tb_config::ThemeWatch`):

- **Directory watch (fast path).** `omarchy theme set` stages the new theme in
  `current/next-theme`, deletes `current/theme` and renames the staged directory into place, then
  writes `current/theme.name`. The watch on `current/` reports the rename of `theme` and the write
  of `theme.name`; nothing else there (`background`, `theme.generation`) is a switch. Editing
  `current/theme/colors.toml` by hand is not a switch.
- **`theme-set.d` hook.** `contrib/omarchy/hooks/theme-set.d/touchbar-customizer` writes the
  theme name to `<home>/.local/state/touchbar-customizer/theme-set`, which the same watch sees.
  Omarchy runs hooks after it has retinted every app, often detached, so this path can be seconds
  late. Install it (a human step):
  `omarchy hook install theme-set contrib/omarchy/hooks/theme-set.d/touchbar-customizer`.

Changes read together count once, and a reload that gives the same scene presents nothing, so one
switch draws one frame. Buttons that use token overrides change with it. If `current/` does not
exist when `touchbard` starts, it logs that it is not following switches and keeps its theme.

The configurator reads the active theme when it starts; following switches while it is open is a
follow-up.

## The saved theme source

A config's `theme` is `{ source = "omarchy" }` (the active theme) or `{ source = "file", path }`.
The daemon, the doctor and the configurator resolve it the same way
(`tb_config::ConfigFile::load_resolved`) and validate colour tokens against the resolved theme, so
a token only the selected file defines is accepted, previewed and saved. A selected file that is
missing or malformed is an error naming the file and the reason, never a fallback: the daemon keeps
its last valid config, the doctor fails `runtime.theme`, and the configurator blocks saving so the
layout on disk is kept. Every configurator save resolves the source again
(`ConfigFile::save_resolved`); if the file broke or lost a token the layout uses since opening, the
save is refused with the cause, the edits stay, and a later save after the repair succeeds.

The first-run default is `Config::default_for(theme)`: the shipped default with every colour
override the theme cannot resolve cleared (the button keeps its role colour; no colour is guessed).
Under every shipped Omarchy theme it is the shipped default byte for byte; under a theme with only
the required tokens the Fn + Ctrl `lock` loses its optional `red` fill and the `background` text
paired with it, so it is drawn in the idle roles (button text on button fill). A `text` override
is cleared whenever its button's `fill` is. Files already saved are never changed by this.

## Label font

Labels are set in Rune's `Family::SystemMono` (`tb_render::view::label_style`): fontconfig's
`monospace`, which is JetBrainsMono Nerd Font on Omarchy and follows `omarchy font set` when the
bar starts. The bar and the label-fit check (`tb_render::label_width`) use that one style, so what
is measured is what is drawn. Without fontconfig's answer Rune tries DejaVu Sans Mono, Liberation
Mono, Noto Sans Mono and Ubuntu Mono, then the mono slot (Inter). Rune loads only the face
fontconfig names (Regular for JetBrainsMono Nerd Font), so the requested semibold may draw as that
face's weight.

## Adding another theme source

Implement a constructor that returns `BarTheme` (pure, from text) and add a `ThemeSource` variant.
No renderer change is needed: the renderer only reads `BarTheme`.
