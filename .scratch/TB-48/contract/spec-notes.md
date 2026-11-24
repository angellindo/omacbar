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

# TB-48 slice 2: spec notes (QA)

Base: `t3/tb48-slice1-qa-contract` @ f14be7c (slice 1 final PASS, 43 cases + 2 checks green).
Slice 1's contract, history, final record and frozen tests are unchanged; `support/generations.rs`
is reused as is and listed again among slice 2's frozen files (its bytes are already sealed by
slice 1; listing it pins that the slice 2 suites read the store with exactly these helpers).

## API and CLI the frozen tests pin (plan Seams, design §3-4)

- Binary `touchbar-recover`, a `[[bin]]` of tb-config, found at run time with
  `option_env!("CARGO_BIN_EXE_touchbar-recover")`. On a tree without it every CLI case compiles and
  fails on that `expect` (RED_BEHAVIOR, never COMPILE_ERROR): the setup before it (real protected
  saves through slice 1's library) runs and passes first.
- Reads `HOME` / `XDG_STATE_HOME` only (store = slice 1's `Backups::locate`); config =
  `default_path(HOME)`. Runs with `DISPLAY` / `WAYLAND_DISPLAY` removed and a bogus `VK_ICD_FILENAMES`.
- `list` (exit 0): stdout names the store path; one line per generation of this config holding the
  id as a word plus the words `<reason>` (kebab, e.g. `save`), `<kind>` (`file` | `absent`) and
  `verified` | `damaged`. No store / no config: exit 0, creates nothing.
- `verify <id>`: 0 when the generation is verified **and restorable into this profile's config**
  (stdout names the id); 1 otherwise (missing, damaged, linked in, another config's), stderr
  names the id. `verify` answers "can `restore <id>` use it here".
- `restore <id> [--remove-file]`: 0 done; 1 refused or failed (stderr says why: names the id, the
  config path for a symlink, the store path for an unwritable store, `os error 27` for EFBIG);
  2 usage; 3 conflict. Malformed ids and a missing `--remove-file` may answer 1 or 2 (never 0 or
  3), with stderr, and must change nothing. Usage cases pinned to exactly 2: no args, unknown
  subcommand, `restore`/`verify` without id, `list extra`, unknown flag, `--remove-file` alone,
  two ids.
- `kind`, `reason = "recover"`, `mode` (decimal or `0o`) are read from `<store>/<id>/manifest.toml`
  as slice 1 fixed it; data in `<store>/<id>/config.toml`.
- Library: `ConfigFile::look(&mut self) -> Result<_, ConfigFileError>`,
  `ConfigFile::recover(&mut self, &GenerationId) -> Result<_, ConfigFileError>`, `Reason::Recover`,
  `last_generation()` names the `recover` generation. Red on base: E0599 naming `look`/`recover`
  (symbol `recover`).

## Decisions made testable

- Every restore that writes keeps exactly one `recover` generation of what it replaced (file:
  exact bytes + bits; missing file: `Absent { dir_existed: true }`; missing dir:
  `Absent { dir_existed: false }`), newest in `list`, verified; earlier generations all remain.
- A refusal (bad id, damaged/foreign/linked generation, symlinked config, absent without
  `--remove-file`, conflict) keeps nothing and changes neither the config entry nor the store.
- `--remove-file` removes only the file, never its directory, after keeping it.
- Write failures: unwritable store → exit 1, config and store unchanged; unwritable config dir →
  exit 1, original entry identical and no temp beside it (a `recover` generation of the untouched
  file may remain); EFBIG under `prlimit --fsize=65536` with `SIGXFSZ` ignored → exit 1, original
  intact. A retry once fixed succeeds.
- Store under absolute `XDG_STATE_HOME`, else `HOME/.local/state` (unset, relative, empty).
  All test stores are outside the profile tree a restore replaces.

## Conflict window (QA-10)

Deterministic, no sleeps: the test holds `flock` on the store directory and on the config
directory (the locks every protected writer takes, ADR-0015 order store → directory), starts
`restore`, waits until `/proc/locks` lists the child's pid as a blocked waiter, replaces the file
(temp + rename, no lock), then releases. This pins the plan's "`restore` = `look`, then `recover`"
(the look is unlocked, before the writer's locks): an implementation that looks under the lock
would see the newer file and fail QA-10.

## Daemon reload (QA-27)

`touchbar-recover` cannot be located from a tb-daemon test (cargo gives `CARGO_BIN_EXE_*` only for
the package's own bins), so the running `touchbard --sim` (HOME + XDG_STATE_HOME temp) sees a
restore done through the same library `look` + `recover` the binary wraps. Needs the GPU like the
other `--sim` frame tests.

## No Rune / GPU (check `recover-graph`)

`.scratch/TB-48/contract/recover_graph_check.py` reads `cargo metadata` (offline, locked): tb-config
must have the `touchbar-recover` bin and its normal+build closure must not contain rune*, wgpu*,
ash, winit, naga, vello, tb-render, tb-app or tb-daemon. Red on base (no bin).

## Cross-slice note

Slice 1's `spawn-isolation` check runs every test target naming `CARGO_BIN_EXE_` (now also
`recover_terminal*` and `touchbard_recovery`) under a decoy `XDG_STATE_HOME`. Until slice 2 is
implemented those targets fail, so slice 1's check is red on intermediate trees; it is green
again once slice 2 is (each spawn passes its own `XDG_STATE_HOME`). This also guards the new
spawns' isolation. Re-run slice 1's gate after slice 2's implementation.

## Feasibility and red evidence

- Red on f14be7c (seal below): QA-01..23 RED_BEHAVIOR, each failing in `recover_bin()` after its
  setup (real protected saves) ran; QA-24..27 RED_MISSING_SYMBOL (E0599 `look`, `recover`,
  `Recover`); check `recover-graph` FAILED (no bin). Logs under `.scratch/TB-48/gate/`.
- Satisfiable: a throwaway ~100-line prototype (`look`, `recover`, `Reason::Recover`, a
  `--remove-file` path and the bin) built in a scratch copy outside the repo passed all 27 cases
  (QA run `.scratch/TB-48/qa/spec2-proto-*`); nothing of it is in this tree.
- Lint: `cargo clippy -D warnings` clean for both CLI suites here and for `config_recover` /
  `touchbard_recovery` against stub symbols in that copy; rustfmt checked by `--stamp`.
- Evidence history (gate-written, `slice-2.evidence.json`): seal `8f4bc486…` run 1 clean red (then a
  Clippy `many_single_char_names` fix in `recover_terminal.rs` re-sealed). Seal `7e6bf614…` run 1
  is **contaminated**: the prototype copy shared the cargo target `/tmp/tb48t`; cargo trusted its
  fresh dep-info and ran prototype-built `config_recover` / `touchbard_recovery` binaries, so
  QA-24..27 read GREEN on the base. QA then ran `cargo clean -p` for the five workspace crates in
  `/tmp/tb48t`, removed the copy and the stray `debug/touchbar-recover` (no dep-info names the
  copy), and run 2 under the same seal is the valid red: QA-01..23 RED_BEHAVIOR (23 × "tb-config
  has no `touchbar-recover` binary target"), QA-24..27 RED_MISSING_SYMBOL (E0599 `look`,
  `recover`, `Recover`), `recover-graph` FAILED (no bin), never_red empty
  (log `.scratch/TB-48/gate/2026-11-24T195942Z-slice-2`). Lesson: never point a scratch copy of
  the workspace at the shared target.
