# Sealed baseline red contract

Production baseline: `8eee25d02473d79ae2c090c28820734c1bb9a97e` on `tb-40-variant-b`.
Kernel release: orchestrator receipt at 2026-11-12 08:15 UTC.

| Slice | SHA-256 prefix | Required red | Preserve green | Never red | Evidence logs |
|---|---|---:|---:|---|---|
| 1 | `50655e2c58cd` | 7 | 2 | none | `.scratch/TB-40/gate/2026-11-12T081826Z-slice-1` |
| 2 | `9defec0faa94` | 7 | 1 | none | `.scratch/TB-40/gate/2026-11-12T081834Z-slice-2` |
| 3 | `c4cd4d4deed1` | 12 | 3 | none | `.scratch/TB-40/gate/2026-11-12T081842Z-slice-3` |
| 4 | `a8ed62039ce4` | 11 | 1 | none | `.scratch/TB-40/gate/2026-11-12T081857Z-slice-4` |

All 37 required cases were observed genuinely red under the current seals: 36 RED_BEHAVIOR and one RED_MISSING_SYMBOL (`set_save_clock`, E0599 only). All seven preservation cases are GREEN. No COMPILE_ERROR, RED_STUB, NOT_RUN or SKIPPED. The gate's exit 1 is the expected baseline red result. Earlier pre-lint seals/runs remain in generated history; only current-seal evidence counts.

Targeted baseline compilation passed for ordinary app targets and core command_validation. Targeted clippy with `-D warnings` passed for those same targets. The isolated clock target cannot compile until the explicitly planned setter exists; no alternate test harness failure was reported.

Commands ran sequentially via task-local thermal guard, nice19, CARGO_BUILD_JOBS=2, RUST_TEST_THREADS=2 and installed Rust1.99.1. CPU stayed below85 C; no system power or hardware settings changed.

Only QA test/contract files changed. Existing tests and product source are untouched. Candidate production files may now be applied by the orchestrator/implementer. Do not edit frozen files; legitimate contract correction requires QA resealing and new genuine baseline red for every affected seal. No final green gate, screenshots or functional-QA approval is claimed.
