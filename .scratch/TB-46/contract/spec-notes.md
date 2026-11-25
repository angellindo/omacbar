# TB-46 slice 1 · contract notes (QA spec) · re-spec 2 (save receipt)

Mode `tooling`. Checks: `tests` (`test_rehearsal*.py`) and `runbook` (`test_hardware_runbook.py`).
`--final` runs `scripts/check.sh`, which already runs every `scripts/tests/test_*.py`.

The first seal (`sha256:d88da825…`, HEAD cce2d715), its evidence, its final record (PASS on
26ca079) and these notes as they were are archived byte for byte in
`.scratch/TB-46/qa-respec-1/archive-cycle-0/` (`SHA256SUMS` there). The live
`slice-1.evidence.json` keeps every earlier run; the gate appends, nothing was rewritten.

## Re-spec 2: the run knows its own save by the save's receipt

Seal `sha256:e7bb6f5a…` (re-spec 1), its evidence, these notes as they were, review-1's verdict and
the correction cycle's verification are archived byte for byte in
`.scratch/TB-46/qa-respec-2/archive-cycle-1/` (`SHA256SUMS`). Every re-spec 1 case is kept with its
criteria; only the fake configurator gained the receipt and three injections, and `SaveReceipt`
was added (`test_rehearsal_run.py`). Same files, same checks, one HOME per run, one save per
scenario.

Why: candidate 27643ff takes the producing save's expectation by reading the config (`lstat`)
after the configurator exits. Another ordinary editor that saves in between (same `Rehearsal`
label, valid TOML) is adopted as the run's own save, settled, and replaced by `restore`: the second
design is lost. Seeing the label or reading the file afterwards does not identify the bytes the
save wrote. A twin profile replaying the flow (the earlier `spec-notes` suggestion) is withdrawn: it
contradicts the sealed single-HOME isolation case and one save per scenario.

### Save receipt (interface shared by the configurator and the fake)

- `run` sets `TB_SAVE_RECEIPT=<path>` for the configurator's save step, a path that is the run's own
  and fresh for that save (a receipt left by an earlier save is not this save's).
- On each confirmed explicit save (Save, Ctrl+S, the conflict overwrite) the configurator writes at
  that path, by temp + rename, the JSON object
  `{"config": "<the config path it saved>", "sha256": "<64 lowercase hex>"}`, hashed from the bytes
  that save wrote (the bytes handed to the swap), never by reading the file back. A failed, refused
  or blocked save writes none. The real configurator does not emit it at 27643ff (product work).
- `run` settles a scenario with `--expect-config <receipt sha256>`, and a captured link's target
  with the captured target state (the normal save replaces the link). A receipt that is absent,
  not this save's, malformed, or for another config path → the run fails, nothing settled or
  restored, the current bytes and the store kept (they survive `cleanup`). The report's scenario
  record carries the receipt's hash.

### New cases (`SaveReceipt`, fake injections)

- `second-editor`: over the personal layout (`label = "Mine"`, 0640, comment), the save completes
  with its receipt; before the configurator exits a second ordinary editor saves another valid
  design that keeps `Rehearsal` and changes `span` 1 → 2 (temp + rename). The run fails; the second
  design stays in place and the personal layout stays in the run's store, after the run and after
  `cleanup`.
- `no-receipt`: the second save (byte-identical to the first one's) writes no receipt. A stale
  receipt from the first save must not pass for it.
- `bad-receipt`: the first save's receipt has a 63-hex hash.

Not covered by the Python contract: that the real configurator writes the receipt. The real-binary
run (functional QA, thermal prefix) needs it: without it every scenario fails by the rules above.

## Re-spec 1: why the seal changed (review-1 BLOCKERs 1–4, all kept)

### Formal incompatibility: the old unrestricted fixture vs the new seal

The first contract let `settle --manifest M` record **whatever it found** as the state the test
left (`Fixture.settle(*extra)` passed no expectation). Under that contract an edit landing between
the test's save and `settle` was adopted and later replaced by `restore` (BLOCKER 1). That
behaviour is now a contract violation, so the old fixture and the new seal cannot both hold:

- `Fixture.settle(expect_config, *extra, expect_target=None)` always sends
  `--expect-config`, and `--expect-target` for a captured symlink. Every existing call site was
  migrated with the expectation of the step that produced the config (the test's own `SAVED`, the
  captured target, `absent`). Criteria and scenarios of the first contract are unchanged.
- `settle` **without** an expectation (the old call) must now be refused and must not authorize
  restore. A tree that passes the old seal fails the new one and vice versa; the old final record
  is void for this seal (the gate deleted it on `--stamp`; the archived copy is history only).

### New interface the contract pins (CLI `scripts/rehearsal.py`, exit 0 ok · 1 refusal · 3 conflict)

- `settle --manifest M --expect-config EXPECT [--expect-target EXPECT] [--pid N]… [--artifact PATH SHA256]…`
  `EXPECT` is `absent`, a 64-hex SHA-256 of a regular file's bytes, or `symlink:<link text>`.
  `--expect-target` is required when the capture was a symlink. Missing, malformed, wrong-kind or
  different → exit 1 or 3 (never 2/usage for a well-formed call), nothing recorded that restore
  could act on, current bytes kept in place, original kept in the store; `cleanup` keeps both.
- `run`: every scenario settles with an expectation from the producing save (re-spec 2: its
  receipt). The report's scenario record contains the SHA-256 the producing save wrote. A failed save, a failed
  reopen, an external edit of the config or of the old link target after the save: the run fails,
  nothing is settled or restored over those bytes, and they survive `cleanup`.
- `restore`: compares again right before each replacement, after the temp file is prepared (written,
  synced, chmodded), for the config and for a link target (ADR-0013: only the residual window after
  that last comparison is accepted). A change during preparation → non-zero, the other editor's
  bytes kept, original in the store, no temp left.
- `cleanup`: never walks a parent directory or a store swapped for a symlink (same names, bytes and
  modes behind the link are not the test's own); changes nothing but the own unchanged artifacts and
  the tool's own store copies (unknown files in the store, a same-bytes `config.toml.bak.2`, a
  foreign file in HOME all survive).

### How the tests reach those intervals (safe injection, no exploit)

- External edit before settle (CLI): the test itself saves `EXTERNAL` by temp + rename.
- Edit during restore preparation: `rehearsal_fixture.EDITOR_HOOK` runs the real CLI with
  `os.fsync`/`os.fdatasync`/`os.fchmod` wrapped; the first time the tool syncs or chmods a
  descriptor of **another** file that already holds exactly the bytes being put back (checked via
  `/proc/self/fd`, i.e. the tool's own temp file in preparation), a cooperating editor saves the
  watched path by temp + rename. Each test asserts the hook fired, so it cannot pass vacuously; it
  requires the temp to be synced or chmodded before the rename (durability already required).
- `run`: the fake binaries behave like the normal-flow ones (defaults written when absent, `--script`
  save by temp + rename with the edited label, labels in the tree, daemon F5 once + one frame + the
  profile-local mark) and log the SHA-256 each save wrote: that is the producer's expectation the
  run must match. `FAKE_INJECT` (a one-shot token) injects `external`, `target`, `save-fails` or
  `reopen-fails`; `FAKE_FOREIGN` leaves a file the run never owned. A nominal run must pass (exit 0)
  and `cleanup` must leave only the foreign file in the profile.
- Runbook: 2.4's expected changes and evidence name the service manifest; B → A names it in changes
  and evidence, and has a command putting `installed.json` back from the preserved copy, one
  comparing it (`cmp`/`sha256sum`/`diff`) and one comparing its `stat`.
- Baseline (private fixture data): the manifest's content change (`enabled` → `removed`) and a
  mode-only change are differences; the 2.0-style preserved copy put back (`cp -a`: bytes, mode,
  times) matches A.

### Normal-flow binaries

Superseded by re-spec 2: the configurator's status (`saved config <path>`) carries no hash, so the
run needs the save receipt above (a new configurator output). Running the real binaries stays
functional QA (thermal prefix); physical stages are document-only (HIL).

## Red proof under the new seal

Seal `sha256:e7bb6f5acc32356222015d9feabc791153d7c6b89e2d17ea4f59dc59de0d1dd3`, product HEAD
26ca079, gate without flags under the thermal wrapper of `validate-thermal/20261124T173620Z`
(shared lock, isolated HOME/XDG/TMPDIR), 2026-11-24T21:06Z: `tests` FAILED (51 tests, 40
failures, 0 errors), `runbook` FAILED (12 tests, 2 failures). Receipt and lists:
`.scratch/TB-46/qa-respec-1/RECEIPT.md`.

Why each group is red on 26ca079 (reasons read from the failure messages):

- **Behaviour (BLOCKER 1, run):** an external edit after the save (`external`), an edit of the old
  link target (`target`; the run even reports `passed`), a partial failed save (`save-fails`) and a
  failed reopen (`reopen-fails`) are all settled and restored over: the injected bytes are gone
  after the run. Nominal run: passes every scenario with the faithful fakes, but no scenario record
  carries the producing save's hash.
- **Behaviour (BLOCKER 1, CLI):** `settle` with no expectation exits 0 and authorizes restore.
- **Missing interface:** every other `settle` call fails with usage (exit 2) on `--expect-config`
  / `--expect-target`; the refusal tests demand 1 or 3, so a usage error never passes for a refusal.
  This includes the migrated first-contract cases (their criteria unchanged).
- **Behaviour (BLOCKERs 2–3), proven in a scratch mirror** (26ca079 product, byte-identical; only
  the fixture's settle shimmed to the old call): both preparation-edit tests fire the editor hook
  and restore exits 0 having replaced the other editor's version (config and link target); the
  parent-swap and store-swap cleanup tests delete the same-bytes files behind the link. The
  own-only integrity test is green there (preserve). Log: `qa-respec-1/mirror-integrity-26ca079.log`.
- **Runbook (BLOCKER 4):** 2.4's evidence and B → A's changes do not name the manifest; no command
  puts `installed.json` back or compares it.

Green on 26ca079 (preserve, 25): the five `Run` isolation cases with the new fakes, the five
baseline cases (incl. the new manifest content/metadata one), four capture refusals, the artifact
hash mismatch, ten first-contract runbook cases. List: `qa-respec-1/red-green-preserve.txt`.

## Not proven by the contract

The real configurator/daemon run (functional QA), the TB-43 two-editor Runtime cases (cited), a
removal (`unlink`) racing an editor after the last comparison (ADR-0013 residual window), and every
S1–S4 / A / B / B → A physical step (HIL).
