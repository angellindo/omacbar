"""`rehearsal.py run` with fake configurator/daemon binaries: a fresh private profile outside the
repo and the caller's HOME, a fixture theme, keys to a file inside the profile, no command but the
profile-local mark, and the persistent launcher session left alone. The fakes behave like the
normal-flow binaries (defaults on open, temp + rename save, labels in the tree, F5 once, a frame,
the mark), so a nominal run passes; the expectation each scenario settles with must match what the
producing save wrote. The run learns those bytes from the save receipt, the interface the real
configurator and the fake share (`TB_SAVE_RECEIPT`, below), never by reading the config after the
configurator returns. An injected edit, failed save or failed reopen, or a save without a valid
receipt of its own, is never settled or restored: its bytes survive the run and the cleanup. No
cargo, no GPU.

Protected writes (integration with TB-48): like the real binaries, the fakes keep a private
generation in the state directory before every config write and refuse to save over a symlinked
config (no write through the link, no replacement, no receipt). So a nominal run has four saves
with receipts and one explicit symlink refusal; the simulator puts a known regular config of its
own at the link's path and restores the link afterwards. Cleanup keeps every generation."""

from __future__ import annotations

import json
import os
import stat
from pathlib import Path

from rehearsal_fixture import THEME_REL, Fixture, state, tree

FAKE = r'''#!/usr/bin/env python3
# Fake configurator / daemon that behaves like the normal-flow binaries: the configurator writes
# defaults when the config is absent, a `--script` run saves the edited label (temp + rename) and
# the tree lists the status and every label of the editor's layout; the daemon presses F5 once,
# writes one frame and runs the profile-local mark. Every run is logged to FAKE_EVENTS, with the
# SHA-256 of what a save wrote, which is the producer's own expectation, and the lstat type of the
# config it found (`config_type`).
# Protected writes (as the real binaries since TB-48): before any write of the config (first
# default, save) a generation is kept in <state>/touchbar-customizer/backups/<id>/ (0700 dirs,
# 0600 files: manifest.toml, and config.toml holding the replaced bytes), <state> being
# XDG_STATE_HOME when absolute, else HOME/.local/state; the ids are logged as `generations`. A
# symlinked config is shown but never written through or replaced: a save over it is refused with
# "not saved <path>: config file `<path>` is a symbolic link; ...", the draft stays in the editor,
# nothing is written and no receipt is emitted (logged as `refused`).
# Save receipt (the interface shared with the real configurator): when TB_SAVE_RECEIPT names a
# path, each confirmed explicit save writes there, by temp + rename, the JSON object
#   {"config": "<the config path it saved>", "sha256": "<64 lowercase hex>"}
# hashed from the bytes that save wrote (in memory), never by reading the file back. A failed or
# refused save writes no receipt.
# FAKE_INJECT names a one-shot token file whose content picks one injected event (logged with the
# path and bytes it leaves, and `original` when another version must survive next to it):
#   external     the first reopen after a save finds the saved file replaced by another editor
#   target       the save over the symlinked config is refused as usual; later, while the daemon
#                runs on the simulator's regular config, a third party edits the old link target
#                (the link's original target bytes are logged as `original`)
#   save-fails   the first save crashes half way, leaving a partial file, exit 1
#   reopen-fails the first reopen after a save exits 1 before showing anything
#   second-editor the save over the personal layout (label "Mine") completes with its receipt, then,
#                before the configurator exits, a second ordinary editor saves another valid
#                design that keeps the same "Rehearsal" label (span 2) by temp + rename; the
#                personal layout is logged as `original`
#   no-receipt   the second save completes but writes no receipt of its own
#   bad-receipt  the first save completes but its receipt is not a valid one (short hash)
#   link-saved   an unprotected configurator: the save over the symlinked config replaces the link
#                with a regular file and writes a valid receipt for it
#   link-other-refusal the save over the symlinked config is refused for another reason (a theme
#                issue), without the symbolic-link refusal; nothing written, no receipt
#   refuse-regular the first save over a regular file is refused with the symbolic-link wording,
#                nothing written, no receipt
import hashlib, json, os, re, struct, sys
from pathlib import Path
name = Path(sys.argv[0]).name
home = Path(os.environ["HOME"])
config = home / ".config/touchbar-customizer/config.toml"
theme = home / ".local/state/omarchy/current/theme/colors.toml"
xdg = os.environ.get("XDG_STATE_HOME")
store = (Path(xdg) if xdg and os.path.isabs(xdg) else home / ".local/state") / "touchbar-customizer/backups"
args = sys.argv[1:]
MARK = "# saved by fake\n"
DEFAULT = 'label = "F1"\n'
def value(flag):
    return args[args.index(flag) + 1] if flag in args else None
def text(path):
    try:
        return path.read_bytes().decode()
    except (OSError, UnicodeDecodeError):
        return None
def kind(path):
    return "symlink" if path.is_symlink() else "file" if path.is_file() else "absent" if not os.path.lexists(path) else "other"
event = dict(name=name, args=args, home=str(home), cwd=os.getcwd(), store=str(store),
             receipt=os.environ.get("TB_SAVE_RECEIPT"), config_type=kind(config), generations=[],
             theme=theme.read_text() if theme.is_file() else None,
             config=config.read_text(errors="replace") if config.is_file() else None)
token = Path(os.environ["FAKE_INJECT"]) if os.environ.get("FAKE_INJECT") else None
inject = token.read_text().strip() if token and token.is_file() else None
def injected(kind, path, data):
    token.unlink()
    event["injected"] = {"kind": kind, "path": str(path), "bytes": data.decode()}
def log():
    with open(os.environ["FAKE_EVENTS"], "a") as f:
        f.write(json.dumps(event) + "\n")
def replace(path, data, mode=None):
    tmp = path.with_name(f".{path.name}.fake-tmp")
    tmp.write_bytes(data)
    if mode is not None:
        os.chmod(tmp, mode)
    os.replace(tmp, path)
def keep(reason):
    # The generation of what the write replaces, kept before the write.
    store.mkdir(mode=0o700, parents=True, exist_ok=True)
    ids = [int(p.name) for p in store.iterdir() if p.name.isdigit()]
    gen = store / str(max(ids, default=0) + 1)
    gen.mkdir(mode=0o700)
    old = config.read_bytes() if config.is_file() and not config.is_symlink() else None
    (gen / "manifest.toml").write_text(f'config = "{config}"\nreason = "{reason}"\nkind = "{"file" if old is not None else "absent"}"\n')
    os.chmod(gen / "manifest.toml", 0o600)
    if old is not None:
        (gen / "config.toml").write_bytes(old)
        os.chmod(gen / "config.toml", 0o600)
    event["generations"].append(gen.name)
def first_default():
    if not os.path.lexists(config):
        config.parent.mkdir(parents=True, exist_ok=True)
        keep("first-write")
        replace(config, DEFAULT.encode(), 0o600)
if os.environ.get("FAKE_TOUCH_SESSION"):
    with open(os.environ["FAKE_TOUCH_SESSION"], "ab") as f:
        f.write(b"!")
if name == "touchbar-customizer":
    first_default()
    status = "opened config"
    draft = None
    if value("--script"):
        if inject == "save-fails":
            keep("save")
            partial = b'label = "Rehe'
            with open(config, "wb") as f:
                f.write(partial)
            injected("save-fails", config, partial)
            log()
            sys.exit(1)
        base = text(config) or DEFAULT
        saved = (re.sub(r'label = "[^"]*"', 'label = "Rehearsal"', base, count=1) + MARK).encode()
        refusal = f"not saved {config}: config file `{config}` is a symbolic link; writing it is refused, nothing was written"
        if config.is_symlink() and inject != "link-saved":
            draft = saved.decode()
            event["refused"] = "symlink"
            event["link"] = os.readlink(config)
            target = Path(os.path.realpath(config))
            event["target_bytes"] = target.read_bytes().decode() if target.is_file() else None
            status = refusal
            if inject == "target":
                token.write_text("target:" + str(target))
            elif inject == "link-other-refusal":
                event["refused"] = "theme"
                status = f"not saved {config}: 1 issue(s); fix the selected theme file"
                injected("link-other-refusal", target, target.read_bytes())
        elif inject == "refuse-regular":
            draft = saved.decode()
            event["refused"] = "symlink-wording"
            status = refusal
            injected("refuse-regular", config, config.read_bytes())
        else:
            events_log = Path(os.environ["FAKE_EVENTS"])
            nth = 1 + (events_log.read_text().count('"saved_sha256"') if events_log.exists() else 0)
            was_link = config.is_symlink()
            if was_link:  # link-saved: an unprotected save replaces the link
                config.unlink()
            else:
                keep("save")
            replace(config, saved)
            event["saved_sha256"] = hashlib.sha256(saved).hexdigest()
            receipt = {"config": str(config), "sha256": event["saved_sha256"]}
            if inject == "link-saved" and was_link:
                injected("link-saved", config, saved)
            if inject == "bad-receipt":
                receipt["sha256"] = receipt["sha256"][:63]
                injected("bad-receipt", config, saved)
            if inject == "no-receipt" and nth == 2:
                injected("no-receipt", config, saved)
            elif os.environ.get("TB_SAVE_RECEIPT"):
                replace(Path(os.environ["TB_SAVE_RECEIPT"]), json.dumps(receipt).encode())
            if inject == "second-editor" and 'label = "Mine"' in base:
                other = saved.replace(b"width = { span = 1 }", b"width = { span = 2 }", 1)
                other = other if other != saved else saved + b"# second editor\n"
                replace(config, other)
                injected("second-editor", config, other)
                event["injected"]["original"] = base
            status = "saved config"
    elif (text(config) or "").endswith(MARK):
        if inject == "external":
            other = b'label = "Edited elsewhere"\n# another editor\n'
            replace(config, other)
            injected("external", config, other)
        elif inject == "reopen-fails":
            injected("reopen-fails", config, config.read_bytes())
            log()
            sys.exit(1)
    current = draft if draft is not None else text(config)
    labels = re.findall(r'label = "([^"]*)"', current or "")
    if value("--snap"):
        Path(value("--snap")).write_bytes(b"\x89PNG fake")
    if value("--tree"):
        elements = [{"key": "status", "path": "status", "rect": [0, 0, 1, 1],
                     "text": status if current is not None else "invalid config"}]
        elements += [{"key": f"row-{i}", "path": f"row-{i}", "rect": [0, 0, 1, 1], "text": label}
                     for i, label in enumerate(labels)]
        Path(value("--tree")).write_text(json.dumps({"elements": elements}))
else:
    first_default()
    for _ in sys.stdin:
        pass
    if inject and inject.startswith("target:"):
        target, other = Path(inject[len("target:"):]), b'label = "Target edited elsewhere"\n'
        original = target.read_bytes()
        replace(target, other)
        injected("target", target, other)
        event["injected"]["original"] = original.decode()
    current = text(config) or ""
    if value("--emit"):
        keys = b""
        if 'keys = ["F5"]' in current:
            keys = b"".join(struct.pack("<qqHHi", 0, 0, *e) for e in ((1, 63, 1), (0, 0, 0), (1, 63, 0), (0, 0, 0)))
        Path(value("--emit")).write_bytes(keys)
    if value("--sim"):
        Path(value("--sim")).mkdir(parents=True, exist_ok=True)
        (Path(value("--sim")) / "frame-0001.png").write_bytes(b"\x89PNG fake frame")
    if "rehearsal-mark" in current:
        with open(home / "rehearsal-mark", "ab") as f:
            f.write(b"x")
    if os.environ.get("FAKE_FOREIGN"):
        (home / "left by the daemon.txt").write_bytes(b"not the run's own\n")
log()
'''


class RunFixture(Fixture):
    """Fake binaries, an events log and a TMPDIR inside the fixture."""

    def setUp(self):
        super().setUp()
        self.bin = self.tmp / "fake bin"
        self.bin.mkdir()
        for name in ("touchbar-customizer", "touchbard"):
            p = self.bin / name
            p.write_text(FAKE)
            p.chmod(0o755)
        self.events_file = self.tmp / "events.jsonl"
        self.env["FAKE_EVENTS"] = str(self.events_file)
        self.report = self.tmp / "report.json"
        self.tmpdir = self.tmp / "tmpdir"
        self.tmpdir.mkdir()
        self.env["TMPDIR"] = str(self.tmpdir)  # the run's mkdtemp profile lands in the fixture
        os.umask(0o022)  # privacy must come from the tool, not from the umask

    def events(self):
        if not self.events_file.exists():
            return []
        return [json.loads(line) for line in self.events_file.read_text().splitlines()]

    def run_tool(self, *extra):
        return self.tool("run", "--bin-dir", self.bin, "--report", self.report, *extra, timeout=120)

    def inside(self, path, root):
        return Path(path).resolve().is_relative_to(Path(root).resolve())


class Run(RunFixture):
    def test_run_isolates_every_binary_in_a_fresh_private_profile(self):
        p = self.run_tool()
        events = self.events()
        self.assertTrue(any(e["name"] == "touchbar-customizer" for e in events),
                        f"the configurator ran at least once: {p.stdout}{p.stderr}")
        homes = {e["home"] for e in events}
        self.assertEqual(len(homes), 1, "one profile for the whole run")
        home = Path(homes.pop())
        for root in (self.real_home, self.repo, self.session, self.tmp / "profile"):
            self.assertFalse(self.inside(home, root), f"profile {home} is inside {root}")
        if home.exists():
            chain = [home, *home.parents]
            chain = chain[:chain.index(self.tmpdir.resolve())] if self.tmpdir.resolve() in chain else chain[:1]
            self.assertTrue(any(stat.S_IMODE(d.stat().st_mode) & 0o077 == 0 for d in chain),
                            "the profile (or its own temp parent) is private to the user")
        fixtures = {t.read_text() for t in (self.repo / "fixtures/omarchy-themes").glob("*/colors.toml")}
        for e in events:
            self.assertIn(e["theme"], fixtures, f"{e['name']} saw the fixture theme")
            for flag in ("--emit", "--sim", "--snap", "--tree"):
                if flag in e["args"]:
                    out = e["args"][e["args"].index(flag) + 1]
                    self.assertTrue(self.inside(out, home) or not self.inside(out, self.repo),
                                    f"{flag} {out} is not written into the repo")
                    self.assertFalse(self.inside(out, self.real_home))
            self.assertNotIn("uinput", e["args"], "keys go to a file, never to uinput")
            if e["name"] == "touchbard" and e["config"]:
                for line in e["config"].splitlines():
                    if "command" in line:
                        self.assertIn("rehearsal-mark", line, "the only command is the local mark")
        self.assertOutsideUntouched()

    def test_a_binary_that_touches_the_persistent_session_fails_the_run(self):
        self.env["FAKE_TOUCH_SESSION"] = str(self.session / "keys.bin")
        p = self.run_tool()
        self.assertNotEqual(p.returncode, 0, "a changed session must fail the rehearsal")
        self.assertIn("manual-test", p.stdout + p.stderr)

    def test_run_refuses_a_home_that_is_not_a_new_private_one(self):
        existing = self.tmp / "already here"
        existing.mkdir()
        for home in (self.real_home, self.real_home / "Documents", self.session / "home", existing,
                     self.repo / "profile"):
            with self.subTest(home=str(home)):
                p = self.run_tool("--home", home)
                self.assertNotEqual(p.returncode, 0)
                self.assertEqual(self.events(), [], "no binary started")
                self.assertFalse((self.repo / "profile").exists())
                self.assertOutsideUntouched()
        self.run_tool()  # control: without --home the binaries do start
        self.assertTrue(self.events(), "a run with its own fresh profile starts the configurator")

    def test_a_temp_dir_inside_the_caller_home_still_gets_a_fresh_private_profile(self):
        tmpdir = self.real_home / ".cache" / "tmp"
        tmpdir.mkdir(parents=True)
        self.env["TMPDIR"] = str(tmpdir)
        sentinel = self.real_home / "Documents/sentinel.txt"
        self.run_tool()
        homes = {e["home"] for e in self.events()}
        self.assertEqual(len(homes), 1, "the binaries start in one profile of the run's own")
        home = Path(homes.pop())
        self.assertTrue(self.inside(home, tmpdir), f"profile {home} comes from TMPDIR")
        self.assertNotEqual(home.resolve(), self.real_home.resolve())
        self.assertEqual(sentinel.read_text(), "caller data\n")
        self.assertEqual(sorted(x.name for x in self.real_home.iterdir()), [".cache", "Documents"])
        self.assertEqual(tree(self.session), self.before_session)

    def test_the_run_report_is_what_cleanup_takes(self):
        self.run_tool()
        self.assertTrue(self.report.is_file(), "run writes its report")
        p = self.tool("cleanup", "--manifest", self.report)
        self.assertNotEqual(p.returncode, 2, f"cleanup --manifest <report> is valid usage: {p.stderr}")
        self.assertNotIn("usage:", p.stderr)
        self.assertOutsideUntouched()


class Expectations(RunFixture):
    """The run settles each scenario only with the producing save's expectation."""

    def inject(self, kind):
        token = self.tmp / "inject.token"
        token.write_text(kind)
        self.env["FAKE_INJECT"] = str(token)
        return token

    def report_data(self):
        return json.loads(self.report.read_text())

    def test_a_nominal_run_settles_with_the_savers_expectation_and_cleans_up_only_its_own(self):
        self.env["FAKE_FOREIGN"] = "1"
        p = self.run_tool()
        self.assertEqual(p.returncode, 0, f"a nominal run with faithful binaries passes: {p.stdout}{p.stderr}")
        report = self.report_data()
        home = Path(report["home"])
        config = home / ".config/touchbar-customizer/config.toml"
        self.assertEqual([r["name"] for r in report["scenarios"]],
                         ["absent-no-parents", "absent-with-parents", "commented-0640", "invalid", "symlink"])
        mine = [e for e in self.events() if Path(e["home"]) == home]
        saves = [e for e in mine if e.get("saved_sha256")]
        self.assertEqual(len(saves), 4, "four saves with receipts in the profile, one per regular scenario")
        for rec, save in zip(report["scenarios"][:4], saves):
            self.assertEqual(rec.get("expect_config"), save["saved_sha256"],
                             f"scenario {rec['name']} settled with its save's receipt")
            self.assertTrue(save["receipt"], "the save got a receipt path")
        # The fifth attempt, over the symlinked config, is an explicit refusal with no receipt.
        refused = [e for e in mine if e.get("refused")]
        self.assertEqual([e["refused"] for e in refused], ["symlink"], refused)
        link = refused[0]
        self.assertTrue(link["receipt"], "the refused save had a receipt path of its own")
        self.assertFalse(os.path.lexists(link["receipt"]), "and nothing was written there")
        sym = report["scenarios"][4]
        self.assertNotIn("expect_config", sym, "nothing settled from the refused save")
        self.assertTrue(sym.get("save_refused") and sym.get("receipt_absent"), sym)
        # The simulator ran the daemon on a known regular config of the run's own, then put the link back.
        daemons = [e for e in mine if e["name"] == "touchbard"]
        self.assertEqual(len(daemons), 1, daemons)
        self.assertEqual(daemons[0]["config_type"], "file", "the daemon saw a regular config, not the link")
        self.assertIn("rehearsal-mark", daemons[0]["config"])
        self.assertEqual(state(config), {"type": "symlink", "link": link["link"]}, "the original link is back")
        target = Path(os.path.realpath(config))
        self.assertEqual(target.read_bytes().decode(), link["target_bytes"], "and its target is unchanged")
        # Generations: one per protected write, private, in the profile's state directory.
        store = home / ".local/state/touchbar-customizer/backups"
        self.assertEqual({e["store"] for e in mine}, {str(store)}, "every binary uses the profile's store")
        kept = [g for e in mine for g in e["generations"]]
        self.assertGreaterEqual(len(kept), 4, "at least one generation before each save")
        self.assertEqual(sorted(kept, key=int), sorted((d.name for d in store.iterdir()), key=int))
        # Cleanup: removes only the run's own unchanged artifacts.
        foreign = home / "left by the daemon.txt"
        self.assertEqual(foreign.read_bytes(), b"not the run's own\n")
        self.assertTrue((home / "keys.bin").is_file())
        beside = store / "third-party.txt"
        beside.write_bytes(b"not the run's own either\n")
        owned = [Path(a["path"]) for a in report["artifacts"]]
        changed = next(a for a in owned if a.parent.name == "rehearsal-snaps" and a.suffix == ".png")
        changed.write_bytes(b"\x89PNG changed after the run\n")
        generations = tree(store)
        c = self.tool("cleanup", "--manifest", self.report)
        self.assertEqual(c.returncode, 0, c.stdout + c.stderr)
        self.assertEqual(tree(store), generations, "every generation (and a file beside them) is kept as it was")
        self.assertEqual(foreign.read_bytes(), b"not the run's own\n", "a file the run did not own stays")
        self.assertEqual(changed.read_bytes(), b"\x89PNG changed after the run\n", "a changed artifact stays")
        self.assertFalse((home / "keys.bin").exists(), "the run's own unchanged keys file is removed")
        self.assertFalse(os.path.lexists(config), "the run's own restored link is removed")
        left = sorted(str(p.relative_to(home)) for p in home.rglob("*") if not p.is_dir())
        expected = sorted([foreign.name, str(changed.relative_to(home))]
                          + [str((store / k).relative_to(home)) for k, v in generations.items() if v["type"] == "file"])
        self.assertEqual(left, expected, "everything else in the profile was the run's own")
        self.assertOutsideUntouched()

    def assertInjectionSurvives(self, kind):
        token = self.inject(kind)
        p = self.run_tool()
        self.assertFalse(token.exists(), f"the {kind} injection happened")
        hit = [e["injected"] for e in self.events() if "injected" in e]
        self.assertEqual(len(hit), 1, hit)
        path, data = Path(hit[0]["path"]), hit[0]["bytes"].encode()
        self.assertNotEqual(p.returncode, 0, f"the run fails on {kind}: {p.stdout}{p.stderr}")
        self.assertEqual(state(path).get("bytes"), data, f"{kind}: the bytes at {path} survive the run")
        self.tool("cleanup", "--manifest", self.report)
        self.assertEqual(state(path).get("bytes"), data, f"{kind}: and survive cleanup")
        if "original" in hit[0]:
            original = hit[0]["original"].encode()
            profile = Path(self.report_data()["profile"])
            held = [q for q in profile.rglob("*") if q.is_file() and not q.is_symlink()
                    and q != path and q.read_bytes() == original]
            self.assertTrue(held, f"{kind}: the original is still held in the run's store after cleanup")
        self.assertOutsideUntouched()
        return hit[0]

    def test_an_edit_landing_after_the_save_is_never_restored_over(self):
        self.assertInjectionSurvives("external")

    def test_an_edit_of_the_old_link_target_after_the_save_is_never_restored_over(self):
        # A third party edits the old link target while the simulator's regular config stands in
        # for the link: the link is never put back over it, and the original target stays stored.
        hit = self.assertInjectionSurvives("target")
        self.assertIn("Linked", hit["original"], "the edit replaced the original target layout")
        daemon = next(e for e in self.events() if "injected" in e)
        self.assertEqual((daemon["name"], daemon["config_type"]), ("touchbard", "file"),
                         "the edit landed while the simulator's regular config was in place")

    def test_a_failed_save_is_never_settled_or_restored(self):
        self.assertInjectionSurvives("save-fails")

    def test_a_failed_reopen_is_never_settled_or_restored(self):
        self.assertInjectionSurvives("reopen-fails")


class SaveReceipt(Expectations):
    """The run recognises the bytes of its own confirmed save by that save's receipt, so another
    editor's save that keeps the same label is never taken for the run's own."""

    def test_a_second_editor_keeping_the_label_keeps_its_design_and_the_personal_copy(self):
        self.assertInjectionSurvives("second-editor")
        hit = next(e["injected"] for e in self.events() if "injected" in e)
        second, original = hit["bytes"].encode(), hit["original"].encode()
        self.assertIn(b'label = "Rehearsal"', second, "the second design keeps the same label")
        self.assertIn(b"span = 2", second, "and differs from the run's save in another valid change")
        self.assertIn(b'label = "Mine"', original, "the personal layout was the one saved over")
        config = Path(hit["path"])
        profile = Path(self.report_data()["profile"])
        held = [p for p in profile.rglob("*") if p.is_file() and not p.is_symlink()
                and p != config and p.read_bytes() == original]
        self.assertTrue(held, "the personal layout is still held in the run's store after cleanup")

    def test_a_save_without_a_receipt_of_its_own_is_never_settled_or_restored(self):
        self.assertInjectionSurvives("no-receipt")

    def test_a_save_with_an_invalid_receipt_is_never_settled_or_restored(self):
        self.assertInjectionSurvives("bad-receipt")

    def assertLinkKept(self):
        config = Path(self.report_data()["home"]) / ".config/touchbar-customizer/config.toml"
        self.assertEqual(state(config).get("type"), "symlink", "the link is still the config")

    def test_a_save_that_replaces_the_link_instead_of_refusing_fails_the_run(self):
        self.assertInjectionSurvives("link-saved")

    def test_a_link_refused_for_another_reason_fails_the_run_and_keeps_the_link(self):
        self.assertInjectionSurvives("link-other-refusal")
        self.assertLinkKept()

    def test_a_symlink_refusal_of_a_regular_file_save_fails_the_run(self):
        self.assertInjectionSurvives("refuse-regular")
