#!/usr/bin/env python3
"""Prepare private local doctor context and hand it to an installed Codex CLI (Python 3.11+)."""
import argparse
import shutil
import subprocess
import sys
import time
from pathlib import Path

from doctor_support import ROOT
from triage_session import prepare


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--agent', choices=['codex'], default='codex')
    parser.add_argument('--profile', choices=['build', 'runtime'], default='runtime')
    parser.add_argument('--prepare-only', action='store_true', help='write evidence without resolving or launching an agent')
    args = parser.parse_args()
    try:
        session = prepare(args.profile, time.time_ns())
    except (OSError, ValueError, KeyError, TypeError):
        print('triage: doctor/preparation failed; check the local doctor report and artifact directory safety', file=sys.stderr)
        return 2
    print(f'Triage artifacts: {session}', flush=True)
    if args.prepare_only:
        return 0
    executable = shutil.which('codex')
    if not executable:
        print('triage: installed codex CLI missing from PATH; inspect the artifacts above', file=sys.stderr)
        return 1
    prompt = f'Read and follow the local diagnostic prompt at {session / "prompt.md"}'
    try:
        code = subprocess.call([str(Path(executable).resolve()), '-C', str(ROOT), prompt], cwd=ROOT)
    except OSError:
        print('triage: installed codex could not launch; artifacts remain available', file=sys.stderr)
        return 1
    return code if code >= 0 else 128 - code


if __name__ == '__main__':
    sys.exit(main())
