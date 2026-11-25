"""Read-only manifests of the laptop state for the hardware runbook's baselines A and B, and their
comparison. Reads only under `--root` and asks `systemctl` only `is-active` / `is-enabled`; a file
only root can read is recorded as a human step, never read with privilege. The output directory is
private (0700) and must be outside the repository. Item list: research 0007 §1."""

from __future__ import annotations

import json
import os
import shutil
import stat
import subprocess
from datetime import datetime, timezone
from pathlib import Path

from rehearsal_fs import ROOT, Refused, inside, sha256, write_json

SERVICE = "macbook-touchbar-local.service"
NEEDS_SUDO = "needs sudo — human step"
APPLE, T1_IDS = "05ac", ("8600", "1281")  # iBridge, and its recovery identity (research 0001)
SERVICE_FILES = (
    "usr/local/lib/macbook-touchbar-local.py",
    f"etc/systemd/system/{SERVICE}",
    "etc/systemd/system/systemd-suspend.service.d/94-macbook-touchbar-display.conf",
    "var/lib/macbook-touchbar-local/installed.json",
)
KEY_MODE_FILES = (
    "usr/lib/modprobe.d/apple-touchbar.conf",
    "etc/modprobe.d/90-macbook-pr298-fn.conf",
    "etc/modprobe.d/apple-touchbar-idle.conf",
    "usr/lib/udev/rules.d/99-ibridge.rules",
    "etc/udev/rules.d/99-mbp143-t1-nowake.rules",
    "usr/lib/systemd/system-sleep/mbp143-t1-socw",
)
MODULES = ("apple_ibridge", "apple_touchbar", "appletbdrm", "hid_appletb_bl", "hid_appletb_kbd",
           "hid_multitouch")


def read_text(path: Path) -> str:
    try:
        return path.read_text().strip()
    except FileNotFoundError:
        return "absent"
    except PermissionError:
        return NEEDS_SUDO
    except OSError as e:
        return f"unreadable: {e.strerror}"


def file_record(path: Path) -> dict | str:
    try:
        st = path.lstat()
        data = path.read_bytes()
    except FileNotFoundError:
        return "absent"
    except PermissionError:
        return NEEDS_SUDO
    return {"sha256": sha256(data), "mode": oct(stat.S_IMODE(st.st_mode)), "size": st.st_size,
            "uid": st.st_uid, "gid": st.st_gid, "mtime_ns": st.st_mtime_ns}


def usb(root: Path) -> dict:
    out = {}
    for dev in sorted((root / "sys/bus/usb/devices").glob("*")):
        if read_text(dev / "idVendor") == APPLE and read_text(dev / "idProduct") in T1_IDS:
            out[dev.name] = {"idProduct": read_text(dev / "idProduct"),
                             "bConfigurationValue": read_text(dev / "bConfigurationValue")}
    return out


def service_state() -> dict:
    exe = shutil.which("systemctl")
    if not exe:
        return {"is-active": "systemctl unavailable", "is-enabled": "systemctl unavailable"}
    out = {}
    for verb in ("is-active", "is-enabled"):
        p = subprocess.run([exe, verb, SERVICE], capture_output=True, text=True, timeout=30)
        out[verb] = p.stdout.strip() or f"exit {p.returncode}"
    return out


def collect(root: Path) -> dict:
    modules = root / "usr/lib/modules"
    return {
        "usb_t1": usb(root),
        "service": service_state(),
        "files": {f"/{rel}": file_record(root / rel) for rel in SERVICE_FILES + KEY_MODE_FILES},
        "mem_sleep": read_text(root / "sys/power/mem_sleep"),
        "kernel_release": read_text(root / "proc/sys/kernel/osrelease"),
        "installed_module_trees": sorted(p.name for p in modules.iterdir()) if modules.is_dir() else [],
        "loaded_srcversion": {m: read_text(root / "sys/module" / m / "srcversion") for m in MODULES},
        "cmdline": read_text(root / "proc/cmdline"),
    }


def baseline(label: str, out: Path, root: Path) -> str:
    if inside(out, ROOT):
        raise Refused(f"--out {out} is inside the repository; baselines stay private")
    if os.path.lexists(out):
        raise Refused(f"--out {out} already exists")
    data = {"label": label, "root": str(root), "taken_at": datetime.now(timezone.utc).isoformat(),
            "items": collect(root)}
    out.mkdir(mode=0o700)
    os.chmod(out, 0o700)
    path = out / f"baseline-{label}.json"
    write_json(path, data)
    return f"baseline {label} written to {path} (private; do not commit)"


def load(where: Path) -> dict:
    files = sorted(where.glob("baseline-*.json")) if where.is_dir() else [where]
    if len(files) != 1:
        raise Refused(f"{where} holds {len(files)} baselines; expected one")
    return json.loads(files[0].read_text())["items"]


def flatten(value, prefix: str = "") -> dict:
    if isinstance(value, dict):
        out = {}
        for k, v in value.items():
            out.update(flatten(v, f"{prefix}{k}."))
        return out
    return {prefix.rstrip("."): value}


def compare(a: Path, b: Path) -> list[str]:
    left, right = flatten(load(a)), flatten(load(b))
    return [f"{k}: {left.get(k, 'missing')} → {right.get(k, 'missing')}"
            for k in sorted(set(left) | set(right)) if left.get(k) != right.get(k)]
