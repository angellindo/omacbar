# TB-52 QA spec · slice 1 (behavior)

Manifest `contract/slice-1.json`, seal `sha256:9f7a9e062c13…` (red run 1 under this seal, HEAD `b03eb73`, Rune `P` = `f73f72c`).

## Acceptance → evidence
| Acceptance | Contract |
|---|---|
| 1 public pinned sources, versioned lock, no prototype paths | check `pin` = `scripts/tests/test_rune_pin.py` (also runs in `scripts/check.sh`). The clean checkout, empty cache and `--locked` build are R1 (implementer); see the plan |
| 2 consumer can use `hover_tracks_bounds`; simulator unchanged | QA-01, QA-02 (required, symbol `hover_tracks_bounds`); QA-03 control without the opt-in (preserve, own target); QA-04..06 simulator (preserve) |
| 3 full `check.sh` + simulated screen; snapshots | `--final` runs `check.sh`. Snapshot comparison is functional QA |
| 4 manifest/lock documented; isolated revert | implementer `pin-change.md`; the revert is functional QA (the `pin` check goes red again) |

## Red at P (run 1)
- QA-01, QA-02: `RED_MISSING_SYMBOL`. One compile error: `E0599 no method named hover_tracks_bounds found for struct El`.
- QA-03 (control) GREEN at P, compiled and run in its own target `pointer_leave_retention`, so the missing symbol in `pointer_leave_tracking` does not touch it.
- QA-04..06 GREEN.
- `pin` FAILED. Exactly 2 of 17 tests fail, and both are about the rev or source:
  - `rune rev is 'f73f72c…', expected '22ebd7e…'`
  - `rune resolves from git+…rune?rev=f73f72c…#f73f72c…, expected git+…rune?rev=22ebd7e…#22ebd7e…` (plus the same for `rune-motion`, `rune-staging`, `rune-taffy` and the other rune packages)

  The 13 fixture tests (prototype path patch, absolute path, `[patch."<rune git>"]` override, other rev, sourceless non-member, `[[patch.unused]]`, declared patch not in lock) and the patch, sourceless-package and `.cargo/config` rules hold at P.

## Scope of the pin check (not global policy)
The check covers only the `rune` dependency, the `[patch.*]` tables (each must be `{ git = rune, rev = 22ebd7e }`, crates-io only, names within Rune's 8 vendored crates), the lock packages whose source is the Rune repo (exact `?rev=H#H`), sourceless lock packages (workspace members only), `[[patch.unused]]` and `.cargo/config*`. It does not restrict other registry or git dependencies, and it does not require any patch to be present.

## Pre-seal throwaway probe at H (not contract evidence)
Main tree `b03eb73` + the two Rune test files + TB-51's disposable `app-head` Cargo.toml/lock (Rune `22ebd7e` from TB-51's fetched source by path), offline, in a pruned reflink copy of TB-51 `target-head`. Result: QA-01, QA-02, QA-03 pass at H.

Cargo warned that 3 of the 8 patches were not used in the graph: `accesskit_atspi_common`, `accesskit_macos`, `accesskit_windows`. Under the pin check an unused patch must not remain declared (`[[patch.unused]]` is rejected), so the implementer drops whichever patches are unused, as the plan says.

## History
- Seal 1 `sha256:ef13d905…` was never run as red. The H probe found that the test passed `f32` to `Runtime::frame` (expects `f64`, E0308 at H). `STEP` became `f64` in both Rune files and the contract was re-sealed. No evidence existed under seal 1.

## Run profile
Shared lock via TB-46 `thermal-command.py` (sha256 `641b6ba4…`; ≤65 / ≥74 / ≤64 °C, no duty cycle, one CPU, nice 19, jobs = tests = 1). `env -i` with Rust 1.99.1 toolchain `bin/` first in `PATH`; private HOME/XDG*/target.

The P target is a reflink of the warm TB-46 target, keeping third-party artifacts only (no `tb-*` artifact, no executable). Only `tb-*` compiled.

Following this project's QA precedent, `CARGO_HOME` was the user's registry, offline (`CARGO_NET_OFFLINE=true`, `--locked`). The H probe used TB-51's disposable `cargo-home-head`. Logs are kept locally and are not committed: `qa-spec/run/`, `qa-spec/run-1-probe-fail/` and `gate/2026-11-25T001605Z-slice-1/`.
