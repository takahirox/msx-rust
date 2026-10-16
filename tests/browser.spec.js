import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

async function boot(page, path = "/") {
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(path);
  await expect(page.locator('#status')).toHaveText('Running · Weave BASIC');
  await expect(page.locator('#transcript')).toContainText('READY');
  await page.locator('#screen').focus();
  return errors;
}
async function enter(page, line) {
  await page.locator('#screen').focus();
  await page.keyboard.type(line);
  await page.keyboard.press('Enter');
  await expect(page.locator('#transcript')).toContainText(`>${line}\n`);
}
async function colors(page) {
  return page.locator('#screen').evaluate(canvas => {
    const p = canvas.getContext('2d').getImageData(0, 0, 256, 192).data;
    const counts = {};
    for (let i = 0; i < p.length; i += 4) { const c = `${p[i]},${p[i+1]},${p[i+2]}`; counts[c] = (counts[c] || 0) + 1; }
    return counts;
  });
}
// Recognize the five output digits directly from the Canvas, independently of
// the interpreter transcript or emulated RAM. Each row contains a single digit.
async function visibleDigits(page) {
  return page.locator('#screen').evaluate(canvas => {
    const data = canvas.getContext('2d').getImageData(0, 0, 256, 192).data;
    const patterns = ['04,0c,04,04,04,04,0e', '0e,11,01,02,04,08,1f', '1e,01,01,0e,01,01,1e', '02,06,0a,12,1f,02,02', '1f,10,10,1e,01,01,1e'];
    const result = [];
    for (let row = 0; row < 24; row++) {
      const values = [];
      for (let y = 0; y < 7; y++) {
        let bits = 0;
        for (let x = 0; x < 5; x++) { const i = (((row*8+y)*256)+8+x)*4; bits = (bits << 1) | (data[i] === 255 && data[i+1] === 255 && data[i+2] === 255 ? 1 : 0); }
        values.push(bits.toString(16).padStart(2, '0'));
      }
      const digit = patterns.indexOf(values.join(',')) + 1;
      let extra = false;
      for (let y = 0; y < 8; y++) for (let x = 14; x < 248; x++) { const i = ((row*8+y)*256+x)*4; if (data[i] === 255 && data[i+1] === 255 && data[i+2] === 255) extra = true; }
      if (digit && !extra) result.push(digit);
    }
    return result;
  });
}
test('boots, edits, lists and runs a BASIC program with visible output', async ({ page }) => {
  const errors = await boot(page);
  expect((await colors(page))['255,255,255']).toBeGreaterThan(300);
  await enter(page, '10 FOR I=1 TO 5 : PRINT I : NEXT I');
  await enter(page, 'LIST');
  await expect(page.locator('#transcript')).toContainText('10 FOR I=1 TO 5 : PRINT I : NEXT I\nREADY');
  await enter(page, 'RUN');
  await expect(page.locator('#transcript')).toContainText('1\n2\n3\n4\n5\nREADY');
  await expect.poll(() => visibleDigits(page)).toEqual([1, 2, 3, 4, 5]);
  await page.screenshot({ path: 'test-results/basic.png', fullPage: true });
  await enter(page, '10 PRINT 9');
  await enter(page, 'RUN');
  await expect(page.locator('#transcript')).toContainText('>RUN\n9\nREADY');
  await enter(page, '10');
  await page.keyboard.type('PRNT 42');
  await page.keyboard.press('Home');
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('ArrowRight');
  await page.keyboard.type('I');
  await page.keyboard.press('End');
  await page.keyboard.press('Backspace');
  await page.keyboard.type('3');
  await page.keyboard.press('Enter');
  await expect(page.locator('#transcript')).toContainText('>PRINT 43\n43\nREADY');
  await page.getByRole('button', { name: 'Pause', exact: true }).click();
  await expect(page.locator('#status')).toContainText('Paused');
  await page.getByRole('button', { name: 'Resume', exact: true }).click();
  await page.getByRole('button', { name: 'Reset', exact: true }).click();
  await expect(page.locator('#transcript')).not.toContainText('PRINT 43');
  expect(errors).toEqual([]);
});
test('graphics uses the VDP and Escape interrupts an infinite BASIC program', async ({ page }) => {
  await boot(page);
  await page.getByRole('button', { name: 'Graphics demo' }).click();
  await expect.poll(async () => (await colors(page))['212,193,84'] || 0).toBeGreaterThan(200);
  await page.screenshot({ path: 'test-results/graphics.png', fullPage: true });
  await page.keyboard.press('Escape');
  await expect(page.locator('#transcript')).toContainText('BREAK');
  await enter(page, '10 GOTO 10');
  await enter(page, 'RUN');
  await page.keyboard.press('Escape');
  await expect(page.locator('#transcript')).toContainText('>RUN\nBREAK\nREADY');
  await enter(page, 'PRINT 6*7');
  await expect(page.locator('#transcript')).toContainText('42\nREADY');
});
test('Web Audio plays a PSG tone and fresh buffers settle to silence after muting', async ({ page }) => {
  await page.addInitScript(() => {
    window.audioEvidence = { state: '', buffers: 0, maximum: 0, zeroCrossings: 0, connections: 0, recent: [] };
    const NativeContext = window.AudioContext;
    window.AudioContext = class extends NativeContext {
      constructor(...args) {
        super(...args);
        window.audioClock = this;
        const create = this.createBufferSource.bind(this);
        this.createBufferSource = () => {
          const source = create();
          const start = source.start.bind(source);
          const connect = source.connect.bind(source);
          source.connect = (...args) => { window.audioEvidence.connections++; return connect(...args); };
          source.start = (...args) => {
            const d = source.buffer.getChannelData(0);
            const startTime = Math.max(this.currentTime, args[0] || 0);
            const record = { startTime, endTime: startTime + source.buffer.duration, maximum: 0, ended: false };
            source.addEventListener('ended', () => { record.ended = true; });
            window.audioEvidence.buffers++;
            window.audioEvidence.state = this.state;
            for (let i = 0; i < d.length; i++) {
              record.maximum = Math.max(record.maximum, Math.abs(d[i]));
              window.audioEvidence.maximum = Math.max(window.audioEvidence.maximum, Math.abs(d[i]));
              if (i && d[i-1] <= 0 && d[i] > 0) window.audioEvidence.zeroCrossings++;
            }
            window.audioEvidence.recent.push(record);
            if (window.audioEvidence.recent.length > 180) window.audioEvidence.recent.shift();
            return start(...args);
          };
          return source;
        };
      }
    };
  });
  await boot(page);
  await page.getByRole('button', { name: 'Enable audio', exact: true }).click();
  await expect(page.locator('#audio')).toHaveText('Mute audio');
  await page.getByRole('button', { name: 'Sound demo', exact: true }).click();
  await expect(page.locator('#tone')).toHaveText('Stop tone');
  await expect(page.locator('#screen')).toBeFocused();
  await expect(page.locator('#transcript')).toContainText('>BEEP');
  await expect.poll(() => page.evaluate(() => window.audioEvidence.maximum)).toBeGreaterThan(0.05);
  await expect.poll(() => page.evaluate(() => window.audioEvidence.zeroCrossings)).toBeGreaterThan(10);
  const evidence = await page.evaluate(() => window.audioEvidence);
  expect(evidence.state).toBe('running');
  expect(evidence.connections).toBeGreaterThan(0);
  await page.getByRole('button', { name: 'Stop tone', exact: true }).click();
  await expect(page.locator('#tone')).toHaveText('Sound demo');
  // Bound settling to 500 ms: the app queues at most 150 ms of playback,
  // and the PSG DC filter gets 250 ms to decay, plus frame/command margin.
  const settledAfter = await page.evaluate(() => window.audioClock.currentTime + 0.5);
  const completedFreshBuffers = () => page.evaluate(after => {
    const now = window.audioClock.currentTime;
    return window.audioEvidence.recent.filter(buffer => buffer.startTime >= after && buffer.endTime <= now && buffer.ended);
  }, settledAfter);
  // Require at least 12 new, completed buffers (about 200 ms), so neither
  // stale tone statistics nor a stopped audio scheduler can pass as silence.
  await expect.poll(async () => (await completedFreshBuffers()).length).toBeGreaterThanOrEqual(12);
  const muted = await completedFreshBuffers();
  expect(muted.every(buffer => Number.isFinite(buffer.maximum) && buffer.maximum < 0.0001)).toBe(true);
  expect(await page.evaluate(() => window.audioClock.state)).toBe('running');
  await page.getByRole('button', { name: 'Mute audio', exact: true }).click();
  await expect(page.locator('#audio-state')).toContainText('Audio off');
  await expect.poll(() => page.evaluate(() => window.audioClock.state)).toBe('suspended');
  await page.getByRole('button', { name: 'Enable audio', exact: true }).click();
  await expect(page.locator('#audio')).toHaveText('Mute audio');
  await page.evaluate(() => window.audioClock.suspend());
  await expect(page.locator('#audio')).toHaveText('Resume audio');
  await page.getByRole('button', { name: 'Resume audio', exact: true }).click();
  await expect(page.locator('#audio-state')).toHaveText('Audio on');
});
test('file picker boots original cartridge, handles bad ROMs, ejects and loads local system ROM', async ({ page }) => {
  const errors = await boot(page);
  const requests = [];
  page.on('request', request => requests.push({ method: request.method(), url: request.url() }));
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Open ROM', exact: true }).click();
  await (await chooser).setFiles('dist/test-cartridge.rom');
  await expect(page.locator('#status')).toContainText('test-cartridge.rom');
  await expect(page.locator('#example')).toBeDisabled();
  await expect(page.locator('#starter-help')).toContainText('Built-in BASIC');
  await expect(page.locator('#eject')).toBeEnabled();
  await expect.poll(async () => (await colors(page))['212,193,84'] || 0).toBeGreaterThan(20);
  await page.screenshot({ path: 'test-results/cartridge.png', fullPage: true });
  // Select an invalid ROM and ensure the running cartridge survives.
  await page.locator('#cartridge').setInputFiles({ name: 'invalid.rom', mimeType: 'application/octet-stream', buffer: Buffer.alloc(17) });
  await expect(page.locator('#status')).toContainText('plain 8, 16, 32 or 48');
  await page.getByRole('button', { name: 'Eject cartridge' }).click();
  await expect(page.locator('#transcript')).toContainText('READY');
  await expect(page.locator('#example')).toBeEnabled();
  await expect(page.locator('#screen')).toBeFocused();
  // Authored fixture fills the screen with VDP backdrop color A, then HALTs.
  // Its OUT F1 is deliberately ignored: native services must be disabled.
  const bios = Buffer.alloc(32768);
  bios.set([0x3e,0x0a,0xd3,0x99,0x3e,0x87,0xd3,0x99,0xaf,0xd3,0xf1,0x76]);
  await page.locator('#advanced-panel > summary').click();
  await page.locator('#bios').setInputFiles({ name: 'authored-system.rom', mimeType: 'application/octet-stream', buffer: bios });
  await expect(page.locator('#status')).toContainText('authored-system.rom');
  await expect.poll(async () => (await colors(page))['212,193,84'] || 0).toBe(256*192);
  expect(requests.filter(r => r.method !== 'GET')).toEqual([]);
  expect(requests.filter(r => !r.url.startsWith('http://127.0.0.1:41873/'))).toEqual([]);
  await page.getByRole('button', { name: 'Built-in BASIC', exact: true }).click();
  await enter(page, 'PRINT "BACK"');
  await expect(page.locator('#transcript')).toContainText('BACK\nREADY');
  expect(errors).toEqual([]);
});
test('deployment assets are complete and Wasm has no imports', async ({ page }) => {
  await boot(page);
  const bytes = await readFile('dist/msx_wasm.wasm');
  const module = await WebAssembly.compile(bytes);
  expect(WebAssembly.Module.imports(module)).toEqual([]);
  expect(WebAssembly.Module.exports(module).some(e => e.name === 'msx_frame')).toBe(true);
  for (const name of ['app.js', 'style.css', 'test-cartridge.rom', 'weave-boot.rom']) expect((await page.request.get(`/${name}`)).ok()).toBe(true);
});

// Opt-in real BIOS integration. No third-party ROM is bundled or fetched by tests.
if (process.env.MSX_TEST_BIOS) {
  test('locally supplied C-BIOS boots the authored cartridge in the browser', async ({ page }) => {
    const errors = await boot(page);
    await page.locator('#bios').setInputFiles(process.env.MSX_TEST_BIOS);
    await page.locator('#cartridge').setInputFiles('dist/test-cartridge.rom');
    await expect.poll(async () => (await colors(page))['212,193,84'] || 0, { timeout: 20000 }).toBeGreaterThan(20);
    await page.screenshot({ path: 'test-results/cbios-cartridge.png', fullPage: true });
    expect(errors).toEqual([]);
  });
}


test('multiline paste is delivered across frames without truncating the program', async ({ page }) => {
  await boot(page);
  // Ctrl+V must reach the browser's paste action, rather than being canceled
  // by the MSX key matrix handler. The paste event below avoids OS clipboard use.
  expect(await page.locator('#screen').evaluate(canvas => {
    const event = new KeyboardEvent('keydown', { key: 'v', code: 'KeyV', ctrlKey: true, cancelable: true, bubbles: true });
    canvas.dispatchEvent(event);
    return event.defaultPrevented;
  })).toBe(false);
  const source = Array.from({ length: 30 }, (_, i) => `${(i+1)*10} PRINT ${i+1}`).join('\n') + '\nRUN\n';
  expect(source.length).toBeGreaterThan(256);
  await page.locator('#screen').evaluate((canvas, text) => {
    const clipboardData = new DataTransfer();
    clipboardData.setData('text/plain', text);
    canvas.dispatchEvent(new ClipboardEvent('paste', { clipboardData, bubbles: true, cancelable: true }));
  }, source);
  await expect(page.locator('#transcript')).toContainText('>300 PRINT 30\nREADY\n>RUN\n1\n2');
  await expect(page.locator('#transcript')).toContainText('29\n30\nREADY');
});

test('boots with all assets beneath a GitHub Pages project prefix', async ({ page }) => {
  // A real static server exposes the unchanged artifact at the project prefix.
  const requests = [];
  const failures = [];
  page.on('request', request => requests.push(new URL(request.url()).pathname));
  page.on('requestfailed', request => failures.push(request.url()));
  page.on('response', response => { if (response.status() >= 400) failures.push(response.url()); });
  const errors = await boot(page, 'http://127.0.0.1:41874/msx-rust/');
  await enter(page, 'PRINT 42');
  await expect(page.locator('#transcript')).toContainText('42\nREADY');
  expect((await colors(page))['255,255,255']).toBeGreaterThan(300);
  for (const file of ['app.js', 'style.css', 'msx_wasm.wasm']) expect(requests).toContain(`/msx-rust/${file}`);
  expect(requests.every(path => path.startsWith('/msx-rust/'))).toBe(true);
  expect(failures).toEqual([]);
  expect(errors).toEqual([]);
});

async function expectScreenFits(page) {
  const bounds = await page.locator('#screen').boundingBox();
  const viewport = page.viewportSize();
  expect(bounds.width).toBeGreaterThan(180);
  expect(bounds.x).toBeGreaterThanOrEqual(0);
  expect(bounds.x + bounds.width).toBeLessThanOrEqual(viewport.width);
  expect(Math.abs(bounds.width / bounds.height - 4 / 3)).toBeLessThan(0.01);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
}

for (const [width, height] of [[1280, 800], [768, 1024], [375, 812], [320, 640]]) {
  test(`screen, toolbar and disclosures fit at ${width}×${height}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width, height });
    await boot(page);
    await expectScreenFits(page);
    const bounds = await page.locator('#screen').boundingBox();
    expect(bounds.y + bounds.height).toBeLessThan(height);
    await expect(page.locator('#cartridge')).toBeHidden();
    if (width >= 768) expect((await page.locator('.toolbar').boundingBox()).height).toBeLessThanOrEqual(70);
    const buttons = await page.locator('.toolbar button').all();
    const boxes = [];
    for (const button of buttons) {
      await expect(button).toBeVisible();
      const box = await button.boundingBox();
      expect(box.height).toBeGreaterThanOrEqual(44);
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(width);
      for (const other of boxes) {
        expect(box.x >= other.x + other.width - 1 || other.x >= box.x + box.width - 1 || box.y >= other.y + other.height - 1 || other.y >= box.y + box.height - 1).toBe(true);
      }
      boxes.push(box);
    }
    for (const id of ['help-panel', 'advanced-panel', 'transcript-panel']) {
      await expect(page.locator(`#${id}`)).not.toHaveAttribute('open');
    }
    await page.screenshot({ path: `test-results/ui-${width}.png`, fullPage: true });
    await testInfo.attach(`emulator-${width}`, { path: `test-results/ui-${width}.png`, contentType: 'image/png' });
    await page.locator('#help-panel > summary').click();
    await expect(page.getByText('Touch typing and an on-screen keyboard are not supported.', { exact: false })).toBeVisible();
    await page.locator('#advanced-panel > summary').click();
    await expect(page.getByLabel('Local system ROM (32 KiB)')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Built-in BASIC', exact: true })).toBeVisible();
    await page.locator('#transcript-panel > summary').click();
    await expect(page.locator('#transcript')).toBeVisible();
    await expectScreenFits(page);
    await page.screenshot({ path: `test-results/details-${width}.png`, fullPage: true });
  });
}

test('focus is visible, Tab leaves the display, and samples resume paused BASIC', async ({ page }) => {
  await boot(page);
  await expect(page.locator('#focus-screen')).toHaveText('Keyboard active');
  await page.keyboard.press('Tab');
  await expect(page.locator('#focus-screen')).toBeFocused();
  await expect(page.locator('#focus-screen')).toHaveText('Click / tap to type');
  await page.keyboard.type('PRINT 99');
  await expect(page.locator('#transcript')).not.toContainText('PRINT 99');
  await page.locator('#focus-screen').click();
  await expect(page.locator('#screen')).toBeFocused();
  await page.getByRole('button', { name: 'Pause', exact: true }).click();
  await expect(page.locator('#screen')).toBeFocused();
  await page.getByRole('button', { name: 'BASIC example', exact: true }).click();
  await expect(page.locator('#status')).toHaveText('Running · Weave BASIC');
  await expect(page.locator('#screen')).toBeFocused();
  await expect(page.locator('#transcript')).toContainText('1\n2\n3\n4\n5\nREADY');
  await page.getByRole('button', { name: 'Reset', exact: true }).click();
  await expect(page.locator('#screen')).toBeFocused();
  await expect(page.locator('#transcript')).not.toContainText('>RUN');
});

for (const mode of ['native', 'unavailable', 'rejected']) {
  test(`fullscreen keeps controls and a 4:3 screen with ${mode} API`, async ({ page }) => {
    if (mode !== 'native') await page.addInitScript(reject => {
      Element.prototype.requestFullscreen = reject ? () => Promise.reject(new Error('Denied')) : undefined;
    }, mode === 'rejected');
    await page.setViewportSize({ width: 1280, height: 800 });
    await boot(page);
    await page.getByRole('button', { name: 'Fullscreen', exact: true }).click();
    await expect(page.locator('#fullscreen')).toHaveText('Exit fullscreen');
    if (mode === 'native') expect(await page.evaluate(() => document.fullscreenElement?.id)).toBe('emulator');
    else await expect(page.locator('#emulator')).toHaveClass(/focus-view/);
    await expectScreenFits(page);
    for (const viewport of [{ width: 1280, height: 800 }, { width: 375, height: 812 }, { width: 812, height: 375 }]) {
      // Chromium cannot resize a window while it is in native fullscreen.
      if (mode === 'native') await page.locator('#fullscreen').click();
      await page.setViewportSize(viewport);
      if (mode === 'native') {
        await page.locator('#fullscreen').click();
        await expect(page.locator('#fullscreen')).toHaveText('Exit fullscreen');
      }
      await expectScreenFits(page);
      const screen = await page.locator('#screen').boundingBox();
      const exit = await page.locator('#fullscreen').boundingBox();
      expect(screen.y + screen.height).toBeLessThanOrEqual(viewport.height);
      expect(exit.y).toBeGreaterThanOrEqual(0);
      expect(exit.y + exit.height).toBeLessThanOrEqual(viewport.height);
    }
    await page.screenshot({ path: `test-results/fullscreen-${mode}.png` });
    await page.getByRole('button', { name: 'Exit fullscreen', exact: true }).click();
    await expect(page.locator('#fullscreen')).toHaveText('Fullscreen');
    await expect(page.locator('#screen')).toBeFocused();
    if (mode !== 'native') {
      await page.locator('#fullscreen').click();
      await page.keyboard.press('Escape');
      await expect(page.locator('#emulator')).not.toHaveClass(/focus-view/);
      await expect(page.locator('#example')).toBeEnabled();
      expect(await page.locator('#help-panel').evaluate(el => el.inert)).toBe(false);
    }
  });
}

test('loading disables commands and shows progress before Wasm is ready', async ({ page }) => {
  let release;
  const gate = new Promise(resolve => { release = resolve; });
  await page.route('**/msx_wasm.wasm', async route => { await gate; await route.continue(); });
  await page.goto('/');
  await expect(page.locator('#status')).toHaveText('Loading WebAssembly…');
  for (const id of ['pause', 'open-rom', 'audio', 'example', 'bios']) await expect(page.locator(`#${id}`)).toBeDisabled();
  await expect(page.locator('#fullscreen')).toBeEnabled();
  release();
  await expect(page.locator('#status')).toHaveText('Running · Weave BASIC');
  await expect(page.locator('#emulator')).toHaveAttribute('aria-busy', 'false');
});

for (const failure of ['http', 'invalid-wasm']) {
  test(`initialization failure (${failure}) has visible feedback and disabled commands`, async ({ page }) => {
    await page.route('**/msx_wasm.wasm', route => route.fulfill({ status: failure === 'http' ? 503 : 200, body: 'unavailable' }));
    await page.goto('/');
    await expect(page.locator('#status')).toContainText('Unable to start:');
    await expect(page.locator('#status')).toContainText('Reload to retry');
    await expect(page.locator('#status')).toHaveAttribute('data-kind', 'error');
    for (const id of ['pause', 'open-rom', 'audio', 'example', 'bios']) await expect(page.locator(`#${id}`)).toBeDisabled();
    await expect(page.locator('#emulator')).toHaveAttribute('aria-busy', 'false');
  });
}

test('invalid and oversized ROMs keep the selected machine and explain how to recover', async ({ page }) => {
  await boot(page);
  await page.locator('#cartridge').setInputFiles({ name: 'large.rom', mimeType: 'application/octet-stream', buffer: Buffer.alloc(49153) });
  await expect(page.locator('#status')).toContainText('ROM too large');
  await expect(page.locator('#eject')).toBeDisabled();
  await expect(page.locator('#example')).toBeEnabled();
  await page.locator('#advanced-panel > summary').click();
  await page.locator('#bios').setInputFiles('dist/test-cartridge.rom');
  await expect(page.locator('#status')).toContainText('32 KiB');
  await expect(page.locator('#status')).toContainText('Selected ROMs are unchanged');
  await expect(page.locator('#example')).toBeEnabled();
  await page.locator('#builtin').click();
  await expect(page.locator('#status')).toHaveText('Running · Weave BASIC');
});

test('audio unlock failures are visible and retryable', async ({ page }) => {
  await page.addInitScript(() => { window.AudioContext = class { constructor() { throw new Error('Audio is unavailable'); } }; });
  await boot(page);
  await page.locator('#tone').click();
  await expect(page.locator('#status')).toContainText('Audio: Audio is unavailable');
  await expect(page.locator('#audio')).toBeEnabled();
  await expect(page.locator('#tone')).toHaveText('Sound demo');
});
