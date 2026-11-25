---
status: accepted (2026-11-22, TB-43)
---

# Config saves are compare-and-swap under a directory lock

Two configurator windows, a configurator and the daemon's first-run default, or a configurator
and a text editor can hold the same `config.toml`. A blind atomic save (temp file + `rename`)
lets the last writer win, so a window with no edits pressing Save used to put its old snapshot
over a newer file.

Every `tb_config::ConfigFile` write now replaces only the version of the file it last read or
wrote:

- **Version = content.** A `DiskVersion` is "absent" or the exact bytes seen. Size and mtime are
  not trusted (an in-place edit can keep both); an identical rewrite is not a newer version.
  Every read records the bytes it saw, valid or not, because that is what the user is shown. A
  `ConfigFile` that never read its path may only create the file, never replace one.
- **Compare-and-swap.** `save`, `save_resolved`, the missing-file default (`load`,
  `load_resolved`) and the acknowledged overwrite all take an exclusive `flock` on the config's
  **directory** (no lock file is left behind), compare the file with the expected version, write
  the temp file, compare again immediately before the `rename`, and rename. A difference is
  `ConfigFileError::Changed(DiskVersion)`: nothing is written, the current config and the seen
  version stay. A missing-file default that finds a file another writer created reads that file
  instead of replacing it.
- **Acknowledged overwrite.** `save_over(config, active, &acknowledged)` is the same swap against
  the version a `Changed` refusal named, never a blind force: a further change is `Changed` again.
  Before the `rename` it writes the bytes being replaced to a new file created exclusively next to
  the config, `config.toml.bak` or, if that name is taken (a file or a symlink), `config.toml.bak.<n>`,
  so an existing backup is never replaced or written through. The backup gets the replaced file's
  owner permission bits plus owner read, never more access than the replaced file; it is synced
  before the rename, so an interruption leaves either the original file, or the original file and
  its backup, or the new file and its backup. An overwrite at a symlink is refused
  (`ConfigFileError::Symlink`): replacing the link loses it, writing through it edits a file
  outside the config directory. Ordinary saves never make a backup. (ADR-0015: the binaries'
  protected writes refuse a symlink for every write, and keep a generation of what each write
  replaces in the user's state directory, besides this `.bak`.)
- **Configurator.** A refused save from a clean editor shows the newer file if it resolves
  (selected theme included) and writes nothing; if it does not resolve, or was removed, nothing
  changes and the status says why. A dirty editor keeps its draft and offers `reload` (adopt the
  file only if it resolves; never writes a default over a removed file) and `overwrite`
  (`save_over`, keeping the backup and naming it). Adoption reads through a fresh `ConfigFile`, so
  bytes the user never accepted cannot authorize a later save.

## Limits

- Writers that take no lock (vim, a shell redirect, `cp`) are only checked against: the last
  comparison runs right before the `rename`, which leaves a window of microseconds in which
  their write can still be replaced. Closing it needs `renameat2(RENAME_EXCHANGE)` (libc and
  `unsafe`, R7). A non-renaming in-place write that lands after the `rename` modifies the new
  file and is seen by the next save as a newer version.
- `flock` on a directory is Linux behaviour (this is a Linux-only app); network filesystems may
  not honour it.
- Opening the directory for the lock emits no name-bearing inotify event, so `ConfigWatch` still
  reports one change per accepted save and none for a refused one.

## Considered options

- **Compare mtime/size/inode.** Cheap, but an in-place edit can keep size and restore the mtime,
  and an identical rewrite changes the inode without changing anything.
- **A lock file.** Leaves an artefact and a stale-lock problem; the directory fd needs neither.
- **Last writer wins, warn afterwards.** Loses the newer edits, which is the defect.
