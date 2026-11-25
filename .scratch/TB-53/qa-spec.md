# TB-53 · QA spec, slice 1

Mode `behavior`. Base and HEAD at the red run: `bc3e697` (TB-52 candidate, Rune `22ebd7e`).

## Frozen files (existing tests, unchanged)
| File | git blob = `bc3e697` blob | sha256 |
|---|---|---|
| `crates/tb-daemon/tests/gpu_renderer_frames.rs` | `f0c17234b831b123412e5ee3211110e39708b55d` | `f7e94768061f9e39bcc3d5d339804eaebe28b1ee1a175339c3f8dbaf74216e78` |
| `crates/tb-render/tests/renderer_frames.rs` | `782c75ae580887de477e12c74416937103709fd8` | `6834d25784d88fd446dd7845834a56b4c4b6d2463a0538cdbdfbbce900003adc` |

No new test file and no edit: `git diff bc3e697 HEAD -- crates` is empty at the contract commit.
Seal: `sha256:a760403f9f387477d19aa47a4a5b7f5a5fc2c589c2e4e4b4dce7046441dd86e6`.

## Cases
| Id | Case | Red | Observed at `bc3e697` |
|---|---|---|---|
| QA-01 | `tb-daemon` `gpu_renderer_frames` · `the_gpu_renderer_draws_successive_scenes_byte_identical_to_render_frame` | required | `RED_BEHAVIOR`: `fn: the GPU renderer's frame differs from render_frame` (`gpu_renderer_frames.rs:39:9`) |
| QA-02 | `tb-render` `renderer_frames` · `one_renderer_draws_successive_scenes_byte_identical_to_render_frame` | required | `RED_BEHAVIOR`: `fn: kept renderer differs from render_frame in 6 bytes` (`renderer_frames.rs:53:13`) |
| QA-03 | `tb-render` `renderer_frames` · `a_scene_of_another_size_gets_a_frame_of_its_size` | preserve | `GREEN` |

`checks: []`. The latency and render-speed tests stay in `scripts/check.sh`, which `--final` runs.

## How the required/preserve split was decided
The plan predicted QA-02's red but it had never run at `22ebd7e`. A probe ran before the seal, in
the same profile as the red run. Every case gave the same result in all three runs:
- `renderer_frames` 3/3: QA-02 red with `fn … 6 bytes`, QA-03 green.
- `gpu_renderer_frames` 3/3: QA-01 red with `fn: …`.

QA-02 is therefore `required` and QA-03 is `preserve`. The probe ran on 2026-11-25 from 01:32:04Z to
01:32:34Z: peak 71 °C, no thermal pause.

## Gate red run (Acceptance 1: the failure before the change)
`scripts/pipeline-gate.py TB-53 --slice 1` ran at 2026-11-25T01:33:03Z, run 1 under this seal, and
exited 1 (not green, as expected). QA-01 and QA-02 were `RED_BEHAVIOR` and QA-03 was `GREEN`, so
`seen_red` = QA-01, QA-02 and `never_red` is empty. Evidence is in `contract/slice-1.evidence.json`.
Peak 66 °C, no thermal pause.

What fails, from the TB-52 pixel list (`.scratch/TB-52/qa-render-diagnosis.md`), observed on the
Intel HD 630 with Mesa 26.2.5:
- **Fn** after a first frame: 3 px in `next`, `(1360,31) (1360,33) (1360,34)`, on the stem of the
  bar's first `n`. These are the 6 bytes QA-02 reports: 3 px × ±1 in G and B.
- **Fn+Ctrl** after Fn: 4 px on the stem of the bar's first `h`, at `(620,31) (620,34)` in `theme`
  and `(1142,31) (1142,34)` in `shot`, also ±1 in G and B. Both frozen sequences reach Fn+Ctrl only
  after Fn, so these letters are first seen after the first frame. The tests stop at the first
  differing frame (`fn`), so after the fix they also cover `fn-ctrl` and the later Default frames.
- **Default** as the first frame is identical. A glyph that is already cached never differs again.

TB-52 receipts, cited and not re-run: frozen `gpu_renderer_frames` was red 3/3 at `22ebd7e` and
green 3/3 at `f73f72c` (`.scratch/TB-52/r2/logs/{h-frozen,p-frozen-1,p-frozen-2,p-frozen-3}.log`,
`.scratch/TB-52/qa-snapshots.md`).

## Run profile (probe, stamp and red run)
- The probe and the red run each took one acquisition of the shared lock
  `.scratch/overnight-command.lock` through the TB-46 `thermal-command.py` (sha256 `641b6ba4…0396`).
  No duty cycle; CPU 7, nice 19, `uclampset -M 128`, `CARGO_BUILD_JOBS=1`, `RUST_TEST_THREADS=1`,
  `LP_NUM_THREADS=1`, `RUSTUP_TOOLCHAIN=1.99.1`.
- Every step ran under `env -i`. `PATH` put the Rust 1.99.1 toolchain `bin/` first (`cargo 1.99.1
  (5f94df478 2026-08-27)`, `rustc 1.99.1 (58cf8f9c3 2026-10-13)`). Each step had its own private
  HOME and `XDG_*` directories (0700), and used `CARGO_NET_OFFLINE=true` and `--locked`.
- `CARGO_HOME` was TB-52's `clean-proof/cargo`, at the same path, read-only in use. The target was
  `.scratch/TB-53/target-h`, a `cp --reflink=always` copy of TB-52's `clean-proof/target`. That cache
  was built from a clone at `bc3e697`, and its original still measures 5.3 G and is untouched.
- Workspace provenance, even with the cache warm: `cargo clean -p` removed the five workspace
  crates from the copy before any test. The build then recompiled `tb-core`, `tb-render`,
  `tb-config` and `tb-daemon` from this worktree's `crates/` (in 20 s). Dependencies stayed fresh.
- No hardware, `/dev`, udev, systemd or host changes.

Local receipts (not committed, because they hold absolute host paths) live in
`.scratch/TB-53/qa-spec/`: `receipt-pre.txt`, `probe.sh`, `red.sh`, `profile.sh`, `steps.log`,
`logs/*`, `thermal-*.log`. The gate logs are in `.scratch/TB-53/gate/2026-11-25T013300Z-slice-1/`.

## For the implementer
Use the same profile and target (`.scratch/TB-53/target-h`, `CARGO_HOME` from TB-52's
`clean-proof/cargo`). Change only `crates/tb-render/src/offscreen.rs`, in its own commit. QA does
not run `--final` and did not apply E3.
