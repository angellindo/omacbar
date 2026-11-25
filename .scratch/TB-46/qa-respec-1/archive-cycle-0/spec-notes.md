# TB-46 slice 1 · contract notes (QA spec)

Mode `tooling`. Checks: `tests` (`test_rehearsal*.py`, 32 tests) and `runbook`
(`test_hardware_runbook.py`, 10 tests). `preserve-scripts` from the draft is dropped: `--final`
runs `scripts/check.sh`, which already runs every `scripts/tests/test_*.py`.

Adapted from `.scratch/TB-46/qa-draft/` to `main` at cce2d715 (TB-43 merged) and to plan.md's
"QA integration decisions" and "Source correction" sections.

## Seams the tests pin (CLI `scripts/rehearsal.py`, exit 0 ok · 1 failure/refusal · 3 conflict)

- Lock: exclusive `flock` on the config **directory** (TB-43, ADR-0013, `tb-config` `DirLock`);
  `config.toml.bak` only from an acknowledged overwrite. Checked against the merged code.
- `capture --home H --store S --manifest M`: as in the draft README.
- `settle --manifest M [--pid N]… [--artifact PATH SHA256]…` (**changed from the draft**):
  records the config state the test left; an own artifact is only what is declared with the
  SHA-256 the producing step expects, verified before recording. A mismatch is never owned.
  A new name in a directory listing (even `config.toml.bak.1`) is never owned.
- `restore` / `verify` / `cleanup --manifest M`: as in the draft, plus: a parent the test created
  is removed only when empty (an unknown file in it survives); `verify` fails when a default was
  written over a restored absence (so absence is checked before any reopen); an own artifact
  swapped for a symlink is kept, link and target.
- `run --bin-dir D --report R [--home H]`: refuses an existing `--home` (caller `HOME`, a dir in
  it, the session, an existing dir, a path in the repo). **Relaxed from the draft:** a `TMPDIR`
  inside the caller's `HOME` still gets a fresh profile from it (nothing else in `HOME` touched).
  `cleanup --manifest <report>` is valid usage (the report is what cleanup takes).
- `baseline` / `compare`: unchanged from the draft.

## Runbook test

Columns matched by meaning (owner, target, entry/start, change, stop, return/back/rollback,
evidence/compar), any order, one column each; no exact labels frozen. Also requires: the pinned
Seborider commit, Linux stable commit and the six source/patch SHA-256s from
`driver-build-receipt.md`; a `make … -C … M=… modules` line; no `.scratch/` reference;
lockdown / sig_enforce / modules_disabled as observations to recheck; `clear_display`; seat;
return to config 1 stated unverified; service files vs module tree vs firmware; Esc/F12 key row;
research 0001 without "Both select config 2 at enumeration" and citing `3a00d8d`;
`Rehearsal profile` in `GLOSSARY.md`; link from `docs/hil-checklist.md`; Spanish summary.

## Red proof (seal sha256:d88da825…, HEAD cce2d715)

`tests` FAILED: 32 tests, 44 failures (subtests), 0 errors, all `scripts/rehearsal.py is missing`.
`runbook` FAILED: 10 tests, 10 failures, 0 errors, all `docs/hardware-runbook.md is missing`.
Harness checks in a scratch mirror: stub `exit(0)` → 31/32 fail (the one pass is the
keep-only mismatch guard); stub `exit(1)` → 32/32 fail; a minimal conforming
runbook passes all 10 runbook tests (satisfiable). Existing script tests green.

## Not proven by the contract

The real configurator/daemon run (functional QA, built with the thermal prefix), the TB-43
two-editor Runtime cases (cited, not re-run), and every S1–S4 / A / B / B → A physical step (HIL).
