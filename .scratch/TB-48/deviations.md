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
- **Lock order: store first.** The plan and the first ADR-0015 text took the config-directory
  lock, then the store lock. Two first runs of one profile then raced: the writer that saw the
  other's fresh directory could win the directory lock and record `dir-existed = true`
  (reproduced: 1 of 120 paired `touchbard --sim` starts on one CPU with ce1b946). Every protected
  write now refuses an unkeepable path, takes the store lock, and only then looks at and creates
  the directory and takes its lock; ADR-0015 says so.
- **QA-size-limit harness and this host's GPU.** `run_limited` caps the whole configurator with
  `prlimit --fsize=65536` from `exec`. Mesa `anv` (Kaby Lake, the only Vulkan ICD here; no
  lavapipe installed) cannot create a device below RLIMIT_FSIZE = 1 GiB (measured: 1023 MiB
  fails, 1024 MiB works, also with `MESA_SHADER_CACHE_DISABLE=true` or an empty
  `XDG_CACHE_HOME`), so both size-limit cases stop at "no GPU device: Parent device is lost"
  before the Save: the room case fails and the no-room case passes without reaching the Save.
  With the same limit applied to the live process after GPU init, both cases reach the Save and
  behave as the contract says. Needs a QA harness change; see the RESULT.
