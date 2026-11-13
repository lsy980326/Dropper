import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


GUARD = Path(__file__).resolve().parents[2] / 'src-tauri/src/release_guard.rs'


@unittest.skipUnless(shutil.which('rustc'), 'Rust compiler required')
class ReleaseGuardTests(unittest.TestCase):
    def compile(self, *, desktop, dev, debug):
        with tempfile.TemporaryDirectory() as folder:
            args = ['rustc', '--edition=2021', '--crate-type=lib', str(GUARD),
                    '--out-dir', folder, '-C', 'debug-assertions=' + ('on' if debug else 'off')]
            if desktop:
                args += ['--cfg', 'feature="desktop"']
            if dev:
                args += ['--cfg', 'dev']
            return subprocess.run(args, capture_output=True, text=True)

    def test_desktop_release_rejects_development_server_mode(self):
        result = self.compile(desktop=True, dev=True, debug=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Desktop release builds must embed frontend assets', result.stderr)

    def test_embedded_desktop_release_is_allowed(self):
        result = self.compile(desktop=True, dev=False, debug=False)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_development_server_is_allowed_for_debug_builds(self):
        result = self.compile(desktop=True, dev=True, debug=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_core_only_release_does_not_require_desktop_assets(self):
        result = self.compile(desktop=False, dev=True, debug=False)
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == '__main__':
    unittest.main()
