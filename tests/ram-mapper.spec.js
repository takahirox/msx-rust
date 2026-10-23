import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

// The independent disk demo stores the original glyphs as a contiguous font.
const disk = await readFile('dist/weave-disk-demo.dsk');
async function result(page, text, color) {
  const glyphs = [...text].map(ch => [...disk.subarray(2048 + ch.charCodeAt(0)*8, 2056 + ch.charCodeAt(0)*8)]);
  await expect.poll(() => page.locator('#screen').evaluate((canvas, {glyphs, color}) => {
    const ctx = canvas.getContext('2d');
    const data = ctx.getImageData(0,0,glyphs.length*8,8).data;
    const letters = glyphs.every((glyph,col) => glyph.every((bits,y) => Array.from({length:8},(_,x) => {
      const i = (y*glyphs.length*8 + col*8+x)*4;
      const white = data[i] === 255 && data[i+1] === 255 && data[i+2] === 255;
      return white === Boolean(bits & (0x80 >> x));
    }).every(Boolean)));
    const bar = ctx.getImageData(0,16,64,8).data;
    return letters && Array.from({length:512},(_,i) => color.every((v,c) => bar[i*4+c] === v)).every(Boolean);
  }, {glyphs,color}), {timeout:15000}).toBe(true);
}
for (const site of ['/', 'http://127.0.0.1:41874/msx-rust/']) {
  test(`RAM mapper capacity probe through normal picker, C-BIOS and Reset at ${site}`, async ({page}) => {
    const errors=[];
    page.on('pageerror',e=>errors.push(e.message));
    await page.goto(site);
    await expect(page.locator('#transcript')).toContainText('READY');
    const chooser=page.waitForEvent('filechooser');
    await page.locator('#open-rom').click();
    await (await chooser).setFiles('dist/ram-mapper-test-cartridge.rom');
    await expect(page.locator('#status')).toContainText('C-BIOS');
    await expect(page.locator('#status')).toContainText('Auto: Plain');
    await result(page, 'RAM MAPPER PASS', [36,218,36]);
    await page.locator('#screen').evaluate(canvas => canvas.getContext('2d').clearRect(0,0,canvas.width,canvas.height));
    await page.locator('#reset').click();
    await result(page, 'RAM MAPPER PASS', [36,218,36]);
    await page.screenshot({path:`test-results/ram-mapper-${site === '/' ? 'root' : 'pages'}.png`});
    await page.locator('#cartridge').setInputFiles({name:'invalid.rom',mimeType:'application/octet-stream',buffer:Buffer.alloc(17)});
    await expect(page.locator('#status')).toContainText('unchanged');
    await result(page, 'RAM MAPPER PASS', [36,218,36]);
    // Authored firmware loaded through the paired local inputs initializes itself.
    await page.locator('#advanced-panel > summary').click();
    await page.locator('#bios').setInputFiles('dist/weave-msx2-boot.rom');
    await page.locator('#subrom').setInputFiles('dist/msx2-diagnostic-sub.rom');
    await page.locator('#apply-firmware').click();
    await expect(page.locator('#firmware')).toHaveValue('local');
    await result(page, 'RAM MAPPER PASS', [36,218,36]);
    await page.locator('#screen').evaluate(canvas => canvas.getContext('2d').clearRect(0,0,canvas.width,canvas.height));
    await page.locator('#reset').click();
    await result(page, 'RAM MAPPER PASS', [36,218,36]);
    await page.locator('#builtin').click();
    await expect(page.locator('#transcript')).toContainText('READY');
    await page.locator('#model').selectOption('0');
    await page.locator('#cartridge').setInputFiles('dist/ram-mapper-test-cartridge.rom');
    await result(page, 'RAM MAPPER FAIL', [212,82,77]);
    expect(errors).toEqual([]);
  });
}
