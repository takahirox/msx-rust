import { test, expect } from '@playwright/test';
import { readFile, writeFile } from 'node:fs/promises';

// Independent Canvas expectations use the repository's original font, rather
// than examining probe opcodes or any emulator-private state.
const disk = await readFile('dist/weave-disk-demo.dsk');
const glyphs = [...'FH PASS'].map(ch => [...disk.subarray(2048 + ch.charCodeAt(0)*8, 2056 + ch.charCodeAt(0)*8)]);
async function passed(page) {
  await expect.poll(() => page.locator('#screen').evaluate((canvas, glyphs) => {
    const ctx = canvas.getContext('2d');
    const data = ctx.getImageData(0,0,56,8).data;
    const text = glyphs.every((glyph,col) => glyph.every((bits,y) => Array.from({length:8},(_,x) => {
      const i = (y*56+col*8+x)*4;
      return (data[i] === 255 && data[i+1] === 255 && data[i+2] === 255) === Boolean(bits & (128 >> x));
    }).every(Boolean)));
    const bar = ctx.getImageData(0,16,64,8).data;
    return text && Array.from({length:512},(_,i) => bar[i*4] === 36 && bar[i*4+1] === 218 && bar[i*4+2] === 36).every(Boolean);
  }, glyphs), {timeout:20000}).toBe(true);
}
for (const site of ['/', 'http://127.0.0.1:41874/msx-rust/']) {
  test(`original FH polling and IM1 probe through compiled Wasm at ${site}`, async ({page}) => {
    const errors=[];
    page.on('pageerror', e => errors.push(e.message));
    await page.goto(site);
    await expect(page.locator('#transcript')).toContainText('READY');
    const chooser = page.waitForEvent('filechooser');
    await page.locator('#open-rom').click();
    await (await chooser).setFiles('dist/fh-test-cartridge.rom');
    await expect(page.locator('#status')).toContainText('C-BIOS');
    await passed(page);
    await page.locator('#screen').evaluate(canvas => canvas.getContext('2d').clearRect(0,0,canvas.width,canvas.height));
    await page.locator('#reset').click();
    await passed(page);
    const snapshot = await page.locator('#screen').evaluate(canvas => canvas.toDataURL('image/png'));
    await writeFile(`test-results/vdp-fh-${site === '/' ? 'root' : 'pages'}.png`, Buffer.from(snapshot.split(',')[1], 'base64'));
    expect(errors).toEqual([]);
  });
}
