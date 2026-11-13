import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SCRIPT=Path(__file__).resolve().parents[2]/'scripts/verify-macos-installer.py'

class MacBundleTests(unittest.TestCase):
    def load(self):
        spec=importlib.util.spec_from_file_location('mac_bundle',SCRIPT)
        module=importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def fixture(self,root):
        media=root/'media'
        (media/'bin').mkdir(parents=True)
        (media/'licenses').mkdir()
        (media/'licenses'/'COPYING').write_text('fixture license')
        for name in ['ffmpeg','ffprobe']:(media/'bin'/name).write_bytes(name.encode())
        files={str(p.relative_to(media)):hashlib.sha256(p.read_bytes()).hexdigest() for p in media.rglob('*') if p.is_file()}
        (media/'manifest.json').write_text(json.dumps({'architecture':'arm64','files':files}))
        return media

    def test_all_media_files_must_match_the_preserved_manifest(self):
        with tempfile.TemporaryDirectory() as folder:
            media=self.fixture(Path(folder));module=self.load()
            module.verify_media(media,'arm64')
            (media/'bin'/'ffmpeg').write_bytes(b'changed')
            with self.assertRaisesRegex(RuntimeError,'checksum'):module.verify_media(media,'arm64')

    def test_manifest_must_not_escape_the_bundle_or_misstate_architecture(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);media=self.fixture(root);module=self.load()
            with self.assertRaisesRegex(RuntimeError,'architecture'):module.verify_media(media,'x86_64')
            data=json.loads((media/'manifest.json').read_text());data['files']['../outside']='0'*64
            (media/'manifest.json').write_text(json.dumps(data))
            with self.assertRaisesRegex(RuntimeError,'escape'):module.verify_media(media,'arm64')

    def test_engine_binaries_cannot_be_missing_from_the_manifest(self):
        with tempfile.TemporaryDirectory() as folder:
            media=self.fixture(Path(folder));module=self.load()
            data=json.loads((media/'manifest.json').read_text());del data['files']['bin/ffprobe']
            (media/'manifest.json').write_text(json.dumps(data))
            with self.assertRaisesRegex(RuntimeError,'manifest'):module.verify_media(media,'arm64')

    def test_library_references_are_system_or_resolve_inside_the_app(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);binary=root/'Contents/Resources/media/bin/ffmpeg';binary.parent.mkdir(parents=True)
            library=binary.parent.parent/'lib/libavcodec.dylib';library.parent.mkdir();library.write_bytes(b'fixture')
            module=self.load()
            for name in ['/usr/lib/libSystem.B.dylib','/System/Library/Frameworks/Foundation.framework/Foundation','@loader_path/../lib/libavcodec.dylib']:
                module.verify_dependency(name,binary,root)
            for name in ['/opt/homebrew/lib/libavcodec.dylib','@loader_path/../../../../outside','@rpath/missing.dylib']:
                with self.assertRaises(RuntimeError):module.verify_dependency(name,binary,root)

if __name__=='__main__':unittest.main()
