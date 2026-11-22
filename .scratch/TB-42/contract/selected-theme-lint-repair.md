# Selected-theme contract lint repair

The implementer reported in `deviations.md` that two clippy `-D warnings` lints in the frozen
`crates/tb-app/tests/configurator_selected_theme.rs` blocked `--final`. Only those two lints were
changed:

- `clippy::assigning_clones` (in `save_time_failure_then_retry`): `expected...label = "kept edit".to_owned();`
  became `"kept edit".clone_into(&mut expected...label);`. The expected value is the same.
- `clippy::semicolon_if_nothing_returned` (in the `malformed_after_open` break closure): `.unwrap()` became `.unwrap();`.

Assertions, return values, the manifest, the other frozen files and product code are unchanged.
New slice 1 seal: `sha256:f39ab9a986ddf75764090929d9d178104a14cbd61eb7acc86468f6c6e7402fbc`
(replaces `sha256:e0b7849f…`; that seal's runs stay in the evidence history).

Red proof: a separate local shared clone (session scratchpad, outside this checkout) was checked out
detached at pre-implementation commit `4a4109c53e65686e3f34e85ea1f7271fb30220cc`. Only the corrected
test file, the identical intake `.ack` and the current evidence history were added. The manifest and
the other three frozen files were byte-identical to it. The gate stamped the seal there and ran the
contract. The clone used its own `target/`, so no shared build output was used. The gate's evidence
file, seal and log directory were copied back byte for byte. The prior history was checked identical,
and the seal recomputes to the same value in this checkout.

Generated run at 2026-11-22T05:44:51Z, head `4a4109c5`, Rust 1.99.1:
QA-01–QA-08, QA-11, QA-12 RED_BEHAVIOR (assertion failures: the editor refuses or ignores external
tokens and the selected theme source, and the first-run default is rejected under a theme without
`red`). The preserve cases QA-09, QA-10 and QA-13 were GREEN. never_red is empty. Log:
`.scratch/TB-42/gate/2026-11-22T053337Z-slice-1/` (kept local).

After the repair, `cargo clippy -p tb-app --locked --test configurator_selected_theme -- -D warnings`
passed against the implemented tree. All builds ran one at a time through
`.scratch/TB-40/thermal-command.py` (nice 19, jobs 2, threads 2).
