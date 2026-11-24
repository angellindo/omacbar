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

## Repair 1 (2026-11-24): size-limit cases QA-42 / QA-43

The first seal capped the whole configurator binary with `prlimit --fsize=65536` from `exec`.
Mesa `anv` (Intel) cannot create a device below RLIMIT_FSIZE = 1 GiB, so both runs stopped at
"no GPU device" before the Save: QA-42 failed, and QA-43 passed without ever reaching the Save.
A limit applied to the live pid after GPU init was timing-based, so it was not adopted.

Now both cases run the same configurator headless (`rune::Runtime`, no GPU) through its
production entry `tb_app::spec().init` (reads `HOME` / `XDG_STATE_HOME` once, as the binary
does), inside a child process: this test binary again, running only the `#[ignore]`d helper
`size_limited_editor_saves_once` (inert unless `CONFIGURATOR_GENERATIONS_SIZE_LIMITED_CHILD` is
set), under `sh -c 'trap "" XFSZ; exec prlimit --fsize=65536:65536 -- …'`. The cap is in force
from `exec`, so there is nothing to synchronize. Both cases use the same profile layout, the same
limit (soft and hard), the same drag (F3 by -190 pt, as `scripts/drag-f3-before-f1.json`) and a tap
on `save`. Only the seeded file's size differs (control under 16 KiB vs > 256 KiB, mode 0600 vs 0640).

Positive evidence the parent requires from the child's report and the disk:
- the cap: `/proc/self/limits` soft and hard = 65536, `SIGXFSZ` in `SigIgn`;
- the editor opened the seeded file, the drag changed the layout, the dirty summary showed pending
  changes, Save was tapped, and the `status` text changed;
- room: status `saved config <path>`, no pending changes, the file on disk parses to the editor's
  layout, exactly one `file` generation holding the exact seeded bytes (0600), reason `save`;
- no room: the new layout serializes to < 16 KiB (so the only write over the cap is the copy of
  the > 256 KiB original), status `not saved <path>…` containing `os error 27` (EFBIG), the draft
  still pending, the file byte-identical with mode 0640, and no `file` generation listed.

The full-binary cases QA-40 / QA-41 are unchanged (they need a GPU, no cap).

## Repair 2 (2026-11-24): report transport

With `--nocapture` libtest prints `test size_limited_editor_saves_once ... ` without a newline,
so the helper's first report line (`fsize-soft`) did not start with the prefix and the parent
failed with "no `fsize-soft` in the report" on both trees, before judging the save. The helper
now prints one empty line before its first report line. No assertion, key or control changed.

## Repair 3 (2026-11-24): lint only

The `--final` run at 94f633f had all 43 cases and both checks green, but `scripts/check.sh` failed:
Clippy (`-D warnings`) rejected five lints in `crates/tb-app/tests/configurator_generations.rs`
(`doc_markdown` on `RLIMIT_FSIZE`, `print_stdout` three times, `map_unwrap_or` in `Report::get`).
The source now backticks `RLIMIT_FSIZE`, writes with `writeln!` on a locked `std::io::stdout()`
handle instead of `println!`, and reads a report field with `map_or_else`. The child's line protocol
is unchanged (one empty line, then `size-limited-editor: key=value` lines).

A first attempt also printed the parent's copy of the child's report with `writeln!` on
`std::io::stdout()`. That handle bypasses libtest's capture, so the copy landed in the middle of
`test <name> ... FAILED` and the gate classified QA-42 and QA-43 as NOT_RUN on the baseline (both
had failed on the expected assertions). The parent now writes nothing to its own stdout or stderr:
it keeps the child's whole stdout in the case's profile dir as `size-limited-editor.stdout`
(`<CARGO_TARGET_TMPDIR>/configurator_generations/<case>/`), and every report assertion still
carries the raw report in its captured failure message. No assertion, key, control or lint level
changed; no `#[allow]` added.
