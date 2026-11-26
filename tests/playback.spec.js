import { test, expect } from './fixtures/clean-display.js';
import { startSound } from './fixtures/player-controls.js';

// Headless Chromium does not reliably background tabs. Drive the visibility
// event and the scheduler's timestamps explicitly, with the built Wasm and
// native Web Audio graph still executing in Chromium.
async function observePlayback(page) {
  await page.addInitScript(() => {
    let hidden = false;
    Object.defineProperties(document, {
      hidden: { get: () => hidden },
      visibilityState: { get: () => hidden ? 'hidden' : 'visible' },
    });
    window.setTabHidden = value => {
      hidden = value;
      document.dispatchEvent(new Event('visibilitychange'));
    };

    let time = 0;
    let callbacks = [];
    window.requestAnimationFrame = callback => callbacks.push(callback);
    window.advanceDisplay = elapsed => {
      time += elapsed;
      const scheduled = callbacks;
      callbacks = [];
      for (const callback of scheduled) callback(time);
      return scheduled.length;
    };

    const evidence = window.playback = { frames: 0, starts: 0, tone: 0, stops: 0, active: new Set(), keys: [], sticks: [63, 63], resumes: 0 };
    const instantiate = WebAssembly.instantiate;
    WebAssembly.instantiate = async (...args) => {
      const result = await instantiate(...args);
      const w = evidence.wasm = result.instance.exports;
      return { ...result, instance: { exports: { ...w,
        msx_frame() { w.msx_frame(); evidence.frames++; },
        msx_key(row, bit, down) { w.msx_key(row, bit, down); if (down) evidence.keys.push([row, bit]); },
        msx_joystick_indexed(port, value) { w.msx_joystick_indexed(port, value); evidence.sticks[port] = value; },
        msx_release_keys() { w.msx_release_keys(); evidence.keys = []; evidence.sticks.fill(63); },
      } } };
    };

    const Audio = window.AudioContext;
    window.AudioContext = class extends Audio {
      constructor(...args) {
        if (evidence.failAudioConstruction) throw new Error('AudioContext unavailable');
        super(...args);
        evidence.audio = this;
        if (evidence.blockSound) evidence.suspending = super.suspend();
      }
      get state() { return evidence.blockSound ? 'suspended' : super.state; }
      resume() {
        evidence.resumes++;
        return evidence.blockSound ? Promise.reject(new Error('Autoplay blocked')) : super.resume();
      }
      createBufferSource() {
        const source = super.createBufferSource();
        const start = source.start.bind(source);
        const stop = source.stop.bind(source);
        source.start = (...args) => {
          evidence.starts++;
          if (source.buffer.getChannelData(0).some(value => Math.abs(value) > 0.05)) evidence.tone++;
          evidence.active.add(source);
          return start(...args);
        };
        source.stop = (...args) => {
          evidence.stops++;
          evidence.active.delete(source);
          return stop(...args);
        };
        source.addEventListener('ended', () => evidence.active.delete(source));
        return source;
      }
    };
  });
}

const advance = (page, elapsed) => page.evaluate(elapsed => window.advanceDisplay(elapsed), elapsed);
const visibility = (page, hidden) => page.evaluate(hidden => window.setTabHidden(hidden), hidden);
const state = page => page.evaluate(() => {
  const { frames, starts, tone, stops, active, keys, sticks, wasm } = window.playback;
  return { frames, starts, tone, stops, active: active.size, keys, sticks, tape: wasm.msx_tape_position() };
});
const status = page => page.locator('#status');

test('visibility suspends frames, media time and sound and releases input without changing manual pause', async ({ page }) => {
  await observePlayback(page);
  await page.goto('/');
  await expect(status(page)).toContainText('Running');
  await advance(page, 0);
  await advance(page, 17);
  await expect(page.locator('#transcript')).toContainText('READY');

  await page.locator('#tape').setInputFiles('target/browser-fixtures/cassette-test.cas');
  await expect(page.locator('#tape-status')).toContainText('playing');
  await startSound(page);
  await page.keyboard.type('BEEP');
  await page.keyboard.press('Enter');
  await advance(page, 100);
  await expect(page.locator('#transcript')).toContainText('>BEEP');
  const before = await state(page);
  expect(before.tape).toBeGreaterThan(0);
  expect(before.tone).toBeGreaterThan(0);

  // A panel can take focus without suspending the visible tab.
  await page.locator('#help').click();
  await advance(page, 17);
  expect((await state(page)).frames).toBeGreaterThan(before.frames);
  await page.keyboard.press('Escape');
  await page.locator('#screen').focus();
  for (const key of ['ArrowUp', 'KeyN']) await page.keyboard.down(key);
  const held = await state(page);
  expect(held.sticks).toEqual([62, 47]);
  expect(held.keys.length).toBeGreaterThanOrEqual(2);

  for (let pass = 0; pass < 2; pass++) {
    await advance(page, 17);
    const playing = await state(page);
    await visibility(page, true);
    await expect(status(page)).toContainText('Suspended (tab hidden)');
    await expect(page.locator('#pause')).toHaveText('Pause');
    const hidden = await state(page);
    expect(hidden).toMatchObject({ frames: playing.frames, tape: playing.tape, active: 0, keys: [], sticks: [63, 63] });
    if (pass === 0) expect(hidden.stops).toBeGreaterThan(0);
    expect(await advance(page, 60_000)).toBe(1);
    expect(await state(page)).toEqual(hidden);
    await visibility(page, false);
    await expect(status(page)).toContainText('Running');
    for (const key of ['ArrowUp', 'KeyN']) await page.keyboard.down(key); // repeated physical holds
    expect(await advance(page, 60_000)).toBe(1);
    expect(await state(page)).toEqual(hidden);
    await advance(page, 17);
    const resumed = await state(page);
    expect(resumed).toMatchObject({ frames: hidden.frames + 1, keys: [], sticks: [63, 63] });
    expect(resumed.tape - hidden.tape).toBeCloseTo(0.017, 2);
    expect(resumed.tone).toBeGreaterThan(hidden.tone);
  }
  for (const key of ['ArrowUp', 'KeyN']) await page.keyboard.up(key);

  await page.locator('#pause').click();
  const paused = await state(page);
  for (let pass = 0; pass < 2; pass++) {
    await visibility(page, true);
    await advance(page, 5000);
    await visibility(page, false);
    await advance(page, 5000);
    await expect(page.locator('#pause')).toHaveText('Resume');
    await expect(status(page)).toContainText('Paused');
    expect(await state(page)).toEqual(paused);
  }
  await page.locator('#pause').click();
  await advance(page, 16);
  await advance(page, 17);
  expect((await state(page)).frames).toBe(paused.frames + 1);
});

test('hidden startup, import and deliberate Reset execute their first frame only when visible and requested', async ({ page }) => {
  await observePlayback(page);
  let deliverWasm;
  const wasmPending = new Promise(resolve => { deliverWasm = resolve; });
  await page.route('**/msx_wasm.wasm', async route => { await wasmPending; await route.continue(); });
  await page.addInitScript(() => {
    const read = File.prototype.arrayBuffer;
    File.prototype.arrayBuffer = async function () {
      const bytes = await read.call(this);
      return new Promise(resolve => { window.deliverRom = () => resolve(bytes); });
    };
  });

  await page.goto('http://127.0.0.1:41874/msx-rust/');
  await visibility(page, true);
  deliverWasm();
  await expect(status(page)).toHaveText('Suspended (tab hidden) · MSX2 · Weave BASIC');
  await advance(page, 1000);
  expect((await state(page)).frames).toBe(0);
  await expect(page.locator('#transcript')).toHaveText('');
  await visibility(page, false);
  await advance(page, 1000);
  await advance(page, 17);
  expect((await state(page)).frames).toBe(1);
  await expect(page.locator('#transcript')).toContainText('READY');

  await page.locator('#cartridge').setInputFiles('target/browser-fixtures/cbios-test-cartridge.rom');
  await expect(page.locator('#cancel-cartridge-import')).toBeVisible();
  await visibility(page, true);
  await visibility(page, false);
  await expect(status(page)).toContainText('Waiting for import');
  await advance(page, 1000);
  expect((await state(page)).frames).toBe(1);
  await page.locator('#pause').click();
  await visibility(page, true);
  await expect.poll(() => page.evaluate(() => typeof window.deliverRom)).toBe('function');
  await page.evaluate(() => window.deliverRom());
  await expect(status(page)).toContainText('Paused · MSX2 C-BIOS');
  await advance(page, 1000);
  await visibility(page, false);
  await advance(page, 1000);
  expect((await state(page)).frames).toBe(1);
  await expect(page.locator('#pause')).toHaveText('Resume');

  await visibility(page, true);
  await page.locator('#reset').click();
  await expect(status(page)).toContainText('Suspended (tab hidden) · MSX2 C-BIOS');
  await advance(page, 1000);
  expect((await state(page)).frames).toBe(1);
  await visibility(page, false);
  await advance(page, 1000);
  await advance(page, 17);
  expect((await state(page)).frames).toBe(2);
});

test('speaker shows audio initialization failure and retries while playback stays paused', async ({ page }) => {
  await observePlayback(page);
  await page.goto('/');
  await expect(status(page)).toContainText('Running');
  await page.locator('#pause').click();
  await expect(status(page)).toContainText('Paused');

  const speaker = page.locator('#mute');
  const audioStatus = page.locator('#audio-state');
  await expect(audioStatus).toBeEmpty();
  await page.evaluate(() => { window.playback.failAudioConstruction = true; });
  await speaker.press('Enter');
  expect(await page.evaluate(() => window.playback.audio)).toBeUndefined();
  await expect(speaker).toHaveAccessibleName('Start sound');
  await expect(audioStatus).toContainText('Use the speaker or volume to try again');

  const paused = await state(page);
  await page.evaluate(() => { window.playback.failAudioConstruction = false; });
  await speaker.press('Space');
  await expect(speaker).toHaveAccessibleName('Mute');
  await expect(audioStatus).toBeEmpty();
  expect(await page.evaluate(() => window.playback.audio.state)).toBe('running');
  await advance(page, 1000);
  expect(await state(page)).toEqual(paused);
  await expect(status(page)).toContainText('Paused');
});

test('speaker retries blocked audio and remembers volume and mute across reload and visibility changes', async ({ page }) => {
  await observePlayback(page);
  await page.addInitScript(() => {
    if (!localStorage.getItem('msx-rust:volume')) {
      localStorage.setItem('msx-rust:volume', '{"volume":200,"lastNonzeroVolume":0}');
    }
    if (sessionStorage.getItem('blockAudioPreference')) {
      for (const method of ['getItem', 'setItem']) {
        const access = Storage.prototype[method];
        Storage.prototype[method] = function (key, ...args) {
          if (key === 'msx-rust:volume') throw new Error('Storage blocked');
          return access.call(this, key, ...args);
        };
      }
    }
  });
  await page.goto('/');
  const speaker = page.locator('#mute');
  const volume = page.getByRole('slider', { name: 'Volume' });
  const audioStatus = page.locator('#audio-state');
  await expect(speaker).toHaveAccessibleName('Start sound');
  await expect(volume).toHaveValue('100');
  expect(await page.evaluate(() => window.playback.audio)).toBeUndefined();

  await page.evaluate(() => { window.playback.blockSound = true; });
  await speaker.press('Enter');
  await expect(speaker).toBeFocused();
  await expect(speaker).toHaveAccessibleName('Start sound');
  await expect(audioStatus).toContainText('Use the speaker or volume to try again');
  await expect(speaker.locator('.sound-waves')).toBeVisible();
  await expect(speaker).toHaveAttribute('aria-pressed', 'false');
  await page.evaluate(async () => { await window.playback.suspending; window.playback.blockSound = false; });
  await speaker.press('Space');
  await expect(speaker).toHaveAccessibleName('Mute');
  await expect(audioStatus).toBeEmpty();

  await visibility(page, true);
  const resumeCount = await page.evaluate(async () => {
    const playback = window.playback;
    await playback.audio.suspend();
    return playback.resumes;
  });
  await visibility(page, false);
  await expect(speaker).toHaveAccessibleName('Mute');
  expect(await page.evaluate(() => window.playback.resumes)).toBe(resumeCount + 1);

  await volume.fill('37');
  await volume.press('Home');
  await expect(volume).toHaveAttribute('aria-valuetext', '0% (muted)');
  await expect(speaker).toHaveAccessibleName('Unmute');
  await expect(speaker).toHaveAttribute('aria-pressed', 'true');
  await expect(speaker.locator('.mute-mark')).toBeVisible();
  await expect(speaker.locator('.sound-waves')).toBeHidden();
  await page.evaluate(() => window.playback.audio.suspend());
  await visibility(page, true);
  await visibility(page, false);
  expect(await page.evaluate(() => window.playback.audio.state)).toBe('suspended');
  expect(await page.evaluate(() => window.playback.resumes)).toBe(resumeCount + 1);
  await expect(audioStatus).toBeEmpty();

  await page.reload();
  await expect(speaker).toHaveAccessibleName('Unmute');
  await expect(volume).toHaveValue('0');
  expect(await page.evaluate(() => window.playback.audio)).toBeUndefined();
  await speaker.press('Space');
  await expect(speaker).toHaveAccessibleName('Mute');
  await expect(volume).toHaveValue('37');
  await volume.press('Home');
  await volume.press('ArrowRight');
  await expect(speaker).toHaveAccessibleName('Mute');
  await expect(volume).toHaveValue('1');

  await page.evaluate(() => sessionStorage.setItem('blockAudioPreference', 'true'));
  await page.reload();
  await expect(volume).toHaveValue('100');
  await volume.fill('25');
  await startSound(page);
  await expect(volume).toHaveValue('25');
});
