import { useCallback, useEffect, useRef, useState } from 'react';
import { ArrowDown, ArrowDownToLine, ArrowRight, Check, ChevronDown, ChevronLeft, ChevronRight, FileVideo2, Folder, FolderOpen, FolderPlus, Plus, ShieldCheck, Trash2, TriangleAlert, X } from 'lucide-react';
import { bridge, desktop, type Environment, type ImportProgress } from './bridge';
import { applyEvent, applySettingsChange, DEFAULT_SETTINGS, formatBytes, formatDuration, pendingRows, totals, type FileItem, type Row, type Settings } from './model';

const qualities = [
  {value:'high',label:'화질 우선',description:'디테일을 더 선명하게'},
  {value:'balanced',label:'균형',description:'화질도, 용량도 알맞게'},
  {value:'small',label:'용량 우선',description:'파일 크기를 더 작게'},
] as const;
type Operation = 'adding' | 'estimate' | 'compress' | null;
const errorText=(error:unknown)=>error instanceof Error?error.message:String(error);

export default function App() {
  const [rows,setRows]=useState<Row[]>([]);
  const [settings,setSettings]=useState<Settings>(DEFAULT_SETTINGS);
  const [environment,setEnvironment]=useState<Environment|null>(null);
  const [listening,setListening]=useState(false);
  const [operation,setOperation]=useState<Operation>(null);
  const [folder,setFolder]=useState<string|null>(null);
  const [error,setError]=useState<string|null>(null);
  const [dragging,setDragging]=useState(false);
  const [cancelling,setCancelling]=useState(false);
  const [importProgress,setImportProgress]=useState<ImportProgress|null>(null);
  const [importNotice,setImportNotice]=useState<string|null>(null);
  const [importWarnings,setImportWarnings]=useState<string[]>([]);
  const [page,setPage]=useState(0);
  const [batchIds,setBatchIds]=useState<string[]>([]);
  const busyRef=useRef(false);
  const runId=useRef(0);
  const busy=operation!==null;
  const ready=desktop&&!!environment?.ready&&listening;

  useEffect(()=>{
    if(!desktop)return;
    let disposed=false;
    let cleanup:(()=>void)[]=[];
    bridge.environment().then(value=>{if(!disposed)setEnvironment(value);}).catch(e=>{if(!disposed)setError(errorText(e));});
    void Promise.allSettled([
      bridge.onProgress(event=>{if(!disposed)setRows(current=>applyEvent(current,event,runId.current));}),
      bridge.onImportProgress(event=>{if(!disposed&&busyRef.current&&event.runId===runId.current)setImportProgress(event);}),
    ]).then(results=>{
      const failed=results.find(result=>result.status==='rejected');
      const connected=results.flatMap(result=>result.status==='fulfilled'?[result.value]:[]);
      if(disposed){connected.forEach(unlisten=>unlisten());return;}
      cleanup=connected;
      if(failed?.status==='rejected')setError(`작업 상태 연결에 실패했습니다. 앱을 다시 열어 주세요. ${errorText(failed.reason)}`);
      else setListening(true);
    });
    return ()=>{disposed=true;cleanup.forEach(unlisten=>unlisten());};
  },[]);

  const addPaths=useCallback(async(paths:string[])=>{
    if(busyRef.current||paths.length===0)return;
    if(!desktop){setError('이 화면은 미리보기입니다. 파일 압축은 드롭퍼 데스크톱 앱에서 사용할 수 있습니다.');return;}
    busyRef.current=true;setOperation('adding');setError(null);setImportNotice(null);setImportWarnings([]);setImportProgress(null);
    const id=++runId.current;
    try{
      const result=await bridge.importPaths(paths,folder,id);
      setRows(current=>{
        const byId=new Map(current.map(row=>[row.id,row]));
        for(const item of result.files)if(byId.get(item.id)?.version!==item.version)byId.set(item.id,toRow(item));
        return [...byId.values()];
      });
      setPage(0);
      if(result.folderCount>0||result.cancelled||result.warnings.length>0||result.skippedLinks>0){
        const summary=[result.cancelled?'추가 취소':result.folderCount>0?'하위 폴더 포함':'파일 추가',`영상 ${result.files.length}개 확인`];
        if(result.skippedResults>0)summary.push(`기존 결과 폴더 ${result.skippedResults}개 제외`);
        if(result.skippedLinks>0)summary.push(`링크 ${result.skippedLinks}개 제외`);
        if(result.warnings.length>0)summary.push(`읽기 오류 ${result.warnings.length}개`);
        if(result.files.length===0&&!result.cancelled)summary.push('지원하는 영상 파일이 있는지 확인해 주세요');
        setImportNotice(summary.join(' · '));setImportWarnings(result.warnings);
      }
    }catch(e){setError(errorText(e));}
    finally{busyRef.current=false;setOperation(null);setCancelling(false);}
  },[folder]);

  useEffect(()=>{
    if(!desktop)return;
    let disposed=false;let cleanup:(()=>void)|undefined;
    bridge.onDrop(paths=>{void addPaths(paths);},setDragging).then(fn=>{if(disposed)fn();else cleanup=fn;})
      .catch(()=>{if(!disposed)setError('끌어놓기를 연결하지 못했습니다. 파일 추가 버튼을 사용해 주세요.');});
    return()=>{disposed=true;cleanup?.();};
  },[addPaths]);

  const chooseFiles=async()=>{
    if(!desktop){setError('이 화면은 미리보기입니다. 파일 압축은 드롭퍼 데스크톱 앱에서 사용할 수 있습니다.');return;}
    try{await addPaths(await bridge.chooseFiles());}catch(e){setError(errorText(e));}
  };
  const chooseSourceFolder=async()=>{
    if(!desktop){setError('폴더 전체 압축은 드롭퍼 데스크톱 앱에서 사용할 수 있습니다.');return;}
    try{const selected=await bridge.chooseSourceFolder();if(typeof selected==='string')await addPaths([selected]);}catch(e){setError(errorText(e));}
  };
  const changeSettings=(value:Partial<Settings>)=>{
    if(busyRef.current)return;
    setSettings(current=>({...current,...value}));setRows(current=>applySettingsChange(current,value));setError(null);
  };
  const remove=async(id:string)=>{
    if(busyRef.current)return;
    try{await bridge.remove(id);setRows(current=>current.filter(row=>row.id!==id));}catch(e){setError(errorText(e));}
  };
  const clear=async()=>{
    if(busyRef.current)return;
    busyRef.current=true;setOperation('adding');
    try{await bridge.clear();setRows([]);setError(null);setImportNotice(null);setImportWarnings([]);setPage(0);}catch(e){setError(errorText(e));}
    finally{busyRef.current=false;setOperation(null);}
  };
  const run=async(kind:'estimate'|'compress')=>{
    if(busyRef.current||!ready)return;
    const eligible=pendingRows(rows);
    if(!eligible.length)return;
    busyRef.current=true;setOperation(kind);setError(null);setCancelling(false);
    const id=++runId.current;
    const selected=new Set(eligible.map(row=>row.id));
    setBatchIds([...selected]);
    setRows(current=>current.map(row=>selected.has(row.id)?{...row,phase:'ready',progress:0,message:undefined,remainingSeconds:undefined,
      ...(kind==='estimate'?{estimatedBytes:undefined}:{outputBytes:undefined,outputPath:undefined})}:row));
    try{await bridge.run([...selected],settings,folder,kind,id);}
    catch(e){setError(errorText(e));}
    finally{busyRef.current=false;setOperation(null);setCancelling(false);}
  };
  const cancel=useCallback(async()=>{
    if(!busyRef.current||!desktop)return;
    setCancelling(true);
    try{await bridge.cancel();}catch(e){setError(errorText(e));setCancelling(false);}
  },[]);
  useEffect(()=>{
    const onKey=(event:KeyboardEvent)=>{if(event.key==='Escape'&&busyRef.current){event.preventDefault();void cancel();}};
    window.addEventListener('keydown',onKey);return()=>window.removeEventListener('keydown',onKey);
  },[cancel]);

  const changeFolder=async()=>{
    if(!desktop||busyRef.current)return;
    try{const selected=await bridge.chooseFolder();if(typeof selected==='string')setFolder(selected);}catch(e){setError(errorText(e));}
  };
  const summary=totals(rows);
  const candidates=rows.filter(row=>!row.blocked);
  const pending=pendingRows(rows);
  const totalBytes=rows.reduce((sum,row)=>sum+row.bytes,0);
  const codecAvailable=environment?.codecs.includes(settings.codec)??false;
  const hasHdr=candidates.some(row=>row.hdr!==null);
  const configurationValid=codecAvailable&&(!hasHdr||settings.codec==='h265');
  const activeRows=rows.filter(row=>row.phase==='processing');
  const active=activeRows[0];
  const gpu=environment?.gpuEncoders.find(encoder=>encoder.codec===settings.codec);
  const deviceName=settings.acceleration==='cpu'||!gpu?'CPU':`GPU · ${{nvidia:'NVIDIA',intel:'Intel',amd:'AMD',apple:'Apple'}[gpu.backend]}`;
  const concurrentLimit=environment?Math.min(settings.parallelism||(environment.cpuThreads>=4?2:1),environment.cpuThreads):null;
  const hasFolder=rows.some(row=>!!row.folderName);
  const pageCount=Math.max(1,Math.ceil(rows.length/50));
  const currentPage=Math.min(page,pageCount-1);
  const visibleRows=rows.slice(currentPage*50,currentPage*50+50);
  const selectedIds=new Set(batchIds);
  const batchFinished=rows.filter(row=>selectedIds.has(row.id)&&['done','unchanged','error','cancelled'].includes(row.phase)).length;

  return <div className="app-shell" onDragOver={event=>event.preventDefault()} onDrop={event=>event.preventDefault()}>
    <header className="app-header">
      <div className="brand"><span className="brand-mark" aria-hidden="true"><ArrowDownToLine size={23} strokeWidth={2}/></span><span>dropper<span className="brand-period">.</span></span></div>
      <span className="local-indicator"><ShieldCheck size={15}/>내 컴퓨터에서만</span>
    </header>

    <main>
      <div className="intro"><h1>툭 놓고, 가볍게.</h1><p>영상의 무게를 덜어내세요.</p></div>

      {!desktop&&<div className="notice preview-notice" role="status"><span>화면 미리보기</span>실제 파일 압축은 Windows·macOS 데스크톱 앱에서 사용할 수 있어요.</div>}
      {desktop&&!environment&&!error&&<div className="notice" role="status">영상 엔진을 준비하고 있어요.</div>}
      {environment?.error&&<div className="notice error-notice" role="alert"><TriangleAlert size={17}/><span>{environment.error}</span></div>}
      {error&&<div className="notice error-notice" role="alert"><TriangleAlert size={17}/><span>{error}</span><button className="icon-button" aria-label="안내 닫기" onClick={()=>setError(null)}><X size={16}/></button></div>}
      {importNotice&&<div className="notice import-notice" role="status">{importWarnings.length>0?<details><summary>{importNotice}</summary>{importWarnings.map((warning,index)=><p key={index}>{warning}</p>)}</details>:<span>{importNotice}</span>}</div>}

      <section className={`workspace ${dragging&&!busy?'is-dragging':''}`} aria-label="영상 파일">
        {rows.length===0?<><button className="drop-zone" onClick={()=>void chooseFiles()} disabled={busy||desktop&&!ready}>
          <span className="drop-symbol" aria-hidden="true"><FileVideo2 size={33} strokeWidth={1.5}/><span><Plus size={13}/></span></span>
          <strong>{operation==='adding'?'영상을 확인하고 있어요':'영상이나 폴더를 놓아주세요'}</strong>
          <span className="drop-description">또는 <span className="text-link">파일 선택하기</span></span>
          <span className="format-hint">MP4, MOV, MKV, AVI, WebM 외 주요 영상 형식</span>
        </button><div className="empty-import-actions"><button className="text-button" onClick={()=>void chooseSourceFolder()} disabled={busy||desktop&&!ready}><FolderPlus size={16}/>폴더 추가</button><span>하위 폴더의 영상까지 한 번에</span></div></>:<>
          <div className="list-toolbar"><span>영상 <strong>{rows.length}</strong>개 <span className="toolbar-size">{formatBytes(totalBytes)}</span></span><div><button className="text-button" onClick={()=>void clear()} disabled={busy}><Trash2 size={14}/>비우기</button><button className="text-button" onClick={()=>void chooseSourceFolder()} disabled={busy||!ready}><FolderPlus size={15}/>폴더 추가</button><button className="text-button" onClick={()=>void chooseFiles()} disabled={busy||!ready}><Plus size={16}/>파일 추가</button></div></div>
          <div className="file-list">{visibleRows.map(row=><FileRow key={row.id} row={row} operation={operation} busy={busy} onRemove={()=>void remove(row.id)} onReveal={()=>{void bridge.reveal(row.id).catch(e=>setError(errorText(e)));}}/>)}</div>
          {pageCount>1&&<div className="file-pagination"><span>{currentPage*50+1}–{Math.min(rows.length,currentPage*50+50)} / {rows.length}개 · 압축은 전체 목록에 적용</span><div><button className="icon-button" aria-label="이전 영상 목록" disabled={currentPage===0} onClick={()=>setPage(currentPage-1)}><ChevronLeft size={16}/></button><button className="icon-button" aria-label="다음 영상 목록" disabled={currentPage===pageCount-1} onClick={()=>setPage(currentPage+1)}><ChevronRight size={16}/></button></div></div>}
          <button className="add-more" onClick={()=>void chooseFiles()} disabled={busy||!ready}><Plus size={15}/>영상이나 폴더를 더 놓으세요</button>
        </>}
        {dragging&&!busy&&<div className="drop-overlay"><ArrowDown size={30}/><strong>여기에 놓아주세요</strong></div>}
      </section>

      <section className="controls" aria-label="압축 설정">
        <fieldset className="quality-fieldset" disabled={busy}><legend>어느 쪽이 더 중요한가요?</legend><div className="quality-options">
          {qualities.map(option=><label key={option.value} className={`quality-option ${settings.quality===option.value?'selected':''}`}><input type="radio" name="quality" value={option.value} checked={settings.quality===option.value} onChange={()=>changeSettings({quality:option.value})}/><span className="quality-label">{option.label}{option.value==='balanced'&&<span className="recommended">추천</span>}</span><span className="quality-description">{option.description}</span></label>)}
        </div></fieldset>
        <p className="processing-note">{environment?`${deviceName}${settings.acceleration==='auto'&&gpu?' 우선':''} · 최대 ${concurrentLimit}개 동시 처리`:'GPU·CPU 자동 선택 · 동시 작업 자동'}</p>
        <details className="advanced"><summary>세부 설정<ChevronDown size={15}/></summary><div className="advanced-content">
          <label>압축 방식<select disabled={busy} value={settings.codec} onChange={event=>changeSettings({codec:event.target.value as Settings['codec']})}>
            <option value="h265" disabled={desktop&&!environment?.codecs.includes('h265')}>H.265 · 기본</option>
            <option value="h264" disabled={desktop&&!environment?.codecs.includes('h264')}>H.264 · 재생 호환성</option>
            <option value="av1" disabled={desktop&&!environment?.codecs.includes('av1')}>AV1 · 압축 효율</option>
          </select></label>
          <label>저장 형식<select disabled={busy} value={settings.container} onChange={event=>changeSettings({container:event.target.value as Settings['container']})}><option value="auto">자동 · 원본 트랙 보존</option><option value="mp4">MP4</option><option value="mkv">MKV</option></select></label>
          <label>처리 장치<select disabled={busy} value={settings.acceleration} onChange={event=>changeSettings({acceleration:event.target.value as Settings['acceleration']})}><option value="auto">자동 · GPU 우선</option><option value="cpu">CPU · 전체 코어 활용</option></select></label>
          <label>동시 작업<select disabled={busy} value={settings.parallelism} onChange={event=>changeSettings({parallelism:Number(event.target.value) as Settings['parallelism']})}><option value="0">자동</option><option value="1">1개씩</option><option value="2">2개</option><option value="4">4개</option></select></label>
          <p>H.265·AV1은 재생 앱의 코덱 지원이 필요할 수 있어요. 해상도와 프레임 타이밍을 유지하며, ‘화질 우선’도 손실 압축입니다.</p>
          <p>GPU를 사용할 수 없거나 색 정보 보존이 필요한 영상은 CPU로 처리해요. GPU가 표시되지 않으면 그래픽 드라이버를 확인해 주세요. GPU는 같은 화질에서 결과가 더 커질 수 있어요.</p>
        </div></details>
        {hasHdr&&settings.codec!=='h265'&&<p className="configuration-warning" role="alert">HDR 영상의 색 정보를 보존하려면 세부 설정에서 H.265를 선택해 주세요.</p>}
        {desktop&&environment?.ready&&!codecAvailable&&<p className="configuration-warning" role="alert">현재 엔진이 선택한 압축 방식을 지원하지 않습니다. 세부 설정에서 다른 방식을 선택해 주세요.</p>}
      </section>

      <div className="destination"><Folder size={17}/><div><span className="destination-label">저장 위치</span><span className="destination-path" title={folder??'추가한 파일·폴더의 Compressed 폴더'}>{folder??(hasFolder?'선택한 폴더 / Compressed':'원본 폴더 / Compressed')}</span></div>{folder&&<button className="text-button reset-folder" disabled={busy} onClick={()=>setFolder(null)}>기본값</button>}<button className="text-button" onClick={()=>void changeFolder()} disabled={busy||!desktop}>변경</button></div>
      {hasFolder&&<p className="folder-output-hint">폴더로 추가한 영상은 하위 폴더 구조를 유지해 저장합니다.</p>}

      <footer className="action-area">
        <div className="result-summary" aria-live="polite">
          {busy?<><strong>{cancelling?'안전하게 멈추고 있어요':operation==='adding'?(importProgress?.stage==='scanning'?'폴더에서 영상을 찾고 있어요':'영상 정보를 확인하고 있어요'):operation==='estimate'?'예상 용량을 확인하고 있어요':'가벼워지는 중이에요'}</strong><span>{operation==='estimate'?'여러 구간을 짧게 시험 압축합니다.':operation==='adding'?(importProgress?.stage==='scanning'?`지금까지 ${importProgress.found}개 발견`:importProgress?`${importProgress.checked} / ${importProgress.found}개 확인`:'하위 폴더도 함께 확인합니다.'):`${batchFinished} / ${batchIds.length}개 처리${activeRows.length>1?` · ${activeRows.length}개 동시 압축`:active?` · ${active.name}`:''}`}</span></>:
          summary.percent!==null?<><strong className="success-text">{formatBytes(summary.original-summary.output)} 가벼워졌어요 <span className="savings-badge">{summary.percent.toFixed(1)}%</span></strong><span>완료한 영상의 실제 절감량{summary.complete?'':' · 나머지 파일은 목록에서 확인해 주세요'}</span></>:
          <><strong>{candidates.length>0?`${candidates.length}개 영상, ${formatBytes(candidates.reduce((sum,row)=>sum+row.bytes,0))}`:'준비되면 가볍게 시작하세요'}</strong><span>원본 보존 · 서버 업로드 없음</span></>}
        </div>
        <div className="action-buttons">{busy?<button className="button button-secondary cancel-button" disabled={cancelling} onClick={()=>void cancel()}><X size={16}/>{cancelling?'취소 중…':'작업 취소'}</button>:<>
          <button className="button button-secondary" onClick={()=>void run('estimate')} disabled={!ready||pending.length===0||!configurationValid}>예상 용량 확인</button>
          <button className="button button-primary" onClick={()=>void run('compress')} disabled={!ready||pending.length===0||!configurationValid}><ArrowDown size={17}/>{pending.length>0?`${pending.length}개 영상 압축`:'압축 시작'}</button>
        </>}</div>
      </footer>
      <p className="bottom-note">예상 용량은 시험 압축 결과이며, 실제 크기는 영상에 따라 달라집니다.</p>
    </main>
    <div className="app-bottom"><span>드롭퍼</span><span>가벼워지는 작은 습관.</span><span>v0.2.2</span></div>
  </div>;
}

function toRow(item:FileItem):Row{return {...item,phase:'ready',progress:0};}

function FileRow({row,operation,busy,onRemove,onReveal}:{row:Row;operation:Operation;busy:boolean;onRemove:()=>void;onReveal:()=>void}){
  const processing=row.phase==='processing';
  const output=row.phase==='done'?row.outputBytes:row.phase==='unchanged'?row.bytes:row.estimatedBytes;
  const actual=row.phase==='done'||row.phase==='unchanged';
  const reduction=output!==undefined&&row.bytes>0?(row.bytes-output)/row.bytes*100:null;
  const extension=row.name.split('.').pop()?.toUpperCase()??'영상';
  const issue=row.blocked??row.message;
  return <article className={`file-row ${row.blocked||row.phase==='error'?'has-error':''}`}>
    <span className="file-icon" aria-hidden="true"><FileVideo2 size={23} strokeWidth={1.5}/></span>
    <div className="file-main"><div className="file-title"><strong title={row.name}>{row.name}</strong>{row.hdr&&<span className="file-tag">{row.hdr}</span>}</div>
      <div className="file-meta"><span>{extension}</span>{row.width>0&&<><span>{row.width} × {row.height}</span><span>{formatDuration(row.duration)}</span></>}{row.backend&&<span className="backend-label" title="영상 처리 장치">{row.backend}</span>}{row.folderName&&row.relativePath&&<span className="source-path" title={`${row.folderName}/${row.relativePath}`}>{row.folderName}/{row.relativePath}</span>}</div>
      {processing?<div className="row-progress"><progress max={1} value={row.progress} aria-label={`${row.name} ${operation==='estimate'?'예상 용량 계산':'압축'} 진행률`}/><span>{operation==='estimate'?'시험 압축':'압축 중'} {Math.floor(row.progress*100)}%</span></div>:
      issue?<p className={`row-message ${row.phase==='unchanged'?'muted':''}`}>{row.phase==='cancelled'?'작업 취소 · 원본 유지':issue}</p>:
      row.notes.length>0?<details className="file-details"><summary>트랙·변환 정보</summary>{row.notes.map(note=><p key={note}>{note}</p>)}</details>:null}
      {row.backendNote&&row.phase!=='cancelled'&&row.phase!=='error'&&<p className="row-engine-note">{row.backendNote}</p>}
    </div>
    <div className="file-result"><div className="size-change"><span>{formatBytes(row.bytes)}</span>{output!==undefined&&<><ArrowRight size={12}/><strong>{!actual&&'약 '}{formatBytes(output)}</strong></>}</div>
      {reduction!==null?<span className={reduction>0?'reduction':'result-neutral'}>{actual?(row.phase==='unchanged'?'원본 유지':'실제 '):'예상 '}{row.phase!=='unchanged'&&(reduction>=0?`${reduction.toFixed(1)}% 감소`:`${Math.abs(reduction).toFixed(1)}% 증가`)}</span>:!processing&&!issue?<span className="estimate-prompt">예상 용량 확인 가능</span>:null}
      {row.phase==='done'&&row.outputPath&&<button className="reveal-button" disabled={busy} onClick={onReveal}><FolderOpen size={12}/>폴더 열기<Check size={12}/></button>}
    </div>
    <button className="icon-button remove-file" aria-label={`${row.name} 목록에서 제거`} disabled={busy} onClick={onRemove}><X size={16}/></button>
  </article>;
}
