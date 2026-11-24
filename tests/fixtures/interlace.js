import { readFile } from 'node:fs/promises';

// Original CPU port/status program, also timestamped independently in native tests.
const code = (await readFile(new URL('interlace-probe.hex', import.meta.url), 'utf8'))
  .split('\n').flatMap(line => line.split('#')[0].trim().split(/\s+/).filter(Boolean))
  .map(byte => parseInt(byte, 16));

export async function installInterlaceProbe(page) {
  await page.locator('#advanced-panel').evaluate(el => { el.open = true; });
  const main = Buffer.alloc(32768);
  main.set([0xc3, 0, 1]); main[0x2d] = 1; main.set(code, 0x100);
  const sub = Buffer.alloc(16384); sub.set([0x43, 0x44]);
  const rom = (name, buffer) => ({ name, mimeType: 'application/octet-stream', buffer });
  await page.locator('#bios').setInputFiles(rom('original-interlace-probe.rom', main));
  await page.locator('#subrom').setInputFiles(rom('original-empty-sub.rom', sub));
  await page.locator('#apply-firmware').click();
}
