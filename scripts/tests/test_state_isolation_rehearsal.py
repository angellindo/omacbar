"""`rehearsal.py run` keeps every binary's generation store inside the run's throwaway profile:
the configurator and the daemon resolve one shared state directory (what the real binaries do:
`$XDG_STATE_HOME` when it is absolute, else `$HOME/.local/state`) inside the profile and outside
the config directory the rehearsal restores, while the caller's state directory (a temp decoy set
as `XDG_STATE_HOME`) and HOME stay untouched.

The fakes follow the real binaries' ordinary flow (integration of TB-46 and TB-48): a generation
before every config write, a save receipt (`TB_SAVE_RECEIPT`) for each confirmed save, and an
explicit refusal, with no receipt, of a save over a symlinked config. So the run passes through all
five scenarios to the simulator, where the daemon runs on the run's own regular config. The test
fails if either binary is not reached by that route. Fake binaries; no cargo, no GPU."""

from __future__ import annotations

import json
import os
import stat
from pathlib import Path

from rehearsal_fixture import CONFIG_REL, Fixture, tree

FAKE = r'''#!/usr/bin/env python3
import hashlib, json, os, re, struct, sys
from pathlib import Path
name = Path(sys.argv[0]).name
home = Path(os.environ["HOME"])
xdg = os.environ.get("XDG_STATE_HOME")
store = (Path(xdg) if xdg and os.path.isabs(xdg) else home / ".local/state") / "touchbar-customizer/backups"
config = home / ".config/touchbar-customizer/config.toml"
args = sys.argv[1:]
def value(flag):
    return args[args.index(flag) + 1] if flag in args else None
event = dict(name=name, home=str(home), xdg=xdg, store=str(store), generations=[],
             config_type="symlink" if config.is_symlink() else "file" if config.is_file() else "absent")
def replace(path, data):
    tmp = path.with_name(f".{path.name}.fake-tmp")
    tmp.write_bytes(data)
    os.chmod(tmp, 0o600)
    os.replace(tmp, path)
def keep(reason):
    store.mkdir(mode=0o700, parents=True, exist_ok=True)
    gen = store / str(1 + max((int(p.name) for p in store.iterdir() if p.name.isdigit()), default=0))
    gen.mkdir(mode=0o700)
    old = config.read_bytes() if config.is_file() and not config.is_symlink() else None
    (gen / "manifest.toml").write_text(f'reason = "{reason}"\nkind = "{"absent" if old is None else "file"}"\n')
    os.chmod(gen / "manifest.toml", 0o600)
    if old is not None:
        (gen / "config.toml").write_bytes(old)
        os.chmod(gen / "config.toml", 0o600)
    event["generations"].append(gen.name)
if not os.path.lexists(config):
    config.parent.mkdir(parents=True, exist_ok=True)
    keep("first-write")
    replace(config, b'label = "F1"\n')
try:  # an unreadable (invalid) file opens as the default layout, as in the real editor
    current = config.read_bytes().decode()
except (OSError, UnicodeDecodeError):
    current = 'label = "F1"\n'
if name == "touchbar-customizer":
    status = "opened config"
    if value("--script"):
        saved = re.sub(r'label = "[^"]*"', 'label = "Rehearsal"', current, count=1) + "# saved\n"
        if config.is_symlink():
            status = f"not saved {config}: config file `{config}` is a symbolic link; writing it is refused, nothing was written"
        else:
            keep("save")
            replace(config, saved.encode())
            digest = hashlib.sha256(saved.encode()).hexdigest()
            replace(Path(os.environ["TB_SAVE_RECEIPT"]), json.dumps({"config": str(config), "sha256": digest}).encode())
            status = "saved config"
        current = saved
    if value("--snap"):
        Path(value("--snap")).write_bytes(b"\x89PNG fake")
    if value("--tree"):
        texts = [status, *re.findall(r'label = "([^"]*)"', current)]
        Path(value("--tree")).write_text(json.dumps({"elements": [{"key": f"e{i}", "text": t} for i, t in enumerate(texts)]}))
else:
    for _ in sys.stdin:
        pass
    if value("--emit"):
        keys = b"".join(struct.pack("<qqHHi", 0, 0, *e) for e in ((1, 63, 1), (0, 0, 0), (1, 63, 0), (0, 0, 0)))
        Path(value("--emit")).write_bytes(keys if 'keys = ["F5"]' in current else b"")
    if value("--sim"):
        Path(value("--sim")).mkdir(parents=True, exist_ok=True)
        (Path(value("--sim")) / "frame-0001.png").write_bytes(b"\x89PNG fake frame")
    if "rehearsal-mark" in current:
        with open(home / "rehearsal-mark", "ab") as f:
            f.write(b"x")
with open(os.environ["FAKE_EVENTS"], "a") as f:
    f.write(json.dumps(event) + "\n")
'''


class RehearsalState(Fixture):
    def setUp(self):
        super().setUp()
        self.bin = self.tmp / "fake bin"
        self.bin.mkdir()
        for name in ("touchbar-customizer", "touchbard"):
            p = self.bin / name
            p.write_text(FAKE)
            p.chmod(0o755)
        self.events_file = self.tmp / "events.jsonl"
        self.decoy = self.tmp / "caller state"
        (self.decoy / "touchbar-customizer/backups/1").mkdir(parents=True)
        (self.decoy / "touchbar-customizer/backups/1/manifest.toml").write_text('kind = "absent"\n')
        self.before_decoy = tree(self.decoy)
        self.tmpdir = self.tmp / "tmpdir"
        self.tmpdir.mkdir()
        self.env.update(FAKE_EVENTS=str(self.events_file), TMPDIR=str(self.tmpdir),
                        XDG_STATE_HOME=str(self.decoy))

    def events(self):
        if not self.events_file.exists():
            return []
        return [json.loads(line) for line in self.events_file.read_text().splitlines()]

    def test_every_binary_shares_one_store_inside_the_profile_and_the_callers_state_is_untouched(self):
        report_path = self.tmp / "report.json"
        p = self.tool("run", "--bin-dir", self.bin, "--report", report_path, timeout=120)
        self.assertEqual(p.returncode, 0, f"the run reaches every stage by its ordinary route: {p.stdout}{p.stderr}")
        report = json.loads(report_path.read_text())
        home = Path(report["home"]).resolve()
        events = self.events()
        daemons = [e for e in events if e["name"] == "touchbard"]
        self.assertEqual(len(daemons), 1, f"the daemon ran once, in the simulator stage: {events}")
        self.assertEqual(daemons[0]["config_type"], "file", "on the run's own regular config")
        sim = report["simulator"]
        self.assertTrue(sim.get("passed") and sim.get("f5_once") and sim.get("mark_once"), sim)
        for path in ("keys.bin", "rehearsal-mark"):
            self.assertTrue(any(Path(a["path"]).resolve() == home / path for a in report["artifacts"]),
                            f"the daemon's {path} is in the profile")
        self.assertGreaterEqual(sum(e["name"] == "touchbar-customizer" for e in events), 5, events)
        for e in events:
            self.assertEqual(Path(e["home"]).resolve(), home, f"{e['name']} ran with the profile as HOME")
            self.assertIsNone(e["xdg"], f"{e['name']} was not handed the caller's XDG_STATE_HOME")
        stores = {e["store"] for e in events}
        self.assertEqual(len(stores), 1, f"one store for the whole run: {stores}")
        store = Path(stores.pop()).resolve()
        self.assertEqual(store, home / ".local/state/touchbar-customizer/backups")
        self.assertFalse(store.is_relative_to((home / CONFIG_REL).parent), "outside the restored config dir")
        # What the binaries kept is on disk, private, in the profile's store and nowhere else.
        kept = sorted((g for e in events for g in e["generations"]), key=int)
        self.assertGreaterEqual(len(kept), 4, "a generation before each of the four saves")
        self.assertEqual(sorted((d.name for d in store.iterdir()), key=int), kept)
        for d in store.iterdir():
            self.assertEqual(stat.S_IMODE(d.stat().st_mode), 0o700)
            self.assertTrue(all(stat.S_IMODE(f.stat().st_mode) == 0o600 for f in d.iterdir()))
        self.assertEqual(tree(self.decoy), self.before_decoy, "the caller's XDG_STATE_HOME was touched")
        self.assertOutsideUntouched()
        self.assertTrue(os.path.isdir(store))
