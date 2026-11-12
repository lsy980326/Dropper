import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const windows=process.platform==='win32';
if(!windows&&process.platform!=='darwin')throw new Error('Windows와 macOS 빌드만 지원합니다.');
const python=windows?'python':'python3';
function run(program,args){
  const result=spawnSync(program,args,{cwd:root,stdio:'inherit'});
  if(result.error)throw result.error;
  if(result.status!==0)process.exit(result.status??1);
}
if(windows){
  run(python,['scripts/fetch-windows-media.py']);
  run(python,['scripts/bundle-media.py','--windows','--source-dir','artifacts/windows-media','--output','src-tauri/resources/media-windows']);
}else run(python,['scripts/bundle-media.py']);
run(process.execPath,['node_modules/@tauri-apps/cli/tauri.js','build','--bundles',windows?'nsis':'app,dmg']);
