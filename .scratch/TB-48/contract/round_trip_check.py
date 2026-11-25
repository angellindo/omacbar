#!/usr/bin/env python3
"""End-to-end recovery round trip with the real binaries, headless, in one isolated profile.

Builds `touchbar-customizer` and `touchbard` from this checkout (cargo, --locked), then in a temp
profile (HOME and every XDG_* dir inside it; DISPLAY and WAYLAND_DISPLAY removed, so no screen is
used) seeds a hand-written config (comments, mode 0640, a red button) and starts `touchbard --sim`
on that same HOME / XDG_STATE_HOME. The configurator binary then runs four times with `--script`
(each run is one session: start, input, quit):
  1. reset the default buttons, cancel  -> file bytes, mode and inode unchanged, nothing kept;
  2. reset the default buttons, save    -> one `factory-reset` generation (store/1) holding the
     exact seeded bytes, mode 0640 kept; the daemon's next frame (frame 2) is pixel for pixel the
     frame a second daemon draws for the exact saved file (the design changed, not only a count);
  3. reopen, no input                   -> the editor shows the factory layout, clean, the store
     location and `recover-1` labelled "before factory reset";
  4. reopen, tap `recover-1`            -> the seeded bytes and 0640 are back, the factory file is
     kept as a `recover` generation (store/2), generation 1 is untouched, the status says restored;
     the daemon's frame 3 is pixel for pixel its frame 1.
Closing the daemon's stdin stops it (exit 0) with no further frame; the store then holds exactly
generations 1 and 2 (the daemon only read). Nothing outside the temp profile is used; no device is
opened (`--emit` goes to a file). Exit 0 pass, 1 fail (the failing stage is printed).
Optional: TB_ROUND_TRIP_DIR=<dir> keeps the profile, scripts, trees, PNGs and logs there.
"""

from __future__ import annotations

import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import time
import tomllib
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
WAIT = 240.0  # seconds; the shared laptop pauses jobs for temperature
SIZE = "1200x1600"  # tall enough that every sidebar control is on screen for a script tap
RED = (0xE0, 0x30, 0x40)
SEEDED = b"""# my touch bar, written by hand: keep this comment
version = 1
theme = { source = "omarchy" }

[[layer]]
chord = "default"

[[layer.group]]
[[layer.group.button]]
id = "mine"
label = "Mine"
width = { span = 1 }
action = { type = "key", keys = ["F9"] }
style = { fill = "#e03040" }
# a note at the end
"""


class Failed(Exception):
    pass


def check(ok: bool, stage: str, why: str) -> None:
    if not ok:
        raise Failed(f"{stage}: {why}")


def build() -> dict[str, Path]:
    cmd = ["cargo", "build", "--locked", "-p", "tb-app", "-p", "tb-daemon", "--bins",
           "--message-format=json-render-diagnostics"]
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    sys.stderr.write(p.stderr[-4000:])
    check(p.returncode == 0, "build", f"cargo build exit {p.returncode}")
    found = {}
    for line in p.stdout.splitlines():
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        name = msg.get("target", {}).get("name")
        if msg.get("reason") == "compiler-artifact" and name in ("touchbar-customizer", "touchbard"):
            found[name] = Path(msg["executable"])
    check(set(found) == {"touchbar-customizer", "touchbard"}, "build", f"binaries {found}")
    for name, exe in found.items():
        print(f"built {name}: {exe}")
    return found


def png_rgba(path: Path) -> tuple[int, int, bytes] | None:
    """RGBA8 pixels of a non-interlaced 8-bit RGB/RGBA PNG; None while it is not complete."""
    try:
        data = path.read_bytes()
    except OSError:
        return None
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        return None
    pos, idat, ihdr, ended = 8, b"", None, False
    while pos + 8 <= len(data):
        length, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + length]
        if len(body) < length:
            return None
        if kind == b"IHDR":
            ihdr = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            ended = True
            break
        pos += 12 + length
    if not ended or ihdr is None:
        return None
    width, height, depth, color, _, _, interlace = ihdr
    if depth != 8 or color not in (2, 6) or interlace:
        raise Failed(f"frame {path}: unsupported PNG depth {depth} color {color} interlace {interlace}")
    channels = 4 if color == 6 else 3
    raw = zlib.decompress(idat)
    stride = width * channels
    out = bytearray()
    prev = bytearray(stride)
    for y in range(height):
        f = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for i in range(stride):
            a = line[i - channels] if i >= channels else 0
            b = prev[i]
            c = prev[i - channels] if i >= channels else 0
            if f == 1:
                line[i] = (line[i] + a) & 255
            elif f == 2:
                line[i] = (line[i] + b) & 255
            elif f == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif f == 4:
                p_ = a + b - c
                pa, pb, pc = abs(p_ - a), abs(p_ - b), abs(p_ - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        prev = line
        if channels == 4:
            out += line
        else:
            for x in range(width):
                out += line[x * 3:x * 3 + 3] + b"\xff"
    return width, height, bytes(out)


def differing(a: bytes, b: bytes) -> int:
    """Pixels whose RGB differ by more than 2 in some channel."""
    return sum(1 for i in range(0, len(a), 4)
               if abs(a[i] - b[i]) > 2 or abs(a[i + 1] - b[i + 1]) > 2 or abs(a[i + 2] - b[i + 2]) > 2)


def pixel(frame: bytes, x: int, y: int) -> tuple[int, int, int]:
    i = (y * 2170 + x) * 4
    return frame[i], frame[i + 1], frame[i + 2]


def near(got, want) -> bool:
    return all(abs(g - w) <= 2 for g, w in zip(got, want))


class Profile:
    def __init__(self, root: Path):
        self.root = root
        self.home = root / "home"
        self.state = root / "xdg/state"
        self.config = self.home / ".config/touchbar-customizer/config.toml"
        self.store = self.state / "touchbar-customizer/backups"
        self.env = dict(os.environ, HOME=str(self.home), XDG_STATE_HOME=str(self.state),
                        XDG_CONFIG_HOME=str(root / "xdg/config"), XDG_CACHE_HOME=str(root / "xdg/cache"),
                        XDG_DATA_HOME=str(root / "xdg/data"))
        for var in ("DISPLAY", "WAYLAND_DISPLAY"):
            self.env.pop(var, None)
        for d in (self.home, self.state, root / "xdg/config", root / "xdg/cache", root / "xdg/data"):
            d.mkdir(parents=True, exist_ok=True)

    def seed(self, data: bytes, mode: int) -> None:
        self.config.parent.mkdir(parents=True, exist_ok=True)
        self.config.write_bytes(data)
        os.chmod(self.config, mode)

    def store_entries(self) -> list[str]:
        return sorted(p.name for p in self.store.iterdir()) if self.store.is_dir() else []

    def manifest(self, gen: str) -> dict:
        return tomllib.loads((self.store / gen / "manifest.toml").read_text())


class Daemon:
    def __init__(self, exe: Path, profile: Profile, name: str):
        self.dir = profile.root / name
        self.dir.mkdir()
        self.sim = self.dir / "sim"
        self.log = self.dir / "stderr.log"
        self.proc = subprocess.Popen([str(exe), "--sim", str(self.sim), "--emit", str(self.dir / "keys.bin")],
                                     env=profile.env, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                                     stderr=self.log.open("wb"))

    def frame(self, n: int, stage: str) -> bytes:
        path = self.sim / f"frame-{n:04}.png"
        deadline = time.monotonic() + WAIT
        while True:
            got = png_rgba(path)
            if got:
                w, h, pixels = got
                check((w, h) == (2170, 60), stage, f"{path.name} is {w}x{h}")
                return pixels
            check(self.proc.poll() is None, stage, f"daemon exited {self.proc.returncode}: {self.log.read_text()}")
            check(time.monotonic() < deadline, stage, f"{path} never appeared: {self.log.read_text()}")
            time.sleep(0.05)

    def close(self, stage: str) -> None:
        self.proc.stdin.close()
        try:
            rc = self.proc.wait(timeout=WAIT)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            raise Failed(f"{stage}: closing stdin did not stop the daemon")
        check(rc == 0, stage, f"daemon exit {rc}: {self.log.read_text()}")

    def frames(self) -> list[str]:
        return sorted(p.name for p in self.sim.glob("frame-*.png"))

    def kill(self) -> None:
        if self.proc.poll() is None:
            self.proc.kill()
            self.proc.wait()


def session(exe: Path, profile: Profile, name: str, events: list[dict]) -> dict[str, dict]:
    """One configurator session: run the script headless, quit; the element tree at its end."""
    d = profile.root / name
    d.mkdir()
    script = d / "script.json"
    script.write_text(json.dumps(events + [{"t": events[-1]["t"] + 1.0 if events else 0.5,
                                            "type": "mark", "name": "end"}]))
    tree = d / "tree.json"
    cmd = [str(exe), "--script", str(script), "--at", "mark:end", "--size", SIZE,
           "--snap", str(d / "snap.png"), "--tree", str(tree)]
    p = subprocess.run(cmd, env=profile.env, capture_output=True, text=True, timeout=WAIT * 2)
    (d / "stderr.log").write_text(p.stderr)
    check(p.returncode == 0, name, f"touchbar-customizer exit {p.returncode}: {p.stderr.strip()[-2000:]}")
    elements = json.loads(tree.read_text())["elements"]
    keyed = {}
    for e in elements:
        if e.get("key"):
            keyed.setdefault(e["key"], e)
    for key, e in keyed.items():
        inside = e.get("path", "") + "/"
        texts = [x["text"] for x in elements if x.get("path", "").startswith(inside) and x.get("text")]
        e["shown"] = e.get("text") or " ".join(texts)
    return keyed


def tap(key: str, t: float) -> dict:
    return {"t": t, "type": "tap", "key": key}


def stat(path: Path) -> tuple[bytes, int, int]:
    st = path.lstat()
    return path.read_bytes(), st.st_mode & 0o7777, st.st_ino


def run(work: Path) -> None:
    exes = build()
    app, bard = exes["touchbar-customizer"], exes["touchbard"]
    p = Profile(work / "profile")
    p.seed(SEEDED, 0o640)
    seeded = stat(p.config)
    daemon = Daemon(bard, p, "daemon")
    reference = None
    try:
        first = daemon.frame(1, "start")
        check(near(pixel(first, 100, 30), RED), "start", f"the daemon draws the seeded red button: {pixel(first, 100, 30)}")
        print("ok start: touchbard --sim draws the seeded design (frame 1)")

        tree = session(app, p, "1-reset-cancel", [tap("reset-factory", 0.3), tap("reset-cancel", 0.9)])
        check(stat(p.config) == seeded, "reset-cancel", "the file's bytes, mode or inode changed")
        check(p.store_entries() == [], "reset-cancel", f"something was kept: {p.store_entries()}")
        check("no pending" in tree["dirty-summary"]["shown"], "reset-cancel", tree["dirty-summary"]["shown"])
        print("ok reset-cancel: nothing written, nothing kept, editor clean")

        tree = session(app, p, "2-reset-save", [tap("reset-factory", 0.3), tap("save", 0.9)])
        saved = p.config.read_bytes()
        check(saved != SEEDED and p.config.lstat().st_mode & 0o777 == 0o640, "reset-save", "factory file, mode 0640")
        check(tomllib.loads(saved.decode())["theme"] == {"source": "omarchy"}, "reset-save", "theme source kept")
        check(p.store_entries() == ["1"], "reset-save", f"store {p.store_entries()}")
        m = p.manifest("1")
        check((m.get("reason"), m.get("kind")) == ("factory-reset", "file"), "reset-save", f"manifest {m}")
        check((p.store / "1/config.toml").read_bytes() == SEEDED, "reset-save", "generation 1 holds the seeded bytes")
        status = tree["status"]["shown"]
        check(str(p.store / "1") in status, "reset-save", f"status names the generation: {status}")
        second = daemon.frame(2, "reset-save")
        ref = Profile(work / "reference")
        ref.seed(saved, 0o640)
        reference = Daemon(bard, ref, "daemon")
        expected = reference.frame(1, "reference")
        reference.close("reference")
        check(differing(second, expected) == 0, "reset-save", "frame 2 is not the frame of the saved file")
        check(differing(first, second) > 1000, "reset-save", "the design did not change on the bar")
        print("ok reset-save: generation 1 = seeded bytes; frame 2 = a fresh daemon's frame of the saved file")

        tree = session(app, p, "3-reopen", [])
        check(p.config.read_bytes() == saved and p.store_entries() == ["1"], "reopen", "reopening wrote something")
        check("preview-mine" not in tree, "reopen", "the reopened editor still draws the seeded design")
        check("no pending" in tree["dirty-summary"]["shown"], "reopen", tree["dirty-summary"]["shown"])
        check("recover-1" in tree, "reopen", "generation 1 is not listed as `recover-1`")
        check("before factory reset" in tree["recover-1"]["shown"].lower(), "reopen", tree["recover-1"]["shown"])
        check("backups-location" in tree and str(p.store) in tree["backups-location"]["shown"], "reopen",
              "the store location is not shown")
        print("ok reopen: factory layout, clean, `recover-1` before factory reset, location shown")

        tree = session(app, p, "4-restore", [tap("recover-1", 0.5)])
        check(p.config.read_bytes() == SEEDED, "restore", "the seeded bytes are not back")
        check(p.config.lstat().st_mode & 0o7777 == 0o640, "restore", "mode 0640 is not back")
        check(p.store_entries() == ["1", "2"], "restore", f"store {p.store_entries()}")
        m = p.manifest("2")
        check((m.get("reason"), m.get("kind")) == ("recover", "file"), "restore", f"manifest {m}")
        check((p.store / "2/config.toml").read_bytes() == saved, "restore", "generation 2 holds the factory file")
        check((p.store / "1/config.toml").read_bytes() == SEEDED, "restore", "generation 1 changed")
        status = tree["status"]["shown"].lower()
        check("restored" in status and "not restored" not in status, "restore", status)
        third = daemon.frame(3, "restore")
        check(differing(first, third) == 0, "restore", "frame 3 is not the seeded design of frame 1")
        print("ok restore: seeded bytes and 0640 back, factory file kept as generation 2, frame 3 = frame 1")

        daemon.close("close")
        check(daemon.frames() == ["frame-0001.png", "frame-0002.png", "frame-0003.png"], "close",
              f"frames {daemon.frames()}")
        check(p.store_entries() == ["1", "2"], "close", f"store {p.store_entries()}")
        print("ok close: the daemon stopped with exactly three frames; the store holds generations 1 and 2")
    finally:
        daemon.kill()
        if reference:
            reference.kill()


def main() -> int:
    keep = os.environ.get("TB_ROUND_TRIP_DIR")
    work = Path(keep) if keep else Path(tempfile.mkdtemp(prefix="tb-round-trip-"))
    work.mkdir(parents=True, exist_ok=True)
    print(f"work dir: {work}")
    try:
        run(work)
    except Failed as e:
        print(f"FAIL {e}", file=sys.stderr)
        return 1
    finally:
        if not keep:
            shutil.rmtree(work, ignore_errors=True)
    print("round trip: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
