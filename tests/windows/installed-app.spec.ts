import { test, expect, chromium } from '@playwright/test';
import type { Browser } from '@playwright/test';
import { spawn, execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, mkdirSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import type { Environment } from '../../src/bridge';
import type { JobEvent } from '../../src/model';

declare global {
  interface Window {
    __TAURI_INTERNALS__: {
      invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
      transformCallback<T>(callback: (response: T) => void): number;
    };
    __dropperTestEvents: JobEvent[];
  }
}

test('installed Windows app opens offline and compresses using its bundled engine', async ({}, testInfo) => {
  test.skip(process.platform !== 'win32', 'Requires an installed Windows application');
  const { version } = JSON.parse(readFileSync('package.json', 'utf8')) as { version: string };
  const appFolder = join(process.env.LOCALAPPDATA!, 'Programs', 'Dropper');
  const ffmpeg = join(appFolder, 'media', 'bin', 'ffmpeg.exe');
  const ffprobe = join(appFolder, 'media', 'bin', 'ffprobe.exe');
  const input = testInfo.outputPath('원본 영상.mkv');
  const secondInput = testInfo.outputPath('두 번째 영상.mkv');
  const output = testInfo.outputPath('Compressed');
  mkdirSync(output, { recursive: true });
  execFileSync(ffmpeg, ['-hide_banner', '-loglevel', 'error', '-f', 'lavfi', '-i',
    'testsrc2=size=1280x720:rate=24', '-t', '20', '-c:v', 'ffv1', '-y', input]);
  copyFileSync(input, secondInput);
  const originalHash = createHash('sha256').update(readFileSync(input)).digest('hex');
  const serverRunning = await fetch('http://127.0.0.1:1420', { signal: AbortSignal.timeout(1000) })
    .then(() => true, () => false);
  expect(serverRunning, 'A development server must not mask a broken installer').toBe(false);

  const app = spawn(join(appFolder, 'dropper.exe'), [], {
    env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:
      '--remote-debugging-port=9222 --remote-debugging-address=127.0.0.1' },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let startupLog = '';
  const recordStartup = (data: Buffer | string) => { startupLog = (startupLog + data.toString()).slice(-16_384); };
  app.stdout.on('data', recordStartup);
  app.stderr.on('data', recordStartup);
  app.on('error', error => recordStartup(error.message));
  let browser: Browser | undefined;
  try {
    await expect.poll(async () => fetch('http://127.0.0.1:9222/json/version')
      .then(response => response.status, () => 0)).toBe(200);
    browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
    const context = browser.contexts()[0];
    await expect.poll(() => context.pages().length).toBeGreaterThan(0);
    const page = context.pages()[0];
    await expect(page).toHaveURL(/^https?:\/\/tauri\.localhost\//);
    await expect(page.getByRole('heading', { name: '툭 놓고, 가볍게.' })).toBeVisible();
    await expect(page.getByText(`v${version}`, { exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: '폴더 추가', exact: true })).toBeEnabled();
    const environment = await page.evaluate(() =>
      window.__TAURI_INTERNALS__.invoke<Environment>('get_environment'));
    expect(environment.ready, environment.error ?? undefined).toBe(true);
    expect(environment.source).toBe('bundled');
    expect(environment.codecs).toContain('h264');
    expect(environment.cpuThreads).toBeGreaterThan(0);
    console.log('Verified hardware encoders:', environment.gpuEncoders);

    await page.evaluate(async paths => {
      window.__dropperTestEvents = [];
      const handler = window.__TAURI_INTERNALS__.transformCallback<{payload: JobEvent}>(event => window.__dropperTestEvents.push(event.payload));
      await window.__TAURI_INTERNALS__.invoke('plugin:event|listen', {event:'job-progress',target:{kind:'Any'},handler});
      // Exercise the actual drop handler, import IPC, React state and Start button.
      await window.__TAURI_INTERNALS__.invoke('plugin:event|emit_to', {
        target:{kind:'WebviewWindow',label:'main'},event:'tauri://drag-drop',payload:{paths,position:{x:100,y:100}},
      });
    }, [input, secondInput]);
    await expect(page.getByText('원본 영상.mkv', {exact:true})).toBeVisible();
    await expect(page.getByText('두 번째 영상.mkv', {exact:true})).toBeVisible();
    await page.locator('summary').filter({hasText:'세부 설정'}).click();
    await expect(page.getByRole('combobox',{name:'처리 장치',exact:true})).toHaveValue('auto');
    await expect(page.getByRole('checkbox',{name:/맞춤 추천 자동 적용/})).toBeChecked();
    await page.getByRole('checkbox',{name:/맞춤 추천 자동 적용/}).uncheck();
    await page.getByRole('combobox',{name:'압축 방식',exact:true}).selectOption('h264');
    await page.getByRole('combobox',{name:'동시 작업',exact:true}).selectOption('2');
    await page.getByRole('button',{name:'2개 영상 압축',exact:true}).click();
    await expect(page.getByRole('main',{name:'작업 진행'})).toBeVisible();
    await expect(page.getByText('원본 영상.mkv',{exact:true})).toHaveCount(0);
    await expect(page.getByText('두 번째 영상.mkv',{exact:true})).toHaveCount(0);
    await expect(page.getByRole('progressbar',{name:'전체 압축 진행률'})).toBeVisible();
    await page.getByRole('button',{name:'자세히 보기',exact:true}).click();
    await expect(page.getByText('원본 영상.mkv',{exact:true})).toBeVisible();
    await page.getByRole('button',{name:'간단히 보기',exact:true}).click();
    await expect(page.getByRole('main',{name:'작업 진행'})).toBeVisible();
    await expect(page.getByRole('button',{name:'폴더 열기',exact:true})).toHaveCount(2);
    await expect(page.getByRole('main',{name:'작업 진행'})).toHaveCount(0);
    const events = await page.evaluate(() => window.__dropperTestEvents);
    const active = new Set<string>(); let peak = 0;
    for (const event of events) {
      if (event.phase === 'processing') { active.add(event.id); peak = Math.max(peak,active.size); }
      else active.delete(event.id);
    }
    expect(peak).toBe(Math.min(2,environment.cpuThreads));
    expect(events.filter(event=>event.phase==='done')).toHaveLength(2);
    expect(events.filter(event=>event.phase==='done').every(event=>!!event.backend)).toBe(true);
    const results = readdirSync(output).filter(name => name.endsWith('.mp4'));
    expect(results).toHaveLength(2);
    const compressed = join(output, results[0]);
    expect(statSync(compressed).size).toBeLessThan(statSync(input).size);
    const probe = JSON.parse(execFileSync(ffprobe, ['-v', 'error', '-show_streams', '-show_format',
      '-of', 'json', compressed], { encoding: 'utf8' })) as {
        streams: Array<{ codec_type: string; codec_name: string; width: number; height: number }>;
        format: { duration: string };
      };
    expect(probe.streams.find(stream => stream.codec_type === 'video'))
      .toMatchObject({ codec_name: 'h264', width: 1280, height: 720 });
    expect(Number(probe.format.duration)).toBeCloseTo(20, 1);
    expect(createHash('sha256').update(readFileSync(input)).digest('hex')).toBe(originalHash);
    expect(createHash('sha256').update(readFileSync(secondInput)).digest('hex')).toBe(originalHash);
    console.log('Installed app: embedded UI, no dev server, real drop/import, Auto device, parallel Start-button compression and original preservation passed.');
  } catch (error) {
    console.error('Installed app startup diagnostics', {
      pid: app.pid, exitCode: app.exitCode, signalCode: app.signalCode, startupLog,
    });
    if (app.pid) {
      const diagnostic = spawnSync('powershell.exe', ['-NoProfile', '-Command',
        `Get-Process -Id ${app.pid} -ErrorAction SilentlyContinue | Select-Object Id,ProcessName,Responding,MainWindowTitle; Get-CimInstance Win32_Process -Filter 'ParentProcessId=${app.pid}' | Select-Object ProcessId,Name,CommandLine | Format-List`],
      { encoding: 'utf8', timeout: 10_000 });
      console.error(diagnostic.stdout, diagnostic.stderr);
    }
    throw error;
  } finally {
    if (browser?.isConnected()) await browser.close();
    if (app.pid && app.exitCode === null) spawnSync('taskkill', ['/PID', String(app.pid), '/T', '/F']);
  }
});
