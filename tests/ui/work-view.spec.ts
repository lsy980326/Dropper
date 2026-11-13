import {test,expect} from '@playwright/test';
import type {JobEvent} from '../../src/model';
declare global {interface Window {workTest:{emit:(event:Pick<JobEvent,'id'|'phase'|'progress'>&Partial<JobEvent>)=>void;finish:()=>void;runs:number}}}

const mock=`
export const desktop=true;
const rows=['비공개 이름.mp4','비밀 영상.mov'].map((name,i)=>({id:String(i),version:1,name,bytes:100000,duration:60,width:1280,height:720,codec:'h264',hdr:null,audioCount:0,subtitleCount:0,requiresMkv:false,blocked:null,notes:[]}));
let progress;
window.workTest={emit:(event)=>progress({...event,runId:window.workTest.runId,kind:window.workTest.kind}),finish:()=>window.workTest.resolve(),runs:0};
export const bridge={
 environment:async()=>({ready:true,source:'bundled',codecs:['h264','h265'],error:null,cpuThreads:8,gpuEncoders:[]}),
 onProgress:async cb=>{progress=cb;return()=>{}},onImportProgress:async()=>()=>{},onDrop:async()=>()=>{},
 chooseFiles:async()=>['one','two'],importPaths:async()=>({files:rows,warnings:[],skippedResults:0,skippedLinks:0,cancelled:false,folderCount:0}),
 run:async(ids,settings,folder,kind,runId)=>{window.workTest.runs++;window.workTest.kind=kind;window.workTest.runId=runId;return new Promise(resolve=>window.workTest.resolve=resolve)},
 cancel:async()=>{for(const row of rows)window.workTest.emit({id:row.id,phase:'cancelled',progress:0});window.workTest.finish()},
 clear:async()=>{},remove:async()=>{},reveal:async()=>{}
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
