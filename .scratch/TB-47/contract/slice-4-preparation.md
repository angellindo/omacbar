# TB-47 slice 4 · QA spec (text mode)

Base `01dbc59` (merge of slice 3 `1835391` and adapter corrections `6fc5eaa`). Frozen file:
`scripts/tests/test_opt_in_device_docs.py`. Seal `sha256:23a26866…` (see `slice-4.seal`).

## What the contract asks of the docs

Each check runs one unittest method (`-k`). A requirement is a meaning that must appear in one
paragraph, list item or table row. Each concept is a set of alternative words. No sentence,
heading text or hardware value is pinned. The checklist section is any `##`–`####` heading
naming the opt-in device run (`opt-in`, `--drm`, `device run`, `Linux adapter`).

| Check | Where | Requirement |
|---|---|---|
| checklist-section | hil-checklist | Section exists, links ADR-0014, says the run is human-only and opt-in |
| checklist-preconditions | hil-checklist | TB-28 and TB-9; nothing else holds master/grab (tiny-dfr, service); compositor/seat off the card; permissions come from the human |
| checklist-opt-in-flags | hil-checklist | `--drm auto\|PATH`, `--touch auto\|PATH`; `--touch` refuses every digitizer while `VERIFIED_DIGITIZERS` is empty, until TB-45 |
| checklist-tb10-vs-tb45 | hil-checklist | TB-10 records display observations; TB-45 records digitizer observations and adds the profile from cited evidence |
| checklist-source-vs-observed | hil-checklist | Facts from the candidate driver source (formats, RIGHT_UP, non-desktop) apart from what is unobserved on the T1 |
| checklist-submitted-only | hil-checklist | Driver discards flush/clear results → success = submitted; a lit, legible frame needs human observation |
| checklist-foreign-state-refused | hil-checklist | An active/foreign prior state is refused and left untouched |
| checklist-restore-reported | hil-checklist | Ctrl-C/SIGTERM restores and releases; restore is attempted and errors reported; restore = original inactive state |
| checklist-no-promise-after-kill | hil-checklist | SIGKILL, unplug, power loss covered; frame, panel dark/off and firmware keys named; any outcome stated with a negation (nothing promised) |
| checklist-runbook-return | hil-checklist | Markdown link to `hardware-runbook.md` (the TB-46 return procedures) for the return to prior owners/key mode |
| checklist-human-evidence | hil-checklist | After each run the human records what was seen or the refusal; shared evidence redacted |
| adr-legacy-kms | ADR-0014 | Legacy KMS (SETCRTC/DIRTYFB) with atomic considered |
| adr-sysfs-identity-and-master | ADR-0014 | Card identified in sysfs before open; DRM master ownership, busy refused |
| adr-inactive-prior-state | ADR-0014 | Active/foreign prior state refused; restore to the original inactive state, failures reported |
| adr-submitted-only | ADR-0014 | Flush/clear results discarded → success = submitted; cites `appletbdrm` |
| adr-considered-options | ADR-0014 | tiny-dfr's probe, atomic, the `drm`/`evdev` crates, CLI/config calibration |
| glossary-touch-bar-card / -digitizer-profile / -restore | GLOSSARY | New terms with their core meaning |
| preserve-* (7) | all three | Already written: ADR unsafe boundaries, sigwait, grab after verified profile with TB-45, success path is HIL; glossary Digitizer and Cancelled touch; no host paths, serials, fixed `/dev` nodes, uncited USB IDs or digitizer ranges |
| visual | all three | `--dump` prints the checked sections; exit 1 if one is missing |

## Red at base

`pipeline-gate.py TB-47 --slice 4` at `01dbc59`: 19 requirement checks + `visual` FAILED for
the intended reason (section or term missing, ADR lacks the KMS decisions). 7 preserve GREEN.
Each check log shows `Ran 1 test`. The rest of `scripts/tests` (225 tests) fails only on these 19.

## Satisfiability and probes (outside the repo, not committed)

A free-worded sample of the three docs, in a scratch copy, passed all 26 tests and the dump.
Mutations in that copy were refused: "SIGKILL … the kernel blanks the panel and the firmware keys
come back" (no-promise check), and "Digitizer axis range 0 to 32767" (range guard).

## Notes for the implementer

- The implicit-slot touch fix is in progress elsewhere. No check pins any slot strategy. Document only what the
  merged code does.
- The plan's earlier wording "SIGKILL leaves the kernel's fb release, which blanks the panel" is
  superseded by the KMS-submission notes. The contract refuses any outcome after SIGKILL, unplug or
  power loss that is stated without a negation.
- The privacy checks also refuse `/dev/dri/cardN` and `/dev/input/eventN` with a real number. Use
  `cardN`/`eventN` or `auto`.
- USB IDs allowed: `05ac:8600`, `05ac:8302` and `05ac:1281` (already cited).
