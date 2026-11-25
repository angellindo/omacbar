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
  (absolute path, symlinks unresolved), `reason` (`first-write` | `save` | `overwrite` | `recover` | `factory-reset`), `kind`
  (`file` | `absent`), `mode` (permission bits), `len`, `blake3` (hex), `dir-existed` (absent
  only). BLAKE3 is already in the lock file through Rune; no new package.
- **Private.** Store 0700, generation directories 0700, files 0600, owned by the effective uid,
  under a parent owned by it. Before creating the store hierarchy, verify that the nearest
  existing parent is owned by the effective uid. Create and open each missing component relative
  to its verified parent descriptor. A store that is a symlink (also one swapped in later), not a
  directory, open to others, owned by someone else, or overlapping the config directory in
  either direction is refused with a diagnostic naming its path and the fix; its mode is never changed for the user.
- **Order (atomic, durable, verified).** Refuse a path that cannot be kept, take the store lock,
  only then look at (and create) the config directory, take its lock (TB-43), compare, look at
  the path without following a final symlink (never the reverse lock order). The store lock comes
  first so `dir-existed` is decided while no other protected writer can create the directory: two
  first runs of one profile otherwise let the one that saw the other's fresh directory record
  `true`. For an existing config directory, stage and fsync the replacement there before
  publishing a generation. Build in `backups/.tmp-<pid>-<n>/`: write and fsync data and manifest
  exclusively, fsync the temp directory, check the config again, `renameat(NOREPLACE)` to
  `<id>`, fsync the store, then read back and verify the generation. When the config directory
  is missing, keep the absence before creating any config parents. If the first write fails
  later, try to remove the empty directories created for it in reverse order, using the parent
  descriptors and checking each directory's identity. Directories with other contents or a
  replacement at their path are preserved. Each new directory is synced into its parent.
  Config publication and removal use the pinned config directory descriptor. Immediately before
  either, check that the directory still names the locked one and compare config bytes and mode
  without following a final symlink.
- **Never deleted, deduplicated.** Published generations, leftovers from interrupted writes and
  damaged generations are retained. A damaged generation cannot be reused or restored. When
  its directory or manifest is untrusted, list its ID and diagnostic with the config unknown
  and continue listing valid generations. A store that is itself untrusted is refused. An
  unchanged save reuses the newest verified generation of that config with the same kind, mode
  and hash. Reads, validation failures, and conflicts detected before publication
  (`Changed`) keep nothing. A final check that finds a change after a generation was already
  kept returns `ChangedAfterKeep`, with the current version and retained generation ID. If the
  path has become unsafe or the operation fails after keeping the generation, the refusal also
  names that ID. The configurator keeps its normal conflict controls and reports the ID.
  Terminal recovery reports a late conflict with the same status 3, the retained ID and the
  retry command. Neither path treats the attempt as a successful config replacement.
- **Exactly restorable paths only.** A protected write refuses, before any mutation, a symlinked
  config (dangling or not: `Symlink`), a hard-linked file, a directory, FIFO, socket or device
  (`Unsupported`). Protected reads still follow a symlink (an existing linked profile is shown and
  drawn) but never open a non-regular file, so nothing blocks on a FIFO.
- **Mode.** A protected save gives the new file the replaced file's permission bits; a new file
  is 0600.
- **Recovery.** `ConfigFile::look` records the file on disk as seen without parsing it;
  `ConfigFile::recover(id)` is one more protected write against that version: the generation must
  verify and name this config (`Backups::verify_for`), a `recover` generation of what is replaced
  is kept, and the kept bytes are written with the kept bits (a missing file or directory is
  created). An absence is restored only by `recover_removing`, which removes the file, never its
  directory. The terminal tool `touchbar-recover` (a `[[bin]]` of tb-config: no Rune, GPU or
  display in its graph) wraps them: `list`, `verify <id>`, `restore <id> [--remove-file]`; exit
  0 done, 1 refused or failed, 2 usage, 3 conflict. Its look is unlocked and comes before the
  locks, so a file changed while it waits for another writer is a conflict, never replaced.

## Consequences

- R1: `tb-config` has one binary, `touchbar-recover`, so recovery works when the GUI or the
  daemon cannot start; no new crate arrow.
- R1: `tb-config` also owns the generation store (it is still the only crate that touches the
  config's files); new `tb-config` dependencies `serde`, `toml`, `blake3` (workspace), and
  rustix's `process` feature (effective uid; already enabled in the graph).
- Every test or script that starts a product binary must give it a private `XDG_STATE_HOME` (or
  remove it, with `HOME` temporary); `scripts/try-app.sh` uses `.scratch/manual-test/state`.
- Unbounded growth is accepted (config files are a few KB; dedupe keeps unchanged saves free).

## Limits

- A writer that ignores the locks can change the config between publishing a durable generation
  and the final comparison. The config and generation live in separate directories and can be
  on different filesystems. There is no single atomic update for both. The final check refuses
  the replacement and the published generation remains available. The narrow interval between
  the final comparison and the rename or removal remains. An unlocked writer can also
  swap a directory between its identity check and removal during rollback.
- Interruption safety is shown by the states a crash leaves (temp directories, a complete
  generation whose save never renamed), not by killing a process mid-syscall.
- `flock` on directories and rename durability are Linux local-filesystem behaviour.
