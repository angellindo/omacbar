#!/usr/bin/env python3
"""Reversible rehearsal on a throwaway profile, and read-only hardware baselines.

  run --bin-dir D --report R [--home H]     whole rehearsal with the real (or fake) binaries
  capture --home H --store S --manifest M   record the config path before a test changes it
  settle --manifest M --expect-config E [--expect-target E] [--pid N]… [--artifact PATH SHA256]…
                                            record what the test left, if it is what the producing
                                            step expects (E: absent | sha256 | symlink:<text>)
  restore --manifest M                      compare-and-swap the original back (TB-43 lock)
  verify --manifest M                       compare the profile with the capture
  cleanup --manifest M                      remove the test's own unchanged artifacts
  baseline --label A|B --out DIR [--root R] read-only manifest of the laptop state (human step)
  compare A B                               differences between two baselines

Exit codes: 0 ok · 1 failure or refusal (nothing changed) · 3 conflict (both versions kept).
Agents run `baseline` only against fixture roots; on the real machine it is a human step."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from rehearsal_fs import CONFLICT, FAILED, OK, Conflict, Refused


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="rehearsal.py", description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("capture")
    for flag in ("--home", "--store", "--manifest"):
        c.add_argument(flag, type=Path, required=True)
    s = sub.add_parser("settle")
    s.add_argument("--manifest", type=Path, required=True)
    s.add_argument("--expect-config")  # missing → a refusal (exit 1), not a usage error
    s.add_argument("--expect-target")
    s.add_argument("--pid", type=int, action="append", default=[])
    s.add_argument("--artifact", nargs=2, action="append", default=[], metavar=("PATH", "SHA256"))
    for name in ("restore", "verify", "cleanup"):
        sub.add_parser(name).add_argument("--manifest", type=Path, required=True)
    r = sub.add_parser("run")
    r.add_argument("--bin-dir", type=Path, required=True)
    r.add_argument("--report", type=Path, required=True)
    r.add_argument("--home", type=Path)
    r.add_argument("--theme", default="tokyo-night")
    b = sub.add_parser("baseline")
    b.add_argument("--label", required=True)
    b.add_argument("--out", type=Path, required=True)
    b.add_argument("--root", type=Path, default=Path("/"))
    cmp = sub.add_parser("compare")
    cmp.add_argument("a", type=Path)
    cmp.add_argument("b", type=Path)
    return p


def dispatch(args) -> int:
    if args.cmd == "capture":
        import rehearsal_profile
        rehearsal_profile.capture(args.home, args.store, args.manifest)
        print(f"captured {args.home} into {args.store}; manifest {args.manifest}")
        return OK
    if args.cmd == "settle":
        import rehearsal_profile
        problems = rehearsal_profile.settle(args.manifest, args.pid, args.artifact,
                                            args.expect_config, args.expect_target)
        return report_lines("settle", problems)
    if args.cmd == "restore":
        import rehearsal_restore
        rehearsal_restore.restore(args.manifest)
        print("restored the captured config")
        return OK
    if args.cmd == "verify":
        import rehearsal_profile
        return report_lines("verify", rehearsal_profile.verify(args.manifest))
    if args.cmd == "cleanup":
        import rehearsal_restore
        removed, kept = rehearsal_restore.cleanup(args.manifest)
        for line in removed:
            print(f"removed {line}")
        for line in kept:
            print(f"kept {line}")
        return OK
    if args.cmd == "run":
        import rehearsal_run
        return rehearsal_run.run(args.bin_dir, args.report, args.home, args.theme)
    import rehearsal_baseline
    if args.cmd == "baseline":
        print(rehearsal_baseline.baseline(args.label, args.out, args.root))
        return OK
    return report_lines("compare", rehearsal_baseline.compare(args.a, args.b))


def report_lines(what: str, problems: list[str]) -> int:
    for line in problems:
        print(f"{what}: {line}")
    print(f"{what}: {'ok' if not problems else f'{len(problems)} difference(s)'}")
    return FAILED if problems else OK


def main(argv=None) -> int:
    args = parser().parse_args(argv)
    try:
        return dispatch(args)
    except Conflict as e:
        print(f"conflict: {e}", file=sys.stderr)
        return CONFLICT
    except Refused as e:
        print(f"refused: {e}", file=sys.stderr)
        return FAILED
    except OSError as e:
        print(f"failed: {e}; the current file is whole and the store is kept", file=sys.stderr)
        return FAILED
    except KeyboardInterrupt:
        print("interrupted: the current file is whole and the store is kept", file=sys.stderr)
        return 130


if __name__ == "__main__":
    sys.exit(main())
