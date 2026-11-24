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
