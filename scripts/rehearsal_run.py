"""`rehearsal.py run`: the whole rehearsal in one fresh private profile.

The profile comes from `tempfile.mkdtemp` (or a new `--home`) and is the only HOME any binary sees;
the active theme is a copy of a repo fixture. Five config scenarios each go capture → edit+save
(flow script) → reopen → settle → restore → verify → reopen (valid configs only, after verify so a
default can never hide an absence). Settle carries the producing save's expectation, taken the
moment the save exits successfully (before any other process of the run starts) and checked against
what the save must leave (TOML with the edited label); a captured link's target is expected as
captured unless the save wrote through the link. A failed save or reopen, or a config or target
that is no longer what the save left, stops the run: nothing is settled or restored over those
bytes, and the scenario's store and manifest are kept. A simulator stage runs the daemon with keys
to a file inside the profile on a rehearsal-built config whose only command writes a profile-local
mark. The persistent
launcher session is compared by metadata only (never read) before and after. The report doubles as
the manifest `cleanup` takes: it lists only outputs whose content was checked against what the
producing step must leave."""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import tempfile
import time
import tomllib
from pathlib import Path

import rehearsal_profile as profile
import rehearsal_restore
from rehearsal_fs import (CONFIG_REL, ROOT, SESSION, THEME_REL, FAILED, OK, Conflict, Refused,
                          lstat_state, process_identity, read_json, refuse_shared, sha256, write_json)


def expectation(state: dict) -> str:
    """The `settle` expectation for one lstat state."""
    if state["type"] == "file":
        return state["sha256"]
    if state["type"] == "symlink":
        return f"symlink:{state['link']}"
    if state["type"] == "absent":
        return "absent"
    raise Refused(f"cannot expect a {state['type']}")

FLOW = Path(__file__).resolve().parent / "rehearsal/edit-label-save.json"
EDITED = "Rehearsal"  # the label the flow script types
BUTTON = 'id = "f1"\nlabel = "{label}"\nwidth = {{ span = 1 }}\naction = {{ type = "key", keys = ["F1"] }}\n'
HEAD = 'version = 1\ntheme = { source = "omarchy" }\n\n[[layer]]\nchord = "default"\n\n[[layer.group]]\n'
VALID = ("# rehearsal: a hand-written layout, keep this comment\n" + HEAD + "[[layer.group.button]]\n"
         + BUTTON.format(label="Mine")).encode()
LINKED = (HEAD + "[[layer.group.button]]\n" + BUTTON.format(label="Linked")).encode()
INVALID = b"not = [ valid toml \xff\n"
MARK_RUN = 'printf x >> "$HOME/rehearsal-mark"'
SIM = (HEAD + "".join(f'[[layer.group.button]]\nid = "{i}"\nlabel = "{i}"\nwidth = {{ span = 1 }}\n'
                      f"action = {a}\n\n" for i, a in (
                          ("f5", '{ type = "key", keys = ["F5"] }'), ("quiet", '{ type = "none" }'),
                          ("mark", "{ type = \"command\", run = '" + MARK_RUN + "' }")))).encode()
TAPS = "".join(f"down {2170 * (2 * k + 1) // 6}\nup\n" for k in range(3))  # one tap per third
F5_KEYS = [(1, 63, 1), (0, 0, 0), (1, 63, 0), (0, 0, 0)]  # EV_KEY KEY_F5 press, SYN, release, SYN


def session_metadata() -> dict:
    """Type, size, mtime, mode and inode of every path in the persistent session; never content."""
    out = {}
    if not SESSION.exists():
        return out
    for path in [SESSION, *SESSION.rglob("*")]:
        st = path.lstat()
        out[str(path.relative_to(SESSION))] = [st.st_mode, st.st_size, st.st_mtime_ns, st.st_ino]
    return out


def new_profile(home: Path | None) -> Path:
    if home is None:
        return Path(tempfile.mkdtemp(prefix="tb-rehearsal-")).resolve()
    if os.path.lexists(home):
        raise Refused(f"--home {home} already exists; the rehearsal only uses a new profile")
    refuse_shared(home, "--home")
    home.mkdir(mode=0o700)
    os.chmod(home, 0o700)
    return home.resolve()


def decode_keys(blob: bytes) -> list[tuple[int, int, int]]:
    return [(int.from_bytes(blob[i + 16:i + 18], "little"), int.from_bytes(blob[i + 18:i + 20], "little"),
             int.from_bytes(blob[i + 20:i + 24], "little", signed=True)) for i in range(0, len(blob), 24)]


class Rehearsal:
    def __init__(self, bin_dir: Path, root: Path, theme: str):
        self.bins = {n: bin_dir / n for n in ("touchbar-customizer", "touchbard")}
        self.root, self.home = root, root / "home"
        self.snaps, self.frames = self.home / "rehearsal-snaps", self.home / "sim-frames"
        self.theme = theme
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("XDG_") or k == "XDG_RUNTIME_DIR"}
        self.env["HOME"] = str(self.home)
        self.report = {"kind": "run", "profile": str(root), "home": str(self.home), "scenarios": [],
                       "simulator": {}, "pids": [], "artifacts": [], "stores": [], "verified": False,
                       "dirs": [str(root), str(self.home), str(root / "stores"), str(root / "manifests"),
                                str(root / "linked"), str(self.snaps), str(self.frames)]}
        self.problems: list[str] = []

    def start(self, name: str, args: list, stdin: str | None = None) -> subprocess.CompletedProcess:
        p = subprocess.Popen([str(self.bins[name]), *map(str, args)], cwd=self.home, env=self.env,
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        if identity := process_identity(p.pid):
            self.report["pids"].append(identity)
        try:
            out, err = p.communicate(stdin or "", timeout=120)
        except subprocess.TimeoutExpired:
            p.kill()
            out, err = p.communicate()
        return subprocess.CompletedProcess(p.args, p.returncode, out, err)

    def own(self, path: Path, expected) -> bool:
        """Record `path` as the run's own only if its content is what the step must leave."""
        state = lstat_state(path)
        good = state["type"] == "file" and expected(path.read_bytes())
        if good:
            self.report["artifacts"].append({"path": str(path), **state})
        return good

    def snap(self, tag: str, extra: list) -> tuple[int, str]:
        """Headless configurator run; returns its exit code and every text in its tree."""
        png, tree = self.snaps / f"{tag}.png", self.snaps / f"{tag}.json"
        p = self.start("touchbar-customizer", [*extra, "--snap", png, "--tree", tree])
        self.own(png, lambda b: b.startswith(b"\x89PNG"))
        try:
            elements = json.loads(tree.read_text())["elements"]
            self.own(tree, lambda b: True)
        except (OSError, ValueError, KeyError):
            return p.returncode, ""
        return p.returncode, "\n".join(str(e.get("text") or "") for e in elements)

    def produced(self, captured: dict) -> tuple[str, str | None]:
        """Expectations from the save that just exited, before any other process of the run
        starts: the config it left (TOML holding the edited label), and a captured link's target
        (as captured, unless the save wrote through the link)."""
        config = self.home / CONFIG_REL
        now = lstat_state(config)
        blob = config.read_bytes() if now["type"] in ("file", "symlink") and config.is_file() else None
        try:
            if blob is None or EDITED.encode() not in blob:
                raise ValueError("the edited label is missing")
            tomllib.loads(blob.decode())
        except (ValueError, UnicodeDecodeError) as e:
            raise Refused(f"the save did not leave the edited config ({now['type']}): {e}") from e
        if captured["original"]["type"] != "symlink":
            return expectation(now), None
        through = now == captured["original"]  # still the captured link: the save wrote its target
        target = lstat_state(Path(captured["target"])) if through else captured["target_state"]
        return expectation(now), expectation(target)

    def scenario(self, name: str, original_label: str | None) -> None:
        rec = {"name": name}
        self.report["scenarios"].append(rec)
        store, manifest = self.root / "stores" / name, self.root / "manifests" / f"{name}.json"
        captured = profile.capture(self.home, store, manifest, allow_caller_home=True)
        kept = f"nothing settled or restored; current bytes kept, original in {captured['store']}"
        rc, shown = self.snap(f"{name}-edit", ["--script", FLOW, "--at", "mark:end"])
        rec["save_exit"], rec["passed"] = rc, False
        if rc != 0 or EDITED not in shown:
            raise Refused(f"scenario {name}: the save failed (exit {rc}); {kept}")
        rec["expect_config"], rec["expect_target"] = self.produced(captured)
        rc, reopened = self.snap(f"{name}-reopen-edited", [])
        rec["reopen_exit"] = rc
        theme_error = re.search(r"theme[^\n]*(error|invalid|cannot)", reopened, re.I)
        if rc != 0 or EDITED not in reopened or theme_error:
            raise Refused(f"scenario {name}: the reopen failed or did not show the save (exit {rc}); {kept}")
        try:
            profile.settle(manifest, [], [], rec["expect_config"], rec["expect_target"])
            rehearsal_restore.restore(manifest)
        except (Conflict, Refused) as e:
            raise Conflict(f"scenario {name}: {e}") from e
        rec["verify"] = profile.verify(manifest)
        if original_label and not rec["verify"]:
            rec["reopen_original"] = original_label in self.snap(f"{name}-reopen-original", [])[1]
            rec["verify_after_reopen"] = profile.verify(manifest)
        rec["passed"] = all(v in (True, []) for k, v in rec.items()
                            if k in ("verify", "reopen_original", "verify_after_reopen"))
        if not rec["passed"]:
            raise Refused(f"scenario {name}: verify after restore failed; store and manifest kept")
        data = read_json(manifest)
        self.report["stores"] += [{**st, "verified": data["verified"]} for st in data["stores"]]
        self.own(manifest, lambda b: True)

    def simulator(self) -> None:
        store, manifest = self.root / "stores" / "simulator", self.root / "manifests" / "simulator.json"
        profile.capture(self.home, store, manifest, allow_caller_home=True)
        config = self.home / CONFIG_REL
        tmp = config.with_name(".config.toml.rehearsal-sim")
        tmp.write_bytes(SIM)
        os.replace(tmp, config)
        keys, mark = self.home / "keys.bin", self.home / "rehearsal-mark"
        p = self.start("touchbard", ["--sim", self.frames, "--emit", keys], stdin=TAPS)
        for _ in range(20):
            if mark.exists():
                break
            time.sleep(0.1)
        rec = {"exit": p.returncode, "f5_once": self.own(keys, lambda b: decode_keys(b) == F5_KEYS),
               "mark_once": self.own(mark, lambda b: b == b"x"),
               "frames": sum(self.own(f, lambda b: b.startswith(b"\x89PNG")) for f in sorted(self.frames.glob("*.png")))
               if self.frames.is_dir() else 0}
        captured = read_json(manifest)
        through = captured["original"]["type"] == "symlink"
        profile.settle(manifest, [], [], sha256(SIM),  # the run wrote SIM itself
                       expectation(captured["target_state"]) if through else None)
        rehearsal_restore.restore(manifest)
        rec["verify"] = profile.verify(manifest)
        rec["passed"] = rec["f5_once"] and rec["mark_once"] and not rec["verify"]
        data = read_json(manifest)
        self.report["stores"] += [{**st, "verified": data["verified"]} for st in data["stores"]]
        self.own(manifest, lambda b: True)
        self.report["simulator"] = rec

    def prepare(self, step: str) -> None:
        """The tool's own precondition for the next scenario (from the state the last one restored)."""
        config = self.home / CONFIG_REL
        if step == "absent-with-parents":
            config.parent.mkdir(parents=True, mode=0o700)
        elif step in ("commented-0640", "invalid"):
            config.write_bytes(VALID if step == "commented-0640" else INVALID)
            os.chmod(config, 0o640 if step == "commented-0640" else 0o600)
        elif step == "symlink":
            (self.root / "linked").mkdir(mode=0o700)
            target = self.root / "linked/real.toml"
            target.write_bytes(LINKED)
            config.unlink()
            config.symlink_to(os.path.relpath(target, config.parent))

    def go(self) -> None:
        self.home.mkdir(mode=0o700)
        for d in (self.snaps, self.root / "stores", self.root / "manifests"):
            d.mkdir(mode=0o700)
        theme = self.home / THEME_REL
        theme.parent.mkdir(parents=True, mode=0o700)
        shutil.copyfile(ROOT / "fixtures/omarchy-themes" / self.theme / "colors.toml", theme)
        fixture = (ROOT / "fixtures/omarchy-themes" / self.theme / "colors.toml").read_bytes()
        self.report["dirs"] += [str(self.home / p) for p in (".config", CONFIG_REL.parent)]
        self.report["dirs"] += [str(theme.parents[i]) for i in range(len(THEME_REL.parts) - 1)]
        for step, label in (("absent-no-parents", None), ("absent-with-parents", None),
                            ("commented-0640", "Mine"), ("invalid", None), ("symlink", "Linked")):
            self.prepare(step)
            self.scenario(step, label)
        self.simulator()
        config = self.home / CONFIG_REL
        self.own(theme, lambda b: b == fixture)
        self.own(self.root / "linked/real.toml", lambda b: b == LINKED)
        if lstat_state(config).get("link") == os.path.relpath(self.root / "linked/real.toml", config.parent):
            self.report["artifacts"].append({"path": str(config), **lstat_state(config)})


def run(bin_dir: Path, report: Path, home: Path | None, theme: str) -> int:
    for name in ("touchbar-customizer", "touchbard"):
        if not os.access(bin_dir / name, os.X_OK):
            raise Refused(f"{bin_dir / name} is not an executable; build first")
    if not (ROOT / "fixtures/omarchy-themes" / theme / "colors.toml").is_file():
        raise Refused(f"no fixture theme {theme}")
    before = session_metadata()
    r = Rehearsal(bin_dir, new_profile(home), theme)
    try:
        r.go()
    except (Conflict, Refused, OSError) as e:
        r.problems.append(f"stopped: {e}")
    if session_metadata() != before:
        r.problems.append("the persistent session .scratch/manual-test changed during the run")
    r.problems += [f"scenario {s['name']} failed: {s}" for s in r.report["scenarios"] if not s["passed"]]
    if not r.report["simulator"].get("passed"):
        r.problems.append(f"simulator stage failed: {r.report['simulator']}")
    r.report["problems"] = r.problems
    r.report["verified"] = not r.problems
    write_json(report, r.report)
    for line in r.problems:
        print(line)
    print(f"rehearsal {'passed' if not r.problems else 'FAILED'}; profile {r.root}; report {report}")
    return OK if not r.problems else FAILED
