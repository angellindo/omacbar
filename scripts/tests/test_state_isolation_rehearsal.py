"""`rehearsal.py run` keeps every binary's generation store inside the run's throwaway profile:
the configurator and the daemon resolve one shared state directory (what the real binaries do:
`$XDG_STATE_HOME` when it is absolute, else `$HOME/.local/state`) inside the profile and outside
the config directory the rehearsal restores, while the caller's state directory (a temp decoy set
as `XDG_STATE_HOME`) and HOME stay untouched. Fake binaries; no cargo, no GPU."""

from __future__ import annotations

import json
import os
from pathlib import Path

from rehearsal_fixture import CONFIG_REL, Fixture, tree

FAKE = r'''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
name = Path(sys.argv[0]).name
home = os.environ["HOME"]
xdg = os.environ.get("XDG_STATE_HOME")
state = Path(xdg) if xdg and os.path.isabs(xdg) else Path(home) / ".local/state"
store = state / "touchbar-customizer/backups"
store.mkdir(parents=True, exist_ok=True)
(store / ("marker-" + name)).write_text(name)
with open(os.environ["FAKE_EVENTS"], "a") as f:
    f.write(json.dumps(dict(name=name, home=home, store=str(store))) + "\n")
args = sys.argv[1:]
def value(flag):
    return args[args.index(flag) + 1] if flag in args else None
config = Path(home) / ".config/touchbar-customizer/config.toml"
if name == "touchbar-customizer":
    config.parent.mkdir(parents=True, exist_ok=True)
    if not config.exists():
        config.write_text('label = "F1"\n')
    if value("--snap"):
        Path(value("--snap")).write_bytes(b"\x89PNG fake")
    if value("--tree"):
        Path(value("--tree")).write_text(json.dumps({"elements": []}))
else:
    for _ in sys.stdin:
        pass
    if value("--emit"):
        Path(value("--emit")).write_bytes(b"")
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
        self.tool("run", "--bin-dir", self.bin, "--report", self.tmp / "report.json", timeout=120)
        events = self.events()
        self.assertEqual({e["name"] for e in events}, {"touchbar-customizer", "touchbard"}, events)
        stores = {e["store"] for e in events}
        self.assertEqual(len(stores), 1, f"one store for the whole run: {stores}")
        store = Path(stores.pop()).resolve()
        home = Path(events[0]["home"]).resolve()
        self.assertTrue(store.is_relative_to(home), f"{store} is inside the profile {home}")
        self.assertFalse(store.is_relative_to((home / CONFIG_REL).parent), "outside the restored config dir")
        self.assertEqual(tree(self.decoy), self.before_decoy, "the caller's XDG_STATE_HOME was touched")
        self.assertOutsideUntouched()
        self.assertTrue(os.path.isdir(store))
