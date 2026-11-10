"""Bounded, offline process capture and versioned diagnostic rows."""
from __future__ import annotations

import json
import os
import selectors
import signal
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STATUSES = ("ok", "warning", "failed", "skipped")
BUILD = "nice -n 19 env CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 cargo build --workspace --locked"


def row(id, status, required, message, evidence=(), next=()):
    return dict(id=id, status=status, required=required, message=message,
                evidence=list(evidence), next=list(next))


def capture(argv, timeout=5, limit=16384, cwd=ROOT):
    """Cap combined output during execution, then kill/reap on timeout or overflow.

    Discard stderr from results: tool diagnostics may contain private config values.
    """
    env = dict(os.environ, MISE_AUTO_INSTALL="0", MISE_NOT_FOUND_AUTO_INSTALL="0",
               RUSTUP_AUTO_INSTALL="0", CARGO_NET_OFFLINE="true", PYTHONDONTWRITEBYTECODE="1")
    try:
        process = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    except OSError:
        return None, b"", "could not execute installed tool"
    output = bytearray()
    size = 0
    problem = ""
    deadline = time.monotonic() + timeout
    with selectors.DefaultSelector() as selector:
        for stream in (process.stdout, process.stderr):
            selector.register(stream, selectors.EVENT_READ)
        while selector.get_map():
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                problem = "timed out"
                break
            for key, _ in selector.select(min(remaining, 0.1)):
                chunk = os.read(key.fd, 4096)
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                size += len(chunk)
                if size > limit:
                    problem = "output limit exceeded"
                    break
                if key.fileobj is process.stdout:
                    output.extend(chunk)
            if problem:
                break
        if problem:
            os.killpg(process.pid, signal.SIGKILL)
        try:
            code = process.wait(timeout=max(0.01, deadline - time.monotonic()))
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            code = process.wait()
            problem = "timed out"
    process.stdout.close()
    process.stderr.close()
    return code, bytes(output), problem or ("tool returned failure" if code else "")


def valid_rows(checks):
    return isinstance(checks, list) and all(
        isinstance(c, dict) and isinstance(c.get("id"), str) and
        c.get("status") in STATUSES and type(c.get("required")) is bool and
        isinstance(c.get("message"), str) and bool(c["message"]) and
        all(isinstance(c.get(k), list) and all(isinstance(x, str) for x in c[k])
            for k in ("evidence", "next")) for c in checks
    ) and len({c["id"] for c in checks}) == len(checks)


def report(profile, checks):
    counts = {s: sum(c["status"] == s for c in checks) for s in STATUSES}
    counts["ready"] = not any(c["required"] and c["status"] in ("failed", "skipped") for c in checks)
    return dict(schema_version=1, profile=profile, checks=checks, summary=counts)


def decode_report(data):
    value = json.loads(data)
    if not isinstance(value, dict) or value.get("schema_version") != 1 or not valid_rows(value.get("checks")):
        raise ValueError("invalid report")
    return value
