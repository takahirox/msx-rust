import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

// Shared, original MIT Z80 bytes; no commercial firmware or recorded audio.
const code = (await readFile('tests/fixtures/envelope-probe.hex', 'utf8'))
  .split('\n').flatMap(line => line.split('#')[0].trim().split(/\s+/).filter(Boolean))
  .map(byte => parseInt(byte, 16));

for (const site of ['/', 'http://127.0.0.1:41874/msx-rust/']) {
  for (const model of ['0', '1']) {
    test(`Z80 AY envelope ramp interval in Wasm and Web Audio: model ${model} at ${site}`, async ({ page }) => {
      await page.addInitScript(() => {
        const instantiate = WebAssembly.instantiate;
        WebAssembly.instantiate = async (...args) => {
          const result = await instantiate(...args);
          const exports = result.instance.exports;
          window.envelopeWasm = exports;
          return { ...result, instance: { exports: { ...exports,
            msx_frame() {
              if (!window.envelopeManual) {
                exports.msx_frame();
                window.envelopeFrame = (window.envelopeFrame ?? 0) + 1;
              }
            },
          } } };
        };
        window.envelopeAudio = [];
        const NativeContext = window.AudioContext;
        window.AudioContext = class extends NativeContext {
          constructor(...args) {
            super(...args);
            const create = this.createBufferSource.bind(this);
            this.createBufferSource = () => {
              const source = create();
              const start = source.start.bind(source);
              source.start = (...args) => {
                window.envelopeAudio.push({ frame: window.envelopeFrame, data: Array.from(source.buffer.getChannelData(0)) });
                return start(...args);
              };
              return source;
            };
          }
        };
      });
      await page.goto(site);
      await expect(page.locator('#transcript')).toContainText('READY');
      await page.locator('#model').selectOption(model);
      await page.locator('#advanced-panel').evaluate(el => { el.open = true; });
      const main = Buffer.alloc(32768);
      main.set([0xc3, 0, 1]); // JP 0100h; generation byte remains outside code.
      main.set(code, 0x100);
      main[0x2d] = Number(model);
      await page.locator('#bios').setInputFiles({ name: 'original-envelope-probe.rom', mimeType: 'application/octet-stream', buffer: main });
      if (model === '1') {
        const sub = Buffer.alloc(16384);
        sub.set([0x43, 0x44]);
        await page.locator('#subrom').setInputFiles({ name: 'original-empty-sub.rom', mimeType: 'application/octet-stream', buffer: sub });
        await page.locator('#apply-firmware').click();
      }
      await expect(page.locator('#status')).toContainText('user BIOS');
      const result = await page.evaluate(() => {
        window.envelopeManual = true;
        const w = window.envelopeWasm;
        function run() {
          w.msx_reset();
          const samples = [];
          for (let i = 0; i < 20; i++) {
            w.msx_frame();
            // Copy each owned frame before the next call can reuse/grow memory.
            samples.push(...new Float32Array(w.memory.buffer, w.msx_audio(), w.msx_audio_len()));
          }
          return samples;
        }
        const first = run(), second = run();
        const repeats = first.flatMap((v, i) => i && v - first[i-1] > 0.12 ? [i] : []);
        return {
          count: first.length, frameCycles: w.msx_frame_cycles(),
          finiteBounded: first.every(v => Number.isFinite(v) && Math.abs(v) <= 0.6),
          reproducible: first.length === second.length && first.every((v, i) => v === second[i]),
          repeats, intervals: repeats.slice(1).map((v, i) => v - repeats[i]),
        };
      });
      expect(result.finiteBounded).toBe(true);
      expect(result.reproducible).toBe(true);
      expect(Math.abs(result.count - Math.floor(result.frameCycles * 20 * 44100 / 3579545))).toBeLessThanOrEqual(1);
      // 256 PSG clocks/ramp * CPU/PSG ratio 2 * EP=257. The old core's
      // sixteenfold slowdown cannot produce even one repeat in this window.
      const interval = Math.floor(256 * 2 * 257 * 44100 / 3579545);
      expect(result.repeats.length).toBeGreaterThanOrEqual(8);
      expect(result.intervals.every(n => n === interval || n === interval + 1)).toBe(true);

      // Also measure the actual buffers scheduled to the native audio destination.
      await page.evaluate(() => { window.envelopeManual = false; });
      await page.locator('#audio').click();
      await expect(page.locator('#audio')).toHaveText('Mute audio');
      await expect.poll(() => page.evaluate(() => window.envelopeAudio.reduce((n, b) => n + b.data.length, 0))).toBeGreaterThan(14000);
      const scheduled = await page.evaluate(() => {
        // The existing latency cap may omit buffers during catch-up. Measure only
        // contiguous emulated frames; joining across a dropped frame invents edges.
        const intervals = [];
        let previousFrame, previous, lastRise, position = 0, finiteBounded = true;
        for (const buffer of window.envelopeAudio) {
          if (buffer.frame !== previousFrame + 1) { previous = undefined; lastRise = undefined; }
          for (const value of buffer.data) {
            finiteBounded &&= Number.isFinite(value) && Math.abs(value) <= 0.6;
            if (previous !== undefined && value - previous > 0.12) {
              if (lastRise !== undefined) intervals.push(position - lastRise);
              lastRise = position;
            }
            previous = value;
            position++;
          }
          previousFrame = buffer.frame;
        }
        return { finiteBounded, intervals };
      });
      expect(scheduled.finiteBounded).toBe(true);
      expect(scheduled.intervals.length).toBeGreaterThanOrEqual(3);
      expect(scheduled.intervals.filter(n => n !== interval && n !== interval + 1)).toEqual([]);
    });
  }
}
