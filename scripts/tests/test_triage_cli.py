"""Triage's public process handoff and private artifacts, with an installed fake Codex."""

import json
import os
import shutil
import stat
import unittest
from pathlib import Path

from doctor_cli_fixture import CliFixture, ROOT, SECRET, blocker_report


class TriageCli(unittest.TestCase):
    def setUp(self):
        self.f = CliFixture()
        self.addCleanup(self.f.close)
        playbook = ROOT / "docs/agents/install-triage.md"
        if playbook.exists():
            target = self.f.root / "docs/agents/install-triage.md"
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(playbook, target)
        self.f.executable(self.f.root / "scripts/doctor.py", '''import json, os, sys
from pathlib import Path
cfg=json.loads(Path(os.environ["FIXTURE_SETTINGS"]).read_text())
if cfg.get("invalid_doctor"): print("not a report"); sys.exit(2)
report=cfg["doctor"]
if "--profile" in sys.argv: report["profile"]=sys.argv[sys.argv.index("--profile")+1]
print(json.dumps(report))
sys.exit(0 if report["summary"]["ready"] else 1)
''')
        self.f.configure({"doctor": blocker_report()})
        self.launch = self.f.base / "launch.json"
        self.f.env["FIXTURE_LAUNCH"] = str(self.launch)
        # These are deliberately uncollected data, never instructions or context.
        private = self.f.home / ".config/touchbar-customizer/config.toml"
        private.parent.mkdir(parents=True)
        private.write_text(SECRET)
        (self.f.root / "private-filename-should-not-be-collected").write_text(SECRET)

    def codex(self, code=0):
        self.f.executable(self.f.bin / "codex", f'''import json, os, sys
from pathlib import Path
Path(os.environ["FIXTURE_LAUNCH"]).write_text(json.dumps({{"argv":sys.argv[1:],"cwd":os.getcwd()}}))
sys.exit({code})
''')

    def sessions(self):
        parent = self.f.root / ".scratch/triage"
        return sorted(parent.iterdir()) if parent.exists() and not parent.is_symlink() else []

    def artifacts(self, result):
        sessions = self.sessions()
        self.assertTrue(sessions, result.stdout + result.stderr)
        session = sessions[-1]
        self.assertIn(str(session), result.stdout + result.stderr)
        self.assertEqual(stat.S_IMODE(session.stat().st_mode), 0o700)
        for name in ("doctor.json", "context.json", "prompt.md"):
            path = session / name
            self.assertTrue(path.is_file(), name)
            self.assertFalse(path.is_symlink())
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600, name)
            self.assertNotIn(SECRET, path.read_text())
            self.assertNotIn("private-filename-should-not-be-collected", path.read_text())
        self.assertFalse(json.loads((session / "doctor.json").read_text())["summary"]["ready"])
        context = json.loads((session / "context.json").read_text())
        self.assertTrue(context)
        prompt = (session / "prompt.md").read_text()
        self.assertIn("context.json", prompt)
        playbook = (self.f.root / "docs/agents/install-triage.md").read_text()
        self.assertIn(playbook.strip(), prompt)
        return session

    def test_prepare_only_needs_no_agent_and_creates_unique_private_redacted_sessions(self):
        for _ in range(2):
            result = self.f.run("triage.py", "--prepare-only")
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.artifacts(result)
        self.assertEqual(len(self.sessions()), 2)
        self.assertFalse(self.launch.exists())

    def test_blockers_still_launch_installed_codex_with_safe_argv_cwd_and_exit(self):
        self.codex(7)
        result = self.f.run("triage.py", "--agent", "codex", "--profile", "build")
        self.assertEqual(result.returncode, 7, result.stdout + result.stderr)
        session = self.artifacts(result)
        launch = json.loads(self.launch.read_text())
        self.assertEqual(launch["cwd"], str(self.f.root))
        self.assertEqual(launch["argv"][:2], ["-C", str(self.f.root)])
        self.assertEqual(len(launch["argv"]), 3, "no implicit model, approval or shell flags")
        self.assertIn(str(session / "prompt.md"), launch["argv"][2])
        self.assertEqual(json.loads((session / "doctor.json").read_text())["profile"], "build")

    def test_missing_selected_cli_leaves_inspectable_artifacts_and_never_switches_agents(self):
        self.f.tool("claude")
        result = self.f.run("triage.py")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.artifacts(result)
        self.assertIn("codex", (result.stdout + result.stderr).lower())
        self.assertFalse(self.launch.exists())
        self.assertNotIn("claude", [c["tool"] for c in self.f.invocations()])

    def test_unsupported_agent_and_invalid_doctor_are_usage_or_preparation_errors(self):
        self.codex()
        result = self.f.run("triage.py", "--agent", "claude")
        self.assertEqual(result.returncode, 2)
        self.assertFalse(self.launch.exists())
        self.f.configure({"invalid_doctor": True})
        result = self.f.run("triage.py", "--prepare-only")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertFalse(self.launch.exists())
        self.assertIn("doctor", (result.stdout + result.stderr).lower())

    def test_unsafe_artifact_parent_symlink_is_rejected_without_writing_through_it(self):
        outside = self.f.base / "outside"
        outside.mkdir()
        parent = self.f.root / ".scratch"
        parent.mkdir()
        (parent / "triage").symlink_to(outside, target_is_directory=True)
        result = self.f.run("triage.py", "--prepare-only")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(list(outside.iterdir()), [])
        self.assertFalse(self.launch.exists())

    def test_preparation_never_installs_authenticates_collects_remotes_or_executes_config(self):
        result = self.f.run("triage.py", "--prepare-only")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.artifacts(result)
        for call in self.f.invocations():
            self.assertEqual(call["tool"], "git", call)
            self.assertFalse({"remote", "config", "fetch", "pull", "clone"}.intersection(call["argv"]), call)
        config = self.f.home / ".config/touchbar-customizer/config.toml"
        self.assertEqual(config.read_text(), SECRET)
        self.assertTrue(os.access(self.f.root / "scripts/triage.py", os.X_OK))


class OnboardingSources(unittest.TestCase):
    def test_readme_documents_the_reproducible_commands_paths_and_limits(self):
        readme = (ROOT / "README.md").read_text()
        for command in ("git clone", "mise install", "scripts/doctor.py", "--profile runtime",
                        "--json", "scripts/triage.py", "--prepare-only", "1.99.1",
                        "nice -n 19 env CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 cargo build --workspace --locked",
                        "scripts/try-app.sh", "cargo run -p tb-app", "--snap", "--sim", "--emit"):
            self.assertIn(command, readme, command)
        for fact in ("python", "pkgconf", "libxkbcommon", "wayland", "fontconfig",
                     "vulkan-icd-loader", "vulkan-swrast", "libxkbcommon-x11", "monospace",
                     ".config/touchbar-customizer/config.toml", ".local/state/omarchy/current/theme/colors.toml",
                     ".scratch", "target", "out", "uinput", "fn", "pending"):
            self.assertIn(fact, readme.lower(), fact)
        self.assertTrue("hil-checklist" in readme or "human" in readme.lower())

    def test_primary_source_research_and_local_playbook_are_present(self):
        research = list((ROOT / "docs/research").glob("0009-*.md"))
        self.assertEqual(len(research), 1, "promote the cited doctor/triage research")
        text = research[0].read_text()
        for source in ("https://vercel.com/docs/cli/doctor", "https://docs.brew.sh/", "github.com/pingdotgg/t3code/blob/", "f73f72cdc563534e4f7de9f5a79bed007ac42384"):
            self.assertIn(source, text)
        playbook = (ROOT / "docs/agents/install-triage.md").read_text().lower()
        for instruction in ("agents.md", "doctor", "data", "authorization", "hardware", "report"):
            self.assertIn(instruction, playbook)
        self.assertIn(".scratch/triage/", (ROOT / ".gitignore").read_text())


if __name__ == "__main__":
    unittest.main()
