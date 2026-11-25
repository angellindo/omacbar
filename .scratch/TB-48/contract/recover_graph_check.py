#!/usr/bin/env python3
"""Terminal recovery needs neither Rune nor a GPU: tb-config has a `touchbar-recover` binary
target, and nothing tb-config builds against (normal and build dependencies, transitively, as
cargo resolves them for this lockfile) is Rune, wgpu, a Vulkan/windowing crate or another of this
workspace's GUI or daemon crates. Reads `cargo metadata` only (offline, locked); builds nothing.
Exit 0 pass, 1 fail."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
FORBIDDEN = ("rune", "wgpu", "ash", "winit", "naga", "tb-render", "tb-app", "tb-daemon", "vello")


def main() -> int:
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--offline"],
        cwd=ROOT, capture_output=True, text=True,
    )
    if out.returncode != 0:
        print(out.stderr, file=sys.stderr)
        return 1
    meta = json.loads(out.stdout)
    packages = {p["id"]: p for p in meta["packages"]}
    config = next(p for p in meta["packages"] if p["name"] == "tb-config" and p["id"] in meta["workspace_members"])
    bins = [t["name"] for t in config["targets"] if "bin" in t["kind"]]
    ok = True
    if "touchbar-recover" not in bins:
        print(f"tb-config has no `touchbar-recover` binary target (bins: {bins})", file=sys.stderr)
        ok = False
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    seen: set[str] = set()
    stack = [config["id"]]
    while stack:
        node = nodes[stack.pop()]
        for dep in node["deps"]:
            kinds = {k.get("kind") for k in dep["dep_kinds"]}
            if kinds & {None, "build"} and dep["pkg"] not in seen:
                seen.add(dep["pkg"])
                stack.append(dep["pkg"])
    bad = sorted({packages[i]["name"] for i in seen if packages[i]["name"].startswith(FORBIDDEN)})
    if bad:
        print(f"tb-config (and so touchbar-recover) builds against: {bad}", file=sys.stderr)
        ok = False
    print(f"tb-config bins {bins}; {len(seen)} crates in its build graph; forbidden: {bad or 'none'}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
