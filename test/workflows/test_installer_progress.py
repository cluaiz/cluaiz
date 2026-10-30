"""Actual bootstrap download functions; tiny owned HTTP fixture, no real installation."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import json
import os
import re
import shutil
import subprocess
import threading
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
PROOF = ROOT / '.agent/installer-logo-proof'
PAYLOAD = bytes(range(256)) * 1024


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        try:
            if self.path == '/missing':
                self.send_error(404)
                return
            if self.path == '/redirect':
                self.send_response(302)
                self.send_header('Location', '/unknown')
                self.send_header('Content-Length', '999')
                self.end_headers()
                return
            self.send_response(200)
            if self.path != '/unknown':
                self.send_header('Content-Length', str(len(PAYLOAD)))
            self.end_headers()
            body = PAYLOAD[:32768] if self.path == '/truncated' else PAYLOAD
            for offset in range(0, len(body), 32768):
                self.wfile.write(body[offset:offset + 32768])
                self.wfile.flush()
                time.sleep(0.22)
        except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
            pass


def runtimes():
    for label, executable in (('ps5', 'powershell.exe'), ('ps7', 'pwsh.exe')):
        if shutil.which(executable):
            yield label, shutil.which(executable)
    shell = os.environ.get('CLUAIZ_TEST_SH')
    if not shell:
        shell = str(Path(os.environ.get('ProgramFiles', '')) / 'Git/bin/bash.exe') if os.name == 'nt' else shutil.which('sh')
    if shell and Path(shell).is_file():
        yield 'sh', shell


class InstallerProgressTests(unittest.TestCase):
    def test_real_known_unknown_redirect_and_failed_transfers(self):
        PROOF.mkdir(parents=True, exist_ok=True)
        server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        results = []
        try:
            available = list(runtimes())
            if not available:
                self.skipTest('No supported shell runtime found')
            for runtime, executable in available:
                for endpoint in ('known', 'unknown', 'redirect', 'missing', 'truncated', 'plain-log'):
                    with self.subTest(runtime=runtime, endpoint=endpoint):
                        output_file = PROOF / (f'test-{runtime}-{endpoint} with spaces.bin')
                        output_file.unlink(missing_ok=True)
                        route = 'known' if endpoint == 'plain-log' else endpoint
                        url = f'http://127.0.0.1:{server.server_port}/{route}'
                        if runtime == 'sh':
                            prefix = (ROOT / 'install.sh').read_text(encoding='utf-8').split('# ── Mascot', 1)[0]
                            wrapper = PROOF / 'test-download-case.sh'
                            wrapper.write_text(prefix + '\ninteractive=' + ('0' if endpoint == 'plain-log' else '1') + '\ndownload_pid=""\ntrap cleanup EXIT\ndownload_file "$1" "$2" "Fixture archive"\n', encoding='utf-8', newline='\n')
                            command = [executable, str(wrapper), url, output_file.as_posix()]
                        else:
                            command = [executable, '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File',
                                       str(Path(__file__).with_name('installer_download_case.ps1')), '-Source', str(ROOT / 'install.ps1'),
                                       '-Url', url, '-Destination', str(output_file)]
                            if endpoint != 'plain-log':
                                command.append('-Live')
                        run = subprocess.run(command, cwd=ROOT, capture_output=True, timeout=35)
                        output = (run.stdout + run.stderr).decode('utf-8', errors='replace')
                        (PROOF / f'test-{runtime}-{endpoint}.log').write_text(output, encoding='utf-8')
                        failed = endpoint in ('missing', 'truncated')
                        self.assertEqual(run.returncode != 0, failed, output[-1400:])
                        if failed:
                            self.assertNotIn('100%', output)
                        else:
                            self.assertEqual(output_file.read_bytes(), PAYLOAD)
                            self.assertIn('100%', output)
                            self.assertIn('Left 0.0 B', output)
                            if endpoint == 'known':
                                percents = [int(n) for n in re.findall(r'\]\s+(\d+)%', output)]
                                self.assertTrue(any(0 < n < 100 for n in percents), output)
                                self.assertIn('File ETA ~', output)
                            if endpoint in ('unknown', 'redirect'):
                                self.assertIn('--%', output)
                                self.assertIn('Left unknown', output)
                                self.assertNotIn('/ 999.0 B', output)
                            if endpoint == 'plain-log':
                                self.assertNotIn('\x1b[2A', output)
                                self.assertNotIn('\x1b[2K', output)
                        row = {'runtime': runtime, 'case': endpoint, 'result': 'PASS', 'exit': run.returncode}
                        results.append(row)
                        print(json.dumps(row), flush=True)
        finally:
            server.shutdown()
            server.server_close()
            (PROOF / 'progress-tests-result.json').write_text(json.dumps(results, indent=2), encoding='utf-8')


if __name__ == '__main__':
    unittest.main(verbosity=2)
