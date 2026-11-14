import {test,expect} from '@playwright/test';
import type {JobEvent,Settings} from '../../src/model';
declare global {interface Window {workTest:{emit:(event:Pick<JobEvent,'id'|'phase'|'progress'>&Partial<JobEvent>)=>void;finish:()=>void;runs:number;lastSettings?:Settings;reveals:Array<[string,string|undefined]>}}}

const mock=`
export const desktop=true;
const rows=['비공개 이름.mp4','비밀 영상.mov'].map((name,i)=>({id:String(i),version:1,name,bytes:100000,duration:60,width:1280,height:720,codec:'h264',hdr:null,audioCount:0,subtitleCount:0,requiresMkv:false,blocked:null,notes:[],dataTracks:i===0?['#2 data (tmcd)']:[]}));
let progress;
window.workTest={emit:(event)=>progress({...event,runId:window.workTest.runId,kind:window.workTest.kind}),finish:()=>window.workTest.resolve(),runs:0,reveals:[]};
export const bridge={
 environment:async()=>({ready:true,source:'bundled',codecs:['h264','h265'],error:null,cpuThreads:8,gpuEncoders:[]}),
 onProgress:async cb=>{progress=cb;return()=>{}},onImportProgress:async()=>()=>{},onDrop:async()=>()=>{},
 chooseFiles:async()=>['one','two'],importPaths:async()=>({files:rows,warnings:[],skippedResults:0,skippedLinks:0,cancelled:false,folderCount:0}),
 run:async(ids,settings,folder,kind,runId)=>{window.workTest.runs++;window.workTest.lastSettings=settings;window.workTest.kind=kind;window.workTest.runId=runId;return new Promise(resolve=>window.workTest.resolve=resolve)},
 cancel:async()=>{for(const row of rows)window.workTest.emit({id:row.id,phase:'cancelled',progress:0});window.workTest.finish()},
 clear:async()=>{},remove:async()=>{},reveal:async(id,target)=>{window.workTest.reveals.push([id,target])}
};`;

test.beforeEach(async({page})=>{
 await page.route('**/src/bridge.ts',route=>route.fulfill({contentType:'application/javascript',body:mock}));
 await page.goto('/');
 await expect(page.getByRole('button',{name:'폴더 추가',exact:true})).toBeEnabled();
 await expect(page.getByRole('main',{name:'작업 진행'})).toHaveCount(0);
 await page.getByRole('button',{name:/파일 선택하기/}).click();
 await expect(page.getByText('비공개 이름.mp4',{exact:true})).toBeVisible();
});

test('시작 때만 파일명 없는 진행 화면을 열고 상세 전환 후 완료하면 결과로 돌아간다',async({page})=>{
 await page.getByRole('button',{name:'2개 영상 압축',exact:true}).click();
 const view=page.getByRole('main',{name:'작업 진행'});
 await expect(view).toBeVisible();
 await expect(page.locator('body')).not.toContainText('비공개 이름');
 expect(await page.locator('body').innerHTML()).not.toContain('비밀 영상');
 await page.evaluate(()=>{const w=window.workTest;w.emit({id:'0',phase:'processing',progress:0.5})});
 await expect(page.getByRole('progressbar',{name:'전체 압축 진행률'})).toHaveAttribute('value','25');
 await page.getByRole('button',{name:'자세히 보기',exact:true}).click();
 await expect(page.getByText('비공개 이름.mp4',{exact:true})).toBeVisible();
 await page.getByRole('button',{name:'간단히 보기',exact:true}).click();
 await expect(view).toBeVisible();
 expect(await page.evaluate(()=>window.workTest.runs)).toBe(1);
 await page.evaluate(()=>{const w=window.workTest;w.emit({id:'0',phase:'done',progress:1,outputBytes:50000,outputPath:'/out/a.mp4'})});
 await expect(page.getByRole('status')).toHaveText('2개 중 1개 완료');
 await page.screenshot({path:'artifacts/ui/dropper-work-view.png',fullPage:true});
 await page.evaluate(()=>{const w=window.workTest;w.emit({id:'1',phase:'done',progress:1,outputBytes:50000,outputPath:'/out/b.mp4'});w.finish()});
 await expect(view).toHaveCount(0);
 await expect(page.getByText('비공개 이름.mp4',{exact:true})).toBeVisible();
 await expect(page.getByRole('button',{name:'폴더 열기',exact:true})).toHaveCount(2);
});

test('예상 용량 확인은 기존 화면을 유지하고 압축 취소는 원래 화면으로 돌아간다',async({page})=>{
 await page.getByRole('button',{name:'예상 용량 확인',exact:true}).click();
 await expect(page.getByText('비공개 이름.mp4',{exact:true})).toBeVisible();
 await expect(page.getByRole('main',{name:'작업 진행'})).toHaveCount(0);
 await page.evaluate(()=>window.workTest.finish());
 await page.getByRole('button',{name:'2개 영상 압축',exact:true}).click();
 await expect(page.getByRole('main',{name:'작업 진행'})).toBeVisible();
 await page.getByRole('button',{name:'작업 취소',exact:true}).click();
 await expect(page.getByRole('main',{name:'작업 진행'})).toHaveCount(0);
 await expect(page.getByText('비공개 이름.mp4',{exact:true})).toBeVisible();
});


test('성공·실패 필터와 보관 위치, 기록을 구분하고 작업 화면에는 파일명을 노출하지 않는다',async({page})=>{
 await page.getByRole('button',{name:'2개 영상 압축',exact:true}).click();
 await page.evaluate(()=>{const w=window.workTest;w.emit({id:'0',phase:'done',progress:1,outputBytes:50000,outputPath:'/out/Results/a.mp4',originalMoved:true,archivedPath:'/out/Originals/비공개 이름.mp4',reportPath:'/out/Dropper-results.csv'});w.emit({id:'1',phase:'error',progress:1,message:'비밀 영상.mov 실패'});});
 await expect(page.getByText('성공 1 · 실패 1 · 원본 유지 0',{exact:true})).toBeVisible();
 expect(await page.locator('body').innerHTML()).not.toContain('/out/Originals');
 await expect(page.locator('body')).not.toContainText('비밀 영상.mov');
 await page.evaluate(()=>window.workTest.finish());
 await expect(page.locator('.row-archive-note').filter({hasText:'원본 보관 완료'})).toBeVisible();
 await page.getByRole('button',{name:'원본 보관 폴더',exact:true}).click();
 await page.getByRole('button',{name:'작업 기록',exact:true}).click();
 expect(await page.evaluate(()=>window.workTest.reveals)).toEqual([['0','original'],['0','report']]);
 await page.getByRole('button',{name:/^실패\s*1$/}).click();
 await expect(page.getByText('비밀 영상.mov',{exact:true})).toBeVisible();
 await expect(page.getByText('비공개 이름.mp4',{exact:true})).toHaveCount(0);
 await page.getByRole('button',{name:/^성공\s*1$/}).click();
 await expect(page.getByText('비공개 이름.mp4',{exact:true})).toBeVisible();
 await page.screenshot({path:'artifacts/ui/dropper-v024-results.png',fullPage:true});
});

test('승인한 데이터 제외·성공 원본 보관이 기본이고 용량 우선에 최대 효율을 강제하지 않는다',async({page})=>{
 await expect(page.getByRole('checkbox',{name:/성공한 원본을 보관 폴더로 이동/})).toBeChecked();
 await page.getByRole('radio',{name:/^용량 우선/}).check();
 await page.locator('summary').filter({hasText:'세부 설정'}).click();
 await expect(page.getByRole('checkbox',{name:/부가 데이터 트랙 제외/})).toBeChecked();
 await expect(page.getByRole('combobox',{name:'처리 강도',exact:true})).toHaveValue('balanced');
 await page.getByRole('button',{name:'2개 영상 압축',exact:true}).click();
 expect(await page.evaluate(()=>window.workTest.lastSettings)).toMatchObject({goal:'size',effort:'balanced',stripData:true,archiveOriginals:true});
 await page.getByRole('button',{name:'작업 취소',exact:true}).click();
});

test('보관 완료한 데이터 영상이 남아 있어도 다음 일반 영상의 설정을 막지 않는다',async({page})=>{
 await page.getByRole('button',{name:'2개 영상 압축',exact:true}).click();
 await page.evaluate(()=>{const w=window.workTest;w.emit({id:'0',phase:'done',progress:1,outputBytes:50000,outputPath:'/out/Results/a.mp4',originalMoved:true,archivedPath:'/out/Originals/a.mp4'});w.emit({id:'1',phase:'error',progress:1,message:'retry'});w.finish()});
 await page.locator('summary').filter({hasText:'세부 설정'}).click();
 await page.getByRole('checkbox',{name:/부가 데이터 트랙 제외/}).uncheck();
 await page.getByRole('checkbox',{name:/성공한 원본을 보관 폴더로 이동/}).uncheck();
 const start=page.getByRole('button',{name:'1개 영상 압축',exact:true});
 await expect(start).toBeEnabled();
 await start.click();
 expect(await page.evaluate(()=>window.workTest.lastSettings)).toMatchObject({stripData:false,archiveOriginals:false,organizeOutput:true});
 await page.getByRole('button',{name:'작업 취소',exact:true}).click();
});

test('압축 성공 후 원본 이동에 실패하면 성공 결과와 보관 경고를 함께 보여준다',async({page})=>{
 await page.getByRole('button',{name:'2개 영상 압축',exact:true}).click();
 await page.evaluate(()=>{const w=window.workTest;w.emit({id:'0',phase:'done',progress:1,outputBytes:50000,outputPath:'/out/Results/a.mp4',originalMoved:false,archivedPath:'/out/Originals/a.mp4',archiveWarning:'원본 이동 실패 · 원본을 제자리에 유지합니다.'});w.emit({id:'1',phase:'cancelled',progress:1});w.finish()});
 await expect(page.getByText('보관 사본 위치 · 원본 이동 확인 필요',{exact:false})).toBeVisible();
 await expect(page.getByText('원본 이동 실패 · 원본을 제자리에 유지합니다.',{exact:true})).toBeVisible();
 await expect(page.getByRole('button',{name:'폴더 열기',exact:true})).toHaveCount(1);
 await expect(page.getByRole('button',{name:'원본 보관 폴더',exact:true})).toHaveCount(0);
});
