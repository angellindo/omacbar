# TB-42 deviations from plan

- **No `State::open_home`.** The sealed contract opens the editor via
  `State::open(Some(default_path(home)), ActiveTheme::under_home(home).load().theme)`, exactly as
  `init` does, so the extra public constructor is not needed for correctness and was not added.
- **Save-time re-resolution** (plan clarification): added `tb_config::ConfigFile::save_resolved`
  (resolve the candidate's theme source, validate, write atomically, return the theme), a sibling of
  `load_resolved`. `ConfigFile::save` is unchanged for its existing callers. A refused save never sets
  the open-time block, so a retry after repair succeeds; an `Invalid` refusal lists each issue (so a
  lost token is named).
- **Frozen-test lint (blocks `--final`).** `scripts/check.sh` clippy (`-D warnings`) fails inside the
  sealed `crates/tb-app/tests/configurator_selected_theme.rs`, which the implementer may not edit:
  - line 247 `clippy::assigning_clones` (`... .label = "kept edit".to_owned();`)
  - line 265-266 `clippy::semicolon_if_nothing_returned` (`fs::write(theme, ...).unwrap()` in a closure)
  QA lint repair + reseal needed (same as TB-40 `908c3e4`). With only those two lints allowed on the
  command line, workspace clippy, all tests, fmt, mirrors and file size pass.
