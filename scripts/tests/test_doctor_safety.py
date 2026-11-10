"""Additional public CLI checks for process bounds and target-directory integration."""
import json
import shutil
import unittest

from doctor_cli_fixture import CliFixture, ROOT, blocker_report


class ProcessBounds(unittest.TestCase):
    def setUp(self):
        self.f = CliFixture()
        self.addCleanup(self.f.close)

    def runtime(self):
        result = self.f.run('doctor.py', '--profile', 'runtime', '--json')
        self.assertEqual(result.returncode, 1, result.stderr)
        return {row['id']: row for row in json.loads(result.stdout)['checks']}

    def test_native_timeout_blocks_readiness_without_dumping_process_output(self):
        self.f.executable(self.f.root / 'target/debug/touchbar-doctor-probe', '''import sys, time
if 'config' in sys.argv:
    print('private subprocess output', flush=True)
    time.sleep(7)
else:
    print('invalid protocol')
''')
        rows = self.runtime()
        self.assertEqual(rows['runtime.probe']['status'], 'failed')
        self.assertIn('timed out', rows['runtime.config']['message'])
        self.assertNotIn('private subprocess output', json.dumps(rows))

    def test_oversized_stderr_cannot_escape_capture_limits_or_become_report_data(self):
        self.f.executable(self.f.root / 'target/debug/touchbar-doctor-probe', '''import sys
sys.stderr.write('private stderr'*10000)
sys.stderr.flush()
''')
        rows = self.runtime()
        self.assertEqual(rows['runtime.probe']['status'], 'failed')
        self.assertNotIn('private stderr', json.dumps(rows))

    def test_custom_target_directory_is_used_for_native_discovery(self):
        self.f.native()
        target = self.f.root / 'custom target ; spaces'
        shutil.move(str(self.f.root / 'target'), target)
        self.f.env['CARGO_TARGET_DIR'] = str(target)
        rows = self.runtime()
        self.assertEqual(rows['runtime.probe']['status'], 'ok')
        self.assertEqual(rows['runtime.config']['status'], 'ok')

    def test_triage_rejects_valid_doctor_json_when_aggregate_stderr_exceeds_its_limit(self):
        playbook = self.f.root / 'docs/agents/install-triage.md'
        playbook.parent.mkdir(parents=True)
        shutil.copy2(ROOT / 'docs/agents/install-triage.md', playbook)
        self.f.executable(self.f.root / 'scripts/doctor.py', f'''import json, sys
print(json.dumps({blocker_report()!r}), flush=True)
sys.stderr.write('private stderr' * 10000)
sys.stderr.flush()
sys.exit(1)
''')
        result = self.f.run('triage.py', '--prepare-only')
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertFalse((self.f.root / '.scratch/triage').exists())
        self.assertNotIn('private stderr', result.stdout + result.stderr)
