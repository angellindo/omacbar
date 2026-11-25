"""`rehearsal.py baseline` / `compare` against a fixture root and a fake `systemctl`: read-only,
private output outside the repo, root-only items marked for the human, differences reported.
Agents never run `baseline` against `/`; this test is the only place it runs."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import stat
import unittest

from rehearsal_fixture import Fixture, tree

SERVICE = "macbook-touchbar-local.service"
FAKE_SYSTEMCTL = r'''#!/usr/bin/env python3
import json, os, sys
with open(os.environ["FAKE_SYSTEMCTL_LOG"], "a") as f:
    f.write(json.dumps(sys.argv[1:]) + "\n")
verb = next((a for a in sys.argv[1:] if not a.startswith("-")), "")
print({"is-active": "active", "is-enabled": "enabled"}.get(verb, "unknown"))
'''


class Baseline(Fixture):
    def setUp(self):
        super().setUp()
        r = self.fake_root = self.tmp / "fixture root"
        self.files = {
            "sys/bus/usb/devices/1-3/idVendor": "05ac\n",
            "sys/bus/usb/devices/1-3/idProduct": "8600\n",
            "sys/bus/usb/devices/1-3/bConfigurationValue": "1\n",
            "sys/bus/usb/devices/2-1/idVendor": "1d6b\n",
            "sys/power/mem_sleep": "s2idle [deep]\n",
            "proc/cmdline": "root=fixture quiet\n",
            f"etc/systemd/system/{SERVICE}": "[Service]\nExecStart=/fixture run\n",
            "usr/lib/modules/7.2.15-fixture/modules.dep": "",
            "var/lib/macbook-touchbar-local/installed.json": '{"state": "enabled"}\n',
        }
        for rel, text in self.files.items():
            (r / rel).parent.mkdir(parents=True, exist_ok=True)
            (r / rel).write_text(text)
        os.chmod(r / "var/lib/macbook-touchbar-local/installed.json", 0o000)
        bin_dir = self.tmp / "fake bin"
        bin_dir.mkdir()
        (bin_dir / "systemctl").write_text(FAKE_SYSTEMCTL)
        (bin_dir / "systemctl").chmod(0o755)
        self.log = self.tmp / "systemctl.jsonl"
        self.env["FAKE_SYSTEMCTL_LOG"] = str(self.log)
        self.env["PATH"] = f"{bin_dir}{os.pathsep}{self.env['PATH']}"

    def baseline(self, label, out, code=0):
        return self.ok("baseline", "--label", label, "--out", out, "--root", self.fake_root, code=code)

    def text_of(self, out):
        return "\n".join(p.read_text(errors="replace") for p in sorted(out.rglob("*")) if p.is_file())

    def root_meta(self):
        return {p: (p.lstat().st_mode, p.lstat().st_size, p.lstat().st_mtime_ns)
                for p in self.fake_root.rglob("*")}

    def test_baseline_records_the_listed_items_read_only_into_a_private_dir(self):
        before = self.root_meta()
        out = self.tmp / "baseline A"
        self.baseline("A", out)
        self.assertEqual(self.root_meta(), before, "the fixture root is not modified")
        self.assertTrue(out.is_dir(), "the baseline is written to --out")
        self.assertEqual(stat.S_IMODE(out.stat().st_mode), 0o700)
        text = self.text_of(out)
        service_hash = hashlib.sha256(self.files[f"etc/systemd/system/{SERVICE}"].encode()).hexdigest()
        self.assertIn(service_hash, text, "managed service file hash is recorded")
        self.assertIn("s2idle [deep]", text)
        self.assertIn("7.2.15-fixture", text)
        verbs = [json.loads(line) for line in self.log.read_text().splitlines()]
        self.assertTrue(verbs, "service state is asked")
        for argv in verbs:
            self.assertTrue({"is-active", "is-enabled"} & set(argv), f"read-only systemctl only: {argv}")
        self.assertOutsideUntouched()

    @unittest.skipIf(os.geteuid() == 0, "root reads mode 000 files")
    def test_root_only_items_are_left_for_the_human(self):
        out = self.tmp / "baseline A"
        self.baseline("A", out)
        self.assertIn("needs sudo", self.text_of(out))
        self.assertNotIn('"state": "enabled"', self.text_of(out))

    def test_baseline_refuses_an_out_dir_inside_the_repo(self):
        out = self.repo / ".scratch/baseline-A"
        p = self.tool("baseline", "--label", "A", "--out", out, "--root", self.fake_root)
        self.assertNotEqual(p.returncode, 0)
        self.assertFalse(out.exists())
        self.baseline("A", self.tmp / "baseline A")  # control: the same call outside the repo works

    def test_compare_reports_a_changed_usb_configuration_and_a_changed_file(self):
        a, b = self.tmp / "A", self.tmp / "B"
        self.baseline("A", a)
        self.ok("compare", a, a)
        (self.fake_root / "sys/bus/usb/devices/1-3/bConfigurationValue").write_text("2\n")
        (self.fake_root / f"etc/systemd/system/{SERVICE}").write_text("[Service]\nchanged\n")
        self.baseline("B", b)
        p = self.tool("compare", a, b)
        self.assertNotEqual(p.returncode, 0, "a difference is not a match")
        self.assertIn("bConfigurationValue", p.stdout)
        self.assertIn(SERVICE, p.stdout)

    def test_compare_sees_the_service_manifest_content_and_metadata_and_a_preserved_copy_put_back(self):
        """Private test data only: a readable fixture manifest, its 2.0-style preserved copy
        (`cp -a`: bytes, mode and times), 2.4's `state: removed`, then the copy put back."""
        manifest = self.fake_root / "var/lib/macbook-touchbar-local/installed.json"
        os.chmod(manifest, 0o644)
        preserved = self.tmp / "preserved" / "installed.json"
        preserved.parent.mkdir(mode=0o700)
        shutil.copy2(manifest, preserved)
        a, b, a2, a3 = (self.tmp / n for n in ("A", "B", "A2", "A3"))
        self.baseline("A", a)
        self.assertIn(hashlib.sha256(b'{"state": "enabled"}\n').hexdigest(), self.text_of(a))
        manifest.write_text('{"state": "removed"}\n')
        self.baseline("B", b)
        p = self.tool("compare", a, b)
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("installed.json", p.stdout, "the manifest's removed state is a difference")
        shutil.copy2(preserved, manifest)
        os.chmod(manifest, 0o600)
        self.baseline("A2", a2)
        p = self.tool("compare", a, a2)
        self.assertNotEqual(p.returncode, 0, "same bytes with another mode is still a difference")
        self.assertIn("installed.json", p.stdout)
        shutil.copy2(preserved, manifest)  # bytes, mode and times of the preserved copy
        self.baseline("A3", a3)
        p = self.tool("compare", a, a3)
        self.assertNotIn("installed.json", p.stdout, "the preserved copy put back matches A")


if __name__ == "__main__":
    unittest.main()
