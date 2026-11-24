import { test, expect } from '@playwright/test';
import { displayPreferenceKey } from '../web/display.js';
import { installInterlaceProbe } from './fixtures/interlace.js';

test.use({
  launchOptions: { args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'] },
});

const crt = page => page.locator('#crt-screen');
const screen = page => page.locator('#screen');
const select = page => page.getByLabel('Display', { exact: true });
const status = page => page.locator('#display-status');
const surface = page => page.locator('.screen-surface');

async function boot(page, path = '/') {
  await page.goto(path);
  await expect(page.locator('#transcript')).toContainText('READY');
  await expect(select(page)).toHaveValue('crt');
}

async function watchWasm(page) {
  await page.addInitScript(() => {
    const instantiate = WebAssembly.instantiate;
    WebAssembly.instantiate = async (...args) => {
      const result = await instantiate(...args), wasm = result.instance.exports;
      window.displayWasm = wasm;
      const calls = window.displayCalls = { frames: 0, audio: 0, models: 0, resets: 0, sticks: [] };
      return { ...result, instance: { exports: { ...wasm,
        msx_frame() { calls.frames++; wasm.msx_frame(); },
        msx_audio_len() { calls.audio++; return wasm.msx_audio_len(); },
        msx_model(model) { calls.models++; wasm.msx_model(model); },
        msx_reset() { calls.resets++; wasm.msx_reset(); },
        msx_joystick_indexed(port, value) {
          calls.sticks[port] = value;
          return wasm.msx_joystick_indexed(port, value);
        },
      } } };
    };
  });
}

async function paste(page, text) {
  await screen(page).click();
  await expect(screen(page)).toBeFocused();
  await screen(page).evaluate((canvas, text) => {
    const clipboardData = new DataTransfer();
    clipboardData.setData('text/plain', text);
    canvas.dispatchEvent(new ClipboardEvent('paste', { clipboardData, bubbles: true, cancelable: true }));
  }, text);
}

// Sample the page compositor, including whichever canvas is actually on top.
// Decoding with Chromium avoids a Node-side PNG dependency.
async function sample(page, points) {
  const png = await surface(page).screenshot({ scale: 'css' });
  return page.evaluate(async ({ png, points }) => {
    const bitmap = await createImageBitmap(new Blob([Uint8Array.from(atob(png), char => char.charCodeAt(0))]));
    const context = new OffscreenCanvas(bitmap.width, bitmap.height).getContext('2d');
    context.drawImage(bitmap, 0, 0);
    const data = context.getImageData(0, 0, bitmap.width, bitmap.height).data;
    return points.map(([x, y]) => {
      if (x < 0 || x >= bitmap.width || y < 0 || y >= bitmap.height) {
        throw new RangeError(`Screenshot pixel (${x}, ${y}) is outside ${bitmap.width}×${bitmap.height}`);
      }
      const start = (y * bitmap.width + x) * 4;
      return [...data.subarray(start, start + 3)];
    });
  }, { png: png.toString('base64'), points });
}

const white = [255, 255, 255];
const black = [0, 0, 0];
const red = [255, 0, 0];
const green = [0, 255, 0];
const corners = [[48, 48], [560, 48], [48, 430], [560, 430]];
const edge = [318, 319, 320, 321].map(x => [x, 430]);
const chroma = [60, 61, 62].map(x => [x, 49]);
const rows = Array.from({ length: 15 }, (_, i) => [60, 48 + i]);

test('default CRT softens and modulates the visible MSX2 image, with exact Clean and a saved choice', async ({ page }) => {
  await watchWasm(page);
  await boot(page);
  await expect(crt(page)).toBeVisible();
  const textPixels = Array.from({ length: 90 * 24 }, (_, i) => [i % 90 * 3, Math.floor(i / 90) * 3]);
  expect((await sample(page, textPixels)).filter(rgb => rgb.every(channel => channel > 210) && new Set(rgb).size > 1).length)
    .toBeGreaterThan(10);
  expect(await page.evaluate(key => localStorage.getItem(key), displayPreferenceKey)).toBeNull();
  await paste(page, '10 PRINT 73\n');
  await expect(page.locator('#transcript')).toContainText('10 PRINT 73');
  const running = await page.evaluate(() => window.displayCalls);
  await select(page).selectOption('clean');
  await expect(crt(page)).toBeHidden();
  await select(page).selectOption('crt');
  await expect(crt(page)).toBeVisible();
  await expect(page.locator('#status')).toHaveText('Running · MSX2 · Weave BASIC');
  await expect.poll(() => page.evaluate(() => window.displayCalls.frames)).toBeGreaterThan(running.frames);
  expect(await page.evaluate(() => window.displayCalls)).toMatchObject({ models: running.models, resets: running.resets });
  await paste(page, 'RUN\n');
  await expect(page.locator('#transcript')).toContainText('73\nREADY');

  await paste(page, 'COLOR 15,0:SCREEN 8:LINE (0,0)-(127,105),255,BF:LINE (128,0)-(255,105),28,BF:LINE (0,106)-(127,211),224,BF\n');
  await expect.poll(async () => (await sample(page, [corners[2]]))[0][1]).toBeGreaterThan(215);
  await page.locator('#audio').click();
  await expect(page.locator('#audio-state')).toHaveText('Audio on');
  await page.locator('#pause').click();
  const paused = await page.evaluate(() => window.displayCalls);
  const points = [...corners, ...edge, ...chroma, ...rows];
  const retro = await sample(page, points);
  expect(retro[0].every(channel => channel > 215)).toBe(true);
  expect(retro[1]).toEqual([expect.any(Number), 0, 0]);
  expect(retro[1][0]).toBeGreaterThan(215);
  expect(retro[2]).toEqual([0, expect.any(Number), 0]);
  expect(retro[2][1]).toBeGreaterThan(215);
  expect(retro[3]).toEqual(black);
  const [, inside, outside, far] = retro.slice(4, 8).map(rgb => rgb[1]);
  expect(inside).toBeGreaterThan(186);
  expect(outside).toBeGreaterThan(6);
  expect(outside).toBeLessThan(45);
  expect(far).toBe(0);
  const dominant = retro.slice(8, 11).map(rgb => rgb.indexOf(Math.max(...rgb)));
  expect(new Set(dominant).size).toBe(3);
  for (const rgb of retro.slice(8, 11)) {
    expect(Math.max(...rgb) - Math.min(...rgb)).toBeGreaterThan(5);
  }
  const scanlines = retro.slice(11).map(rgb => rgb[0]);
  expect(Math.max(...scanlines) - Math.min(...scanlines)).toBeGreaterThan(12);
  expect(Math.min(...scanlines)).toBeGreaterThan(205);

  await select(page).selectOption('clean');
  expect(await sample(page, points)).toEqual([
    white, red, green, black, green, green, black, black, ...[...chroma, ...rows].map(() => white),
  ]);
  await expect(screen(page)).toHaveAttribute('height', '212');
  expect(await screen(page).evaluate(canvas => {
    const w = window.displayWasm;
    const actual = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data;
    const source = new Uint8Array(w.memory.buffer, w.msx_pixels(), actual.length);
    return actual.every((value, index) => value === source[index]);
  })).toBe(true);
  await select(page).selectOption('crt');
  expect(await sample(page, points)).toEqual(retro);
  await expect(page.locator('#status')).toContainText('Paused');
  expect(await page.evaluate(() => window.displayCalls)).toEqual(paused);

  await page.setViewportSize({ width: 370, height: 800 });
  await expect.poll(() => crt(page).evaluate(canvas => [canvas.width, canvas.height])).toEqual([324, 243]);
  const small = await sample(page, rows);
  expect(small.every(rgb => rgb.every(channel => channel >= 253))).toBe(true);
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('Emulation.setDeviceMetricsOverride', { width: 370, height: 800, deviceScaleFactor: 2, mobile: false });
  await expect.poll(() => page.evaluate(() => devicePixelRatio)).toBe(2);
  await expect.poll(() => crt(page).evaluate(canvas => [canvas.width, canvas.height])).toEqual([648, 486]);
  await cdp.detach();
  expect(await page.evaluate(() => window.displayCalls)).toEqual(paused);
  await select(page).selectOption('clean');
  await page.reload();
  await expect(page.locator('#transcript')).toContainText('READY');
  await expect(select(page)).toHaveValue('clean');
  await expect(crt(page)).toBeHidden();
});

test('512-wide and woven interlaced output retain geometry in fullscreen while paused', async ({ page }) => {
  await watchWasm(page);
  await page.addInitScript(() => {
    const request = window.requestAnimationFrame;
    window.requestAnimationFrame = callback => {
      if (!window.manualAnimation) return request(callback);
      window.advanceDisplay = callback;
      return 1;
    };
  });
  await page.setViewportSize({ width: 1280, height: 1100 });
  await boot(page);
  await paste(page, 'COLOR 15,0:SCREEN 7:LINE (64,25)-(159,90),15,BF:LINE (352,120)-(447,190),15,BF\n');
  await expect.poll(() => page.evaluate(() => [window.displayWasm.msx_width(), window.displayWasm.msx_height()])).toEqual([512, 212]);
  const coords = [[128, 96], [512, 96], [128, 336], [512, 336]];
  await expect.poll(async () => (await sample(page, coords))[3][0]).toBeGreaterThan(200);
  await page.locator('#pause').click();
  const wide = await sample(page, coords);
  expect(wide[0].every(channel => channel > 200)).toBe(true);
  expect(wide[1]).toEqual(black);
  expect(wide[2]).toEqual(black);
  expect(wide[3].every(channel => channel > 200)).toBe(true);
  await select(page).selectOption('clean');
  await expect(screen(page)).toHaveAttribute('width', '512');
  await expect(screen(page)).toHaveAttribute('height', '212');
  expect(await sample(page, coords)).toEqual([white, black, black, white]);
  await select(page).selectOption('crt');
  expect(await sample(page, coords)).toEqual(wide);

  // Freeze at the probe's first complete interlaced image, which has
  // different colors in its two fields.
  await page.evaluate(() => { window.manualAnimation = true; });
  await installInterlaceProbe(page);
  await expect(page.locator('#status')).toContainText('user BIOS');
  await page.waitForFunction(() => typeof window.advanceDisplay === 'function');
  await page.evaluate(() => {
    window.advanceDisplay(0);
    window.advanceDisplay(75);
  });
  expect(await page.evaluate(() => window.displayWasm.msx_height())).toBe(424);
  await page.locator('#pause').click();
  const paused = await page.evaluate(() => window.displayCalls.frames);
  await page.getByRole('button', { name: 'Fullscreen', exact: true }).click();
  await expect(page.locator('#fullscreen')).toHaveText('Exit fullscreen');
  expect(await page.evaluate(() => document.fullscreenElement?.id)).toBe('emulator');
  await expect.poll(() => crt(page).evaluate(canvas => [canvas.width, canvas.height])).toEqual([960, 720]);
  const { width, height } = await surface(page).boundingBox();
  expect(width / height).toBeCloseTo(4 / 3, 2);
  const fields = Array.from({ length: 7 }, (_, y) => [80, y]);
  const pixels = await sample(page, fields);
  const greenRow = pixels.findIndex(([, g, b]) => g > 160 && b < 100);
  const blueRow = pixels.findIndex(([, g, b]) => b > 170 && g < 100);
  expect(greenRow).toBeGreaterThanOrEqual(0);
  expect(blueRow).toBeGreaterThan(greenRow);
  expect(pixels.at(-1)).toEqual(black);
  await select(page).selectOption('clean');
  await expect(screen(page)).toHaveAttribute('height', '424');
  expect(await screen(page).evaluate(canvas => {
    const context = canvas.getContext('2d');
    return [0, 1].map(row => [...context.getImageData(80, row, 1, 1).data].slice(0, 3));
  })).toEqual([[36, 218, 36], [36, 36, 255]]);
  await select(page).selectOption('crt');
  expect(await sample(page, fields)).toEqual(pixels);
  await page.getByRole('button', { name: 'Exit fullscreen', exact: true }).click();
  await expect.poll(() => crt(page).evaluate(canvas => [canvas.width, canvas.height])).toEqual([640, 480]);
  expect(await page.evaluate(() => window.displayCalls.frames)).toBe(paused);
  await expect(screen(page)).toBeFocused();
});

test('the Pages subpath presents an MSX1 demo with CRT and forwards joystick input', async ({ page }) => {
  await watchWasm(page);
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await boot(page, 'http://127.0.0.1:41874/msx-rust/');
  await expect(crt(page)).toBeVisible();
  await page.locator('#demo').selectOption('plustaker');
  await page.locator('#play-demo').click();
  await expect(page.locator('#status')).toContainText('MSX1 C-BIOS');
  await expect(page.locator('#status')).toContainText('plustaker');
  await expect(page.locator('#model')).toHaveValue('0');
  await expect(crt(page)).toBeVisible();
  // The running game's bottom HUD contains white glyphs on black.
  const hud = Array.from({ length: 31 * 12 }, (_, i) => [165 + i % 31 * 10, 420 + Math.floor(i / 31) * 5]);
  await expect.poll(async () => {
    const pixels = await sample(page, hud);
    return pixels.filter(rgb => rgb.every(channel => channel > 200) && new Set(rgb).size > 1).length;
  }).toBeGreaterThan(6);
  await screen(page).click();
  await expect(screen(page)).toBeFocused();
  await page.keyboard.down('ArrowUp');
  expect(await page.evaluate(() => window.displayCalls.sticks)).toEqual([62, 63]);
  await page.keyboard.up('ArrowUp');
  expect(await page.evaluate(() => window.displayCalls.sticks)).toEqual([63, 63]);
  expect(errors).toEqual([]);
});

test('unavailable WebGL2 and blocked storage leave a usable Clean display', async ({ page }) => {
  await page.addInitScript(() => {
    const getContext = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (kind, ...args) {
      if (kind === 'webgl2') return null;
      return getContext.call(this, kind, ...args);
    };
    Object.defineProperty(window, 'localStorage', { get() { throw new DOMException('Blocked', 'SecurityError'); } });
  });
  await boot(page);
  await expect(status(page)).toHaveText('Clean active · WebGL2 is unavailable.');
  await expect(crt(page)).toBeHidden();
  await paste(page, 'PRINT 42\n');
  await expect(page.locator('#transcript')).toContainText('42\nREADY');
  await select(page).selectOption('clean');
  await expect(status(page)).toBeEmpty();
  await expect(crt(page)).toBeHidden();
  const before = await sample(page, [[630, 470]]);
  await select(page).selectOption('crt');
  await expect(status(page)).toContainText('Clean active');
  expect(await sample(page, [[630, 470]])).toEqual(before);
});

test('a smaller output recovers visible CRT after an allocation failure while paused', async ({ page }) => {
  await watchWasm(page);
  await page.addInitScript(() => {
    const prototype = WebGL2RenderingContext.prototype;
    const { get } = Object.getOwnPropertyDescriptor(prototype, 'drawingBufferWidth');
    Object.defineProperty(prototype, 'drawingBufferWidth', {
      get() { return Math.min(get.call(this), 800); },
    });
  });
  await page.setViewportSize({ width: 1280, height: 1100 });
  await boot(page);
  await paste(page, 'COLOR 15,15:CLS\n');
  await expect.poll(async () => (await sample(page, chroma))[0].every(channel => channel > 215)).toBe(true);
  await page.locator('#audio').click();
  await expect(page.locator('#audio-state')).toHaveText('Audio on');
  await page.locator('#pause').click();
  const paused = await page.evaluate(() => window.displayCalls);
  const before = await sample(page, chroma);
  expect(before).not.toEqual(chroma.map(() => white));
  await expect(crt(page)).toHaveAttribute('width', '640');

  await page.getByRole('button', { name: 'Fullscreen', exact: true }).click();
  await expect(crt(page)).toHaveAttribute('width', '960');
  await expect(status(page)).toHaveText('Clean active · WebGL2 output allocation failed.');
  await expect(crt(page)).toBeHidden();
  await expect(select(page)).toHaveValue('crt');
  expect(await sample(page, chroma)).toEqual(chroma.map(() => white));

  await page.getByRole('button', { name: 'Exit fullscreen', exact: true }).click();
  await expect(crt(page)).toHaveAttribute('width', '640');
  await expect(crt(page)).toBeVisible();
  await expect(status(page)).toBeEmpty();
  expect(await sample(page, chroma)).toEqual(before);
  await expect(page.locator('#status')).toContainText('Paused');
  expect(await page.evaluate(() => window.displayCalls)).toEqual(paused);
});

test('shader and draw failures fall back, then real context loss and restoration recover CRT', async ({ page }) => {
  await page.addInitScript(key => {
    localStorage.setItem(key, 'invalid preference');
    const shaderSource = WebGL2RenderingContext.prototype.shaderSource;
    const drawArrays = WebGL2RenderingContext.prototype.drawArrays;
    window.breakShader = true;
    WebGL2RenderingContext.prototype.shaderSource = function (shader, source) {
      return shaderSource.call(this, shader, window.breakShader ? 'invalid GLSL' : source);
    };
    WebGL2RenderingContext.prototype.drawArrays = function (mode, first, count) {
      return drawArrays.call(this, window.breakDraw ? 0xffff : mode, first, count);
    };
  }, displayPreferenceKey);
  await watchWasm(page);
  await boot(page);
  await expect(status(page)).toHaveText('Clean active · WebGL2 shader compilation failed.');
  await paste(page, 'COLOR 15,15:CLS\n');
  await expect.poll(async () => (await sample(page, chroma))[0]).toEqual(white);
  await page.locator('#pause').click();
  const paused = await page.evaluate(() => window.displayCalls.frames);
  await page.evaluate(() => { window.breakShader = false; window.breakDraw = true; });
  await select(page).selectOption('clean');
  await select(page).selectOption('crt');
  await expect(status(page)).toHaveText('Clean active · WebGL2 could not draw the display.');
  expect(await sample(page, chroma)).toEqual(chroma.map(() => white));
  await page.evaluate(() => { window.breakDraw = false; });
  await select(page).selectOption('clean');
  await select(page).selectOption('crt');
  await expect(crt(page)).toBeVisible();
  const before = await sample(page, chroma);
  expect(before).not.toEqual(chroma.map(() => white));

  await crt(page).evaluate(canvas => {
    window.lossExtension = canvas.getContext('webgl2').getExtension('WEBGL_lose_context');
    window.lossExtension.loseContext();
  });
  await expect(status(page)).toHaveText('Clean active · WebGL2 context lost.');
  await expect(crt(page)).toBeHidden();
  await expect(select(page)).toHaveValue('crt');
  expect(await sample(page, chroma)).toEqual(chroma.map(() => white));
  await page.evaluate(() => window.lossExtension.restoreContext());
  await expect(crt(page)).toBeVisible();
  await expect(status(page)).toBeEmpty();
  expect(await sample(page, chroma)).toEqual(before);
  expect(await page.evaluate(() => window.displayCalls.frames)).toBe(paused);
});
