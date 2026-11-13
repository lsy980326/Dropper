export type Quality = 'high' | 'balanced' | 'small';
export type Codec = 'h264' | 'h265' | 'av1';
export type Container = 'auto' | 'mp4' | 'mkv';
export type Goal = 'speed' | 'balanced' | 'quality' | 'size';
export type Settings = { quality: Quality; codec: Codec; container: Container; acceleration: 'auto' | 'cpu' | 'nvidia' | 'intel' | 'amd' | 'apple'; goal: Goal; effort: 'speed' | 'balanced' | 'efficiency'; recommendation: boolean; parallelism: 0 | 1 | 2 | 4 };
export type FileItem = {
  id: string; version: number; name: string; bytes: number; duration: number; width: number; height: number;
  codec: string; hdr: string | null; audioCount: number; subtitleCount: number;
  requiresMkv: boolean; blocked: string | null; notes: string[];
  folderName?: string | null; relativePath?: string | null;
};
export type Phase = 'ready' | 'processing' | 'done' | 'error' | 'cancelled' | 'unchanged';
export type Row = FileItem & {
  phase: Phase; progress: number; estimatedBytes?: number; outputBytes?: number;
  outputPath?: string; message?: string; remainingSeconds?: number;
  backend?: string; backendNote?: string;
};
export type JobEvent = {
  runId: number; id: string; kind: 'estimate' | 'compress'; phase: Phase;
  progress: number; estimatedBytes?: number; outputBytes?: number;
  outputPath?: string; message?: string; remainingSeconds?: number;
  backend?: string; backendNote?: string;
};
export const DEFAULT_SETTINGS: Settings = { quality: 'balanced', codec: 'h265', container: 'auto', acceleration: 'auto', parallelism: 0, goal: 'balanced', effort: 'balanced', recommendation: true };
export function pendingRows(rows:Row[]):Row[]{ return rows.filter(row=>!row.blocked&&row.phase!=='done'&&row.phase!=='unchanged'); }

export function applyEvent(rows: Row[], event: JobEvent, runId: number): Row[] {
  if (event.runId !== runId) return rows;
  return rows.map(row => {
    if (row.id !== event.id) return row;
    const backendChanged=!!event.backend&&!!row.backend&&event.backend!==row.backend;
    const estimatedBytes=backendChanged?undefined:row.estimatedBytes;
    const backend=event.backend??row.backend;
    const backendNote=event.backendNote??(backendChanged?undefined:row.backendNote);
    if (event.kind === 'estimate') {
      if(row.phase==='done'||row.phase==='unchanged')return row;
      return {...row, backend, backendNote, progress:event.progress, phase:event.phase==='done'?'ready':event.phase,
        estimatedBytes:event.estimatedBytes ?? estimatedBytes, message:event.message,
        remainingSeconds:undefined};
    }
    return {...row,backend,backendNote,estimatedBytes,phase:event.phase,progress:event.progress,outputBytes:event.outputBytes,
      outputPath:event.outputPath,message:event.message,remainingSeconds:event.remainingSeconds};
  });
}
export function invalidate(rows: Row[]): Row[] {
  return rows.map(row=>({...row,phase:'ready',progress:0,estimatedBytes:undefined,
    outputBytes:undefined,outputPath:undefined,message:undefined,remainingSeconds:undefined,backend:undefined,backendNote:undefined}));
}
export function applySettingsChange(rows:Row[],change:Partial<Settings>):Row[]{
  return change.quality!==undefined||change.codec!==undefined||change.container!==undefined||change.acceleration!==undefined||change.goal!==undefined||change.effort!==undefined||change.recommendation!==undefined?invalidate(rows):rows;
}
export function totals(rows: Row[]): { original: number; output: number; percent: number | null; complete: boolean } {
  const finished=rows.filter(row=>row.phase==='done'||row.phase==='unchanged');
  const original=finished.reduce((sum,row)=>sum+row.bytes,0);
  const output=finished.reduce((sum,row)=>sum+(row.phase==='unchanged'?row.bytes:(row.outputBytes??row.bytes)),0);
  return {original,output,percent:original>0?(original-output)/original*100:null,
    complete:rows.length>0&&finished.length===rows.length};
}

export function formatBytes(bytes:number): string {
  if (!Number.isFinite(bytes)||bytes<0) return '—';
  if(bytes<1000) return `${Math.round(bytes)} B`;
  const exponent=Math.min(3,Math.floor(Math.log10(bytes)/3));
  return `${(bytes/1000**exponent).toLocaleString('ko-KR',{maximumFractionDigits:exponent===1?0:2})} ${['B','KB','MB','GB'][exponent]}`;
}
export function formatDuration(seconds:number): string {
  if(!Number.isFinite(seconds)||seconds<=0) return '—';
  const s=Math.round(seconds); const minutes=Math.floor(s/60);
  return minutes>=60?`${Math.floor(minutes/60)}:${String(minutes%60).padStart(2,'0')}:${String(s%60).padStart(2,'0')}`:`${minutes}:${String(s%60).padStart(2,'0')}`;
}

export function batchProgress(rows:Row[], ids:string[]):{percent:number;finished:number;total:number} {
  const selected=new Set(ids);
  const batch=rows.filter(row=>selected.has(row.id));
  let weight=0,done=0,finished=0;
  for(const row of batch){
    const w=Number.isFinite(row.duration)&&row.duration>0?row.duration:1;
    const complete=['done','unchanged','error','cancelled'].includes(row.phase);
    weight+=w;done+=w*(complete?1:row.phase==='processing'?Math.max(0,Math.min(1,Number.isFinite(row.progress)?row.progress:0)):0);
    if(complete)finished++;
  }
  const percent=weight>0?Math.floor(done/weight*100):0;
  return {percent:finished===ids.length?100:Math.min(99,percent),finished,total:ids.length};
}
