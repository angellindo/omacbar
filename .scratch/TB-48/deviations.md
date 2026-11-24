# TB-48 deviations

## Slice 1

- **ADR number.** The plan names ADR-0014; TB-47 plans `0014-kms-and-digitizer-adapters.md`, so
  the generation decision is `docs/adr/0015-config-generations.md`. Code and docs cite ADR-0015.
- **Test helpers isolate by removing `XDG_STATE_HOME`** (store under the temp `HOME`) rather than
  setting a second temp dir; the decoy check proves it at runtime.
- **QA-19 needs a short Cargo target dir.** The frozen test binds a Unix socket at
  `<CARGO_TARGET_TMPDIR>/config_generation_refusals/socket/home/.config/touchbar-customizer/config.toml`
  (83 bytes after the target dir). In this worktree the target dir path is 79 bytes, so `bind`
  fails with "path must be shorter than SUN_LEN" before the product code runs, whatever it does.
  The implementer's runs and the final gate use `CARGO_TARGET_DIR=/tmp/tb48t`, a symlink to this
  checkout's own `target/` (same cache, no rebuild of dependencies); the test binary was rebuilt
  once (mtime touch, content and seal unchanged) so `CARGO_TARGET_TMPDIR` is `/tmp/tb48t/tmp`.
  QA should either keep the target dir under 24 bytes or shorten the test's socket path.
- No `taken-at` clock yet: nothing in slice 1 shows or orders by time (ids are monotonic).
