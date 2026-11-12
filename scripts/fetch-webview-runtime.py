#!/usr/bin/env python3
"""Download Microsoft's official WebView2 bootstrapper without running it."""
from pathlib import Path
import hashlib
import json
import os
import tempfile
import urllib.request

def prepare(root):
    url='https://go.microsoft.com/fwlink/p/?LinkId=2124703'
    file=root/'artifacts/MicrosoftEdgeWebview2Setup.exe'
    file.parent.mkdir(parents=True,exist_ok=True)
    record=file.parent/'webview-source.json'
    if file.is_file() and record.is_file():
        try:
            info=json.loads(record.read_text())
        except (OSError,ValueError):
            info={}
        data=file.read_bytes()
        if info.get('url')==url and info.get('size')==len(data) and info.get('sha256')==hashlib.sha256(data).hexdigest() and data.startswith(b'MZ'):
            return file
    pending=None
    manifest=None
    try:
        with tempfile.NamedTemporaryFile(dir=file.parent,prefix='.webview-',suffix='.part',delete=False) as target:
            pending=Path(target.name)
            digest=hashlib.sha256();size=0;prefix=b''
            with urllib.request.urlopen(url) as response:
                expected=response.headers.get('Content-Length')
                for chunk in iter(lambda:response.read(64*1024),b''):
                    if not prefix:prefix=chunk[:2]
                    target.write(chunk);digest.update(chunk);size+=len(chunk)
            target.flush();os.fsync(target.fileno())
        if expected is not None and size!=int(expected):
            raise RuntimeError('Incomplete WebView2 download; refusing to cache it')
        if prefix!=b'MZ':
            raise RuntimeError('The official download did not return a Windows executable')
        with tempfile.NamedTemporaryFile(dir=file.parent,prefix='.webview-source-',suffix='.part',mode='w',delete=False) as output:
            manifest=Path(output.name)
            json.dump({'url':url,'size':size,'sha256':digest.hexdigest()},output,indent=2)
            output.write('\n');output.flush();os.fsync(output.fileno())
        os.replace(pending,file);pending=None
        os.replace(manifest,record);manifest=None
        return file
    finally:
        for temporary in (pending,manifest):
            if temporary is not None:temporary.unlink(missing_ok=True)

if __name__=='__main__':
    prepare(Path(__file__).resolve().parents[1])
    print('Prepared official Microsoft WebView2 bootstrapper (not executed).')
