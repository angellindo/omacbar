---
status: accepted (2026-11-24, TB-48)
---

# Config writes keep a verified generation in the user's state directory

Every test of the bar must be reversible, and the user's exact previous file (comments, mode)
is worth more than any factory preset. TB-43's `.bak` exists only for an acknowledged overwrite
and lives in the directory a restore would replace. So before **every** write of the config by
the configurator or the daemon (the default written for a missing file, `save`, `save_resolved`,
`save_over`), `tb_config` keeps a **generation** of what the write replaces.

## Decision

- **Where.** `<state>/touchbar-customizer/backups/`, `<state>` = `$XDG_STATE_HOME` when set and
  absolute, else `$HOME/.local/state` (XDG: relative or empty values are ignored).
  `Backups::locate(home, xdg_state_home)` is pure; both binaries read the two variables at
  startup only (R12) and open `ConfigFile::protected(path, Backups::at(store))`.
  `ConfigFile::new` stays unprotected (tests, the read-only doctor probe).
- **Layout.** One directory per generation, `backups/<id>/` (id = 1, 2, ... allocated under an
  exclusive `flock` on `backups/`), holding `manifest.toml` and, for a kept file, `config.toml`
  (exact bytes). Manifest (`deny_unknown_fields`, kebab-case): `format = 1`, `id`, `config`
  (absolute path, symlinks unresolved), `reason` (`first-write` | `save` | `overwrite`), `kind`
  (`file` | `absent`), `mode` (permission bits), `len`, `blake3` (hex), `dir-existed` (absent
  only). BLAKE3 is already in the lock file through Rune; no new package.
- **Private.** Store 0700, generation directories 0700, files 0600, owned by the effective uid,
  under a parent owned by it. A store that is a symlink (also one swapped in later), not a
  directory, open to others, owned by someone else, or at/inside the config directory is
  refused with a diagnostic naming its path and the fix; its mode is never changed for the user.
- **Order (atomic, durable, verified).** Refuse a path that cannot be kept, take the store lock,
  only then look at (and create) the config directory, take its lock (TB-43), compare, look at
  the path without following a final symlink (never the reverse lock order). The store lock comes
  first so `dir-existed` is decided while no other protected writer can create the directory: two
  first runs of one profile otherwise let the one that saw the other's fresh directory record
  `true`. Build in
  `backups/.tmp-<pid>-<n>/`: data and manifest written exclusively and fsynced, temp directory
  fsynced, `renameat(NOREPLACE)` to `<id>`, store fsynced, then read back (manifest, id, length,
  hash, the same bytes). Only a generation that verifies lets the config write proceed; any error
  aborts before the config or its directory changes. Store operations are anchored to the store's
  file descriptor (`openat`/`mkdirat`/`renameat`), so a directory swapped under its path is not
  used. New directories (config or store hierarchy) are fsynced into their parents.
- **Never deleted, deduplicated.** The library removes only its own failed temp directory. A
  leftover temp directory or a damaged generation (edited data, garbled manifest, a symlink for
  data) is kept, never listed as verified, never reused. An unchanged save reuses the newest
  verified generation of that config with the same kind, mode and hash. Reads and refused saves
  (`Changed`, `Invalid`, theme) keep nothing.
- **Exactly restorable paths only.** A protected write refuses, before any mutation, a symlinked
  config (dangling or not: `Symlink`), a hard-linked file, a directory, FIFO, socket or device
  (`Unsupported`). Protected reads still follow a symlink (an existing linked profile is shown and
  drawn) but never open a non-regular file, so nothing blocks on a FIFO.
- **Mode.** A protected save gives the new file the replaced file's permission bits; a new file
  is 0600.

## Consequences

- R1: `tb-config` also owns the generation store (it is still the only crate that touches the
  config's files); new `tb-config` dependencies `serde`, `toml`, `blake3` (workspace), and
  rustix's `process` feature (effective uid; already enabled in the graph).
- Every test or script that starts a product binary must give it a private `XDG_STATE_HOME` (or
  remove it, with `HOME` temporary); `scripts/try-app.sh` uses `.scratch/manual-test/state`.
- Unbounded growth is accepted (config files are a few KB; dedupe keeps unchanged saves free).

## Limits

- Writers that take no lock keep TB-43's window between the last compare and the `rename`.
- Interruption safety is shown by the states a crash leaves (temp directories, a complete
  generation whose save never renamed), not by killing a process mid-syscall.
- `flock` on directories and rename durability are Linux local-filesystem behaviour.
