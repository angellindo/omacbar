# The active Omarchy theme is read and watched by tb-config; labels use the system monospace font

`tb-config` now owns both files the bar follows on disk: the config file (ADR-0007) and the active
Omarchy theme, `<home>/.local/state/omarchy/current/theme/colors.toml`. Both binaries need the
theme (the daemon draws it, the configurator previews it), so the reasoning of ADR-0007 applies
unchanged: no new crate and no new dependency arrow. `tb-core` still only turns the text into a
`BarTheme`.

A theme switch is seen with the same inotify directory watch as the config: on
`<home>/.local/state/omarchy/current/` for `theme` (Omarchy renames the staged `next-theme`
directory into place) and `theme.name`, and on `<home>/.local/state/touchbar-customizer/` for
`theme-set`, a file the shipped `theme-set.d` hook writes. The hook therefore signals through a
file, not a Unix signal or a socket: catching a signal needs `unsafe` or a new crate (R7), a socket
needs a server, and the file rides the watch that already exists. Omarchy runs `theme-set` hooks
late (after the app retints, often detached), so the directory watch is the fast path and the hook
is the documented Omarchy one.

Labels are set in Rune's `Family::SystemMono`, which asks fontconfig for `monospace`
(JetBrainsMono Nerd Font on Omarchy; `omarchy font set` changes it), drawn and measured in the same
style. We do not bundle the font: the repo is public and each JetBrainsMono Nerd Font face is about
2.5 MB of history that can never be removed, a bundled face would ignore the user's Omarchy font,
and Rune documents `SystemMono` as "loaded from the system, never bundled". The cost is that label
widths depend on the machine's monospace font (DejaVu Sans Mono in CI), so tests assert monospace
behaviour, not JetBrains metrics.

## Considered options

- **Bundle JetBrainsMono Nerd Font (OFL 1.1) through `AppSpec::fonts` / `TextSystem::new`.**
  Deterministic widths everywhere, but see above; OFL would allow it.
- **Put the mono slot on `ui-monospace` (`TextSystem::new(&[], ("Inter", "ui-monospace"))`).**
  Fixes every `.mono()` at once, but `rune::text::measure` follows the families of the most recent
  text system in the process, so measuring and drawing could disagree. An explicit family in the
  one label style cannot drift.
- **Theme file I/O in `tb-daemon`.** The configurator would then depend on the daemon (ADR-0006).
- **A Unix signal from the hook (`pkill -USR1 touchbard`).** Needs a signal handler: `unsafe` or
  `signal-hook`.
