# The config file has its own crate, watched with inotify through rustix

`tb-config` (library `tb_config`) owns the config file on disk: it loads
`~/.config/touchbar-customizer/config.toml` (writing the default config when the file is
missing), keeps the last valid config when the file is invalid, saves atomically (a temp file in
the same directory, then `rename`), and reports changes to the file. It depends on `tb-core`
only, which keeps parsing and validation (R1: `tb-core` takes text and returns values). Both
binaries need the file: the configurator saves it (TB-19) and the daemon loads and reloads it.
Putting the file in `tb-app` would make the daemon depend on the configurator, and putting it in
`tb-daemon` would make the configurator depend on the daemon (ADR-0006: nothing depends on it),
so it gets its own crate. The arrows `tb-app → tb-config` and `tb-daemon → tb-config` are
covered by this ADR when those tickets add them.

Changes are seen with Linux inotify through `rustix::fs::inotify` (safe API, so R7 holds).
`rustix` 1.x is already in `Cargo.lock` through Rune's Wayland/X11 dependencies, so enabling its
`fs` feature adds no package and `cargo --locked` stays offline-safe. The watch is on the
config's **directory**, filtered by file name, for "written and closed" and "moved into place":
editors that save by writing a temp file and renaming it replace the inode, which a watch on the
file itself would lose after the first save.

## Considered options

- **`notify` crate.** Cross-platform with its own thread and debouncer; several new packages
  for a Linux-only POC that needs one directory watch.
- **`inotify` crate.** Small and maintained, but two new packages for what `rustix` already
  provides in the lockfile.
- **Polling the file's mtime.** Adds latency and periodic wake-ups to an always-on daemon.
- **File I/O in `tb-app` or `tb-daemon`.** Rejected above (dependency direction).
