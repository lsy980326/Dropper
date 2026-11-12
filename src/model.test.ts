import { describe, expect, it } from 'vitest';
import { applyEvent, invalidate, totals, pendingRows, type Row } from './model';

const row = (id: string, bytes: number): Row => ({id,version:1,name:`${id}.mp4`,bytes,duration:60,width:1920,height:1080,codec:'h264',hdr:null,audioCount:1,subtitleCount:0,requiresMkv:false,blocked:null,notes:[],phase:'ready',progress:0});

describe('실제 결과와 예상치의 수명주기', () => {
  it('설정을 바꾸면 이전 예상과 결과 상태를 지운다', () => {
    const result = invalidate([{...row('a',1000),estimatedBytes:400,outputBytes:300,phase:'done',outputPath:'/out/a.mp4',message:'완료',progress:1}]);
    expect(result[0].estimatedBytes).toBeUndefined();
    expect(result[0].outputBytes).toBeUndefined();
    expect(result[0].phase).toBe('ready');
    expect(result[0].outputPath).toBeUndefined();
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
