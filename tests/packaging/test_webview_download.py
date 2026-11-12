import importlib.util
import io
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

script=Path(__file__).resolve().parents[2]/'scripts/fetch-webview-runtime.py'
spec=importlib.util.spec_from_file_location('webview_download',script)
webview=importlib.util.module_from_spec(spec)
spec.loader.exec_module(webview)

class Response(io.BytesIO):
    def __init__(self,data,length=None):
        super().__init__(data)
        self.headers={'Content-Length':str(len(data) if length is None else length)}

class BrokenResponse(Response):
    def read(self,size=-1):
        if self.tell()>0:raise OSError('connection interrupted')
        return super().read(4)

class WebviewDownloadTests(unittest.TestCase):
    def test_interrupted_transfer_never_becomes_final_executable(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            with patch.object(webview.urllib.request,'urlopen',return_value=BrokenResponse(b'MZ12345')):
                with self.assertRaises(OSError):webview.prepare(root)
            self.assertFalse((root/'artifacts/MicrosoftEdgeWebview2Setup.exe').exists())
            self.assertEqual(list((root/'artifacts').iterdir()),[])

    def test_early_eof_does_not_cache_a_truncated_pe(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            with patch.object(webview.urllib.request,'urlopen',return_value=Response(b'MZ1234',1000)):
                with self.assertRaises(RuntimeError):webview.prepare(root)
            self.assertFalse((root/'artifacts/MicrosoftEdgeWebview2Setup.exe').exists())

    def test_success_is_cached_only_after_complete_download(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);data=b'MZcomplete fixture'
            with patch.object(webview.urllib.request,'urlopen',return_value=Response(data)):
                file=webview.prepare(root)
            self.assertEqual(file.read_bytes(),data)
            with patch.object(webview.urllib.request,'urlopen',side_effect=AssertionError('must use verified cache')):
                self.assertEqual(webview.prepare(root).read_bytes(),data)

    def test_unverified_existing_partial_file_is_replaced(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);(root/'artifacts').mkdir()
            file=root/'artifacts/MicrosoftEdgeWebview2Setup.exe';file.write_bytes(b'MZpartial')
            with patch.object(webview.urllib.request,'urlopen',return_value=Response(b'MZcomplete fixture')):
                webview.prepare(root)
            self.assertEqual(file.read_bytes(),b'MZcomplete fixture')

if __name__=='__main__':unittest.main()
