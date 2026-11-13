import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(shutil.which('makensis'), 'NSIS compiler required')
class WindowsInstallerTests(unittest.TestCase):
    def test_installer_script_finds_payload_from_project_root(self):
        with tempfile.TemporaryDirectory(prefix='dropper packaging ') as folder:
            root = Path(folder)
            files = {
                'src-tauri/target-windows/x86_64-pc-windows-msvc/release/dropper.exe': b'app fixture',
                'src-tauri/resources/media-windows/bin/ffmpeg.exe': b'ffmpeg fixture',
                'src-tauri/resources/media-windows/bin/ffprobe.exe': b'ffprobe fixture',
                'THIRD_PARTY.md': b'license fixture',
                'artifacts/MicrosoftEdgeWebview2Setup.exe': b'runtime fixture',
            }
            for name, data in files.items():
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
            (root / 'scripts').mkdir()
            (root / 'src-tauri/icons').mkdir()
            shutil.copy2(ROOT / 'scripts/windows-installer.nsi', root / 'scripts')
            shutil.copy2(ROOT / 'src-tauri/icons/icon.ico', root / 'src-tauri/icons')
            result = subprocess.run(
                ['makensis', '-INPUTCHARSET', 'UTF8', '-DVERSION=0.0.0',
                 'scripts/windows-installer.nsi'],
                cwd=root, capture_output=True, text=True, encoding='utf-8', errors='replace',
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            installer = root / 'artifacts/Dropper_0.0.0_x64-setup.exe'
            self.assertTrue(installer.is_file())
            self.assertGreater(installer.stat().st_size, 0)


if __name__ == '__main__':
    unittest.main()
