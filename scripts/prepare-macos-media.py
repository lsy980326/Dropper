#!/usr/bin/env python3
"""Reuse the exact source-covered macOS engine distributions from v0.2.0."""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT=Path(__file__).resolve().parents[1]

def run(*args):
    subprocess.run([str(a) for a in args],cwd=ROOT,check=True,timeout=180)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('architecture',choices=['arm64','x86_64'])
    args=parser.parse_args()
    if args.architecture=='x86_64':
        run('node','scripts/fetch-mac-intel-media.mjs')
        run('python3','scripts/bundle-media.py','--source-dir','artifacts/mac-intel-media','--output','src-tauri/resources/media-intel','--architecture','x86_64')
        return
    archive=ROOT/'artifacts/mac-base/Dropper_0.2.0_macOS_AppleSilicon.dmg'
    archive.parent.mkdir(parents=True,exist_ok=True)
    if not archive.exists():
        run('gh','release','download','v0.2.0','--repo','lsy980326/Dropper','--pattern',archive.name,'--output',archive)
    if archive.stat().st_size!=18821416 or hashlib.sha256(archive.read_bytes()).hexdigest()!='f463d3c13d1b38d24aef7d632f309a63c68f42a70a03ad25f7b02b56665ac3c8':
        raise RuntimeError('Apple Silicon base image checksum mismatch')
    output=ROOT/'src-tauri/resources/media'
    if output.exists():raise RuntimeError(f'Refusing to replace existing media: {output}')
    with tempfile.TemporaryDirectory(prefix='dropper-base-') as folder:
        mount=Path(folder)/'mount';mount.mkdir()
        run('hdiutil','attach',archive,'-readonly','-nobrowse','-mountpoint',mount)
        try:
            source=mount/'Dropper.app/Contents/Resources/media'
            if not (source/'manifest.json').is_file():raise RuntimeError('Original media manifest missing')
            shutil.copytree(source,output,symlinks=True)
        finally:run('hdiutil','detach',mount)
    print('Reused the original Apple Silicon engine, libraries and notices.')

if __name__=='__main__':main()
