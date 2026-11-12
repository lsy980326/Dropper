import { readFile, writeFile, mkdir, stat, rename, chmod } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const spec=JSON.parse(await readFile(join(root,'scripts/mac-intel-media.json'),'utf8'));
const folder=join(root,'artifacts/mac-intel-media');
await mkdir(folder,{recursive:true});
for(const [name,info] of Object.entries(spec)){
  const archive=join(folder,new URL(info.url).pathname.split('/').pop());
  let exists;
  try{exists=(await stat(archive)).isFile();}catch(error){if(error.code!=='ENOENT')throw error;exists=false;}
  if(!exists){
    const response=await fetch(info.url,{signal:AbortSignal.timeout(120000)});
    if(!response.ok)throw new Error(`Download failed: ${response.status}`);
    const data=Buffer.from(await response.arrayBuffer());
    if(data.length!==info.bytes||createHash('sha256').update(data).digest('hex')!==info.sha256)throw new Error(`${name} archive checksum mismatch`);
    await writeFile(archive+'.part',data);await rename(archive+'.part',archive);
  }
  const data=await readFile(archive);
  if(createHash('sha256').update(data).digest('hex')!==info.sha256)throw new Error(`${name} cached archive checksum mismatch`);
  const result=spawnSync('python3',['-c',
    `import sys,zipfile
from pathlib import Path
z=zipfile.ZipFile(sys.argv[1])
m=[i for i in z.infolist() if Path(i.filename).name==sys.argv[2]]
assert len(m)==1, 'Expected exactly one engine binary'
notices={i.filename:i for i in z.infolist() if '/' not in i.filename and i.filename.startswith(('LICENSE','COPYING','NOTICE'))}
for required in ('LICENSE.txt','NOTICE-sources.md'):
    if required not in notices:raise RuntimeError('Missing '+required)
target=Path(sys.argv[3])
for name,info in notices.items():
    destination=target.parent/name
    data=z.read(info)
    if destination.exists() and destination.read_bytes()!=data:raise RuntimeError('Conflicting notice '+name)
    destination.write_bytes(data)
target.write_bytes(z.read(m[0]))`,
    archive,name,join(folder,name)],{encoding:'utf8'});
  if(result.status!==0)throw new Error(result.stderr);
  await chmod(join(folder,name),0o755);
}
await writeFile(join(folder,'source.json'),JSON.stringify(spec,null,2)+'\n');
console.log('Prepared checksum-verified Intel macOS tools (not executed).');
