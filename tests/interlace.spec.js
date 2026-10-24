import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

// Original CPU port/status program, also timestamped independently in native tests.
const code = (await readFile('tests/fixtures/interlace-probe.hex', 'utf8'))
  .split('\n').flatMap(line => line.split('#')[0].trim().split(/\s+/).filter(Boolean))
  .map(byte => parseInt(byte, 16));
const expectedBudget = n => n === 1 ? 59736 : [59850, 71250, 71364, 59736][Math.floor((n - 2) / 8) % 4];

for (const site of ['/', 'http://127.0.0.1:41874/msx-rust/']) {
  test(`interlaced CPU probe, Canvas, PCM and animation-frame pacing at ${site}`, async ({ page }) => {
    const errors = [];
    page.on('pageerror', e => errors.push(e.message));
    await page.addInitScript(() => {
      // Drive the app's actual animation callback with deterministic timestamps.
      // Wasm calls, accumulator subtraction and Canvas rendering remain real.
      window.requestAnimationFrame = callback => { window.probeAnimation = callback; return 1; };
      window.probeBuffers = [];
      const NativeContext = window.AudioContext;
      window.AudioContext = class extends NativeContext {
        createBuffer(...args) {
          const buffer = super.createBuffer(...args);
          if (window.probeTrack) window.probeBuffers.push(args[1]);
          return buffer;
        }
      };
      const instantiate = WebAssembly.instantiate;
      WebAssembly.instantiate = async (...args) => {
        const result = await instantiate(...args), w = result.instance.exports;
        window.probeWasm = w;
        window.probeTrace = [];
        return { ...result, instance: { exports: { ...w,
          msx_frame() {
            const budget = w.msx_frame_cycles();
            w.msx_frame();
            if (window.probeTrack) {
              const pcm = new Float32Array(w.memory.buffer, w.msx_audio(), w.msx_audio_len());
              window.probeTrace.push({ budget, next: w.msx_frame_cycles(), height: w.msx_height(), samples: pcm.length, audible: pcm.some(v => Math.abs(v) > 0.05) });
            }
          },
        } } };
      };
    });
    await page.goto(site);
    await expect(page.locator('#transcript')).toContainText('READY');
    await page.locator('#model').selectOption('1');
    await page.locator('#advanced-panel').evaluate(el => { el.open = true; });
    const main = Buffer.alloc(32768);
    main.set([0xc3, 0, 1]); main[0x2d] = 1; main.set(code, 0x100);
    const sub = Buffer.alloc(16384); sub.set([0x43, 0x44]);
    await page.locator('#bios').setInputFiles({ name: 'original-interlace-probe.rom', mimeType: 'application/octet-stream', buffer: main });
    await page.locator('#subrom').setInputFiles({ name: 'original-empty-sub.rom', mimeType: 'application/octet-stream', buffer: sub });
    await page.locator('#apply-firmware').click();
    await expect(page.locator('#status')).toContainText('user BIOS');
    await page.getByRole('button', { name: 'Enable audio', exact: true }).click();
    await expect(page.locator('#audio-state')).toContainText('Audio on');

    const result = await page.evaluate(() => {
      window.probeWasm.msx_reset();
      window.probeTrack = true;
      window.probeTrace = [];
      window.probeBuffers = [];
      const observations = [];
      let time = 0;
      const drive = () => {
        const callback = window.probeAnimation;
        window.probeAnimation = undefined;
        callback(time);
        const canvas = document.querySelector('#screen'), ctx = canvas.getContext('2d');
        const pixel = y => [...ctx.getImageData(0, y, 1, 1).data].slice(0, 3);
        observations.push({ time, count: window.probeTrace.length, height: canvas.height, rows: [pixel(0), pixel(1)] });
      };
      drive(); // Establish app timestamp without executing emulated time.
      for (let i = 0; i < 300 && window.probeTrace.length < 110; i++) {
        time += [0.25, 7, 16, 33, 0, 45, 2, 19, 80][i % 9];
        drive();
      }
      return { trace: window.probeTrace, buffers: window.probeBuffers, observations };
    });
    expect(result.trace.length).toBeGreaterThanOrEqual(110);
    // The real enqueueAudio path prepares exactly one fresh Web Audio buffer
    // per executed field, including catch-up and timing transitions. Its normal
    // latency cap may omit playback of some buffers under accelerated test time.
    expect(result.buffers).toEqual(result.trace.map(frame => frame.samples));
    let nominal = 0, samples = 0;
    for (const [i, frame] of result.trace.entries()) {
      expect(frame.budget, `field ${i + 1}`).toBe(expectedBudget(i + 1));
      expect(frame.next, `next field ${i + 2}`).toBe(expectedBudget(i + 2));
      nominal += frame.budget; samples += frame.samples;
      // Native tests check exact executed T-states. Browser observes PCM through
      // the existing ABI: less than one instruction of debt, never a lost field.
      expect(samples).toBeGreaterThanOrEqual(Math.floor(nominal * 44100 / 3579545));
      expect(samples).toBeLessThanOrEqual(Math.floor((nominal + 20) * 44100 / 3579545));
      expect(frame.audible).toBe(true);
    }
    // Independent wall-time scheduler, using literal expected budgets rather
    // than querying the production scheduling export to set expectations.
    let accumulator = 0, count = 0, previous = 0;
    for (const observation of result.observations) {
      accumulator += observation.time - previous;
      previous = observation.time;
      let catchup = 0;
      while (accumulator >= 1000 * expectedBudget(count + 1) / 3579545 && catchup++ < 5) {
        accumulator -= 1000 * expectedBudget(count + 1) / 3579545;
        count++;
      }
      expect(observation.count, `animation time ${observation.time}`).toBe(count);
      const fieldInCycle = (count - 1) % 32 + 1;
      if (count >= 4 && fieldInCycle >= 4 && fieldInCycle <= 15) {
        expect(observation.height).toBe(424);
        expect(observation.rows).toEqual([[36, 218, 36], [36, 36, 255]]);
      }
      if (fieldInCycle >= 20 && fieldInCycle <= 31) expect(observation.height).toBe(212);
    }
    expect(new Set(result.trace.map(f => f.budget))).toEqual(new Set([59736, 59850, 71250, 71364]));
    expect(errors).toEqual([]);
  });
}
