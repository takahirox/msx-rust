import { test, expect } from '@playwright/test';

// Observe the real compiled Wasm and scheduled native Web Audio buffers. No
// substitute synthesizer, commercial ROM, recorded music or debug ABI is used.
test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    const instantiate = WebAssembly.instantiate;
    WebAssembly.instantiate = async (...args) => {
      const result = await instantiate(...args);
      window.sccWasm = result.instance.exports;
      return result;
    };
    window.sccBuffers = [];
    window.sccScheduled = 0;
    const NativeContext = window.AudioContext;
    window.AudioContext = class extends NativeContext {
      constructor(...args) {
        super(...args);
        window.sccClock = this;
        const create = this.createBufferSource.bind(this);
        this.createBufferSource = () => {
          const source = create();
          const start = source.start.bind(source);
          source.start = (...args) => {
            window.sccScheduled++;
            const data = source.buffer.getChannelData(0);
            const record = {
              time: args[0], end: args[0] + source.buffer.duration, ended: false,
              length: data.length, finite: true, peak: 0, energy: 0, crossings: 0,
              first: data[0], last: data[data.length - 1],
              square: true,
            };
            for (let i = 0; i < data.length; i++) {
              record.finite &&= Number.isFinite(data[i]);
              record.peak = Math.max(record.peak, Math.abs(data[i]));
              record.energy += data[i] * data[i];
              if (i && data[i - 1] < 0 && data[i] > 0) record.crossings++;
              // Independent DAC calculation: floor(+/-120 * 15 / 16) / 2000.
              record.square &&= Math.abs(data[i] - 0.056) < 0.000001 || Math.abs(data[i] + 0.0565) < 0.000001;
            }
            source.addEventListener('ended', () => { record.ended = true; });
            window.sccBuffers.push(record);
            if (window.sccBuffers.length > 240) window.sccBuffers.shift();
            return start(...args);
          };
          return source;
        };
      }
    };
  });
});

async function measure(page) {
  // Exclude queued output and allow the PSG DC blocker to settle. Require fresh,
  // completed buffers so a stopped scheduler cannot satisfy silence assertions.
  const after = await page.evaluate(() => window.sccClock.currentTime + 0.35);
  const records = () => page.evaluate(after => window.sccBuffers.filter(r => r.time >= after && r.ended), after);
  await expect.poll(async () => (await records()).length).toBeGreaterThanOrEqual(12);
  const buffers = await records();
  expect(buffers.every(r => r.finite && r.peak <= 0.9)).toBe(true);
  expect(buffers.every(r => r.length >= 734 && r.length <= 737)).toBe(true);
  return buffers;
}
async function silence(page) {
  expect((await measure(page)).every(r => r.peak < 0.0001)).toBe(true);
}
async function square(page) {
  // C-BIOS boots again after Reset; wait for the fixture to resume execution.
  await expect.poll(() => page.evaluate(() => window.sccBuffers.at(-1)?.square), { timeout: 15000 }).toBe(true);
  const buffers = await measure(page);
  expect(buffers.every(r => r.square)).toBe(true);
  let crossings = buffers.reduce((n, r) => n + r.crossings, 0);
  for (let i = 1; i < buffers.length; i++) if (buffers[i - 1].last < 0 && buffers[i].first > 0) crossings++;
  const seconds = buffers.reduce((n, r) => n + r.length, 0) / 44100;
  expect(Math.abs(crossings / seconds - 3_579_545 / (32 * 254))).toBeLessThan(8);
}

for (const site of ['/', 'http://127.0.0.1:41874/msx-rust/']) {
  for (const model of ['0', '1']) {
    for (const mapper of ['5', '0']) {
      test(`classic SCC native audio, lifecycle and PSG: model ${model}, mapper ${mapper} at ${site}`, async ({ page }) => {
        test.setTimeout(60000);
        await page.goto(site);
        await expect(page.locator('#transcript')).toContainText('READY');
        await page.locator('#model').selectOption(model);
        await page.locator('#advanced-panel').evaluate(el => { el.open = true; });
        await page.locator('#mapper').selectOption(mapper);
        await page.locator('#cartridge').setInputFiles('dist/scc-test-cartridge.rom');
        await expect(page.locator('#status')).toContainText(mapper === '0' ? 'Auto: Konami SCC' : '(Konami SCC)');
        await expect.poll(() => page.evaluate(() => window.sccWasm.msx_firmware())).toBe(1);
        await page.locator('#audio').click();
        await expect(page.locator('#audio')).toHaveText('Mute audio');
        // Actual Z80 boot completes when Space produces the independently known DAC levels.
        await page.locator('#screen').focus();
        await page.keyboard.down('Space');
        await expect.poll(() => page.evaluate(() => window.sccBuffers.some(r => r.square)), { timeout: 15000 }).toBe(true);
        await square(page);
        // Disabling the mapper's overlay must gate SCC even with enabled voices.
        await page.keyboard.down('ArrowRight');
        await silence(page);
        await page.keyboard.up('ArrowRight');
        await square(page);
        await page.keyboard.up('Space');
        await silence(page);
        await page.keyboard.down('ArrowLeft');
        const psg = await measure(page);
        expect(psg.some(r => r.peak > 0.09)).toBe(true);
        await page.keyboard.down('Space');
        const both = await measure(page);
        expect(both.some(r => r.peak > 0.14)).toBe(true);
        const meanSquare = buffers => buffers.reduce((n, r) => n + r.energy, 0) / buffers.reduce((n, r) => n + r.length, 0);
        expect(meanSquare(both)).toBeGreaterThan(meanSquare(psg) + 0.001);
        await page.keyboard.up('ArrowLeft');
        await page.keyboard.up('Space');
        await page.keyboard.down('ArrowUp');
        expect((await measure(page)).some(r => r.peak > 0.07)).toBe(true);
        await page.keyboard.up('ArrowUp');
        await page.keyboard.down('Space');
        // Failed imports preserve the actively sounding cartridge and waveform.
        await page.locator('#cartridge').setInputFiles({ name: 'invalid.rom', mimeType: 'application/octet-stream', buffer: Buffer.alloc(17) });
        await expect(page.locator('#status')).toContainText('unchanged');
        await page.locator('#screen').focus();
        await page.keyboard.down('Space');
        await square(page);
        await page.locator('#audio').click();
        await expect.poll(() => page.evaluate(() => window.sccClock.state)).toBe('suspended');
        const count = await page.evaluate(() => window.sccScheduled);
        await page.waitForTimeout(100);
        expect(await page.evaluate(() => window.sccScheduled)).toBe(count);
        await page.keyboard.up('Space');
        await page.locator('#audio').click();
        await silence(page);
        await page.evaluate(() => window.sccClock.suspend());
        await expect(page.locator('#audio')).toHaveText('Resume audio');
        await page.locator('#audio').click();
        await page.locator('#screen').focus();
        await page.keyboard.down('Space');
        await square(page);
        // Pause and focus loss release keys and flush queued sources.
        await page.locator('#pause').click();
        await page.keyboard.up('Space');
        await page.locator('#pause').click();
        await silence(page);
        await page.keyboard.down('Space');
        await square(page);
        await page.locator('#mapper').focus(); // Canvas blur releases the held key.
        await silence(page);
        await page.keyboard.up('Space');
        await page.locator('#screen').focus();
        await page.keyboard.down('Space');
        await square(page);
        await page.locator('#reset').click();
        await page.keyboard.up('Space');
        await silence(page);
        await page.locator('#screen').focus();
        await page.keyboard.down('Space');
        await square(page);
        await page.locator('#eject').click();
        await page.keyboard.up('Space');
        await silence(page);
        await expect(page.locator('#status')).toContainText('Weave BASIC');
      });
    }
  }
}
