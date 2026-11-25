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

# TB-48 slice 3: spec notes (QA)

Base: `t3/tb48-slice1-qa-contract` @ 5889053 (slices 1 and 2 final PASS). Seals 1 and 2, their
manifests, history and frozen files are unchanged. Slice 3 freezes two new suites and, read-only,
the shared helpers they use (`tb-config/tests/support/{generations,recovery}.rs`, already sealed by
slices 1-2; `tb-app/tests/support/{configurator,ui}.rs`, unchanged): the implementer has no need
to edit them for this slice.

## UI evidence: `crates/tb-app/tests/configurator_factory_reset.rs` (QA-01..08)

Real configurator, headless `rune::Runtime`, opened as startup opens it
(`State::open_protected(path, Backups::at(store), theme)`) on a hand-written config (comments,
mode 0640) in a temp profile under `CARGO_TARGET_TMPDIR`, own store. Keys pinned (plan Seams):
`reset-factory`, `reset-cancel`, `reset-notice`, existing `save`, Ctrl+S, `status`,
`dirty-summary`, `save-blocked-reason`, `field-*`, `row-*`, `tab-*`, `preview-<id>`,
`conflict-reload/overwrite`; scene `reset` in `tb_app::spec().scenes`.

- Draft only: `config.layout == Config::default_for(&state.theme).layout`, `config.theme` kept,
  preview draws the factory buttons, unsaved changes shown, `reset-cancel` offered; whole profile
  tree (bytes, mode, inode, temps, store) unchanged; no generation.
- `reset-notice`: says the source is kept (`kept|keep|keeps|retained|unchanged`) and names
  `omarchy`, or the full path of the selected theme file.
- Cancel: `config` equal to the pre-reset one (pending label edits included), shown layer `Fn`,
  selection `other`, `field-label`, `dirty-summary` and `save-blocked-reason` (an invalid width
  draft of another button) identical; that button's width text and another's label come back;
  `reset-cancel` gone; disk unchanged.
- Save and Ctrl+S: file parses to the factory layout with the same theme source and version, mode
  0640 kept; exactly one new generation, newest, manifest `reason = "factory-reset"` (read from
  disk, no Rust symbol pinned), kind file, verified, mode 0640, data = exact seeded bytes,
  `Backups::verify` ok; `status` contains `<store>/<id>` (as given or canonical), not "not saved";
  editor clean, no cancel left.
- Conflict (file replaced externally after the reset): Save and Ctrl+S → status "not saved …
  changed on disk", profile tree unchanged, no generation and nothing left in the store (an empty
  store directory created to take its lock is allowed), draft kept, both resolutions and
  `reset-cancel` offered; cancel then gives the original config, disk still the newer file.

Not pinned (free): where the controls sit, what layer/selection shows while a reset is pending,
whether a reset is offered while saving is blocked, the reason of an overwrite after a reset
conflict, the notice after cancel.

## CLI evidence: `crates/tb-config/tests/recover_terminal_validation.rs` (QA-09..13)

Regression of plan §4 ("after a restore it reports whether the restored file validates and that
the daemon keeps its last valid config otherwise"). Real `touchbar-recover` (slice 2 helpers:
`HOME=<dir>/home`, `XDG_STATE_HOME=<dir>/state`, no display, bogus Vulkan ICD). Generations are
seeded through the protected library (seed bytes, `load`, `save` over them). Every restore must
exit 0 with exact bytes and mode, add exactly one generation (the `recover` one, holding what it
replaced) and leave only `config.toml` in the config dir: the check never writes. Expected
diagnostics are what the library reports for the restored file without writing
(`ConfigFile::new(..).reload_resolved(ActiveTheme::under_home(home))`).

- Valid, Omarchy source: a stdout/stderr line with the whole word `valid` and `omarchy`.
- Valid, theme file: a line with the word `valid` and the theme file path; theme file untouched.
- Not TOML: restore still exit 0; output contains the first line of the library's issue
  (`the config is not valid: config is not valid TOML: TOML parse error at line …`) and names the
  daemon (`touchbard` or `daemon`).
- Missing selected theme file: restore still exit 0; output contains the library's theme error
  (`theme file `<path>`: No such file or directory (os error 2)`), the path, the daemon; the theme
  file and its directory are never created.
- Absence + `--remove-file` (preserve; green on 5889053): exit 0, file gone, directory kept and
  empty, only the `recover` generation added; a later `list` creates nothing.
- No output may claim a daemon is running (`is running`, `has reloaded`, `reloaded the`,
  `now draws`, `is drawing`). No hardware, no daemon started.

Check `recover-graph` (slice 2's script) is re-run: the validation must not pull Rune/GPU into
`touchbar-recover`.

## Runs, isolation and provenance

`.scratch/TB-48/slice3/launch.sh <label> <inner>`: slice 2's launcher with slice 3 names; same
shared lock and thermal guard (`slice2/thermal-command.py`: start ≤ 65 °C, pause ≥ 74 °C,
resume ≤ 64 °C, no periodic duty cycle), one CPU, nice 19, one job, one test thread; private
HOME and XDG_* per run; `CARGO_TARGET_DIR=/tmp/tb48t` → this checkout's `target/`.
`inner-red.sh` checks HEAD 5889053, the seal and the target link, runs `cargo clean -p tb-app
-p tb-config`, clippy `-D warnings` on both new targets, the gate, then
`target_provenance.py`: every executable the gate ran lies under `/tmp/tb48t`, was built during
this run, and tb-app / tb-config were compiled from this checkout (`Compiling` lines). The
clean is what makes this a proof: dep-info lists workspace sources by relative path, so a copy of
the workspace sharing the target (slice 2's contamination) cannot be told apart by dep-info alone.

## Red evidence (gate-written `slice-3.evidence.json`)

- Seal `b14a13a0…` run 1: tb-app suite COMPILE_ERROR (`tb_core::config::Layout` is private;
  now `tb_core::model::Layout`); CLI cases red as below. Not valid red for the UI part.
- Seal `0644cc14…` run 1: all 12 required red, but clippy rejected `map_unwrap_or` in the tb-app
  suite (fixed with `map_or_else`, no assertion changed).
- **Seal `70e1d531…` run 1 (valid):** HEAD 5889053, run `.scratch/TB-48/slice3/red-20261124T203113Z`,
  log `.scratch/TB-48/gate/2026-11-24T203119Z-slice-3`. QA-01..07 RED_BEHAVIOR at
  "missing public Runtime key reset-factory" after their setup (open accepted, edits, preview);
  QA-08 RED_BEHAVIOR "the configurator has a `reset` scene"; QA-09..12 RED_BEHAVIOR on the
  diagnostic only (exit 0, exact bytes and mode, one `recover` generation already hold on
  5889053; the output is the generic "touchbard reloads a valid config; an invalid one keeps the
  last valid design"); QA-13 GREEN (preserve); `recover-graph` GREEN; never_red empty. Clippy
  `-D warnings` clean on both targets; provenance proven (both executables built in this run from
  this checkout after `cargo clean -p`). Peak 80 °C, 16 thermal pauses (temperature only).

# TB-48 slice 4: spec notes (QA)

Base: `t3/tb48-slice1-qa-contract` @ fd7bc50 (slices 1-3 final PASS). Seals 1-3, their manifests,
history and frozen files are unchanged. Slice 4 freezes one new suite, one check script and,
read-only, `tb-app/tests/support/{configurator,ui}.rs` (already sealed by slice 3, unchanged).
The new suite names no `CARGO_BIN_EXE_`, so slice 1's `spawn-isolation` check does not pick it up.

## UI evidence: `crates/tb-app/tests/configurator_recovery.rs` (QA-01..14)

Real configurator, headless `rune::Runtime`, opened as startup opens it
(`State::open_protected(path, Backups::at(store), theme)`) in a temp profile under
`CARGO_TARGET_TMPDIR/configurator_history/<case>` (no path word is `restore`, `recover` or
`pending`, so text matches cannot come from a path). History is made by the real protected library:
each hand-written version (comments, mode 0640) is replaced by `ConfigFile::protected(..).save`,
so generation n holds version n exactly. Keys pinned (plan Seams): `backups-location`,
`recover-<id>` (`id` = `GenerationId` Display; tapping it restores), `recovery-more`,
`key-row-return`, existing `reset-factory`, `reset-cancel`, `save`, `status`, `dirty-summary`,
`row-*`, `field-label`; scene `recovery`.

- Location: `backups-location` text holds the store path (as given or canonical), fully visible.
- Listing: newest five of this config, newest first (top to bottom); the 6th+ and another config's
  generations (same store) are not listed; `recovery-more` has the number of the rest and
  `touchbar-recover list`; with exactly five, no `recovery-more`.
- Restore: exact bytes of the chosen (not the newest) generation, mode 0640; one new newest
  generation, `Reason::Recover`, file, verified, 0640, holding the replaced bytes; every earlier one
  still listed and verified; the editor shows the restored config, clean; `status` says `restored`
  (not `not restored`) and names `<store>/<new id>`; the new generation is listed at once.
- Restart: a reopened editor lists the generations and restoring the `recover` one undoes it.
- Factory reset: the generation kept by a UI reset+save is labelled `before factory reset`; a
  restore's own generation is not; restoring it brings back the personal file and layout.
- Refusals: with unsaved edits, and with a pending reset, a restore writes and keeps nothing,
  keeps the draft (and `reset-cancel`), and some element text names the restore and why
  (`restore`/`recover` + `save or cancel`/`unsaved`/`pending`). Hiding or disabling the control is
  allowed as long as that text is shown.
- Conflicts (TB-43): file replaced externally after open → file and store untouched, the clean
  editor shows the newer config, status `changed on disk`; file removed after open → nothing
  written or kept, status says `removed`.
- Damaged (hash mismatch) and absence generations: listed with why (`damaged`; `no file` /
  `absent` / `did not exist` / `no config`); tapping either changes nothing.
- A restored file that does not validate: exact bytes, a `recover` generation, status `restored`
  plus `issue` / `not valid` / `invalid`.
- Three things: `reset-factory` (says default/factory, not restore), `recover-<id>` (says restore,
  not "default buttons") and `key-row-return` (names `docs/hardware-runbook.md` and its section
  `Physical return to key mode`, says the editor does not / never does it, and that the layout and
  its backups/generations/copies stay/remain/are kept) never overlap; tapping `key-row-return`
  changes nothing. The runbook heading must exist in the tree (it comes from TB-46).
- Reach: at 1200x760, 1200x500, 900x600, 640x400 every new control is revealed by real wheel input
  and fully visible, and a restore tapped there works.
- Scene `recovery`: exists, writes nothing, the location, two rows and `key-row-return` reachable.

Not pinned (free): where the region sits, row wording beyond the words above, whether a refused
restore hides, disables or explains, what the editor shows after a removed-file conflict.

## End-to-end check `round-trip` (`.scratch/TB-48/contract/round_trip_check.py`)

Real binaries, built by the check (`cargo build --locked -p tb-app -p tb-daemon --bins`, paths
from cargo's JSON artifacts), one temp profile: `HOME` and every `XDG_*` inside it, `DISPLAY` and
`WAYLAND_DISPLAY` removed, `touchbard --sim <dir> --emit <file>` (no device). Seed: a hand-written
config with comments, one red (`#e03040`) button, mode 0640. Four configurator sessions
(`--script`, `--size 1200x1600` so script taps land on screen, `--tree` at `mark:end`, `--snap`):
reset+cancel (bytes, mode, inode unchanged, store empty, clean), reset+save (generation `1`,
`factory-reset`, exact seeded bytes; mode 0640 kept; status names `<store>/1`), reopen (factory
layout, clean, `recover-1` says `before factory reset`, location shown), tap `recover-1` (seeded
bytes and 0640 back, generation `2` `recover` holds the factory file, `1` untouched, status
`restored`). The daemon shares HOME and XDG_STATE_HOME: frame 1 has the red pixel; frame 2 equals
pixel for pixel (±2) the frame a second, fresh daemon draws for the exact saved bytes, and differs
from frame 1 in > 1000 pixels; frame 3 equals frame 1; stdin close → exit 0 with exactly three
frames; the store holds exactly `1` and `2`. `TB_ROUND_TRIP_DIR=<dir>` keeps everything locally.

Left for functional QA with real binaries: the windowed configurator on a real display (`cargo run
-p tb-app`, mouse and keyboard), `touchbard` on the physical bar (HIL, TB-9/TB-45), and the
physical return to the key row (human runbook; never run by an agent).

## Runs, isolation and provenance

`.scratch/TB-48/slice4/launch.sh <label> <inner>`: slice 3's launcher with slice 4 names (same
shared lock and thermal guard `slice2/thermal-command.py`: start ≤ 65 °C, pause ≥ 74 °C, resume
≤ 64 °C, no duty cycle; one CPU, nice 19, one job, one test thread; private HOME/XDG_*;
`CARGO_TARGET_DIR=/tmp/tb48t` → this checkout's `target/`). `inner-red.sh` checks HEAD fd7bc50 and
the seal, `cargo clean -p tb-app -p tb-config -p tb-daemon`, clippy `-D warnings` on the new
target, the gate, then `slice4/target_provenance.py` (slice 3's, plus the round trip's own build:
binaries under `/tmp/tb48t`, built in this run, tb-app and tb-daemon compiled from this checkout).

Probe before sealing (fd7bc50, `slice4/probe-20261124T210548Z`): 14 cases failed on missing
recovery keys/scene after their setup; the round trip passed start, reset-cancel and reset-save
(frame 2 equal to the reference daemon's frame) and failed at `reopen: generation 1 is not listed
as recover-1`. A Clippy `assert_is_empty` was fixed before the seal (no assertion weakened).

## Red evidence (gate-written `slice-4.evidence.json`)

**Seal `4402224f…` run 1 (valid):** HEAD fd7bc50, run `.scratch/TB-48/slice4/red-20261124T210659Z`,
log `.scratch/TB-48/gate/2026-11-24T211123Z-slice-4`. QA-01..14 RED_BEHAVIOR, each after its setup
(protected history written, editor opened, edits / reset / external change done): missing
`backups-location` / `recover-<id>` keys or their text, no `recovery` scene, no restore
explanation while dirty. Check `round-trip` FAILED at `reopen: generation 1 is not listed as
recover-1` after `start`, `reset-cancel` and `reset-save` passed (frame 2 equal to a fresh daemon's
frame of the saved file). never_red empty. Clippy `-D warnings` clean on the new target;
provenance proven after `cargo clean -p tb-app -p tb-config -p tb-daemon` (test executable, and
both round-trip binaries, built in this run from this checkout). Peak 71 °C, no thermal pause
(the run waited ~4 min for the shared lock held by another ticket's job).

# Integration amendment of slice 1 (TB-46 + TB-48, candidate b0de64b)

Seal `sha256:0f6f66b3…`, its manifest, evidence, these notes, and the final records of slices 1–4
(PASS on b1ce152) are archived byte for byte in `.scratch/TB-48/qa/integration-respec/archive/`
(`SHA256SUMS`). Those finals stay valid for b1ce152. New slice 1 seal: `sha256:8e999ff4…`.
Slices 2–4 are not re-sealed: manifests, frozen files and seals unchanged.

Superseded, and only this: `scripts/tests/test_state_isolation_rehearsal.py` (check `launcher-state`).
Its fake configurator wrote a default and returned an empty tree. With TB-46's receipts, the
integrated `run` stops at the first save without a label or a receipt, before `touchbard` starts,
so the check could not see the daemon at all. The double is now a faithful configurator and daemon:
a generation before every config write, `TB_SAVE_RECEIPT` for each confirmed save, an explicit
symlink refusal with no receipt, F5 / frame / mark from the daemon. The run must pass by its
ordinary route; the daemon must run exactly once, in the simulator stage, on the run's regular
config, with its keys file and mark in the profile; every binary gets the profile as HOME and no
`XDG_STATE_HOME` (the caller's decoy is never handed down); the generations it kept are on disk
(0700 / 0600) in `<profile home>/.local/state/touchbar-customizer/backups` and nowhere else; the
decoy and the caller's HOME stay unchanged. The 43 cargo cases, `spawn-isolation`, the launcher
tests and every other frozen file are byte-identical; the manifest changed only in `reason`.

A new seal needs a new red run: it ran under the gate on f3d8ec2 (the slice 1 contract commit
before the implementation, the same base as the 0f6f66b3 red run), with the sealed frozen files
overlaid, in an isolated checkout with its own target. Results and green: `.scratch/TB-48/integration/qa-amend/RECEIPT.md`.

## Environment changes in theme-fixture tests (scope only)

TB-48 added `.env_remove("XDG_STATE_HOME")` (with a one-line comment) to the spawn sites in
`crates/tb-app/tests/{active_theme_snap.rs,support/bin.rs}` and
`crates/tb-daemon/tests/{selected_theme_agreement.rs,touchbard_theme.rs,touchbard_theme_live.rs}`.
Scope: the binary started by the test keeps its generations under the test's temp HOME instead of
the caller's state directory. It changes no input, theme fixture or expectation. In the
integration, `git diff 3285da1 b0de64b` over those files plus `configurator_selected_theme.rs`
removes 0 lines and adds 7 `env_remove` lines. Against b1ce152, the only removals (including two
`assert` lines) are exactly TB-46's own changes already on 3285da1. No assertion was dropped.
