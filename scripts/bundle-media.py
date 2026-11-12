#!/usr/bin/env python3
"""Bundle locally supplied FFmpeg tools; never modify their installation.

macOS: copy all non-system dylibs and rewrite copies to relative paths.
Windows: provide --source-dir containing static ffmpeg.exe / ffprobe.exe.
Run this script separately for each target architecture. No implicit downloads.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]

def run(*args):
    return subprocess.run([str(a) for a in args], check=True, capture_output=True, text=True).stdout

def deps(path):
    return [line.strip().split(' (compatibility version')[0] for line in run('otool','-L',path).splitlines()[1:]]

def resolve(name, source):
    if name.startswith('/usr/lib/') or name.startswith('/System/'):
        return None
    if name.startswith('@loader_path/'):
        return (source.parent / name[len('@loader_path/'):]).resolve(strict=True)
    if name.startswith('@rpath/'):
        # Installed Homebrew libraries normally use absolute paths. Respect real LC_RPATH entries too.
        lines=run('otool','-l',source).splitlines()
        for i,line in enumerate(lines):
            if line.strip()=='cmd LC_RPATH':
                entry=lines[i+2].strip().split('path ',1)[1].rsplit(' (offset',1)[0]
                entry=entry.replace('@loader_path',str(source.parent))
                candidate=Path(entry)/name[len('@rpath/'):]
                if candidate.is_file(): return candidate.resolve()
        candidate=source.parent/name[len('@rpath/'):]
        if candidate.is_file(): return candidate.resolve()
        raise RuntimeError(f'Cannot resolve {name} from {source}')
    return Path(name).resolve(strict=True)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-dir',type=Path)
    parser.add_argument('--output',type=Path,default=ROOT/'src-tauri/resources/media')
    parser.add_argument('--windows',action='store_true')
    parser.add_argument('--skip-execution',action='store_true',help='Cross-target bundle: do not claim a host execution check')
    parser.add_argument('--architecture',choices=['arm64','aarch64','x86_64'])
    args=parser.parse_args()
    windows=args.windows or sys.platform=='win32'
    suffix='.exe' if windows else ''
    source_dir=args.source_dir
    sources={}
    for name in ('ffmpeg','ffprobe'):
        source=source_dir/(name+suffix) if source_dir else Path(shutil.which(name+suffix) or '')
        if not source.is_file(): raise RuntimeError(f'{name}{suffix} not found; supply --source-dir')
        sources[name]=source.resolve()
    output=args.output.resolve()
    # Limit cleanup to a generated directory with our own marker.
    if output.exists() and any(output.iterdir()):
        if not (output/'.dropper-media').is_file(): raise RuntimeError(f'Refusing to replace unmarked folder: {output}')
        shutil.rmtree(output)
    (output/'bin').mkdir(parents=True,exist_ok=True)
    (output/'licenses').mkdir()
    (output/'.dropper-media').write_text('Generated FFmpeg bundle.\n')
    copied={}
    for name,source in sources.items():
        target=output/'bin'/(name+suffix)
        shutil.copy2(source,target)
        target.chmod(0o755)
        copied[source]=target
    if not windows and sys.platform=='darwin':
        (output/'lib').mkdir()
        queue=list(copied)
        seen=set()
        while queue:
            source=queue.pop()
            if source in seen: continue
            seen.add(source)
            target=copied[source]
            changes=[]
            for name in deps(source):
                dependency=resolve(name,source)
                if dependency is None or dependency==source: continue
                if dependency not in copied:
                    dest=output/'lib'/dependency.name
                    if dest.exists(): raise RuntimeError(f'Conflicting library basename: {dest.name}')
                    shutil.copy2(dependency,dest);dest.chmod(0o755)
                    copied[dependency]=dest;queue.append(dependency)
                relative=os.path.relpath(copied[dependency],target.parent)
                changes.extend(['-change',name,'@loader_path/'+relative])
            if changes: run('install_name_tool',*changes,target)
            if target.parent.name=='lib': run('install_name_tool','-id','@rpath/'+target.name,target)
        for target in copied.values():
            run('codesign','--force','--sign','-',target)
        for source,target in copied.items():
            if source.parent.name=='lib':
                formula_root=source.parent.parent
                license_dir=output/'licenses'/formula_root.name
                license_dir.mkdir(exist_ok=True)
                for pattern in ('*LICENSE*','*COPYING*','*NOTICE*'):
                    for file in formula_root.glob(pattern):
                        if file.is_file(): shutil.copy2(file,license_dir/file.name)
    for name in sources:
        binary=output/'bin'/(name+suffix)
        # A cross-target bundle cannot be executed by the host.
        if args.skip_execution or (windows and sys.platform!='win32'):continue
        (output/'licenses'/f'{name}-build.txt').write_text(run(binary,'-version')+'\n'+run(binary,'-L'))
    if source_dir:
        for pattern in ('*LICENSE*','*COPYING*','*NOTICE*','source.json'):
            for file in source_dir.glob(pattern):
                if file.is_file():shutil.copy2(file,output/'licenses'/file.name)
    manifest={
        'platform':'windows' if windows else platform.system().lower(),
        'architecture': args.architecture or ('x86_64' if windows else platform.machine()),
        'sources':{name:str(path) for name,path in sources.items()},
        'files':{str(file.relative_to(output)):hashlib.sha256(file.read_bytes()).hexdigest() for file in output.rglob('*') if file.is_file()},
        'sourceInformation':'https://ffmpeg.org/download.html; build information and licenses in licenses/',
    }
    (output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print(f'Bundled {len(copied)} executables/libraries into {output}')
    print('Per-target build; review THIRD_PARTY.md before redistributing.')

if __name__=='__main__':
    try:main()
    except (RuntimeError,subprocess.CalledProcessError,OSError) as error:
        print(f'Media bundle failed: {error}',file=sys.stderr);sys.exit(1)
