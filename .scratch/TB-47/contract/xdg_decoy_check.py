#!/usr/bin/env python3
"""TB-47 runtime proof that no `touchbard` started by a TB-47 test keeps state in the caller's
XDG_STATE_HOME, by both paths that start one:

1. process path, `--test touchbard_hardware`: the binary started with an empty HOME; and
2. in-process path, `--bin touchbard` (its `tests`, `tests::hardware` and `devices_tests` modules
   and the test-binary children they start): the daemon run with an artificial HOME.

Each path runs against its own new decoy (`XDG_STATE_HOME=<temp dir>` holding a `theme-set` file)
and fails if that decoy changed in any way (mode, size, mtime, bytes or inode of any entry), a test
failed, or the run proves too little (fail-closed): path 1 needs both its tests passed and, with
`TB_TEST_REQUIRE_PRIVATE_GENERATION` set, the signal test's own report that each HUP/INT/TERM round
kept exactly one absent first-write generation in that round's HOME; path 2 needs passing
`tests::hardware::` and `devices_tests::` cases. Nothing is deleted outside the decoys, and nothing
is read from or written to the real state directory. Exit 0 pass, 1 fail."""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SIGNAL_TEST = "the_simulator_still_ends_on_sighup_sigint_and_sigterm_by_their_default_action"
REFUSAL_TEST = "an_explicit_device_path_outside_dev_is_refused_with_exit_one_and_left_unchanged"


def snapshot(root: Path) -> dict:
    out = {}
    for p in [root, *sorted(root.rglob("*"))]:
        st = p.lstat()
        data = p.read_bytes() if p.is_file() and not p.is_symlink() else None
        out[p.relative_to(root).as_posix()] = (st.st_mode, st.st_size, st.st_mtime_ns, st.st_ino, data)
    return out


def enough_for_process_path(passed: list[str], out: str) -> bool:
    reports = dict(re.findall(r"^private first-write generations SIG(HUP|INT|TERM): (\d+)$", out, re.M))
    return {SIGNAL_TEST, REFUSAL_TEST} <= set(passed) and reports == {"HUP": "1", "INT": "1", "TERM": "1"}


def enough_for_in_process_path(passed: list[str], _out: str) -> bool:
    return any(t.startswith("tests::hardware::") for t in passed) and any(
        t.startswith("devices_tests::") for t in passed)


PATHS = (
    ("process: --test touchbard_hardware", ["--test", "touchbard_hardware", "--", "--show-output"],
     enough_for_process_path),
    ("in-process: --bin touchbard", ["--bin", "touchbard"], enough_for_in_process_path),
)


def main() -> int:
    failed = []
    for label, args, enough in PATHS:
        with tempfile.TemporaryDirectory(prefix="tb47-xdg-decoy-") as tmp:
            decoy = Path(tmp) / "caller state"
            (decoy / "touchbar-customizer").mkdir(parents=True)
            (decoy / "touchbar-customizer/theme-set").write_text("caller\n")
            env = dict(os.environ, XDG_STATE_HOME=str(decoy), TB_TEST_REQUIRE_PRIVATE_GENERATION="1")
            before = snapshot(decoy)
            cmd = ["nice", "-n", "19", "cargo", "test", "-p", "tb-daemon", "--locked", *args]
            print("$ XDG_STATE_HOME=<decoy> TB_TEST_REQUIRE_PRIVATE_GENERATION=1 " + " ".join(cmd), flush=True)
            p = subprocess.run(cmd, cwd=ROOT, env=env, capture_output=True, text=True)
            sys.stdout.write(p.stdout)
            sys.stderr.write(p.stderr)
            after = snapshot(decoy)
            changed = sorted(k for k in set(before) | set(after) if before.get(k) != after.get(k))
            kept = len(list(decoy.glob("touchbar-customizer/backups/*/manifest.toml")))
        passed = re.findall(r"^test (\S+) \.\.\. ok$", p.stdout, re.M)
        failing = re.findall(r"^test (\S+) \.\.\. FAILED$", p.stdout, re.M)
        proved = enough(passed, p.stdout)
        ok = p.returncode == 0 and not changed and not failing and proved
        print("TB-47 xdg decoy: " + json.dumps({
            "path": label, "exit": p.returncode, "tests_passed": len(passed), "tests_failed": failing,
            "proof_complete": proved, "decoy_changed": changed, "decoy_generations": kept, "pass": ok}), flush=True)
        if not ok:
            failed.append(label)
    if failed:
        print(f"a TB-47 test let touchbard write into the caller's XDG_STATE_HOME, failed or proved too little: "
              f"{failed}", file=sys.stderr)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
