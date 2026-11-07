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

Source: `~/.local/state/omarchy/current/theme/colors.toml`, falling back to the bundled Tokyo
Night copy. A missing `background`, `foreground` or `accent` is an error, not a guess.

## Live switching

The daemon watches the theme (Omarchy `theme-set.d` hook + inotify on `theme.name`), rebuilds the
`BarTheme` and re-renders. Buttons that use token overrides change with it.

## Adding another theme source

Implement a constructor that returns `BarTheme` (pure, from text) and add a `ThemeSource` variant.
No renderer change is needed: the renderer only reads `BarTheme`.
