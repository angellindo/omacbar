# 0005 · Omarchy theme source and change hooks

Status: resolved (TB-5)

- Active theme: `~/.local/state/omarchy/current/theme/` (name in `../theme.name`).
  Shipped themes: `/usr/share/omarchy/themes/<name>/`.
- `colors.toml` is a flat table of tokens shared by every theme: `accent`, `background`,
  `foreground`, `dark_/darker_/lighter_background`, `selection`, `muted`, and the 16 ANSI-style
  colours (`red`, `bright_red`, ...). This is our only required input.
- `shell.toml` (generated) carries control chrome: `[controls] normal-fill-alpha = 0.04`,
  `normal-border-width = 1`, `normal-border-alpha = 0.4`, `[hyprland] active-border`. Our defaults
  copy these values; reading `shell.toml` directly is optional (TB-14).
- Default UI font: `fc-match monospace` → JetBrainsMono Nerd Font.
- Change notification: `~/.config/omarchy/hooks/theme-set.d/` runs scripts on `omarchy theme set`.
  Plan: a hook that signals the daemon, plus an inotify watch on `theme.name` as a fallback.

## Sources

- Live: `/usr/share/omarchy/themes/tokyo-night/colors.toml`, `~/.local/state/omarchy/current/theme/shell.toml`,
  `omarchy theme --help`, `ls ~/.config/omarchy/hooks`
