"""What the docs must say about the opt-in Touch Bar device run (`touchbard --drm/--touch`).

Covers ADR-0014, the device-run section of `docs/hil-checklist.md` and the glossary terms it
needs. Every check looks for a meaning inside one paragraph, list item or table row (a "block"):
each concept is a set of alternative words, so any phrasing that says the same thing passes. No
check pins a sentence, a heading's exact text or a hardware value. The privacy checks refuse host
paths, fixed device nodes, unknown USB IDs and digitizer ranges that no verified evidence backs.

`python3 -I -B scripts/tests/test_opt_in_device_docs.py --dump` prints the sections checked here
(the text contract's visual dump) and exits 1 when one is missing.
"""

from __future__ import annotations

import re
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ADR = ROOT / "docs/adr/0014-kms-and-digitizer-adapters.md"
CHECKLIST = ROOT / "docs/hil-checklist.md"
GLOSSARY = ROOT / "GLOSSARY.md"

# A heading for the device run: any level-2..4 heading that names it.
DEVICE_RUN_HEADING = r"(?i)opt-in|--drm|device run|linux adapter|touch bar adapter"
NEGATION = r"(?i)\b(not|no|never|cannot|can't|nor|neither|without|unverified|unknown|unobserved|nothing)\b"
# USB IDs already cited in research 0001/0006/0010: T1 iBridge, T2 bar, recovery mode.
CITED_USB_IDS = {"05ac:8600", "05ac:8302", "05ac:1281"}


def read(path: Path) -> str:
    return path.read_text() if path.is_file() else ""


def heading_sections(text: str) -> list[tuple[int, str, str]]:
    """(level, heading, body up to the next heading of the same or a higher level)."""
    lines = text.splitlines()
    heads = [(i, len(m.group(1)), m.group(2)) for i, line in enumerate(lines)
             if (m := re.match(r"^(#{1,6}) (.+)$", line))]
    out = []
    for n, (i, level, title) in enumerate(heads):
        end = next((j for j, lv, _ in heads[n + 1:] if lv <= level), len(lines))
        out.append((level, title, "\n".join(lines[i + 1:end])))
    return out


def device_run_section(text: str) -> str:
    for level, title, body in heading_sections(text):
        if level >= 2 and re.search(DEVICE_RUN_HEADING, title):
            return title + "\n" + body
    return ""


def blocks(text: str) -> list[str]:
    """Paragraphs, list items and table rows, each joined into one line."""
    out: list[str] = []
    cur: list[str] = []
    for line in text.splitlines():
        s = line.strip()
        starts = re.match(r"^([-*+]|\d+[.)]|\|)\s", s) or s.startswith("|") or s.startswith("#")
        if not s or starts:
            if cur:
                out.append(" ".join(cur))
            cur = [s] if s else []
            if s.startswith("|") or s.startswith("#"):
                out.append(s)
                cur = []
        else:
            cur.append(s)
    if cur:
        out.append(" ".join(cur))
    return [b for b in out if b]


def says(block: str, *concepts: str) -> bool:
    return all(re.search(c, block, re.I) for c in concepts)


def glossary_entry(text: str, term: str) -> str:
    """Body of a `**Term**:` glossary entry whose bold name matches `term` (a regex)."""
    parts = re.split(r"^\*\*(.+?)\*\*:\s*$", text, flags=re.M)
    for i in range(1, len(parts) - 1, 2):
        if re.fullmatch(term, parts[i].strip(), re.I):
            return parts[i + 1].split("\n## ")[0]
    return ""


class DocsCase(unittest.TestCase):
    def setUp(self):
        self.adr = read(ADR)
        self.checklist = read(CHECKLIST)
        self.glossary = read(GLOSSARY)
        self.section = device_run_section(self.checklist)

    def run_blocks(self):
        self.assertTrue(self.section, "docs/hil-checklist.md has no section for the opt-in device run")
        return blocks(self.section)

    def assert_block(self, where: list[str], label: str, *concepts: str):
        self.assertTrue(any(says(b, *concepts) for b in where),
                        f"no paragraph, item or row says: {label}")


class OptInDeviceRunChecklist(DocsCase):
    """The checklist section a human follows before, during and after `touchbard --drm`."""

    def test_checklist_names_the_device_run_and_links_its_decision(self):
        b = self.run_blocks()
        self.assertTrue(re.search(r"0014-kms-and-digitizer-adapters\.md", self.section),
                        "the section links ADR-0014")
        self.assert_block(b, "the run is human-only and opt-in",
                          r"--drm", r"\bhuman|\byou\b|operator", r"opt-in|explicit|only when|never (by )?default|not (the )?default")

    def test_checklist_states_the_seat_and_ownership_preconditions_from_tb28_and_tb9(self):
        b = self.run_blocks()
        self.assertTrue(re.search(r"TB-28", self.section), "names TB-28 (key-mode service retired)")
        self.assertTrue(re.search(r"TB-9\b", self.section), "names TB-9 (pixel mode, seat/owner isolation)")
        self.assert_block(b, "nothing else owns the card or the digitizer (master / grab / owner)",
                          r"master|grab|own(er|ership|s)?\b|exclusive", r"tiny-dfr|service|compositor|Hyprland|other (client|process|program)|nothing else|no other")
        self.assert_block(b, "the desktop compositor/seat keeps off the Touch Bar card",
                          r"Hyprland|compositor|seat|desktop session", r"card|DRM|Touch Bar|display")
        self.assert_block(b, "permissions come from the human, never from the daemon or an agent",
                          r"permission|access|privilege", r"human|you\b|already|existing|before")

    def test_checklist_describes_drm_and_touch_as_separate_opt_in_selections(self):
        b = self.run_blocks()
        self.assert_block(b, "--drm takes auto or a /dev/dri/cardN path and is opt-in",
                          r"--drm", r"\bauto\b", r"PATH|cardN|/dev/dri")
        self.assert_block(b, "--touch takes auto or a /dev/input/eventN path and is opt-in",
                          r"--touch", r"\bauto\b|PATH|eventN|/dev/input")
        self.assert_block(b, "--touch refuses every digitizer while VERIFIED_DIGITIZERS is empty, until TB-45",
                          r"--touch|digitizer", r"VERIFIED_DIGITIZERS|verified (digitizer )?profile",
                          r"empty|no (verified )?(profile|entr)|none|refus", r"TB-45")

    def test_checklist_splits_display_evidence_for_tb10_from_digitizer_evidence_for_tb45(self):
        b = self.run_blocks()
        self.assert_block(b, "TB-10 records the display observations",
                          r"TB-10", r"orientation|colou?r|left end|latency|DIRTYFB|legib|lit\b|format|driver")
        self.assert_block(b, "TB-45 records the digitizer observations",
                          r"TB-45", r"digitizer|touch", r"\bname|ax(is|es)|range|contact|orientation")
        self.assert_block(b, "TB-45 adds the verified digitizer profile from cited evidence",
                          r"TB-45", r"VERIFIED_DIGITIZERS|profile", r"evidence|cite|observ|record")

    def test_checklist_separates_facts_read_in_the_driver_source_from_what_nobody_has_observed(self):
        b = self.run_blocks()
        self.assert_block(b, "facts known only from the (candidate) driver source",
                          r"source|candidate|patch|appletbdrm\.c", r"XRGB8888|BGR888|RIGHT_UP|right side up|non[-_ ]desktop")
        self.assert_block(b, "what is still unobserved on the T1 is marked so",
                          r"unverified|not (yet )?(been )?(observed|verified|seen)|unobserved|until .*observ",
                          r"T1|panel|hardware|physical")

    def test_checklist_says_a_successful_present_is_only_a_submitted_frame(self):
        b = self.run_blocks()
        self.assert_block(b, "the driver discards the flush/clear (USB) result, so success = submitted",
                          r"flush|plane update|USB (transfer|result)|clear", r"discard|ignor|drop|not (returned|reported|passed)|never (returns|reports)|swallow",
                          r"submit")
        self.assert_block(b, "a lit, legible frame needs a human observation",
                          r"submit", r"lit\b|legib|visible|on the panel|shown|displayed", r"observ|human|you\b|see|look|photo|not (proof|evidence)|cannot know")

    def test_checklist_says_the_daemon_refuses_a_card_someone_else_already_shows(self):
        b = self.run_blocks()
        self.assert_block(b, "an active/foreign prior KMS state is refused and left untouched",
                          r"active|enabled|foreign|already|another|other (client|owner)",
                          r"refus|declin|will not|won't|does not (take|change|touch)|leaves? (it )?untouched|exit",
                          r"state|framebuffer|scanout|mode|CRTC|card|frame")

    def test_checklist_says_restore_is_attempted_and_its_failure_reported(self):
        b = self.run_blocks()
        self.assert_block(b, "an ordinary stop (Ctrl-C / SIGTERM / SIGINT) restores and releases",
                          r"Ctrl-?C|SIGTERM|SIGINT|SIGHUP", r"restor", r"master|grab|releas")
        self.assert_block(b, "restore is attempted and an error is reported, not hidden",
                          r"restor", r"attempt|tries|tried|try\b|every step|each step",
                          r"error|fail", r"report|print|stderr|log|exit|shown|named")
        self.assert_block(b, "restore means the originally inactive KMS objects, not a prior frame",
                          r"restor", r"inactive|disabled|off\b|prior|original")

    def test_checklist_promises_nothing_after_sigkill_unplug_or_power_loss(self):
        b = self.run_blocks()
        hard = [x for x in b if re.search(r"SIGKILL|unplug|disconnect|power (loss|cut|failure)|loses power|power is lost", x, re.I)]
        self.assertTrue(any(re.search(r"SIGKILL", x) for x in hard), "SIGKILL is covered")
        self.assertTrue(any(re.search(r"unplug|disconnect", x, re.I) for x in hard), "unplug is covered")
        self.assertTrue(any(re.search(r"power", x, re.I) for x in hard), "power loss is covered")
        joined = " ".join(hard)
        for label, concept in [("frame", r"frame|image|picture"),
                               ("panel dark/off/cleared", r"blank|dark|off\b|clear"),
                               ("firmware keys / key mode back", r"key(s| row| mode)|F-keys|Esc")]:
            self.assertTrue(re.search(concept, joined, re.I), f"the no-promise text names the {label}")
        for x in hard:
            if re.search(r"\b(blanks?|clears?|turns? off|restores?|returns?|recovers?|comes? back)\b", x, re.I):
                self.assertRegex(x, NEGATION, f"states an outcome after SIGKILL/unplug/power loss as certain: {x[:160]}")

    def test_checklist_links_the_return_to_key_mode_of_the_hardware_runbook(self):
        b = self.run_blocks()
        self.assertTrue(re.search(r"\]\((\.\./)?(docs/)?hardware-runbook\.md(#[^)]*)?\)", self.section),
                        "links docs/hardware-runbook.md (the TB-46 return procedures)")
        self.assert_block(b, "the return to the prior owners / key mode follows the runbook",
                          r"hardware-runbook|runbook", r"return|revers|key mode|prior owner|back to")

    def test_checklist_asks_the_human_for_evidence_after_the_run(self):
        b = self.run_blocks()
        self.assert_block(b, "after each run the human records what was seen (or the refusal)",
                          r"record|note|evidence|log|photo|write down",
                          r"after|on exit|once|each run|every run|refus|observ|seen")
        self.assert_block(b, "shared evidence is redacted",
                          r"redact|private|no serial|sensitive|personal")


class KmsAndDigitizerDecision(DocsCase):
    """ADR-0014 records the KMS display decisions next to the digitizer ones it already has."""

    def adr_blocks(self):
        self.assertTrue(self.adr, "ADR-0014 is missing")
        return blocks(self.adr)

    # Already written (preserve).
    def test_adr_keeps_unsafe_confined_to_the_three_device_boundaries(self):
        for module in ("drm/card.rs", "digitizer/device.rs", "stop_signals.rs"):
            self.assertIn(module, self.adr, f"names the unsafe boundary {module}")
        self.assertRegex(self.adr, r"rustix")
        self.assertRegex(self.adr, r"\blibc\b")

    def test_adr_keeps_the_sigwait_thread_and_no_signal_handler(self):
        self.assert_block(self.adr_blocks(), "a sigwait thread, no handler touching daemon state",
                          r"sigwait", r"SIGTERM|SIGINT|SIGHUP|signal")

    def test_adr_keeps_the_digitizer_grab_after_a_verified_profile_and_tb45_owning_it(self):
        b = self.adr_blocks()
        self.assert_block(b, "the grab comes only after identity and range match a verified profile",
                          r"grab", r"verified profile|profile", r"range|identity|name")
        self.assert_block(b, "TB-45 owns the first verified profile; none ships before it",
                          r"TB-45", r"profile|VERIFIED_DIGITIZERS")

    def test_adr_keeps_the_success_path_as_hardware_only(self):
        self.assert_block(self.adr_blocks(), "the real ioctl success path needs the T1",
                          r"success|real|physical|orientation", r"hardware|T1|HIL|human")

    # To be written.
    def test_adr_chooses_legacy_kms_over_atomic(self):
        self.assert_block(self.adr_blocks(), "legacy KMS (SETCRTC/DIRTYFB) is chosen and atomic considered",
                          r"legacy|SETCRTC", r"atomic")

    def test_adr_identifies_the_card_in_sysfs_before_opening_and_takes_master(self):
        b = self.adr_blocks()
        self.assert_block(b, "the card is identified in sysfs before any node is opened",
                          r"card|DRM|KMS", r"sysfs", r"before|prior to|first|only then|then open")
        self.assert_block(b, "exclusive display ownership is DRM master; busy is refused",
                          r"\bmaster\b", r"busy|EBUSY|refus|exclusive|own")

    def test_adr_accepts_only_an_inactive_prior_state_and_restores_it(self):
        b = self.adr_blocks()
        self.assert_block(b, "an active or foreign prior presentation is refused before any modeset",
                          r"active|foreign|enabled|already", r"refus|declin|not take|won't|will not|rejects?",
                          r"state|framebuffer|CRTC|scanout|presentation|mode")
        self.assert_block(b, "restore returns the originally inactive CRTC and reports failures",
                          r"restor", r"inactive|disabl|original|prior", r"error|fail|report")

    def test_adr_records_that_kms_success_only_means_submitted(self):
        self.assert_block(self.adr_blocks(), "the driver discards flush/clear results, so success = submitted",
                          r"flush|plane update|USB|clear", r"discard|ignor|drop|not (returned|reported|passed)|never (returns|reports)",
                          r"submit")
        self.assertRegex(self.adr, r"appletbdrm", "cites the appletbdrm driver source")

    def test_adr_lists_the_options_it_did_not_take(self):
        for label, concept in [("tiny-dfr's open-every-card probe", r"tiny-dfr"),
                               ("atomic KMS", r"atomic"),
                               ("the drm/evdev crates", r"`?drm`? (and|or|/|,) `?evdev`?|`drm`|`evdev`|drm crate|evdev crate"),
                               ("a CLI or config calibration", r"calibrat\w* (flag|option|table|config)|--touch-axis|(flag|config|CLI)[^.]{0,40}calibrat")]:
            self.assertRegex(self.adr, re.compile(concept, re.I), f"names the rejected option: {label}")


class DeviceRunGlossary(DocsCase):
    """Terms the device run introduces; Digitizer and Cancelled touch already exist."""

    def test_glossary_keeps_digitizer_and_cancelled_touch(self):
        self.assertTrue(glossary_entry(self.glossary, r"Digitizer"), "Digitizer stays defined")
        self.assertTrue(glossary_entry(self.glossary, r"Cancell?ed touch"), "Cancelled touch stays defined")

    def test_glossary_defines_the_touch_bar_card(self):
        e = glossary_entry(self.glossary, r"Touch Bar card")
        self.assertTrue(e, "defines **Touch Bar card**")
        self.assertRegex(e, r"(?i)DRM|KMS|/dev/dri|card")
        self.assertRegex(e, r"(?i)identif|validat|match|confirm|verified")

    def test_glossary_defines_the_digitizer_profile(self):
        e = glossary_entry(self.glossary, r"Digitizer profile|Verified digitizer profile")
        self.assertTrue(e, "defines **Digitizer profile**")
        self.assertRegex(e, r"(?i)identity|name|vendor|product")
        self.assertRegex(e, r"(?i)range|ax(is|es)|min|max")
        self.assertRegex(e, r"(?i)verified|evidence")

    def test_glossary_defines_restore_of_the_prior_presentation(self):
        e = glossary_entry(self.glossary, r"[^*]*\bRestore\b[^*]*")
        self.assertTrue(e, "defines **Restore** (of the bar's prior presentation)")
        self.assertRegex(e, r"(?i)prior|original|inactive|before")
        self.assertRegex(e, r"(?i)master|grab|releas|disabl|card|presentation")


class NothingPrivateOrUnverified(DocsCase):
    """No host data, fixed nodes, unknown USB IDs or digitizer ranges in these docs."""

    def texts(self):
        return {"ADR-0014": self.adr, "hil-checklist": self.checklist, "GLOSSARY": self.glossary}

    def test_no_host_paths_serials_or_fixed_device_nodes(self):
        bad = [r"/home/", r"~/Documents", r"Documents/mac", r"\bECID\b", r"(?i)serial(?: number)?\s*[:=]\s*\S",
               r"/dev/dri/card\d", r"/dev/input/event\d", r"\brenderD\d", r"/sys/[^\s`]*\d-\d"]
        for name, text in self.texts().items():
            for pattern in bad:
                with self.subTest(doc=name, pattern=pattern):
                    self.assertNotRegex(text, pattern)

    def test_no_uncited_usb_ids_or_unverified_digitizer_ranges(self):
        for name, text in self.texts().items():
            ids = {m.lower() for m in re.findall(r"\b[0-9a-fA-F]{4}:[0-9a-fA-F]{4}\b", text)}
            self.assertLessEqual(ids, CITED_USB_IDS, f"{name}: USB IDs not cited in research")
            for line in text.splitlines():
                if re.search(r"(?i)\bax(is|es)\b|\brange|ABS_|digitizer|\bmin\b|\bmax\b", line):
                    with self.subTest(doc=name, line=line[:120]):
                        self.assertNotRegex(line, r"(?<![\w.])-?\d+\s*(\.\.=?|–|—|\bto\b)\s*-?\d{3,}")
                        self.assertNotRegex(line, r"(?i)\b(min|max)(imum)?\s*[=:]\s*-?\d{2,}")


def dump() -> int:
    missing = []
    adr = read(ADR)
    section = device_run_section(read(CHECKLIST))
    print("==== docs/adr/0014-kms-and-digitizer-adapters.md ====")
    print(adr or "(missing)")
    print("==== docs/hil-checklist.md: opt-in device run section ====")
    print(section or "(missing)")
    if not adr:
        missing.append("ADR-0014")
    if not section:
        missing.append("checklist device-run section")
    glossary = read(GLOSSARY)
    for label, term in [("Touch Bar card", r"Touch Bar card"),
                        ("Digitizer profile", r"Digitizer profile|Verified digitizer profile"),
                        ("Restore", r"[^*]*\bRestore\b[^*]*"),
                        ("Digitizer", r"Digitizer"),
                        ("Cancelled touch", r"Cancell?ed touch")]:
        entry = glossary_entry(glossary, term)
        print(f"==== GLOSSARY.md: {label} ====")
        print(entry.strip() or "(missing)")
        if not entry:
            missing.append(f"glossary {label}")
    if missing:
        print("missing: " + ", ".join(missing), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--dump"]:
        sys.exit(dump())
    unittest.main()
