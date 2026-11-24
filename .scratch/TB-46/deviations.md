# TB-46 deviations (implementer, slice 1)

- **Module split.** The plan names `rehearsal.py`, `rehearsal_profile.py` and `rehearsal_baseline.py`.
  To keep each Python module under ~300 lines (plan, doctor_* convention) the code is split by seam:
  `rehearsal_fs.py` (guards, hashes, manifest IO, no-follow walks, whole-file writes),
  `rehearsal_profile.py` (capture, settle, verify), `rehearsal_restore.py` (restore, cleanup),
  `rehearsal_run.py` (run), `rehearsal_baseline.py` (baseline, compare). The CLI seams, exit codes
  and file names the contract pins are unchanged; all match the fixture's `scripts/rehearsal*` copy.
- **Flow script saves by tapping `save`**, not Ctrl+S, like the existing
  `crates/tb-app/scripts/drag-f3-before-f1.json`; it clears the label with Backspace presses before
  typing `Rehearsal`. Key names (`row-f1`, `field-label`, `save`) match the configurator today; QA's
  functional run with the real binaries is what proves the flow.
- **Run-level ownership.** Each scenario's settle records the config state only; snapshots, trees,
  frames, the keys file, the mark, manifests and the final fixture files are owned at report level,
  each only after its content was checked against what its step must leave (PNG magic, parsable
  tree, exact F5 press/release records, a single `x`, fixture bytes). The report is the manifest
  `cleanup` takes; each store is removed only when its own scenario verified.

## Correction cycle 1 (review-1 BLOCKERs 1–4)

- **Run expectation source: producing save, not a twin profile.** The plan's QA decision allows
  "capture expected file identity from the operation that produced it". A twin private profile
  (replaying the save under a second HOME) was rejected because the sealed contract pins one HOME
  for every binary of a run (`Run.test_run_isolates…`, `…temp_dir_inside_the_caller_home…`:
  `len(homes) == 1`) and exactly one save per scenario in that profile (`Expectations` nominal
  case). `run` therefore takes the hash of what the save left the moment the configurator exits 0,
  before any other process of the run starts, and accepts it only if it is TOML holding the edited
  label; a captured link's target is expected as captured (from the capture manifest) unless the
  save wrote through the link. The simulator stage's expectation is `sha256(SIM)`, written by the
  run itself. Residual, documented in the runbook: a writer replacing the config between the save's
  exit and that read is taken for the save. The CLI `settle` never reads its own expectation.
- **Any failed scenario stops the run.** Continuing would let the next scenario's `prepare` write
  over bytes a conflict kept; the failing scenario's store and manifest are not owned, so `cleanup`
  keeps them.
- **Paths recorded resolved.** `capture` records the store by `realpath`, like the HOME, artifacts
  and link target already were; `cleanup` and `restore` reach them from `/` one component at a time
  with `O_NOFOLLOW`, so a symlink anywhere on the way means a swapped directory and the item is kept.
