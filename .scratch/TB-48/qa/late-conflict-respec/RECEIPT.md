# QA amendment: TB-48 slices 1–4, late conflicts (branch tb-48-integrated-recovery)

Base and candidate e7a500e. Product, ADRs and TB-46 contract unchanged: this commit adds tests and
contract files only. Design, decisions and coverage limits: `contract/spec-notes.md`, last section.

## Seals

| Slice | Archived seal (PASS at e7a500e) | New seal | New required |
|---|---|---|---|
| 1 | a8932ce1 | 97ffc0e4 (fbf85e29 superseded: clippy) | QA-44..51 |
| 2 | 7e6bf614 | 28ea7e67 (3b0f3b7a superseded: clippy) | QA-28..33 |
| 3 | 70e1d531 | b31de79f (0b7ce8e7 superseded: clippy) | QA-14..15 |
| 4 | 4402224f | 6942a867 | QA-15..16 |

Archive before `--stamp`: `archive/` (manifests, seals, evidence, the four final records, spec
notes; `SHA256SUMS`, `ARCHIVED-FROM`). New frozen files: `crates/tb-config/tests/{config_late_conflict,
recover_terminal_late_conflict}.rs`, `crates/tb-config/tests/support/{late_conflict.rs,syscall_pause.py}`,
`crates/tb-app/tests/configurator_late_conflict.rs`, plus the existing support modules they compile
(byte-identical). Manifests were produced by `amend.py` (this directory) from the archive.

## Runs (gate-written evidence; raw logs kept locally, not committed)

Environment for all runs: Rust 1.99.1 called directly, shared thermal gate 65/74/64 °C, `bwrap`
with a disposable `/dev` (no physical devices), software Vulkan, private HOME and XDG, one
CPU/job/test thread.

1. Red on 25230e9: clone with its own reflinked target, sealed frozen files overlaid (only the 5 new
   files differ), `cargo clean -p` of the workspace crates, new test executables built in that run.
   Seals fbf85e29/3b0f3b7a/0b7ce8e7/6942a867: 18/18 new RED_BEHAVIOR, never_red empty, then a Clippy
   `clone_on_copy` in the frozen library suite. Fixed and re-sealed 1–3; seals 97ffc0e4/28ea7e67/
   b31de79f: 16/16 RED_BEHAVIOR, never_red empty, Clippy clean. Reasons: no id (plain `Changed`, `Symlink`,
   "nothing was written and both versions are kept"), and at stage a generation published anyway.
2. Preservation at e7a500e under the four current seals: every case and check GREEN (120), never_red
   empty; peak 76 °C, one thermal pause.

## Handoff for integration and finals

- No Sol work is needed for this contract: e7a500e already passes all new cases. The mount-integrity
  candidate must keep them green and run the five finals (TB-46 s1 unchanged at d73153ea… as
  archived; TB-48 s1–s4 at the seals above) on its committed HEAD.
- Under the default libtest thread count the new suites start several held children at once.
  Each case has its own profile and supervisor, so there is no shared state between them.
- If a candidate changes the store's publish order (no `fsync` of the store after the rename, or
  no `.tmp-*` staging), the held cases fail with "never reached the … step". That calls for a QA
  re-spec. It is never a reason to edit the tests.
