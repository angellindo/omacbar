# TB-48 slice 1: spec notes (QA)

Base for design: `main` cce2d71 (TB-43). The checkout also carries TB-46 (26ca079) on purpose, so
the rehearsal tool is exercised as it stands; no TB-46 file is changed by this contract.

## API the frozen tests pin (from the plan's Seams)

- `tb_config::{Backups, Generation, GenerationId, GenerationKind, Health, Reason}`.
- `Backups::at(impl Into<PathBuf>)` (the `backups` directory itself; touches nothing),
  `Backups::locate(Option<&Path>, Option<&OsStr>) -> Option<PathBuf>`,
  `list(&self, &Path) -> Result<Vec<Generation>, _>` newest first (an absent store may list empty),
  `verify(&self, &GenerationId) -> Result<_, _>`.
- `Generation { id, reason, kind, mode: u32, len: u64, health, .. }`: `Debug + Clone + PartialEq`.
  `GenerationId: Display` = its directory name in the store. `Reason: Copy + Eq + Debug` with
  `FirstWrite`, `Save`, `Overwrite` (plus the plan's others). `GenerationKind::{File, Absent {
  dir_existed: bool }}`. `Health::{Verified, Damaged(_)}`.
- `ConfigFile::protected(path, Backups)`, `ConfigFile::last_generation()` (by value or by
  reference; the tests read both). `ConfigFileError::{Backup(_), Unsupported { .. }, Symlink { .. }}`;
  a refusal's `Display` names the config path (symlink, hard link) or the store path (unsafe store).

## On-disk layout read by the binary tests (ADR-0014 per plan)

`<store>/<id>/manifest.toml` (0600) with at least `kind = "absent" | "file"`, `dir-existed`
(absent), `reason` (`"save"`, ...), `mode` (decimal or `0o` integer); `<store>/<id>/config.toml`
(0600, exact bytes) only for `file`. Store 0700, generation dir 0700. Temp generation names
start with `.` (they are not generations).

## Decisions taken in the contract (from the plan, made testable)

- A protected save keeps the replaced file's permission bits; a new config is created 0600.
- Unsafe stores that must be refused before mutation: mode with group/other bits, a symlinked
  `backups` (also swapped in after generations exist), a store at or inside the config directory,
  a store path that is a file, an unwritable store (a retry after the fix succeeds and leaves no
  leftover in the store), no room for the copy (EFBIG under `prlimit --fsize`).
- A symlinked config directory or state parent: the outcome is free (save or refuse) but no
  version may be lost; the parent link is never replaced.
- A damaged generation is never `Verified`, never reused by dedupe, never deleted.
- Two daemons starting at once on one profile keep exactly one `absent` generation.

## Isolation proofs are runtime, not textual

- `scripts/tests/test_state_isolation_launcher.py`: fake windows resolve the store as the real
  binaries do; both must share one store inside `.scratch/manual-test` (persistent across
  relaunch), outside the config dir, with the caller's `XDG_STATE_HOME` decoy and HOME untouched
  (set, unset, relative, empty); the launcher prints the session store. How the launcher achieves
  it (setting `XDG_STATE_HOME`, unsetting it, ...) is free.
- `scripts/tests/test_state_isolation_rehearsal.py`: same for `rehearsal.py run` (already holds:
  it strips `XDG_*`); preserve guard.
- `.scratch/TB-48/contract/xdg_decoy_check.py`: runs every Rust test target that starts a product
  binary under `XDG_STATE_HOME=<decoy>`; fails if the decoy changes or a test fails. The plan's
  helper list (`tests/support/bin.rs`, `active_theme_snap.rs`, tb-daemon `touchbard_*`,
  `selected_theme_agreement.rs`, `doctor_config.rs`) must give their binaries a private
  `XDG_STATE_HOME` once the binaries honour it.

## Not in this contract (risks for review)

- CLAUDE.md "Look at it" commands and ADR/GLOSSARY text (slice 1 lists them; not runtime-testable
  here). `docs/` wording is left to review.
- Restore (slice 2), reset (slice 3), recovery UI (slice 4).
