#!/usr/bin/env python3
"""Fetch a checksum-pinned Windows FFmpeg archive and extract only tools/licenses."""
from pathlib import Path
import hashlib
import json
import shutil
import sys
import urllib.request
import zipfile

root=Path(__file__).resolve().parents[1]
spec=json.loads((root/'scripts/windows-media.json').read_text())
cache=root/'artifacts/windows-media'
cache.mkdir(parents=True,exist_ok=True)
archive=cache/spec['asset']
if not archive.is_file():
    print('Downloading checksum-pinned Windows FFmpeg...',flush=True)
    with urllib.request.urlopen(urllib.request.Request(spec['url'],headers={'User-Agent':'Dropper-local-build'})) as response,archive.open('wb') as target:
        shutil.copyfileobj(response,target)
digest=hashlib.sha256()
with archive.open('rb') as data:
    for chunk in iter(lambda:data.read(1024*1024),b''):
        digest.update(chunk)
actual=digest.hexdigest()
if actual!=spec['sha256']:
    sys.exit(f'Checksum mismatch; refusing to extract. Expected {spec["sha256"]}, got {actual}')
with zipfile.ZipFile(archive) as source:
    for name in ('ffmpeg.exe','ffprobe.exe','LICENSE.txt'):
        matches=[info for info in source.infolist() if Path(info.filename).name==name]
        if len(matches)!=1:sys.exit(f'Expected exactly one {name} in archive')
        with source.open(matches[0]) as data,(cache/name).open('wb') as output:
            shutil.copyfileobj(data,output)
(cache/'source.json').write_text(json.dumps(spec,indent=2)+'\n')
print(f'Verified SHA-256 and extracted tools to {cache}')
