"""Exercise the release launcher with real child processes and an injected developer PATH."""
import base64
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
LAUNCHER = ROOT / '.github/scripts/run_clean_windows.ps1'


@unittest.skipUnless(os.name == 'nt', 'Windows release launcher')
class CleanWindowsRuntimeTests(unittest.TestCase):
    def setUp(self):
        self.windows = Path(os.environ['WINDIR'])
        self.powershell = self.windows / 'System32/WindowsPowerShell/v1.0/powershell.exe'

    def run_launcher(self, code):
        encoded = base64.b64encode(code.encode('utf-16le')).decode('ascii')
        env = os.environ.copy()
        env['PATH'] += ';' + str(Path(tempfile.gettempdir()) / 'cluaiz developer PATH marker')
        env['CLUAIZ_LAUNCHER_TEST_VALUE'] = 'preserve other environment values'
        # A PS7 parent may export its own module paths to this Windows PowerShell child.
        env.pop('PSModulePath', None)
        return subprocess.run(
            [str(self.powershell), '-NoProfile', '-File', str(LAUNCHER),
             str(self.powershell), '-NoProfile', '-EncodedCommand', encoded],
            cwd=ROOT, env=env, capture_output=True, text=True, timeout=20,
            creationflags=subprocess.CREATE_NO_WINDOW,
        )

    def test_removes_developer_path_and_uses_executable_directory(self):
        result = self.run_launcher(
            '[pscustomobject]@{ path=$env:Path; cwd=(Get-Location).ProviderPath; '
            'value=$env:CLUAIZ_LAUNCHER_TEST_VALUE } | ConvertTo-Json -Compress'
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        actual = json.loads(result.stdout.strip())
        paths = {Path(p) for p in actual['path'].split(';')}
        self.assertEqual(paths, {self.windows / 'System32', self.windows})
        self.assertEqual(Path(actual['cwd']), self.powershell.parent)
        self.assertEqual(actual['value'], 'preserve other environment values')

    def test_propagates_engine_failure(self):
        result = self.run_launcher('exit 7')
        self.assertEqual(result.returncode, 7, result.stderr)

    def test_rejects_missing_executable_argument(self):
        result = subprocess.run(
            [str(self.powershell), '-NoProfile', '-File', str(LAUNCHER)],
            capture_output=True, text=True, timeout=20,
            creationflags=subprocess.CREATE_NO_WINDOW,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('A packaged executable is required', result.stderr)


if __name__ == '__main__':
    unittest.main(verbosity=2)
