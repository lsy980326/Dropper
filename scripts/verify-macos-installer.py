#!/usr/bin/env python3
"""Install a DMG in a temporary location and verify the native app and engine."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import socket
import subprocess
import tempfile
import time

ROOT=Path(__file__).resolve().parents[1]

def run(*args,env=None,timeout=120):
    result=subprocess.run([str(a) for a in args],cwd=ROOT,env=env,capture_output=True,text=True,timeout=timeout)
    if result.returncode:raise RuntimeError(f"{args[0]} failed ({result.returncode}): "+(result.stderr+result.stdout)[-16384:])
    return result.stdout

def inside(path,root):return path.resolve().is_relative_to(root.resolve())

def verify_media(media,architecture):
    manifest=json.loads((media/'manifest.json').read_text())
    if manifest.get('architecture')!=architecture:raise RuntimeError('Media architecture mismatch')
    files=manifest.get('files',{})
    for filename,expected in files.items():
        path=media/filename
        if Path(filename).is_absolute() or not inside(path,media):raise RuntimeError('Media manifest path escape')
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest()!=expected:
            raise RuntimeError(f'Media checksum mismatch: {filename}')
    for name in ['ffmpeg','ffprobe']:
        if f'bin/{name}' not in files:raise RuntimeError(f'Engine missing from manifest: {name}')
    if not any(p.is_file() for p in (media/'licenses').rglob('*')):raise RuntimeError('Engine licenses missing')

def verify_dependency(name,binary,app):
    if name.startswith(('/usr/lib/','/System/Library/')):return
    if name.startswith('@loader_path/'):
        target=binary.parent/name.removeprefix('@loader_path/')
    elif name.startswith('@executable_path/'):
        target=app/'Contents/MacOS'/name.removeprefix('@executable_path/')
    elif name=='@rpath/'+binary.name:
        return # LC_ID_DYLIB names the library itself, not an external dependency.
    else:raise RuntimeError(f'External library dependency: {name}')
    if not inside(target,app) or not target.is_file():raise RuntimeError(f'Unresolved or escaping library: {name}')

def verify_bundle(app,architecture,minimum,version):
    info=plistlib.loads((app/'Contents/Info.plist').read_bytes())
    if info['CFBundleShortVersionString']!=version:raise RuntimeError('Application version mismatch')
    if info['LSMinimumSystemVersion']!=minimum:raise RuntimeError('Deployment version mismatch')
    executable=app/'Contents/MacOS'/info['CFBundleExecutable']
    media=app/'Contents/Resources/media'
    verify_media(media,architecture)
    binaries=[executable,media/'bin/ffmpeg',media/'bin/ffprobe',*sorted((media/'lib').glob('*.dylib'))]
    for binary in binaries:
        if run('lipo','-archs',binary).strip()!=architecture:raise RuntimeError(f'Binary architecture mismatch: {binary.name}')
        run('codesign','--verify','--strict',binary)
        for line in run('otool','-L',binary).splitlines()[1:]:
            verify_dependency(line.strip().split(' (compatibility version')[0],binary,app)
    run('codesign','--verify','--deep','--strict',app)
    return executable

def verify_startup(executable):
    try:
        with socket.create_connection(('127.0.0.1',1420),timeout=1):
            raise RuntimeError('A development server must not mask a broken installer')
    except OSError:pass
    env={k:v for k,v in os.environ.items() if not k.startswith('DYLD_')}
    env['PATH']='/usr/bin:/bin:/usr/sbin:/sbin'
    with tempfile.TemporaryFile(mode='w+') as log:
        app=subprocess.Popen([str(executable)],env=env,stdout=log,stderr=log)
        try:
            time.sleep(6)
            if app.poll() is not None:
                log.seek(0);raise RuntimeError('Installed application exited: '+log.read()[-16384:])
            print(run('xcrun','swift',ROOT/'tests/macos/window-visible.swift',app.pid))
        finally:
            if app.poll() is None:
                app.terminate()
                try:app.wait(timeout=10)
                except subprocess.TimeoutExpired:app.kill();app.wait(timeout=10)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('dmg',type=Path)
    parser.add_argument('--architecture',required=True,choices=['arm64','x86_64'])
    parser.add_argument('--minimum',required=True)
    args=parser.parse_args()
    version=json.loads((ROOT/'package.json').read_text())['version']
    run('hdiutil','verify',args.dmg)
    with tempfile.TemporaryDirectory(prefix='dropper-installed-') as folder:
        root=Path(folder);mount=root/'mount';mount.mkdir()
        app=root/'Applications/Dropper.app';app.parent.mkdir()
        run('hdiutil','attach',args.dmg,'-readonly','-nobrowse','-mountpoint',mount)
        try:run('ditto',mount/'Dropper.app',app)
        finally:run('hdiutil','detach',mount)
        executable=verify_bundle(app,args.architecture,args.minimum,version)
        verify_startup(executable)
        env=os.environ.copy();env['DROPPER_INSTALLED_APP']=str(app)
        print(run('cargo','test','--manifest-path','src-tauri/Cargo.toml','--no-default-features','--locked','--test','macos_installed','--','--ignored','--nocapture',env=env,timeout=600))
    print(f'Installed macOS {args.architecture} app, signatures, offline window and bundled-engine batch verified.')

if __name__=='__main__':main()
