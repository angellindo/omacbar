# QA amendment: TB-47 slice 3, caller XDG state isolation (branch t3/integrate-tb47-tb48)

Integration merge `987a398` (parents `204663a` TB-47/main and `354da2e` TB-48). QA work only: no
product file, `devices_tests`, other frozen test, hardware, tracker or push. Finding: in the
directed run `target/tb47-tb48-directed/run.log` (local), the HUP/INT/TERM starts of
`crates/tb-daemon/tests/touchbard_hardware.rs` got a private HOME but inherited the caller's
`XDG_STATE_HOME` and kept 3 absent generations in the caller's (decoy) state.

## What changed

| File | Change |
|---|---|
| `crates/tb-daemon/tests/touchbard_hardware.rs` | Both spawn sites (`run_in`, the signal test) `.env_remove("XDG_STATE_HOME")`, as TB-48 did for the other `touchbard_*` tests; the signal test keeps each round's HOME explicit and, through `private_first_writes`, prints the absent first-write generations kept there and requires exactly one when `TB_TEST_REQUIRE_PRIVATE_GENERATION` is set. Every earlier assertion is unchanged; nothing is required without the variable (a daemon without a generation store keeps none). |
| `.scratch/TB-47/contract/xdg_decoy_check.py` (new, frozen) | Check `xdg-decoy`: runs `--test touchbard_hardware` and `--bin touchbard` (with `tests::hardware`, `devices_tests` and their test-binary children) each against its own decoy `XDG_STATE_HOME`; fails on any decoy change, a failed test, or too little proof (process path: both tests and the three `SIGHUP/INT/TERM: 1` reports; in-process path: passing `tests::hardware::` and `devices_tests::` cases). Deletes nothing outside its decoys and pairs no shared temp dirs. |
| `.scratch/TB-47/contract/slice-3.{json,seal}` | `files` + the check, `reason` extended. Seal `ebfd3c65…` → `9e7eea76421ba6f9110009df97f1bbd3d55b876f4576c875d27cce9901e7c3bf`. Superseded drafts: `c145fc9b…` (never run), `29420787…` (one base run, kept in the history; replaced for Clippy `doc_markdown`/`print_stdout` in the test). |

Scope of the TB-48 guard (`.scratch/TB-48/contract/xdg_decoy_check.py`, TB-48 s1, unchanged): it
selects integration test files naming `CARGO_BIN_EXE_`, so it covers `touchbard_hardware` (it fails
at 987a398 for the 3 generations above) but not the bin unit tests in `src/main/`. The new TB-47
check covers both. TB-48 s1 was not amended, so its 8 required cases keep their red evidence.

## Red and green per path (all through the shared thermal wrapper)

Environment: `target/tb47-qa-isolation/qa-env.sh` (local): canonical `thermal-command.py`
(sha256 `641b6ba4…`, shared lock, start ≤65 °C, pause ≥74 °C until ≤64 °C, no duty cycle, one CPU,
nice 19, one job, one test thread), `bwrap` with `/` read-only and only this checkout writable,
disposable `/dev` (no dri/input/uinput), Rust 1.99.1 called directly, private HOME/XDG/TMPDIR per
call, offline private Cargo home, software Vulkan. Warm target `target/tb47-tb48-directed/target`
reused; provenance: `touchbard` `9ae78d38…`, `touchbar-customizer` `5a442b67…`,
`touchbar-recover` `4a5e42eb…` equal to the directed run's hashes at 987a398.

| Run | Tree | Process path (`touchbard_hardware`) | In-process path (`--bin touchbard`) |
|---|---|---|---|
| Red probe (`probe_xdg_isolation.py`, report `red-987a398-probe.json`) | 987a398, crates clean | 2/2 tests ok; decoy gained generations 1–3 (all absent); the 3 rounds showed their first frame, their HOMEs held 0 | not run |
| Check, seal 9e7eea76 | 987a398 + this amendment | PASS: decoy unchanged, `SIGHUP/INT/TERM: 1` each in the round's own HOME | RED: 27 tests ok, decoy gained 6 absent generations (product reads `XDG_STATE_HOME` inside `ConfigState::start`) |
| Check ×2 (1 and 2 test threads), Clippy `-p tb-daemon --all-targets -D warnings`, fmt | `3174c1e` (Sol) + this amendment | PASS both thread counts | PASS both thread counts: decoy unchanged |
| Gate, seal 9e7eea76 (also 29420787) | 5ce810b + sealed files overlaid | QA-315 `RED_BEHAVIOR` (usage error, no `--drm`); QA-326..330 GREEN | QA-301..314 `RED_MISSING_SYMBOL` (`run_hardware`), QA-320..325 COMPILE_ERROR (preserve, same as the original red); `xdg-decoy` FAILED (fails closed) |

seen_red under 9e7eea76: QA-301..QA-315 (all 15 required); never_red empty. The base runs were made
in a detached clone of 5ce810b (`target/tb47-qa-isolation/base-5ce810b`, own reflinked target,
workspace crates cleaned) whose evidence started as this branch's; the result (branch runs + the two
base runs, checked prefix-equal) was copied back. Their gate logs are copied to
`.scratch/TB-47/gate/` (ignored, local).

So the in-process path needs Sol's product correction: TB-47 s3 `--final` cannot pass on this
commit alone, and passes its check on the combination with `3174c1e`.

## Histories imported (verified point-in-time copies)

`archive/` holds, with `SHA256SUMS` and `ARCHIVED-FROM`: this branch's committed evidence at 987a398
(TB-46 s1, TB-47 s1–4 + s3 manifest and seal, TB-48 s1–4) and the parents' working-tree evidence,
finals and `.ack` (TB-47 from 204663a; TB-46 and TB-48 from 354da2e, copied while its QA ran). Each
branch history was a strict prefix of the parent's; the live evidence now holds the parent's copy
(TB-46 s1 20→25 runs, TB-47 s1 1→11, s2 1→10, s3 1→6 (+2 base runs = 8), s4 1→6, TB-48 s1 27→28,
TB-48 s2–4 unchanged). `.ack` for TB-46/47/48 copied to the live (untracked) intake records.
`verify_seals.py` recomputes the nine seals with the gate's `seal_of`: all intact, never_red empty.

## Not done here (by request)

The nine finals (TB-46 s1, TB-47 s1–4, TB-48 s1–4), the TB-46 rehearsal (five scenarios + cleanup,
six generations) and the TB-48 round trip run once, on the SHA that combines this commit with
`3174c1e`. Local logs (private paths) are listed in `local-logs.sha256`; sanitized lines in
`runs-summary.txt`. TB-52 untouched.
