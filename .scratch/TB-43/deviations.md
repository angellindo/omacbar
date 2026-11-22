# TB-43 deviations (implementer, slice 1)

1. **Backup naming follows the addendum, not the plan body.** Plan §3 said `<name>.bak`, "same mode
   as the replaced file". The integration ruling (never replace an existing backup, private, no
   broader access) wins: `config.toml.bak`, or the first free `config.toml.bak.<n>`, created
   exclusively (O_EXCL never follows a symlink); mode = replaced file's owner bits | owner read.
   `ConfigFile::backup()` exposes the path so the configurator status names it (QA-59).
2. **Overwrite at a symlink is refused** with a new `ConfigFileError::Symlink { path }` (ruling:
   refuse unless link + target replacement is reversible). Ordinary non-stale saves at a symlink
   keep the existing behaviour (rename replaces the link), per plan Out of scope.
3. **`save_over` resolves the selected theme** (like `save_resolved`) and returns `()`, as the
   sealed tests require (`Result<(), _>`); the configurator keeps its current theme after an
   overwrite instead of adopting a freshly resolved one.
4. **Adoption (clean stale save, conflict reload) reads through a fresh `ConfigFile`** and swaps it
   in only on success, so bytes the user never accepted (invalid, theme failure, removal) are never
   recorded as the editor's seen version and cannot authorize a later save. A clean editor whose
   adoption fails also gets the conflict controls (status says "nothing saved").
5. **Active theme on adoption.** The configurator has no separate "active Omarchy theme" field, so
   adoption resolves an Omarchy-following newer file against `State::theme` (the same value TB-42's
   save path passes). If the editor itself followed a theme file, that file's theme stands in for
   the active one until reopen. Pre-existing TB-42 limitation, not widened.
6. **`reload_resolved` records "absent"** when the file is missing (a read of absence), still
   returning the NotFound I/O error and never writing a default.
7. **Tests that saved through a never-loaded `ConfigFile` over an existing file:** none outside the
   contract needed a change (`configurator_command_file` saves into an empty dir; daemon/doctor
   only read). Added non-frozen `crates/tb-config/tests/config_backup.rs` (read-only source →
   0o400 backup; taken names skipped in order).
