# Acceptance contract and the pipeline gate

Adapted from hismed-central's `pipeline-gate.mjs` (ADR-0005). The idea: **the person who writes
the code is not the person who decides when it is done.** QA turns each Acceptance item into a
test before any code exists, seals those tests, and proves they fail. The implementer makes them
pass without touching them. `scripts/pipeline-gate.py` is the only judge.

## Files (all under `.scratch/TB-<n>/`)

| File | Writer | What |
|---|---|---|
| `.ack` | orchestrator, from the bouncer's `ticket-approved` | Intake record. No `.ack` → the gate exits 3 |
| `contract/slice-N.json` | QA | Manifest: mode, frozen files, cases, checks |
| `contract/slice-N.seal` | gate `--stamp` (QA runs it) | SHA-256 of manifest + frozen files |
| `contract/slice-N.evidence.json` | gate only | Every run: seal, HEAD, per-case status, log path |
| `contract/slice-N.final.json` | gate `--final` only | PASS record the reviewer checks against HEAD |
| `gate/<time>-slice-N/` | gate only | cargo logs and `check.log` (gitignored) |

Nobody writes evidence, seals or final records by hand.

## Manifest

```json
{
  "version": 1,
  "mode": "behavior",
  "reason": "Protects how free width is split by span",
  "files": ["crates/tb-core/tests/layout_spans.rs"],
  "cases": [
    { "id": "QA-01", "package": "tb-core", "target": "test:layout_spans",
      "test": "a_span_two_button_gets_twice_the_free_width", "red": "required",
      "symbol": "Width" },
    { "id": "QA-02", "package": "tb-core", "target": "test:layout_spans",
      "test": "fixed_width_buttons_keep_their_pixels", "red": "preserve",
      "reason": "Existing behaviour the change must not break" }
  ],
  "checks": []
}
```

- **Modes.** `behavior`: cases are cargo tests; every `required` case must be seen red. `text`
  (labels, docs, theme copy): `cases: []`, a `checks` entry with id `visual` (a `--snap` or
  `--tree` command). `tooling` (scripts, CI, agent files): `cases: []`, a check with id `tests`.
  QA picks the mode from the diff's effect, never from file extensions.
- **Cases.** `target` is `lib`, `bin:NAME` or `test:NAME` (an integration test file). `test` is
  the exact name cargo prints (`module::tests::name` for `--lib`). `symbol` (optional) names the
  new item the test needs, so a compile error about exactly that item counts as red.
- **Frozen files.** Contract tests live in their own files (prefer `crates/<c>/tests/<behaviour>.rs`,
  public API only). The implementer's own unit tests go elsewhere and are not frozen.
- **No ticket keys** in test names or file names: name the behaviour (`GLOSSARY.md` words).
- **Checks** are argv lists, never a shell string. `echo` or an empty command proves nothing.

## Red that counts

| Status | Meaning | Counts as red? |
|---|---|---|
| `RED_BEHAVIOR` | The test ran and an assertion failed | yes |
| `RED_MISSING_SYMBOL` | Compile error E0412/E0422/E0425/E0432/E0433/E0560/E0599/E0609 naming the case's `symbol` | yes |
| `RED_STUB` | Failed on `todo!()` / `unimplemented!()` | no |
| `COMPILE_ERROR` | Any other compile failure (typo, broken harness) | no |
| `SKIPPED` / `NOT_RUN` | `#[ignore]`d, or no test by that exact name | no, and blocks `--final` |
| `GREEN` | Passed | — |

`preserve` cases may start green; they guard existing behaviour.

## Commands

```bash
scripts/pipeline-gate.py TB-<n> --slice N --stamp   # QA: validate, rustfmt-check, seal
scripts/pipeline-gate.py TB-<n> --slice N           # anyone: run under the seal, record evidence
scripts/pipeline-gate.py TB-<n> --slice N --final   # implementer: committed tree, all green,
                                                    # never_red empty, scripts/check.sh green
```

Exit codes: `0` pass · `1` not green or invalid · `2` `SEAL_BROKEN` / `NEVER_RED` · `3` no `.ack`.

## Rules

- Changing the manifest or a frozen file changes the seal. Any change goes back to QA, which
  re-seals, and **the red evidence starts over**: a new seal needs a new red run.
- The implementer has a budget of **5 non-green gate runs per slice** after QA's red run. At 5 it
  returns `contract-stuck` instead of bending the tests.
- A defect found later whose case was already in the contract means the gate was fooled: the
  reviewer reports it against the contract (`Spec` axis), not as a new QA finding.
- A contract that mocks the unit under test does not prove the behaviour. Test the real
  `tb-core` function or the real Rune `Runtime`.
