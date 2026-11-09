# 0005 · Omarchy theme source and change hooks

Status: resolved (TB-5); switch mechanics and font facts added (TB-14)

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

## How a switch lands on disk (TB-14)

| Step in `omarchy-theme-set` | Effect in `~/.local/state/omarchy/current/` | Confidence |
|---|---|---|
| Paths built from `$HOME/.local/state/omarchy/current/...` | `$XDG_STATE_HOME` is not used | high (source) |
| `rm -rf next-theme; mkdir -p next-theme`, copy the theme, `omarchy-theme-set-templates` | all work happens in `next-theme`; a missing `colors.toml` is generated from `alacritty.toml` first | high (source) |
| `rm -rf theme; mv next-theme theme` | `theme` is deleted, then renamed into place whole (inotify: delete, then moved-to `theme`) | high (source); inotify events unverified on the machine |
| `printf … > theme.generation`, then `echo NAME > theme.name` | both written in place after the rename | high (source) |
| `omarchy-hook theme-set NAME` | runs after the app retints, detached (`&`) unless run from a terminal or with `OMARCHY_THEME_WAIT=1`: can be seconds after the rename | high (source) |

- `omarchy-hook` runs every regular file in `~/.config/omarchy/hooks/<name>.d/` as `bash <file>
  <args>`, skipping `*.sample`; a failing hook prints "Hook failed" and the rest still run.
  `omarchy hook install <type> <file>` copies the file there with mode 755.
- The fast trigger is the rename of `theme` (or the write of `theme.name`) in `current/`; the
  hook is late but is the Omarchy-sanctioned signal.

## Font facts (TB-14)

| Fact | Source | Confidence |
|---|---|---|
| `fc-match monospace` → `JetBrainsMono Nerd Font`, file `/usr/share/fonts/TTF/JetBrainsMonoNerdFont-Regular.ttf` | `fc-match --format='%{family[0]}\n%{file}' monospace` | high (machine) |
| Installed faces: Regular, Bold, Italic, BoldItalic; no SemiBold | `fc-list \| grep -i jetbrains` | high (machine) |
| Every Latin glyph advances 600/1000 em: 12 px at 20 px | `hmtx` of the Regular file (`unitsPerEm` 1000, advances 600) | high (machine) |
| Rune's `Family::SystemMono` on Linux = fontconfig's `monospace` (one `fc-match` per process, its one file loaded), then DejaVu Sans Mono, Liberation Mono, Noto Sans Mono, Ubuntu Mono, then the mono slot | rune `f73f72c` `crates/rune/src/text/system_fonts.rs`, `tok.rs` (`Family`) | high (source) |
| `TextStyle::mono()` = the mono slot, which is Inter unless the app names another family (`TextSystem::default()` = `("Inter", "Inter")`) | rune `f73f72c` `text.rs` (`impl Default for TextSystem`) | high (source) |
| Whether Rune synthesizes a heavier weight from the one Regular face | — | unverified |

## Sources

- Live: `/usr/share/omarchy/themes/tokyo-night/colors.toml`, `~/.local/state/omarchy/current/theme/shell.toml`,
  `omarchy theme --help`, `ls ~/.config/omarchy/hooks`
- Live (TB-14): `/usr/share/omarchy/bin/omarchy-theme-set`, `/usr/share/omarchy/bin/omarchy-hook`,
  `/usr/share/omarchy/bin/omarchy-hook-install`, `fc-match`, `fc-list`
- Rune `f73f72c`: `crates/rune/src/text/system_fonts.rs`, `crates/rune/src/text.rs`, `crates/rune/src/tok.rs`
