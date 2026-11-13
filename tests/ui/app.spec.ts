import { test, expect } from '@playwright/test';

test('웹 미리보기는 압축을 흉내 내지 않고 데스크톱 사용을 안내한다',async({page})=>{
  const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message));
  await page.goto('/');
  await expect(page.getByRole('heading',{name:'툭 놓고, 가볍게.'})).toBeVisible();
  await expect(page.getByRole('button',{name:'압축 시작',exact:true})).toBeDisabled();
  await page.getByRole('button',{name:/파일 선택하기/}).click();
  await expect(page.getByRole('alert')).toContainText('데스크톱 앱');
  expect(errors).toEqual([]);
});

test('폴더 전체 압축 진입점과 하위 폴더 포함 안내를 제공한다',async({page})=>{
  await page.goto('/');
  await expect(page.getByText('하위 폴더의 영상까지 한 번에')).toBeVisible();
  await page.getByRole('button',{name:'폴더 추가',exact:true}).click();
  await expect(page.getByRole('alert')).toContainText('폴더 전체 압축');
  await expect(page.getByRole('button',{name:'압축 시작',exact:true})).toBeDisabled();
});

test('품질은 한 가지씩 선택되며 상세 설정은 필요할 때 연다',async({page})=>{
  await page.goto('/');
  await expect(page.getByRole('radio',{name:/^균형/})).toBeChecked();
  await expect(page.getByRole('combobox',{name:'압축 방식',exact:true})).toBeHidden();
  await page.getByRole('radio',{name:/^화질 우선/}).check();
  await expect(page.getByRole('radio',{name:/^화질 우선/})).toBeChecked();
  await expect(page.getByRole('radio',{name:/^균형/})).not.toBeChecked();
  await page.locator('summary').filter({hasText:'세부 설정'}).click();
  await expect(page.getByRole('combobox',{name:'압축 방식',exact:true})).toBeVisible();
  await page.getByRole('combobox',{name:'압축 방식',exact:true}).selectOption('av1');
  await expect(page.getByRole('combobox',{name:'압축 방식',exact:true})).toHaveValue('av1');
});

test('가속과 병렬 설정은 자동이 기본이며 세부 설정 안에서만 보인다', async({page})=>{
  await page.goto('/');
  await expect(page.getByRole('combobox',{name:'처리 장치',exact:true})).toBeHidden();
  await page.locator('summary').filter({hasText:'세부 설정'}).click();
  const device=page.getByRole('combobox',{name:'처리 장치',exact:true});
  const concurrency=page.getByRole('combobox',{name:'동시 작업',exact:true});
  await expect(device).toHaveValue('auto');
  await expect(concurrency).toHaveValue('0');
  await device.selectOption('cpu');
  await concurrency.selectOption('2');
  await expect(device).toHaveValue('cpu');
  await expect(concurrency).toHaveValue('2');
  await page.screenshot({path:'artifacts/ui/dropper-processing-settings.png',fullPage:true});
});

for(const viewport of [{width:920,height:780},{width:720,height:560},{width:390,height:844}]){
  test(`${viewport.width}×${viewport.height} 창에서 가로 잘림 없이 주요 동작에 접근한다`,async({page})=>{
    await page.setViewportSize(viewport);await page.goto('/');
    await page.waitForLoadState('networkidle');
    expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
    await page.getByRole('button',{name:'압축 시작',exact:true}).scrollIntoViewIfNeeded();
    await expect(page.getByRole('button',{name:'압축 시작',exact:true})).toBeInViewport();
    await page.screenshot({path:`artifacts/ui/dropper-${viewport.width}.png`,fullPage:true});
  });
}
