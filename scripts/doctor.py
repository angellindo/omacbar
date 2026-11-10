#!/usr/bin/env python3
"""Read-only offline prebuild/runtime readiness; Python 3.11 or newer."""
import argparse
import json
import sys

import doctor_build
import doctor_runtime
from doctor_support import report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', choices=['build', 'runtime'], default='build')
    parser.add_argument('--json', action='store_true', help='emit one schema_version=1 object')
    args = parser.parse_args()
    try:
        result = report(args.profile, doctor_build.checks() + doctor_runtime.checks(args.profile))
    except (OSError, ValueError, KeyError, TypeError):
        print('doctor: internal report failure; no readiness result available', file=sys.stderr)
        return 2
    if args.json:
        print(json.dumps(result))
    else:
        print(f'Touch Bar doctor: {args.profile} (read-only, offline; scoped readiness)')
        for check in result['checks']:
            print(f"{check['status']:7} {check['id']}: {check['message']}")
            for fact in check['evidence']:
                print('  ' + fact)
            for action in check['next']:
                print('  next: ' + action)
        print(json.dumps(result['summary']))
    return 0 if result['summary']['ready'] else 1


if __name__ == '__main__':
    sys.exit(main())
