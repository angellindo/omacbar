# TB-46 QA re-spec 2 · receipts (save receipt)

- Product under test: 27643ff24dc7515fb42c131219a2b60e53f8e5bd (unchanged; no product file edited).
- New seal: sha256:c56950dc6a0108868f7e98b6b6d8cc593a04fdbc93f90805ceab2d3ef97b396a
- Previous seal sha256:e7bb6f5a… with its manifest, evidence and notes, review-1's verdict
  (CHANGES REQUESTED) and the correction cycle's verification: `archive-cycle-1/` (`SHA256SUMS`).
  The live `slice-1.evidence.json` keeps every earlier run; the gate appended, nothing rewritten.
- Changed: `scripts/tests/test_rehearsal_run.py` only (fake configurator emits the
  `TB_SAVE_RECEIPT` receipt; injections `second-editor`, `no-receipt`, `bad-receipt`; class
  `SaveReceipt`, 3 tests). Same frozen files and checks; no earlier case or criterion changed.
- Red gate (no flags) at 2026-11-24T23:09Z, run under the shared thermal wrapper and lock, with
  private HOME/XDG/TMPDIR: `tests` FAILED (59 tests, 3 failures), `runbook` GREEN (12 tests), exit 1.
  Gate log dir `gate/2026-11-24T230841Z-slice-1` is gitignored and not committed.
- Why red: in all three new tests the candidate run exits 0 ("rehearsal passed"). It takes the
  save's identity by reading the config after the configurator exits, so it adopts the second
  editor's same-label design, settles it and restores over it. It also settles a save that has no
  receipt or an invalid one. List: `red-failing-tests.txt`. Green preserve: the other 56 rehearsal
  tests and the 12 runbook tests.
- Satisfiability: a scratch mirror of 27643ff whose `run` settles on the receipt (fresh path per
  save, validated, own artifact; target expectation from the capture) passed 59/59. It was a light
  Python run with `nice` and no cargo, and the log stays local, not committed.
- No cargo build or `check.sh` run; no hardware, Notion, push, remote, or agents.
