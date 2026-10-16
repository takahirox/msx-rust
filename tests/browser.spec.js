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
  await page.getByRole('button', { name: 'Start', exact: true }).click();
  await page.getByRole('button', { name: 'Reset', exact: true }).click();
  await expect(page.locator('#transcript')).not.toContainText('PRINT 43');
  expect(errors).toEqual([]);
});
test('graphics uses the VDP and Escape interrupts an infinite BASIC program', async ({ page }) => {
  await boot(page);
  await page.getByRole('button', { name: 'Try graphics' }).click();
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
test('Web Audio activation schedules audible PSG waveform buffers', async ({ page }) => {
  await page.addInitScript(() => {
    window.audioEvidence = { state: '', buffers: 0, maximum: 0, zeroCrossings: 0, connections: 0 };
    const NativeContext = window.AudioContext;
    window.AudioContext = class extends NativeContext {
      constructor(...args) {
        super(...args);
        const create = this.createBufferSource.bind(this);
        this.createBufferSource = () => {
          const source = create();
          const start = source.start.bind(source);
          const connect = source.connect.bind(source);
          source.connect = (...args) => { window.audioEvidence.connections++; return connect(...args); };
          source.start = (...args) => {
            const d = source.buffer.getChannelData(0);
            window.audioEvidence.buffers++;
            window.audioEvidence.state = this.state;
            for (let i = 1; i < d.length; i++) {
              window.audioEvidence.maximum = Math.max(window.audioEvidence.maximum, Math.abs(d[i]));
              if (d[i-1] <= 0 && d[i] > 0) window.audioEvidence.zeroCrossings++;
            }
            return start(...args);
          };
          return source;
        };
      }
    };
  });
  await boot(page);
  await page.getByRole('button', { name: 'Enable audio', exact: true }).click();
  await expect(page.locator('#audio')).toHaveText('Audio enabled');
  await enter(page, 'BEEP');
  await expect.poll(() => page.evaluate(() => window.audioEvidence.maximum)).toBeGreaterThan(0.05);
  await expect.poll(() => page.evaluate(() => window.audioEvidence.zeroCrossings)).toBeGreaterThan(10);
  const evidence = await page.evaluate(() => window.audioEvidence);
  expect(evidence.state).toBe('running');
  expect(evidence.connections).toBeGreaterThan(0);
  await enter(page, 'SOUND 8,0');
});
test('file picker boots original cartridge, handles bad ROMs, ejects and loads local system ROM', async ({ page }) => {
  const errors = await boot(page);
  const requests = [];
  page.on('request', request => requests.push({ method: request.method(), url: request.url() }));
  await page.locator('#cartridge').setInputFiles('dist/test-cartridge.rom');
  await expect(page.locator('#status')).toContainText('test-cartridge.rom');
  await expect.poll(async () => (await colors(page))['212,193,84'] || 0).toBeGreaterThan(20);
  await page.screenshot({ path: 'test-results/cartridge.png', fullPage: true });
  // Select an invalid ROM and ensure the running cartridge survives.
  await page.locator('#cartridge').setInputFiles({ name: 'invalid.rom', mimeType: 'application/octet-stream', buffer: Buffer.alloc(17) });
  await expect(page.locator('#status')).toContainText('plain 8, 16, 32 or 48');
  await page.getByRole('button', { name: 'Eject cartridge' }).click();
  await expect(page.locator('#transcript')).toContainText('READY');
  // Authored fixture fills the screen with VDP backdrop color A, then HALTs.
  // Its OUT F1 is deliberately ignored: native services must be disabled.
  const bios = Buffer.alloc(32768);
  bios.set([0x3e,0x0a,0xd3,0x99,0x3e,0x87,0xd3,0x99,0xaf,0xd3,0xf1,0x76]);
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
