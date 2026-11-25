# TB-46 QA re-spec 1 · receipts

- Product under test: 26ca0790925ccf4ba7cda5fc2a982ea076b1562e (unchanged; no product file edited).
- New seal: sha256:e7bb6f5acc32356222015d9feabc791153d7c6b89e2d17ea4f59dc59de0d1dd3
- Previous seal sha256:d88da825… with its evidence, final record and notes: `archive-cycle-0/`
  (`SHA256SUMS`). The gate's `--stamp` removed the live `slice-1.final.json`; the copy here is history.
- Red gate (no flags), thermal wrapper `validate-thermal/20261124T173620Z/thermal-command.py`, shared
  lock, isolated HOME/XDG/TMPDIR: `tests` FAILED 51/40, `runbook` FAILED 12/2, exit 1; evidence run
  appended to `contract/slice-1.evidence.json` (gate log dir `gate/2026-11-24T210617Z-slice-1`,
  gitignored; a local copy in `red-gate-logs/`, not committed).
- Failing tests: `red-failing-tests.txt` (36 names, 40 failures incl. 5 subtests); green preserve:
  `red-green-preserve.txt` (25).
- Scratch-mirror harness (BLOCKERs 2–3 behaviour on 26ca079): `mirror-integrity-26ca079.log`
  (local, not committed): 4 FAIL on assertions after the hook fired / files deleted, 1 ok.
- No workspace build or `check.sh` run; no hardware, Notion, push or agents.
