import hashlib
import json
import shutil
import subprocess
import tempfile
import unittest
import zipfile
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[2] / 'scripts/fetch-mac-intel-media.mjs'


class IntelMediaTests(unittest.TestCase):
    def prepare(self, root, include_sources_notice=True):
        (root / 'scripts').mkdir()
        shutil.copy2(SCRIPT, root / 'scripts' / SCRIPT.name)
        cache = root / 'artifacts/mac-intel-media'
        cache.mkdir(parents=True)
        archive = cache / 'fixture.zip'
        with zipfile.ZipFile(archive, 'w') as output:
            output.writestr('ffmpeg', b'ffmpeg fixture')
            output.writestr('ffprobe', b'ffprobe fixture')
            output.writestr('LICENSE.txt', 'license fixture')
            output.writestr('COPYING-dav1d.txt', 'copyright fixture')
            output.writestr('../NOTICE-escaped.txt', 'must not escape')
            if include_sources_notice:
                output.writestr('NOTICE-sources.md', 'source download directions')
        info = {'url': 'https://example.invalid/fixture.zip',
                'bytes': archive.stat().st_size,
                'sha256': hashlib.sha256(archive.read_bytes()).hexdigest()}
        (root / 'scripts/mac-intel-media.json').write_text(
            json.dumps({'ffmpeg': info, 'ffprobe': info}))
        return cache

    def run_fetch(self, root):
        return subprocess.run(['node', str(root / 'scripts' / SCRIPT.name)],
                              capture_output=True, text=True, timeout=15)

    def test_preparation_preserves_notices_beside_binaries_without_zip_path_escape(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            cache = self.prepare(root)
            result = self.run_fetch(root)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((cache / 'ffmpeg').read_bytes(), b'ffmpeg fixture')
            self.assertTrue((cache / 'LICENSE.txt').is_file(), 'engine preparation lost license notices')
            self.assertEqual((cache / 'LICENSE.txt').read_text(), 'license fixture')
            self.assertEqual((cache / 'COPYING-dav1d.txt').read_text(), 'copyright fixture')
            self.assertEqual((cache / 'NOTICE-sources.md').read_text(), 'source download directions')
            self.assertFalse((cache.parent / 'NOTICE-escaped.txt').exists())

    def test_preparation_rejects_engine_without_source_download_directions(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            cache = self.prepare(root, include_sources_notice=False)
            result = self.run_fetch(root)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('NOTICE-sources.md', result.stderr)
            self.assertFalse((cache / 'ffmpeg').exists())


if __name__ == '__main__':
    unittest.main()
