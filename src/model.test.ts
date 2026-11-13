import { describe, expect, it } from 'vitest';
import { applyEvent, applySettingsChange, batchProgress, invalidate, totals, pendingRows, type Row } from './model';

const row = (id: string, bytes: number): Row => ({id,version:1,name:`${id}.mp4`,bytes,duration:60,width:1920,height:1080,codec:'h264',hdr:null,audioCount:1,subtitleCount:0,requiresMkv:false,blocked:null,notes:[],phase:'ready',progress:0});

describe('실제 결과와 예상치의 수명주기', () => {
  it('동시 작업 수만 바꾸면 완료 결과를 보존하고 새 영상만 처리한다', () => {
    const rows:Row[]=[{...row('a',1000),phase:'done',outputBytes:400,outputPath:'/out/a.mp4'},row('b',1000)];
    const updated=applySettingsChange(rows,{parallelism:2});
    expect(updated).toBe(rows);
    expect(updated[0].outputPath).toBe('/out/a.mp4');
    expect(pendingRows(updated).map(value=>value.id)).toEqual(['b']);
    expect(applySettingsChange(rows,{acceleration:'cpu'})[0].phase).toBe('ready');
  });
  it('설정을 바꾸면 이전 예상과 결과 상태를 지운다', () => {
    const result = invalidate([{...row('a',1000),estimatedBytes:400,outputBytes:300,phase:'done',outputPath:'/out/a.mp4',message:'완료',progress:1}]);
    expect(result[0].estimatedBytes).toBeUndefined();
    expect(result[0].outputBytes).toBeUndefined();
    expect(result[0].phase).toBe('ready');
    expect(result[0].outputPath).toBeUndefined();
  });
  it('병렬 진행 이벤트가 서로 다른 영상 상태를 덮어쓰지 않는다', () => {
    let rows: Row[] = [row('a', 1000), row('b', 1000)];
    rows = applyEvent(rows, {runId:5,id:'a',kind:'compress',phase:'processing',progress:0.3,backend:'GPU · NVIDIA'}, 5);
    rows = applyEvent(rows, {runId:5,id:'b',kind:'compress',phase:'processing',progress:0.7,backend:'CPU'}, 5);
    expect(rows.map(value=>[value.id,value.progress,value.backend])).toEqual([['a',0.3,'GPU · NVIDIA'],['b',0.7,'CPU']]);
  });
  it('GPU에서 CPU로 전환하면 이전 GPU 예상치를 지운다', () => {
    const rows = [{...row('a', 1000),backend:'GPU · NVIDIA',estimatedBytes:400}];
    const result = applyEvent(rows, {runId:6,id:'a',kind:'compress',phase:'processing',progress:0,backend:'CPU',backendNote:'CPU로 전환했습니다.'}, 6);
    expect(result[0].estimatedBytes).toBeUndefined();
    expect(result[0].backend).toBe('CPU');
    expect(result[0].backendNote).toContain('CPU로 전환');
    expect(invalidate(result)[0].backend).toBeUndefined();
  });
  it('다시 GPU를 사용하면 이전 CPU 전환 안내를 남기지 않는다', () => {
    const rows=[{...row('a',1000),backend:'CPU',backendNote:'CPU로 전환했습니다.'}];
    expect(applyEvent(rows,{runId:7,id:'a',kind:'compress',phase:'processing',progress:0,backend:'GPU · NVIDIA'},7)[0].backendNote).toBeUndefined();
  });
  it('지난 작업 이벤트가 현재 결과를 덮어쓰지 않는다', () => {
    const rows=[row('a',1000)];
    expect(applyEvent(rows,{runId:1,id:'a',kind:'estimate',phase:'done',progress:1,estimatedBytes:400},2)).toEqual(rows);
  });
  it('예상 계산 완료를 실제 압축 완료로 표시하지 않는다', () => {
    const result=applyEvent([row('a',1000)],{runId:3,id:'a',kind:'estimate',phase:'done',progress:1,estimatedBytes:400},3);
    expect(result[0].phase).toBe('ready');
    expect(result[0].estimatedBytes).toBe(400);
    expect(result[0].outputBytes).toBeUndefined();
    expect(totals(result).complete).toBe(false);
  });
  it('절감률은 파일별 평균이 아니라 완료 파일의 전체 바이트로 계산한다', () => {
    const result=totals([{...row('a',1000),phase:'done',outputBytes:500},{...row('b',9000),phase:'done',outputBytes:900}]);
    expect(result).toEqual({original:10000,output:1400,percent:86,complete:true});
  });
  it('압축 이득 없는 파일은 보관한 원본 크기로 합산한다', () => {
    expect(totals([{...row('a',1000),phase:'unchanged',outputBytes:1200}]).percent).toBe(0);
    expect(totals([]).percent).toBeNull();
  });
  it('완료한 파일의 실제 결과는 이후 예상 작업으로 덮어쓰지 않는다', () => {
    const rows:Row[]=[{...row('a',1000),phase:'done',outputBytes:400,outputPath:'/out/a.mp4'},row('b',2000)];
    expect(pendingRows(rows).map(row=>row.id)).toEqual(['b']);
    const result=applyEvent(rows,{runId:4,id:'a',kind:'estimate',phase:'done',progress:1,estimatedBytes:500},4);
    expect(result[0]).toEqual(rows[0]);
    expect(totals(result).output).toBe(400);
  });
});

describe('작업 화면의 전체 진행률',()=>{
  it('현재 배치만 기간에 따라 합산하고 대기 파일을 포함한다',()=>{
    const rows:Row[]=[{...row('old',1),phase:'done'},{...row('a',1),duration:30,phase:'done'}, {...row('b',1),duration:90,phase:'processing',progress:0.5},row('queued',1)];
    expect(batchProgress(rows,['a','b','queued'])).toEqual({percent:41,finished:1,total:3});
  });
  it('중간 100% 이벤트가 와도 모두 끝나기 전에는 100%로 표시하지 않는다',()=>{
    expect(batchProgress([{...row('a',1),phase:'processing',progress:1}],['a'])).toEqual({percent:99,finished:0,total:1});
    expect(batchProgress([{...row('a',1),phase:'cancelled'},{...row('b',1),phase:'error'}],['a','b'])).toEqual({percent:100,finished:2,total:2});
  });
  it('추천 목표와 처리 강도 변경은 이전 예측을 무효화한다',()=>{
    const rows:Row[]=[{...row('a',100),phase:'done',estimatedBytes:20}];
    for(const change of [{goal:'speed' as const},{effort:'efficiency' as const},{recommendation:false}])expect(applySettingsChange(rows,change)[0].estimatedBytes).toBeUndefined();
  });
});
