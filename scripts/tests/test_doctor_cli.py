"""Bootstrap contract: real Python CLI, fake offline external executables."""

import json
import os
import unittest

from doctor_cli_fixture import CliFixture, IDS, SECRET


class DoctorCli(unittest.TestCase):
    def setUp(self):
        self.f = CliFixture()
        self.addCleanup(self.f.close)

    def report(self, *args):
        result = self.f.run("doctor.py", *args, "--json")
        self.assertIn(result.returncode, (0, 1), result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["schema_version"], 1)
        self.assertEqual([c["id"] for c in report["checks"]], IDS)
        rows = {c["id"]: c for c in report["checks"]}
        for row in rows.values():
            self.assertIn(row["status"], ("ok", "warning", "failed", "skipped"))
            self.assertIsInstance(row["required"], bool)
            self.assertTrue(row["message"])
            self.assertIsInstance(row["evidence"], list)
            self.assertIsInstance(row["next"], list)
        for status in ("ok", "warning", "failed", "skipped"):
            self.assertEqual(report["summary"][status], sum(c["status"] == status for c in rows.values()))
        ready = not any(c["required"] and c["status"] in ("failed", "skipped") for c in rows.values())
        self.assertEqual(report["summary"]["ready"], ready)
        self.assertEqual(result.returncode, 0 if ready else 1)
        self.assertNotIn(SECRET, result.stdout + result.stderr)
        return rows

    def test_build_profile_reports_missing_prerequisites_with_remedies_and_scoped_skips(self):
        for tool in ("git", "pkg-config"):
            (self.f.bin / tool).unlink()
        rows = self.report()
        self.assertEqual(rows["build.tools"]["status"], "failed")
        self.assertTrue(rows["build.tools"]["next"])
        self.assertIn("pacman", str(rows["build.tools"]["next"]))
        for id in IDS[10:]:
            self.assertEqual(rows[id]["status"], "skipped", id)
            self.assertFalse(rows[id]["required"], id)
        human = self.f.run("doctor.py")
        self.assertEqual(human.returncode, 1)
        self.assertIn("build.tools", human.stdout)
        self.assertTrue(os.access(self.f.root / "scripts/doctor.py", os.X_OK))

    def test_missing_native_probe_never_counts_as_config_theme_or_gpu_validation(self):
        rows = self.report("--profile", "runtime")
        self.assertEqual(rows["runtime.probe"]["status"], "skipped")
        for id in ("runtime.config", "runtime.theme", "runtime.gpu"):
            self.assertEqual(rows[id]["status"], "skipped")
            self.assertTrue(rows[id]["required"])
            self.assertTrue(rows[id]["message"])
        self.assertEqual(rows["hardware.bar"]["status"], "skipped")
        self.assertFalse(rows["hardware.bar"]["required"])
        self.assertFalse((self.f.home / ".config").exists())

    def test_native_findings_window_absence_and_optional_input_have_honest_statuses(self):
        self.f.native()
        rows = self.report("--profile", "runtime")
        self.assertEqual(rows["runtime.probe"]["status"], "ok")
        self.assertEqual(rows["runtime.config"]["status"], "ok")
        self.assertEqual(rows["runtime.theme"]["status"], "warning")
        self.assertEqual(rows["runtime.gpu"]["status"], "failed")
        self.assertIn(rows["runtime.window"]["status"], ("failed", "skipped"))
        self.assertTrue(rows["runtime.window"]["required"])
        for id in ("input.fn", "input.uinput"):
            self.assertFalse(rows[id]["required"])
            self.assertEqual(rows[id]["status"], "warning")

    def test_invalid_crashed_and_oversized_native_reports_block_runtime(self):
        for mode in ("invalid", "crash", "oversize"):
            with self.subTest(mode=mode):
                self.f.native(mode)
                rows = self.report("--profile", "runtime")
                self.assertEqual(rows["runtime.probe"]["status"], "failed")
                self.assertTrue(rows["runtime.probe"]["next"])
                for id in ("runtime.config", "runtime.theme", "runtime.gpu"):
                    self.assertNotEqual(rows[id]["status"], "ok")

    def test_remote_display_stays_unverified_without_a_network_connection(self):
        self.f.env["DISPLAY"] = "remote.example.invalid:0"
        self.f.native()
        rows = self.report("--profile", "runtime")
        self.assertEqual(rows["runtime.window"]["status"], "skipped")
        self.assertTrue(rows["runtime.window"]["required"])

    def test_missing_home_cannot_claim_that_the_users_config_was_validated(self):
        self.f.env.pop("HOME")
        self.f.native()
        rows = self.report("--profile", "runtime")
        self.assertIn(rows["runtime.config"]["status"], ("failed", "skipped"))
        self.assertTrue(rows["runtime.config"]["required"])

    def test_timed_out_tool_has_a_bounded_actionable_diagnostic(self):
        self.f.configure({"sleep": ["pkg-config"]})
        rows = self.report()
        for id in ("libs.xkbcommon", "libs.wayland", "libs.fontconfig"):
            self.assertIn(rows[id]["status"], ("failed", "skipped"))
            self.assertTrue(rows[id]["next"])
        self.assertIn("tim", json.dumps(rows).lower())

    def test_probes_never_install_build_authenticate_or_change_config(self):
        rows = self.report("--profile", "runtime")
        self.assertEqual(rows["hardware.bar"]["status"], "skipped")
        forbidden = {"install", "build", "update", "self", "login", "auth", "default", "override"}
        for call in self.f.invocations():
            self.assertFalse(forbidden.intersection(call["argv"]), call)
            self.assertNotEqual(call["tool"], "mise", call)
            if call["tool"] in ("cargo", "rustc", "rustfmt", "clippy-driver", "cargo-clippy"):
                self.assertTrue(call["executable"].startswith(str(self.f.home / ".rustup/toolchains")), call)
        self.assertFalse((self.f.home / ".config").exists())
        bad = self.f.run("doctor.py", "--profile", "unknown")
        self.assertEqual(bad.returncode, 2)


if __name__ == "__main__":
    unittest.main()
