import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

async function boot(page, path = "/", model = '1') {
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(path);
  await expect(page.locator('#status')).toHaveText('Running · MSX2 · Weave BASIC');
  await expect(page.locator('#model')).toHaveValue('1');
  if (model === '0') await page.locator('#model').selectOption('0');
  await expect(page.locator('#transcript')).toContainText('READY');
  await page.locator('#screen').focus();
  return errors;
}
async function enter(page, line, prefix = '>') {
  await page.locator('#screen').focus();
  await page.keyboard.type(line);
  await page.keyboard.press('Enter');
  await expect(page.locator('#transcript')).toContainText(`${prefix}${line}\n`);
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
for (const path of ['/', 'http://127.0.0.1:41874/msx-rust/']) {
  test(`fresh loads start MSX2 BASIC before the first frame at ${path}`, async ({ page, context }) => {
    const errors = [];
    const observeErrors = target => {
      target.on('pageerror', error => errors.push(error.message));
      target.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    };
    observeErrors(page);
    await context.addInitScript(() => {
      Storage.prototype.setItem = () => { throw new Error('Unexpected preference persistence'); };
      indexedDB.open = () => { throw new Error('Unexpected IndexedDB'); };
      const instantiate = WebAssembly.instantiate;
      WebAssembly.instantiate = async (...args) => {
        const result = await instantiate(...args), exports = result.instance.exports;
        let model;
        return { ...result, instance: { exports: { ...exports,
          msx_model(value) { model = value; exports.msx_model(value); },
          msx_frame() {
            exports.msx_frame();
            if (!window.firstFrame) window.firstFrame = {
              model, selected: document.getElementById('model').value,
              firmware: exports.msx_firmware(), policy: exports.msx_boot_policy(),
              text: new TextDecoder().decode(new Uint8Array(exports.memory.buffer, exports.msx_transcript(), exports.msx_transcript_len())),
            };
          },
        } } };
      };
    });
    const startup = async target => {
      await expect(target.locator('#status')).toHaveText('Running · MSX2 · Weave BASIC');
      await expect(target.locator('#model')).toHaveValue('1');
      await expect(target.locator('#firmware')).toHaveValue('auto');
      await expect(target.locator('#eject')).toBeDisabled();
      await expect(target.locator('#example')).toBeEnabled();
      await expect(target.locator('#transcript')).toContainText('READY');
      const first = await target.evaluate(() => window.firstFrame);
      expect(first).toMatchObject({ model: 1, selected: '1', firmware: 0, policy: 0 });
      expect(first.text).toContain('WEAVE BASIC');
      expect(first.text).toContain('READY');
      await target.locator('#advanced-panel').evaluate(el => { el.open = true; });
      await expect(target.locator('#msx2-firmware')).toBeVisible();
      await expect(target.locator('#firmware option[value=diagnostic]')).toBeEnabled();
    };
    await page.goto(path);
    await startup(page);
    await page.locator('#example').click();
    await expect.poll(() => visibleDigits(page)).toEqual([1, 2, 3, 4, 5]);
    // SCREEN 8 and the last scanline exercise real V9938 bitmap VRAM and Canvas.
    await enter(page, 'SCREEN 8:LINE (4,4)-(120,211),28,BF');
    await expect(page.locator('#screen')).toHaveAttribute('width', '256');
    await expect(page.locator('#screen')).toHaveAttribute('height', '212');
    await expect.poll(() => page.locator('#screen').evaluate(canvas =>
      Array.from(canvas.getContext('2d').getImageData(120,211,1,1).data).slice(0,3)
    )).toEqual([255,0,0]);
    await page.locator('#model').selectOption('0');
    await enter(page, 'PRINT 42');
    await expect(page.locator('#transcript')).toContainText('42\nREADY');
    await page.locator('#reset').click();
    await expect(page.locator('#model')).toHaveValue('0');
    await expect(page.locator('#status')).toHaveText('Running · Weave BASIC');
    await expect(page.locator('#transcript')).not.toContainText('PRINT 42');
    await page.locator('#builtin').click();
    await expect(page.locator('#model')).toHaveValue('0');
    await page.locator('#model').selectOption('1');
    await startup(page);
    // A new tab and a reload ignore another tab's deliberate MSX1 selection.
    await page.locator('#model').selectOption('0');
    const fresh = await context.newPage();
    observeErrors(fresh);
    await fresh.goto(new URL(path, 'http://127.0.0.1:41873').href);
    await startup(fresh);
    await fresh.close();
    await page.reload();
    await startup(page);
    expect(errors).toEqual([]);
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
  await boot(page, '/', '0');
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
  const errors = await boot(page, '/', '0');
  const requests = [];
  page.on('request', request => requests.push({ method: request.method(), url: request.url() }));
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Open ROM', exact: true }).click();
  await (await chooser).setFiles('dist/test-cartridge.rom');
  await expect(page.locator('#status')).toContainText('test-cartridge.rom');
  await expect(page.locator('#example')).toBeDisabled();
  await expect(page.locator('#starter-help')).toContainText('Use Weave BASIC');
  await expect(page.locator('#eject')).toBeEnabled();
  await expect.poll(async () => (await colors(page))['212,193,84'] || 0).toBeGreaterThan(20);
  await page.screenshot({ path: 'test-results/cartridge.png', fullPage: true });
  // Select an invalid ROM and ensure the running cartridge survives.
  await page.locator('#cartridge').setInputFiles({ name: 'invalid.rom', mimeType: 'application/octet-stream', buffer: Buffer.alloc(17) });
  await expect(page.locator('#status')).toContainText('Select its documented mapper');
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
  await page.getByRole('button', { name: 'Use Weave BASIC', exact: true }).click();
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

// Opt-in external BIOS integration. Curated licensed games are tested separately;
// this optional path never downloads user firmware.
if (process.env.MSX_TEST_BIOS) {
  test('locally supplied C-BIOS boots the authored cartridge in the browser', async ({ page }) => {
    const errors = await boot(page, '/', '0');
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
  const errors = await boot(page, 'http://127.0.0.1:41874/msx-rust/', '0');
  await enter(page, 'PRINT 42');
  await expect(page.locator('#transcript')).toContainText('42\nREADY');
  expect((await colors(page))['255,255,255']).toBeGreaterThan(300);
  for (const file of ['app.js', 'style.css', 'msx_wasm.wasm']) expect(requests).toContain(`/msx-rust/${file}`);
  await page.locator('#advanced-panel > summary').click();
  await page.locator('#mapper').selectOption('3');
  await page.locator('#cartridge').setInputFiles(megaRom(128, '3'));
  await backdrop(page, '212,193,84');
  expect(requests.every(path => path.startsWith('/msx-rust/'))).toBe(true);
  expect(failures).toEqual([]);
  expect(errors).toEqual([]);
});

// Original MIT-licensed Z80 fixtures. The reset entry switches the final bank into
// 8000h and jumps there. Only that bank contains the display/keyboard routine.
function megaRom(sizeKiB, mapper) {
  const rom = Buffer.alloc(sizeKiB * 1024);
  rom.set([0x41, 0x42, 0x10, 0x40]);
  const bankSize = mapper === '3' ? 16384 : 8192;
  const lastBank = rom.length / bankSize - 1;
  const register = { '2': 0x7000, '3': 0x7000, '4': 0x8000, '5': 0x9000 }[mapper];
  // Real BIOS init maps the AB header's page. Select both cartridge pages and
  // blank the BIOS text screen so this hardware-only fixture shows a backdrop.
  const prefix = [0xf3, 0x3e, 0xd4, 0xd3, 0xa8, 0x3e, 0, 0xd3, 0x99, 0x3e, 0x81, 0xd3, 0x99,
    ...(mapper === '5' ? [0x3e, 0, 0x32, 0, 0x50] : [])];
  rom.set([...prefix, 0x3e, lastBank, 0x32, register & 255, register >> 8, 0xc3, 0x00, 0x80], 0x10);
  // Select keyboard row 8; while Space is held, change the backdrop A to 6.
  rom.set([0x3e,8,0xd3,0xaa, 0xdb,0xa9,0xe6,1,0x3e,0x0a,0x20,2,0x3e,6,
    0xd3,0x99,0x3e,0x87,0xd3,0x99,0xc3,0x04,0x80], lastBank * bankSize);
  return { name: `authored-${sizeKiB}-${mapper}.rom`, mimeType: 'application/octet-stream', buffer: rom };
}
function ambiguousRom(conflicting = false) {
  const rom = Buffer.alloc(131072);
  rom.set([0x41, 0x42, 0x10, 0x40]);
  const addresses = conflicting ? [0x6800, 0x7800, 0x5000, 0x9000] : [0x6000, 0x7000];
  rom.set([...addresses.flatMap(a => [0x3e, 0, 0x32, a & 255, a >> 8]), 0x76], 0x10);
  return { name: conflicting ? 'conflicting.rom' : 'ambiguous.rom', mimeType: 'application/octet-stream', buffer: rom };
}
async function backdrop(page, rgb) {
  await expect.poll(async () => (await colors(page))[rgb] || 0).toBe(256 * 192);
}
for (const [size, mapper] of [[64, '2'], [128, '3'], [256, '4'], [512, '5'], [1024, '2']]) {
  test(`real file input executes ${size} KiB bank-switched mapper ${mapper} and survives failed imports`, async ({ page }) => {
    const errors = await boot(page, '/', '0');
    const requests = [];
    page.on('request', r => requests.push({ method: r.method(), url: r.url() }));
    const fixture = megaRom(size, mapper);
    await page.locator('#advanced-panel > summary').click();
    // The default Auto path reaches and executes code in the last mapper bank.
    await expect(page.locator('#mapper')).toHaveValue('0');
    await page.locator('#cartridge').setInputFiles(fixture);
    await expect(page.locator('#status')).toContainText(fixture.name);
    if (mapper === '3') await page.screenshot({ path: 'test-results/auto-ascii16.png', fullPage: true });
    const name = { '2': 'ASCII8', '3': 'ASCII16', '4': 'Konami', '5': 'Konami SCC' }[mapper];
    await expect(page.locator('#status')).toContainText(`Auto: ${name}, estimate`);
    await backdrop(page, '212,193,84');
    await page.locator('#screen').focus();
    await page.keyboard.down('Space');
    await backdrop(page, '212,82,77');
    await page.keyboard.up('Space');
    await backdrop(page, '212,193,84');
    await page.getByRole('button', { name: 'Reset', exact: true }).click();
    await backdrop(page, '212,193,84');
    for (const bad of [
      { name: 'bad-header.rom', buffer: Buffer.alloc(65536) },
      ambiguousRom(),
      ambiguousRom(true),
      { name: 'partial.bin', buffer: fixture.buffer.subarray(0, 65535) },
      { name: 'wrong-extension.txt', buffer: fixture.buffer },
    ]) {
      await page.locator('#cartridge').setInputFiles({ mimeType: 'application/octet-stream', ...bad });
      await expect(page.locator('#status')).toContainText('ROM import failed');
      await page.locator('#screen').focus();
      await page.keyboard.down('Space');
      await backdrop(page, '212,82,77');
      await page.keyboard.up('Space');
      await backdrop(page, '212,193,84');
    }
    // Manual controls remain available after failures and bypass ambiguous heuristic choice.
    await expect(page.locator('#mapper')).toHaveValue('0');
    await page.locator('#mapper').selectOption(mapper);
    await page.locator('#cartridge').setInputFiles(fixture);
    await expect(page.locator('#status')).toContainText(`(${name})`);
    await backdrop(page, '212,193,84');
    await page.locator('#bios').setInputFiles({ name: 'large-system.rom', mimeType: 'application/octet-stream', buffer: fixture.buffer });
    await expect(page.locator('#status')).toContainText('32 KiB');
    await backdrop(page, '212,193,84');
    // Replace with a different bank-switched mapping, then a plain image.
    const replacementMapper = mapper === '3' ? '2' : '3';
    await page.locator('#mapper').selectOption(replacementMapper);
    await page.locator('#cartridge').setInputFiles(megaRom(128, replacementMapper));
    await expect(page.locator('#status')).toContainText(`authored-128-${replacementMapper}.rom`);
    await backdrop(page, '212,193,84');
    await page.locator('#mapper').selectOption('0');
    await page.locator('#cartridge').setInputFiles('dist/test-cartridge.rom');
    await expect(page.locator('#status')).toContainText('test-cartridge.rom');
    await expect.poll(async () => (await colors(page))['212,193,84'] || 0).toBeLessThan(256 * 192);
    await page.getByRole('button', { name: 'Eject cartridge' }).click();
    await expect(page.locator('#transcript')).toContainText('READY');
    await enter(page, 'PRINT 42');
    await expect(page.locator('#transcript')).toContainText('42\nREADY');
    await page.locator('#mapper').selectOption(mapper);
    await page.locator('#cartridge').setInputFiles(fixture);
    await backdrop(page, '212,193,84');
    await page.getByRole('button', { name: 'Use Weave BASIC', exact: true }).click();
    await expect(page.locator('#transcript')).toContainText('READY');
    expect(requests.every(r => r.method === 'GET' && r.url.startsWith('http://127.0.0.1:41873/'))).toBe(true);
    expect(errors).toEqual([]);
  });
}

for (const path of ['/', 'http://127.0.0.1:41874/msx-rust/']) {
  test(`Auto inference keeps MSX2 selected and executes MegaROMs at ${path}`, async ({ page }) => {
    const errors = await boot(page, path);
    await page.locator('#model').selectOption('1');
    for (const mapper of ['2', '3', '4', '5']) {
      await page.locator('#cartridge').setInputFiles(megaRom(128, mapper));
      await expect(page.locator('#status')).toContainText('estimate');
      await expect(page.locator('#model')).toHaveValue('1');
      // V9938 uses the default MSX2 palette and backdrop size, independent of mapper choice.
      await expect.poll(() => page.locator('#screen').evaluate(canvas => {
        const p = canvas.getContext('2d').getImageData(0, 0, 1, 1).data;
        return Array.from(p).slice(0, 3);
      })).toEqual([218, 218, 36]);
    }
    await page.locator('#cartridge').setInputFiles(ambiguousRom());
    await expect(page.locator('#status')).toContainText('Ambiguous mapper evidence');
    await expect(page.locator('#status')).toContainText('unchanged');
    await expect(page.locator('#model')).toHaveValue('1');
    expect(errors).toEqual([]);
  });
}

test('ambiguous Auto import can be retried with the same file and an explicit mapper', async ({ page }) => {
  await boot(page);
  await page.locator('#advanced-panel > summary').click();
  const file = ambiguousRom();
  await page.locator('#cartridge').setInputFiles(file);
  await expect(page.locator('#status')).toContainText('Ambiguous mapper evidence');
  await expect(page.locator('#status')).toContainText('same file again');
  await page.locator('#mapper').selectOption('3');
  await page.locator('#cartridge').setInputFiles(file);
  await expect(page.locator('#status')).toContainText(`${file.name} (ASCII16)`);
});

test('Wasm transfer grows memory before creating a view and rejects invalid ABI calls safely', async ({ page }) => {
  await boot(page);
  const result = await page.evaluate(async () => {
    const { instance: { exports: w } } = await WebAssembly.instantiate(await (await fetch('msx_wasm.wasm')).arrayBuffer(), {});
    w.msx_frame();
    const error = () => new TextDecoder().decode(new Uint8Array(w.memory.buffer, w.msx_error(), w.msx_error_len()));
    const initialBuffer = w.memory.buffer;
    const initialSize = initialBuffer.byteLength;
    const size = initialSize + 65536;
    const pointer = w.msx_upload(size);
    const grown = w.memory.buffer !== initialBuffer && w.memory.buffer.byteLength > initialSize;
    // Construct the view after reservation; write the last byte as well as the header.
    const view = new Uint8Array(w.memory.buffer, pointer >>> 0, size);
    view[view.length - 1] = 0x5a;
    const invalidLength = w.msx_load(size - 1, 0, 2);
    const lengthError = error();
    const oversized = w.msx_upload(0xffffffff);
    const resourceError = error();
    const empty = w.msx_upload(0);
    const p = w.msx_upload(8192);
    new Uint8Array(w.memory.buffer, p, 4).set([0x41, 0x42, 0x10, 0x40]);
    const invalidMapper = w.msx_load(8192, 0, 99);
    const mapperError = error();
    const p2 = w.msx_upload(8192);
    new Uint8Array(w.memory.buffer, p2, 4).set([0x41, 0x42, 0x10, 0x40]);
    const valid = w.msx_load(8192, 0, 1);
    const manualMapper = w.msx_detected_mapper();
    const plain = w.msx_upload(8192);
    new Uint8Array(w.memory.buffer, plain, 17).set([0x41, 0x42, 0x10, 0x40, ...Array(12).fill(0), 0x76]);
    const autoPlain = w.msx_load(8192, 0, 0);
    const plainMetadata = [w.msx_detected_mapper(), w.msx_detection_confidence()];
    const ambiguous = w.msx_upload(131072);
    const bytes = new Uint8Array(w.memory.buffer, ambiguous, 131072);
    bytes.set([0x41, 0x42, 0x10, 0x40]);
    bytes.set([0x3e, 0, 0x32, 0, 0x60, 0x3e, 0, 0x32, 0, 0x70, 0x76], 0x10);
    const autoAmbiguous = w.msx_load(131072, 0, 0);
    const ambiguousMetadata = [w.msx_detected_mapper(), w.msx_detection_confidence()];
    const explanation = new TextDecoder().decode(new Uint8Array(w.memory.buffer, w.msx_detection(), w.msx_detection_len()));
    return { manualMapper, autoPlain, plainMetadata, autoAmbiguous, ambiguousMetadata, explanation, pointer: !!pointer, grown, invalidLength, lengthError, oversized, resourceError, empty, invalidMapper, mapperError, valid };
  });
  expect(result).toMatchObject({ pointer: true, grown: true, invalidLength: 0, oversized: 0, empty: 0, invalidMapper: 0, valid: 1 });
  expect(result.lengthError).toContain('transfer length');
  expect(result.resourceError).toContain('memory');
  expect(result.mapperError).toContain('Unsupported cartridge mapper');
  expect(result).toMatchObject({ manualMapper: 1, autoPlain: 1, plainMetadata: [1, 3], autoAmbiguous: 0, ambiguousMetadata: [0, 2] });
  expect(result.explanation).toContain('Ambiguous mapper evidence');
  expect(result.explanation).toContain('ASCII16');
});

// Restrict only the test instance's linear-memory maximum, so allocation failure
// is deterministic without exhausting host memory or changing the shipped module.
function boundedMemoryWasm(bytes, maxPages) {
  let cursor = 8;
  function leb() {
    let value = 0, shift = 0, byte;
    do { byte = bytes[cursor++]; value += (byte & 127) * 2 ** shift; shift += 7; } while (byte & 128);
    return value;
  }
  function encoded(value) {
    const result = [];
    do { const byte = value & 127; value >>>= 7; result.push(byte | (value ? 128 : 0)); } while (value);
    return Buffer.from(result);
  }
  while (cursor < bytes.length) {
    const start = cursor;
    const section = bytes[cursor++];
    const length = leb();
    const end = cursor + length;
    if (section === 5) {
      expect(leb()).toBe(1); // one import-free, 32-bit linear memory
      const flags = leb();
      expect([0, 1]).toContain(flags);
      const minimum = leb();
      expect(minimum).toBeLessThan(maxPages);
      const payload = Buffer.concat([Buffer.from([1, 1]), encoded(minimum), encoded(maxPages)]);
      return Buffer.concat([bytes.subarray(0, start), Buffer.from([5]), encoded(payload.length), payload, bytes.subarray(end)]);
    }
    cursor = end;
  }
  throw new Error('Missing Wasm memory section');
}

test('a reserved 2 MiB cartridge installs and resets without extra memory', async ({ page }) => {
  // 4.6875 MiB permits the upload after MSX2 startup and an explicit MSX1 switch,
  // including the larger V9938 framebuffer/VRAM allocations. It still cannot fit
  // a second cartridge copy; pointer checks also verify framebuffer reuse.
  const bounded = boundedMemoryWasm(await readFile('dist/msx_wasm.wasm'), 75);
  await page.addInitScript(base64 => {
    const instantiate = WebAssembly.instantiate.bind(WebAssembly);
    WebAssembly.instantiate = async (...args) => {
      args[0] = Uint8Array.from(atob(base64), c => c.charCodeAt(0));
      const result = await instantiate(...args);
      const exports = result.instance.exports;
      window.memoryTestWasm = exports;
      return { ...result, instance: { exports: { ...exports, msx_upload(length) {
        const pointer = exports.msx_upload(length);
        window.lastUpload = { length, reserved: pointer !== 0 };
        return pointer;
      } } } };
    };
  }, bounded.toString('base64'));
  const errors = await boot(page, '/', '0');
  const pixels = await page.evaluate(() => window.memoryTestWasm.msx_pixels());
  await page.locator('#advanced-panel > summary').click();
  await page.locator('#mapper').selectOption('2');
  const fixture = megaRom(2048, '2');
  await page.locator('#cartridge').setInputFiles(fixture);
  await expect.poll(() => page.evaluate(() => window.lastUpload)).toEqual({ length: 2 * 1024 * 1024, reserved: true });
  await expect(page.locator('#status')).toHaveText(`Running · MSX1 C-BIOS 0.29a · cartridge BIOS (no BASIC) · ${fixture.name} (ASCII8)`);
  expect(await page.evaluate(() => window.memoryTestWasm.msx_pixels())).toBe(pixels);
  await backdrop(page, '212,193,84');
  await page.locator('#screen').focus();
  await page.keyboard.down('Space');
  await backdrop(page, '212,82,77');
  await page.keyboard.up('Space');
  await page.getByRole('button', { name: 'Reset', exact: true }).click();
  expect(await page.evaluate(() => window.memoryTestWasm.msx_pixels())).toBe(pixels);
  await backdrop(page, '212,193,84');
  await page.getByRole('button', { name: 'Eject cartridge' }).click();
  await enter(page, 'PRINT 42');
  await expect(page.locator('#transcript')).toContainText('42\nREADY');
  expect(errors).toEqual([]);
});

test('resource failures and canceled asynchronous reads preserve the active cartridge', async ({ page }) => {
  // Limit this instance to 4 MiB; a redirected 8 MiB reservation must fail in
  // try_reserve_exact, while the active 64 KiB cartridge keeps running.
  const bounded = boundedMemoryWasm(await readFile('dist/msx_wasm.wasm'), 64);
  await page.addInitScript(base64 => {
    const instantiate = WebAssembly.instantiate.bind(WebAssembly);
    WebAssembly.instantiate = async (...args) => {
      args[0] = Uint8Array.from(atob(base64), c => c.charCodeAt(0));
      const result = await instantiate(...args);
      const exports = result.instance.exports;
      return { ...result, instance: { exports: { ...exports, msx_upload(length) {
        if (window.failReservation) { window.failReservation = false; return exports.msx_upload(8 * 1024 * 1024); }
        return exports.msx_upload(length);
      } } } };
    };
    const arrayBuffer = File.prototype.arrayBuffer;
    File.prototype.arrayBuffer = async function () {
      if (window.failFileRead) { window.failFileRead = false; throw new RangeError('Test browser allocation failure'); }
      if (window.deferFileRead) {
        window.deferFileRead = false;
        await new Promise(resolve => { window.finishFileRead = resolve; });
      }
      return arrayBuffer.call(this);
    };
  }, bounded.toString('base64'));
  const errors = await boot(page, '/', '0');
  await page.locator('#advanced-panel > summary').click();
  await page.locator('#mapper').selectOption('2');
  await enter(page, '10 PRINT 86');
  // A failed transfer for a cartridge or local firmware must retain BASIC too.
  for (const [input, file] of [['cartridge', megaRom(64, '2')], ['bios', 'dist/cbios_main_msx1.rom']]) {
    await page.evaluate(() => { window.failReservation = true; });
    await page.locator(`#${input}`).setInputFiles(file);
    await expect(page.locator('#status')).toContainText('memory');
    await expect(page.locator('#firmware')).toHaveValue('auto');
    await enter(page, 'RUN'); await expect(page.locator('#transcript')).toContainText('86\nREADY');
  }
  await page.locator('#cartridge').setInputFiles(megaRom(64, '2'));
  await backdrop(page, '212,193,84');
  for (const flag of ['failReservation', 'failFileRead']) {
    await page.evaluate(flag => { window[flag] = true; }, flag);
    await page.locator('#cartridge').setInputFiles(megaRom(128, '2'));
    await expect(page.locator('#status')).toContainText('memory');
    await page.locator('#screen').focus();
    await page.keyboard.down('Space');
    await backdrop(page, '212,82,77');
    await page.keyboard.up('Space');
    await backdrop(page, '212,193,84');
  }
  // Eject while File.arrayBuffer is pending: its later completion cannot reload it.
  await page.evaluate(() => { window.deferFileRead = true; });
  await page.locator('#cartridge').setInputFiles(megaRom(128, '2'));
  await expect.poll(() => page.evaluate(() => typeof window.finishFileRead)).toBe('function');
  await page.getByRole('button', { name: 'Eject cartridge' }).click();
  await page.evaluate(() => window.finishFileRead());
  await expect(page.locator('#status')).toHaveText('Running · Weave BASIC');
  await enter(page, 'PRINT 42');
  await expect(page.locator('#transcript')).toContainText('42\nREADY');
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
    const controls = await page.locator('.toolbar button, .toolbar select').all();
    const boxes = [];
    for (const control of controls) {
      await expect(control).toBeVisible();
      const box = await control.boundingBox();
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
    await expect(page.getByRole('button', { name: 'Use Weave BASIC', exact: true })).toBeVisible();
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
  await expect(page.locator('#status')).toHaveText('Running · MSX2 · Weave BASIC');
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
  await expect(page.locator('#model')).toHaveValue('1');
  for (const id of ['pause', 'open-rom', 'audio', 'example', 'bios']) await expect(page.locator(`#${id}`)).toBeDisabled();
  await expect(page.locator('#fullscreen')).toBeEnabled();
  // Simulate a browser restoring a stale form selection while Wasm downloads.
  await page.locator('#model').evaluate(select => { select.value = '0'; });
  release();
  await expect(page.locator('#status')).toHaveText('Running · MSX2 · Weave BASIC');
  await expect(page.locator('#model')).toHaveValue('1');
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

test('unrecognized ROMs keep the selected machine and explain how to recover', async ({ page }) => {
  await boot(page, '/', '0');
  await page.locator('#cartridge').setInputFiles({ name: 'large.rom', mimeType: 'application/octet-stream', buffer: Buffer.alloc(49153) });
  await expect(page.locator('#status')).toContainText('Select its documented mapper');
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

for (const path of ['/', 'http://127.0.0.1:41874/msx-rust/']) {
  test(`classic-style original BASIC programs use keyboard, input and VDP at ${path}`, async ({ page }) => {
    const errors = await boot(page, path, '0');
    const load = async name => {
      await enter(page, 'NEW');
      const source = await readFile(`examples/basic/${name}.bas`, 'utf8');
      for (const line of source.trim().split('\n')) await enter(page, line);
      await enter(page, 'RUN');
    };
    await load('scores');
    await expect(page.locator('#transcript')).toContainText('COMET:3\nORBIT:1\nMOON:2\nREADY');
    // Independently recognize output glyphs in Canvas for array/DATA/GOSUB flow.
    await enter(page, 'NEW');
    for (const line of [
      '10 DIM A(5):FOR I=1 TO 5:READ A(I):NEXT I',
      '20 CLS:FOR I=1 TO 5:GOSUB 100:NEXT I:END',
      '100 PRINT A(I):RETURN',
      '200 DATA 1,2,3,4,5',
    ]) await enter(page, line);
    await enter(page, 'RUN');
    await expect.poll(() => visibleDigits(page)).toEqual([1, 2, 3, 4, 5]);
    await load('greeting');
    await expect(page.locator('#transcript')).toContainText('NAME? ');
    await expect(page.locator('#screen')).toBeFocused();
    await enter(page, 'SKY', 'NAME? ');
    await expect(page.locator('#transcript')).toContainText('COUNT (1-5)? ');
    await enter(page, 'BAD', 'COUNT (1-5)? ');
    await expect(page.locator('#transcript')).toContainText('?REDO FROM START\nCOUNT (1-5)? ');
    await enter(page, '2', 'COUNT (1-5)? ');
    await expect(page.locator('#transcript')).toContainText('HELLO SKY\nHELLO SKY\nREADY');
    await enter(page, 'CLS:PRINT 5');
    await expect.poll(() => visibleDigits(page)).toEqual([5]);
    // BREAK discards partially typed INPUT and resumes command editing.
    await enter(page, 'INPUT "WAIT";N');
    await page.keyboard.type('123');
    await page.keyboard.press('Escape');
    await expect(page.locator('#transcript')).toContainText('WAIT? BREAK\nREADY');
    await enter(page, 'PRINT 4');
    await expect(page.locator('#transcript')).toContainText('>PRINT 4\n4\nREADY');
    await enter(page, 'INPUT N');
    await page.getByRole('button', { name: 'Reset', exact: true }).click();
    await expect(page.locator('#transcript')).not.toContainText('INPUT N');
    await expect(page.locator('#transcript')).toContainText('READY');
    await load('shapes');
    const sampled = () => page.locator('#screen').evaluate(canvas => {
      const data = canvas.getContext('2d').getImageData(0, 0, 256, 192).data;
      return [[16,96],[128,56],[128,96],[32,32],[0,0]].map(([x,y]) => Array.from(data.slice((y*256+x)*4,(y*256+x)*4+3)));
    });
    await expect.poll(sampled).toEqual([[255,255,255],[255,255,255],[212,193,84],[212,82,77],[84,85,237]]);
    await page.screenshot({ path: `test-results/basic-shapes-${path === '/' ? 'root' : 'pages'}.png`, fullPage: true });
    await page.keyboard.press('Enter');
    await expect(page.locator('#transcript')).toContainText('READY');
    await enter(page, '10 GOSUB 10');
    await enter(page, 'RUN');
    await expect(page.locator('#transcript')).toContainText('?TOO MANY GOSUBS IN 10');
    await page.getByRole('button', { name: 'Reset', exact: true }).click();
    await expect(page.locator('#transcript')).not.toContainText('GOSUB');
    await enter(page, '10 SCREEN 2:PAINT (0,0),10,15:GOTO 10');
    await enter(page, 'RUN');
    await expect.poll(async () => (await colors(page))['212,193,84'] || 0).toBeGreaterThan(40000);
    await page.keyboard.press('Escape');
    await expect(page.locator('#transcript')).toContainText('>RUN\nBREAK\nREADY');
    expect(errors).toEqual([]);
  });
}
