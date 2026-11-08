#!/usr/bin/env python3
"""Contract gate of the ticket pipeline (docs/agents/contract.md).

QA freezes a slice's acceptance tests in a manifest and seals it; the implementer may not edit
them. The gate runs the contract under that seal, records evidence, and on --final proves that
every `required` case was seen RED (for the right reason) before it went GREEN.

Usage:
  scripts/pipeline-gate.py TB-<n> --slice N --stamp   QA: validate + seal manifest and files
  scripts/pipeline-gate.py TB-<n> --slice N           run the contract, record evidence (red allowed)
  scripts/pipeline-gate.py TB-<n> --slice N --final   seal intact, all green, never_red empty,
                                                      scripts/check.sh green, clean tree
--stamp and every --final start by deleting slice-N.final.json, before any other check; only a
passing --final writes it. --final re-checks tree, HEAD and seal after every subprocess (each cargo
case group, each check, scripts/check.sh) and stops at the first violation.
Exit: 0 pass · 1 contract not green / invalid · 2 seal broken or NEVER_RED · 3 missing intake .ack
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parent.parent
MISSING_SYMBOL_CODES = ("E0412", "E0422", "E0425", "E0432", "E0433", "E0560", "E0599", "E0609")
VALID_RED = {"RED_BEHAVIOR", "RED_MISSING_SYMBOL"}


def die(code: int, msg: str) -> None:
    print(f"pipeline-gate: {msg}", file=sys.stderr)
    sys.exit(code)


def git(*args: str) -> str:
    """Raw stdout: porcelain output starts with a status column that may be a space."""
    return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=True).stdout


def dirty_outside_scratch(porcelain: str) -> list[str]:
    """Entries of `git status --porcelain` (v1) touching a path outside `.scratch/`.

    Both sides of a rename count, so moving a file into or out of `.scratch/` is dirty.
    """
    dirty = []
    for line in porcelain.splitlines():
        if len(line) < 4:
            continue
        paths = [p.strip('"') for p in line[3:].split(" -> ")]
        if any(not p.startswith(".scratch/") for p in paths):
            dirty.append(line)
    return dirty


def now() -> str:
    return dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


# ---------- manifest + seal ----------

def load_manifest(path: Path) -> dict:
    if not path.is_file():
        die(1, f"missing manifest {path.relative_to(ROOT)} (QA writes it in the spec stage)")
    m = json.loads(path.read_bytes())
    problems = []
    if m.get("version") != 1:
        problems.append("version must be 1")
    mode = m.get("mode")
    if mode not in ("behavior", "text", "tooling"):
        problems.append("mode must be behavior | text | tooling")
    if not m.get("reason"):
        problems.append("reason is required")
    files = m.get("files") or []
    if not files:
        problems.append("files lists the frozen contract files")
    for f in files:
        if not (ROOT / f).is_file():
            problems.append(f"file not found: {f}")
    cases, checks = m.get("cases", []), m.get("checks", [])
    ids = [c.get("id") for c in cases] + [c.get("id") for c in checks]
    if len(ids) != len(set(ids)):
        problems.append("ids must be unique")
    for c in cases:
        if not re.fullmatch(r"QA-\d{2,}", c.get("id", "")):
            problems.append(f"case id {c.get('id')!r} must look like QA-01")
        if not c.get("package") or not c.get("test"):
            problems.append(f"{c.get('id')}: package and test are required")
        if not re.fullmatch(r"lib|bin:[\w-]+|test:[\w-]+", c.get("target", "")):
            problems.append(f"{c.get('id')}: target must be lib | bin:NAME | test:NAME")
        if c.get("red") not in ("required", "preserve"):
            problems.append(f"{c.get('id')}: red must be required | preserve")
        if c.get("red") == "preserve" and not c.get("reason"):
            problems.append(f"{c.get('id')}: preserve needs a reason")
    for c in checks:
        cmd = c.get("command")
        if not (isinstance(cmd, list) and cmd and all(isinstance(a, str) for a in cmd)):
            problems.append(f"check {c.get('id')}: command is a non-empty argv list (no shell)")
    if mode == "behavior" and not cases:
        problems.append("behavior mode needs at least one case")
    if mode in ("text", "tooling") and not checks:
        problems.append(f"{mode} mode needs at least one check")
    if mode == "text" and not any(c.get("id") == "visual" for c in checks):
        problems.append("text mode needs a check with id 'visual' (snapshot or tree dump)")
    if mode == "tooling" and not any(c.get("id") == "tests" for c in checks):
        problems.append("tooling mode needs a check with id 'tests'")
    if problems:
        die(1, "invalid manifest:\n  - " + "\n  - ".join(problems))
    return m


def seal_of(manifest_path: Path, manifest: dict) -> str:
    h = hashlib.sha256()
    raw = manifest_path.read_bytes()
    h.update(len(raw).to_bytes(8, "big") + raw)
    for f in sorted(manifest["files"]):
        data = (ROOT / f).read_bytes()
        name = f.encode()
        h.update(len(name).to_bytes(8, "big") + name + len(data).to_bytes(8, "big") + data)
    return "sha256:" + h.hexdigest()


# ---------- running cases ----------

def target_args(target: str) -> list[str]:
    if target == "lib":
        return ["--lib"]
    kind, name = target.split(":", 1)
    return [f"--{kind}", name]


def names_missing_symbol(out: str, symbol: str) -> bool:
    """A rustc header `error[CODE]` with a documented CODE whose backticked item is `symbol`,
    bare or path-qualified (`tb_core::layout::Width`). Substrings never match."""
    for line in out.splitlines():
        m = re.match(r"error\[(E\d{4})\]", line)
        if m and m.group(1) in MISSING_SYMBOL_CODES:
            if any(t == symbol or t.endswith("::" + symbol) for t in re.findall(r"`([^`]+)`", line)):
                return True
    return False


def classify(cases: list[dict], out: str, compiled: bool) -> dict[str, str]:
    results: dict[str, str] = {}
    for c in cases:
        name = re.escape(c["test"])
        m = re.search(rf"^test {name} \.\.\. (ok|FAILED|ignored)", out, re.M)
        if m:
            status = {"ok": "GREEN", "ignored": "SKIPPED"}.get(m.group(1), "RED_BEHAVIOR")
            if status == "RED_BEHAVIOR":
                block = re.search(rf"^---- {name} stdout ----\n(.*?)(?=^---- |^failures:|\Z)", out, re.M | re.S)
                if block and re.search(r"not (yet )?implemented", block.group(1)):
                    status = "RED_STUB"
            results[c["id"]] = status
        elif not compiled:
            hit = c.get("symbol") and names_missing_symbol(out, c["symbol"])
            results[c["id"]] = "RED_MISSING_SYMBOL" if hit else "COMPILE_ERROR"
        else:
            results[c["id"]] = "NOT_RUN"
    return results


def run_cases(manifest: dict, logdir: Path, after: Callable[[], None]) -> dict[str, str]:
    groups: dict[tuple[str, str], list[dict]] = {}
    for c in manifest["cases"]:
        groups.setdefault((c["package"], c["target"]), []).append(c)
    results: dict[str, str] = {}
    for (pkg, target), cases in groups.items():
        cmd = ["cargo", "test", "-p", pkg, "--locked", *target_args(target), "--", "--exact",
               *[c["test"] for c in cases]]
        p = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
        out = p.stdout + "\n" + p.stderr
        (logdir / f"cargo-{pkg}-{target.replace(':', '-')}.log").write_text(" ".join(cmd) + "\n\n" + out)
        after()
        compiled = "running " in p.stdout
        results.update(classify(cases, out, compiled))
    return results


def run_checks(manifest: dict, logdir: Path, after: Callable[[], None]) -> dict[str, str]:
    results = {}
    for c in manifest["checks"]:
        p = subprocess.run(c["command"], cwd=ROOT, capture_output=True, text=True)
        (logdir / f"check-{c['id']}.log").write_text(" ".join(c["command"]) + "\n\n" + p.stdout + p.stderr)
        after()
        results[c["id"]] = "GREEN" if p.returncode == 0 else "FAILED"
    return results


# ---------- main ----------

def require_committed_tree() -> None:
    dirty = dirty_outside_scratch(git("status", "--porcelain"))
    if dirty:
        die(1, "--final needs a committed tree (outside .scratch/):\n" + "\n".join(dirty))


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("ticket", help="TB-<n>")
    ap.add_argument("--slice", type=int, required=True)
    mode = ap.add_mutually_exclusive_group()
    mode.add_argument("--stamp", action="store_true")
    mode.add_argument("--final", action="store_true")
    a = ap.parse_args()

    if not re.fullmatch(r"TB-\d+", a.ticket):
        die(1, "ticket must look like TB-12")
    tdir = ROOT / ".scratch" / a.ticket
    cdir = tdir / "contract"
    mpath = cdir / f"slice-{a.slice}.json"
    seal_path = cdir / f"slice-{a.slice}.seal"
    ev_path = cdir / f"slice-{a.slice}.evidence.json"
    final_path = cdir / f"slice-{a.slice}.final.json"
    if a.stamp or a.final:
        # A final record is valid only for the seal and HEAD it was written for: a re-seal or a
        # --final that does not pass (whatever the exit code) leaves none behind.
        final_path.unlink(missing_ok=True)

    ack = tdir / ".ack"
    if not ack.is_file() or json.loads(ack.read_text()).get("confirmed") is not True:
        die(3, f"missing intake .ack ({ack.relative_to(ROOT)}): the ticket has not passed intake")
    manifest = load_manifest(mpath)
    seal = seal_of(mpath, manifest)

    if a.stamp:
        rs = [f for f in manifest["files"] if f.endswith(".rs")]
        if rs and subprocess.run(["rustfmt", "--check", "--edition", "2024", *rs], cwd=ROOT).returncode != 0:
            die(1, "contract files are not formatted: run cargo fmt before --stamp")
        seal_path.write_text(seal + "\n")
        print(f"sealed slice {a.slice}: {seal}")
        return

    if not seal_path.is_file():
        die(2, f"slice {a.slice} is not sealed: QA runs --stamp first")
    stored = seal_path.read_text().strip()
    if stored != seal:
        die(2, f"SEAL_BROKEN: manifest or frozen files changed since QA sealed them ({stored} != {seal}). "
               "A legitimate change goes back to QA to re-seal.")

    if a.final:
        require_committed_tree()

    head = git("rev-parse", "HEAD").strip()
    stamp = now().replace(":", "")
    logdir = tdir / "gate" / f"{stamp}-slice-{a.slice}{'-final' if a.final else ''}"
    logdir.mkdir(parents=True, exist_ok=True)

    def intact() -> None:
        """Seal and seal file, and on --final also tree and HEAD, as they were before the gate ran anything.

        Called after every subprocess, so a change undone by a later one is still caught."""
        if seal_of(mpath, manifest) != seal:
            die(2, "SEAL_BROKEN: frozen files changed while the gate was running")
        if not seal_path.is_file() or seal_path.read_text().strip() != seal:
            die(2, f"SEAL_BROKEN: {seal_path.name} changed or was deleted while the gate was running")
        if a.final:
            require_committed_tree()
            if git("rev-parse", "HEAD").strip() != head:
                die(1, f"final: HEAD moved from {head[:8]} while the gate was running")

    results = run_cases(manifest, logdir, intact) if manifest["mode"] == "behavior" else {}
    results.update(run_checks(manifest, logdir, intact))
    intact()

    evidence = json.loads(ev_path.read_text()) if ev_path.is_file() else {"runs": []}
    evidence["runs"].append({"at": now(), "seal": seal, "head": head, "final": a.final, "results": results,
                             "log": str(logdir.relative_to(ROOT))})
    ev_path.write_text(json.dumps(evidence, indent=2) + "\n")

    required = [c["id"] for c in manifest["cases"] if c["red"] == "required"]
    seen_red = sorted({i for r in evidence["runs"] if r["seal"] == seal
                       for i, s in r["results"].items() if s in VALID_RED and i in required})
    never_red = sorted(set(required) - set(seen_red))
    runs_under_seal = sum(1 for r in evidence["runs"] if r["seal"] == seal)

    print(f"slice {a.slice} · seal {seal[:19]}… · head {head[:8]} · run {runs_under_seal} under this seal")
    for i, s in sorted(results.items()):
        print(f"  {i}: {s}")
    print(f"  seen_red: {', '.join(seen_red) or '(none)'}")
    print(f"  never_red: {', '.join(never_red) or '(none)'}")
    print(f"  log: {logdir.relative_to(ROOT)}")

    invalid = {i: s for i, s in results.items() if s in ("SKIPPED", "NOT_RUN", "RED_STUB", "COMPILE_ERROR")}
    if invalid and not a.final:
        print("  note: SKIPPED / NOT_RUN / RED_STUB / COMPILE_ERROR never count as red (fix the test or the symbol)")
    if not a.final:
        sys.exit(0 if all(s == "GREEN" for s in results.values()) else 1)

    if any(s != "GREEN" for s in results.values()):
        die(1, "final: contract is not green")
    if never_red:
        die(2, f"NEVER_RED: required cases never seen red under this seal: {', '.join(never_red)}")
    check = subprocess.run(["scripts/check.sh"], cwd=ROOT, capture_output=True, text=True)
    (logdir / "check.log").write_text(check.stdout + check.stderr)
    intact()
    if check.returncode != 0:
        die(1, f"final: scripts/check.sh failed (log {logdir.relative_to(ROOT)}/check.log)")
    final = {"ticket": a.ticket, "slice": a.slice, "seal": seal, "head": head, "at": now(), "result": "PASS",
             "log": str(logdir.relative_to(ROOT))}
    final_path.write_text(json.dumps(final, indent=2) + "\n")
    print(f"pipeline-gate --final: PASS (slice {a.slice} @ {head[:8]})")


if __name__ == "__main__":
    main()
