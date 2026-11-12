import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { open } from '@tauri-apps/plugin-dialog';
import type { Codec, FileItem, JobEvent, Settings } from './model';

export const desktop = isTauri();
export type Environment = {ready:boolean;source:string;codecs:Codec[];error:string|null};
export type ImportProgress = {runId:number;stage:'scanning'|'analyzing';found:number;checked:number};
export type ImportResult = {files:FileItem[];warnings:string[];skippedResults:number;skippedLinks:number;cancelled:boolean;folderCount:number};
export const extensions = ['mp4','mov','mkv','avi','webm','wmv','flv','mpeg','mpg','ts','mts','m2ts','m4v','3gp'];
export const bridge = {
  environment: () => invoke<Environment>('get_environment'),
  async chooseFiles():Promise<string[]> {
    const selected=await open({multiple:true,directory:false,filters:[{name:'영상 파일',extensions}]});
    return selected===null?[]:Array.isArray(selected)?selected:[selected];
  },
  chooseFolder: () => open({directory:true,multiple:false}),
  chooseSourceFolder: () => open({title:'압축할 영상 폴더 선택 · 하위 폴더 포함',directory:true,multiple:false}),
  add: (paths:string[]) => invoke<FileItem[]>('add_files',{paths}),
  importPaths: (paths:string[],outputFolder:string|null,runId:number) => invoke<ImportResult>('import_paths',{paths,outputFolder,runId}),
  remove: (id:string) => invoke<void>('remove_file',{id}),
  clear: () => invoke<void>('clear_files'),
  run: (ids:string[],settings:Settings,folder:string|null,kind:'estimate'|'compress',runId:number) => invoke<void>('run_jobs',{ids,settings,folder,kind,runId}),
  cancel: () => invoke<void>('cancel_active'),
  reveal: (id:string) => invoke<void>('reveal_output',{id}),
  onProgress: (callback:(event:JobEvent)=>void) => listen<JobEvent>('job-progress',event=>callback(event.payload)),
  onImportProgress: (callback:(event:ImportProgress)=>void) => listen<ImportProgress>('import-progress',event=>callback(event.payload)),
  onDrop: (callback:(paths:string[])=>void,hover:(active:boolean)=>void) => getCurrentWebviewWindow().onDragDropEvent(event=>{
    if(event.payload.type==='drop'){hover(false);callback(event.payload.paths);}
    else hover(event.payload.type==='enter'||event.payload.type==='over');
  }),
};
