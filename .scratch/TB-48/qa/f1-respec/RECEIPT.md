# QA amendment: TB-48 slice 4, reading at every window size (F1), branch t3/integrate-tb47-tb48

Base 3a8f63a (nine finals and the rehearsal PASS, archived in `integration/combined-3a8f63a`).
Diff: `crates/tb-app/tests/configurator_recovery.rs`, new `crates/tb-app/tests/support/reading.rs`,
slice 4's manifest, seal, evidence and spec notes, and this directory. Product, `support/configurator.rs`
(frozen also by TB-40/42/43, TB-48 s1/s3 and TB-49), every other contract, seal, evidence and final record
are unchanged (`before-all-contracts.SHA256SUMS`, checked before the commit). Design: `contract/spec-notes.md`,
last section.

## Seals

| | Seal | State |
|---|---|---|
| Archived | 6942a867 (final PASS at 3a8f63a, 19 runs) | `archive/` by hash, `SHA256SUMS`, `ARCHIVED-FROM` |
| Superseded | a91e95b4 | QA-18..22 used `should_panic`, which the gate reads as NOT_RUN |
| Current | 111e976d | QA-15/16 `required`, QA-01..14 and QA-17..22 `preserve` |

The evidence history keeps the 19 archived runs as a strict prefix.

## Runs (gate-written evidence; raw logs local, not committed: they carry absolute paths)

Environment: `qa-env-long-tmp.sh` = the integration `qa-env.sh` plus `--bind /tmp /tmp` (the two tb-config
regressions that need a foreign-owned `/tmp`). Thermal wrapper 65/74/64 °C, one CPU/job/test thread, nice 19,
bwrap with a disposable `/dev` (no dri/input/uinput), Rust 1.99.1 called directly, private HOME/XDG/TMPDIR,
offline private Cargo home, software Vulkan. Warm target at its real long path
`<worktree>/target/tb47-tb48-directed/target` (no `/run/t` alias); `CARGO_TARGET_TMPDIR` in the test binary
= that path + `/tmp` (checked with `strings`), so QA-13's own store path is 179 bytes.

1. Old seal 6942a867 at 3a8f63a: QA-13 RED_BEHAVIOR ("backups-location remains clipped": 262.5 points in a
   283-point sidebar, visible 229), everything else GREEN, `round-trip` GREEN.
2. New seal on 25230e9 (clone with the sealed frozen files overlaid: 1 modified, 4 new; `support/configurator.rs`
   and `ui.rs` byte-identical there; reflinked copy of the warm target, `cargo clean -p` of the 5 workspace
   crates): QA-15/16 RED_BEHAVIOR for the documented reason (status says "not restored: config changed…" with no
   retained id), all other cases and `round-trip` GREEN. never_red empty.
3. Final at the amendment commit: see below.

## Geometry (private probe, `geometry.json`; 640x400)

| store bytes | text height | sidebar | margin | old `reveal` | new reading |
|---|---|---|---|---|---|
| 164 | 245 | 283 | 38 | pass | 1 view |
| 179 | 262.5 | 283 | 20.5 | panics | 1 view |
| 212 (x-padded) | 280 | 283 | 3 | panics | 1 view |
| 256 | 315 | 283 | -32 | panics | 2 views, bands cover 0..315 |

Other sizes: every length fits in one view (margins 103..398). Negatives (QA-18..22) fail with their own reason.
