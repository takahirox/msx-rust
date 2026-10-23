import { test, expect } from '@playwright/test';

// Observe the real compiled Wasm and scheduled native Web Audio buffers. No
// substitute synthesizer, commercial ROM, recorded music or debug ABI is used.
test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    const instantiate = WebAssembly.instantiate;
    WebAssembly.instantiate = async (...args) => {
      const result = await instantiate(...args);
      const exports = result.instance.exports;
      window.sccWasm = exports;
      window.sccFrame = 0;
      return { ...result, instance: { exports: { ...exports,
        msx_frame() {
          if (!window.sccManual) {
            exports.msx_frame();
            window.sccFrame++;
          }
        },
      } } };
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
              frame: window.sccFrame, rises: [],
              time: args[0], end: args[0] + source.buffer.duration, ended: false,
              length: data.length, finite: true, peak: 0, energy: 0,
              first: data[0], last: data[data.length - 1],
              square: true,
            };
            for (let i = 0; i < data.length; i++) {
              record.finite &&= Number.isFinite(data[i]);
              record.peak = Math.max(record.peak, Math.abs(data[i]));
              record.energy += data[i] * data[i];
              if (i && data[i - 1] < 0 && data[i] > 0) {
                record.rises.push(i);
              }
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
  // The scheduler may omit a frame during catch-up. Check phase only across
  // contiguous emulated frames, including the edge between adjacent buffers.
  const period = 32 * 254 * 44100 / 3_579_545;
  let previousFrame, previous, position = 0, firstRise, count = 0, checked = 0;
  for (const buffer of buffers) {
    if (buffer.frame !== previousFrame + 1) {
      previous = undefined; firstRise = undefined; count = 0; position = 0;
    }
    const rises = [...buffer.rises];
    if (previous < 0 && buffer.first > 0) rises.unshift(0);
    for (const rise of rises) {
      firstRise ??= position + rise;
      expect(Math.abs(position + rise - firstRise - count * period)).toBeLessThan(1);
      count++; checked++;
    }
    position += buffer.length;
    previous = buffer.last;
    previousFrame = buffer.frame;
  }
  expect(checked).toBeGreaterThan(50);
}

async function capturedContinuity(page) {
  const result = await page.evaluate(() => {
    // Deterministic real-Wasm frames: copy every PCM frame so no scheduler gap
    // can hide a discontinuity at an input transition. No reset between states.
    window.sccManual = true;
    const w = window.sccWasm, pcm = [], stages = [];
    for (const [right, down] of [
      [false, false], [true, false], [true, true], [false, true], [false, false],
      [false, true], [true, true], [true, false], [false, false],
    ]) {
      w.msx_key(8, 7, Number(right));
      w.msx_key(8, 6, Number(down));
      const start = pcm.length;
      for (let i = 0; i < 12; i++) {
        w.msx_frame();
        pcm.push(...new Float32Array(w.memory.buffer, w.msx_audio(), w.msx_audio_len()));
      }
      stages.push(pcm.length - start);
    }
    const rises = pcm.flatMap((v, i) => i && pcm[i - 1] < 0 && v > 0 ? [i] : []);
    // Explicit controls still work after hidden intervals, even while hidden.
    w.msx_key(8, 7, 1); w.msx_key(8, 6, 1); w.msx_key(8, 0, 0);
    for (let i = 0; i < 3; i++) w.msx_frame();
    const silent = Array.from(new Float32Array(w.memory.buffer, w.msx_audio(), w.msx_audio_len())).every(v => v === 0);
    w.msx_key(8, 7, 0); w.msx_key(8, 6, 0); w.msx_key(8, 0, 1);
    window.sccManual = false;
    return {
      stages, silent, rises,
      square: pcm.every(v => Math.abs(v - 0.056) < 0.000001 || Math.abs(v + 0.0565) < 0.000001),
    };
  });
  expect(result.square).toBe(true);
  expect(result.silent).toBe(true);
  expect(result.stages.every(n => n > 8800)).toBe(true);
  expect(result.rises.length).toBeGreaterThan(700);
  const period = 32 * 254 * 44100 / 3_579_545;
  for (let n = 0; n < result.rises.length; n++) {
    expect(Math.abs(result.rises[n] - result.rises[0] - n * period)).toBeLessThan(1);
  }
}

for (const site of ['/', 'http://127.0.0.1:41874/msx-rust/']) {
  for (const model of ['0', '1']) {
    for (const mapper of ['5', '0']) {
      test(`classic SCC native audio, lifecycle and PSG: model ${model}, mapper ${mapper} at ${site}`, async ({ page }) => {
        test.setTimeout(90000);
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
        await capturedContinuity(page);
        // Fresh completed Web Audio buffers must keep sounding while either or
        // both register-access paths are hidden by genuine Z80 writes.
        await page.keyboard.down('ArrowRight');
        await square(page);
        await page.keyboard.down('ArrowDown');
        await square(page);
        await page.keyboard.up('ArrowRight');
        await square(page);
        await page.keyboard.up('ArrowDown');
        await square(page);
        // Release the voice control while both access paths remain hidden.
        await page.keyboard.down('ArrowRight');
        await page.keyboard.down('ArrowDown');
        await page.keyboard.up('Space');
        await silence(page);
        await page.keyboard.down('Space');
        await square(page);
        await page.keyboard.up('ArrowRight');
        await page.keyboard.up('ArrowDown');
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
