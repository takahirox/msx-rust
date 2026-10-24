import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

// Shared with the native Machine test; original code, rendered through normal VDP output.
const code = (await readFile('tests/fixtures/wz-probe.hex', 'utf8'))
  .split('\n').flatMap(line => line.split('#')[0].trim().split(/\s+/).filter(Boolean))
  .map(byte => parseInt(byte, 16));

async function capturedFlags(page) {
  return page.locator('#screen').evaluate(canvas => {
    const pixels = canvas.getContext('2d').getImageData(0, 0, 8, 8).data;
    return Array.from({ length: 8 }, (_, y) => {
      let value = 0;
      for (let x = 0; x < 8; x++) {
        const i = (y * 8 + x) * 4;
        value = (value << 1) | Number(pixels[i] === 255 && pixels[i + 1] === 255 && pixels[i + 2] === 255);
      }
      return value;
    });
  });
}

for (const site of ['/', 'http://127.0.0.1:41874/msx-rust/']) {
  for (const model of ['0', '1']) {
    test(`WZ captured flags through compiled Wasm and Reset: model ${model} at ${site}`, async ({ page }) => {
      const errors = [];
      page.on('pageerror', error => errors.push(error.message));
      await page.goto(site);
      await expect(page.locator('#transcript')).toContainText('READY');
      await page.locator('#model').selectOption(model);
      await page.locator('#advanced-panel').evaluate(el => { el.open = true; });
      const main = Buffer.alloc(32768);
      main.set([0xc3, 0, 1]);
      main[0x2d] = Number(model);
      main.set(code, 0x100);
      await page.locator('#bios').setInputFiles({ name: 'original-wz-probe.rom', mimeType: 'application/octet-stream', buffer: main });
      if (model === '1') {
        const sub = Buffer.alloc(16384);
        sub.set([0x43, 0x44]);
        await page.locator('#subrom').setInputFiles({ name: 'original-wz-sub.rom', mimeType: 'application/octet-stream', buffer: sub });
        await page.locator('#apply-firmware').click();
      }
      await expect(page.locator('#status')).toContainText('user BIOS');
      const expected = [0x7d, 0x91, 0x39, 0x7d, 0x75, 0x7d, 0x75, 0x7d];
      await expect.poll(() => capturedFlags(page)).toEqual(expected);
      await page.locator('#screen').evaluate(canvas => canvas.getContext('2d').clearRect(0, 0, canvas.width, canvas.height));
      await page.locator('#reset').click();
      await expect.poll(() => capturedFlags(page)).toEqual(expected);
      await expect(page.locator('#model')).toHaveValue(model);
      expect(errors).toEqual([]);
    });
  }
}
