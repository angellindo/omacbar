#!/usr/bin/env python3
"""Runtime proof that no Rust test hands a product binary the caller's state directory.

Every integration test target that starts `touchbar-customizer` or `touchbard` (it, or a
`tests/support/*.rs` module it declares, names `CARGO_BIN_EXE_`) runs under
`XDG_STATE_HOME=<temp decoy>`. A spawn site that forgets to give its binary a private
`XDG_STATE_HOME` lets the binary keep generations in the decoy; the check fails if the decoy
changed in any way or the tests did not pass. Nothing is read from or written to the real state
directory. Cargo runs one package at a time, niced, two jobs (shared-laptop rule in CLAUDE.md).
Exit 0 pass, 1 fail."""

from __future__ import annotations

import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


def spawning_targets() -> dict[str, list[str]]:
    out: dict[str, list[str]] = {}
    for tests in sorted(ROOT.glob("crates/*/tests")):
        pkg = tests.parent.name
        for f in sorted(tests.glob("*.rs")):
            text = f.read_text()
            for module in re.findall(r"\bmod\s+(\w+)\s*;", text):
                support = tests / "support" / f"{module}.rs"
                if support.is_file():
                    text += support.read_text()
            if "CARGO_BIN_EXE_" in text:
                out.setdefault(pkg, []).append(f.stem)
    return out


def snapshot(root: Path) -> dict:
    out = {}
    for path in [root, *root.rglob("*")]:
        st = path.lstat()
        data = path.read_bytes() if path.is_file() and not path.is_symlink() else None
        out[str(path.relative_to(root))] = (st.st_mode, st.st_size, st.st_mtime_ns, data)
    return out


def main() -> int:
    targets = spawning_targets()
    if not targets:
        print("no test target starts a product binary: nothing proves isolation", file=sys.stderr)
        return 1
    with tempfile.TemporaryDirectory(prefix="tb-xdg-decoy-") as tmp:
        decoy = Path(tmp) / "caller state"
        (decoy / "touchbar-customizer").mkdir(parents=True)
        (decoy / "touchbar-customizer/theme-set").write_text("caller\n")
        before = snapshot(decoy)
        env = dict(os.environ, XDG_STATE_HOME=str(decoy), CARGO_BUILD_JOBS="2", RUST_TEST_THREADS="2")
        failed = []
        for pkg, names in targets.items():
            cmd = ["nice", "-n", "19", "cargo", "test", "-p", pkg, "--locked"]
            for name in names:
                cmd += ["--test", name]
            print("$ XDG_STATE_HOME=<decoy> " + " ".join(cmd), flush=True)
            if subprocess.run(cmd, cwd=ROOT, env=env).returncode != 0:
                failed.append(pkg)
        after = snapshot(decoy)
    changed = sorted(k for k in set(before) | set(after) if before.get(k) != after.get(k))
    if changed:
        print(f"a test let a binary write into the caller's XDG_STATE_HOME: {changed}", file=sys.stderr)
    if failed:
        print(f"tests failed in: {failed}", file=sys.stderr)
    return 1 if changed or failed else 0


if __name__ == "__main__":
    sys.exit(main())
