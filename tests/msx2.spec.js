import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

async function boot(page, path = '/') {
  await page.goto(path);
  await expect(page.locator('#status')).toHaveText('Running · Weave BASIC');
  await page.locator('#model').selectOption('1');
  await expect(page.locator('#status')).toContainText('MSX2 · Original cartridge loader');
}
async function pixel(page, x, y) {
  return page.locator('#screen').evaluate((canvas, [x,y]) => Array.from(canvas.getContext('2d').getImageData(x,y,1,1).data).slice(0,3), [x,y]);
}
async function fixture(page) {
  await page.locator('#cartridge').setInputFiles('dist/msx2-test-cartridge.rom');
  await expect(page.locator('#screen')).toHaveAttribute('width','512');
  await expect(page.locator('#screen')).toHaveAttribute('height','212');
  await expect.poll(() => pixel(page,0,0)).toEqual([255,0,0]);
  await expect.poll(() => pixel(page,10,10)).toEqual([0,255,255]);
  await expect.poll(() => pixel(page,70,32)).toEqual([255,255,255]);
}
test('MSX2 Z80 cartridge draws extended video, uses input/audio, resets and returns to MSX1 locally', async ({page}) => {
  const errors=[], requests=[];
  page.on('pageerror',e=>errors.push(e.message));
  page.on('request',r=>requests.push({method:r.method(),url:r.url(),body:r.postData()}));
  await page.addInitScript(() => {
    window.msx2Audio = [];
    const Native = window.AudioContext;
    window.AudioContext = class extends Native {
      createBufferSource() {
        const source = super.createBufferSource(), start = source.start.bind(source);
        source.start = (...args) => {
          const data = source.buffer.getChannelData(0);
          window.msx2Audio.push(Math.max(...data.map(Math.abs)));
          return start(...args);
        };
        return source;
      }
    };
  });
  await boot(page);await fixture(page);
  await expect(page.locator('#example')).toBeDisabled();
  await page.screenshot({path:'test-results/msx2-screen7.png',fullPage:true});
  await page.getByRole('button',{name:'Enable audio',exact:true}).click();
  await page.keyboard.down('Space');
  await expect.poll(()=>page.evaluate(()=>Math.max(0,...window.msx2Audio))).toBeGreaterThan(0.05);
  await page.keyboard.up('Space');
  await expect.poll(()=>page.evaluate(()=>window.msx2Audio.slice(-5).every(v=>v<0.0001))).toBe(true);
  await page.keyboard.down('8');
  await expect(page.locator('#screen')).toHaveAttribute('width','256');
  await page.keyboard.up('8');
  await expect.poll(()=>pixel(page,0,211)).toEqual([255,0,0]);
  await expect.poll(()=>pixel(page,10,10)).toEqual([0,255,0]);
  await expect.poll(()=>page.locator('#screen').evaluate(canvas=>{
    const data=canvas.getContext('2d').getImageData(0,160,256,1).data, colors=new Set();
    for(let x=0;x<256;x++) colors.add(`${data[x*4]},${data[x*4+1]},${data[x*4+2]}`);
    return colors.size;
  })).toBe(256);
  await expect.poll(()=>pixel(page,35,32)).toEqual([255,255,255]);
  await page.screenshot({path:'test-results/msx2-screen8.png',fullPage:true});
  await page.keyboard.down('7');await expect(page.locator('#screen')).toHaveAttribute('width','512');await page.keyboard.up('7');
  await page.locator('#reset').click();await expect.poll(()=>pixel(page,10,10)).toEqual([0,255,255]);
  await page.locator('#cartridge').setInputFiles({name:'invalid.rom',mimeType:'application/octet-stream',buffer:Buffer.alloc(17)});
  await expect(page.locator('#status')).toContainText('unchanged');await expect.poll(()=>pixel(page,10,10)).toEqual([0,255,255]);
  await page.locator('#model').selectOption('0');await expect(page.locator('#status')).toHaveText('Running · Weave BASIC');
  await expect(page.locator('#screen')).toHaveAttribute('width','256');await expect(page.locator('#screen')).toHaveAttribute('height','192');
  await expect(page.locator('#transcript')).toContainText('READY');
  expect(requests.filter(r=>r.method!=='GET'||r.body||!r.url.startsWith('http://127.0.0.1:41873/'))).toEqual([]);
  expect(errors).toEqual([]);
});
test('MSX2 firmware imports are atomic, paired, validated and reversible', async ({page}) => {
  const requests=[];page.on('request',r=>requests.push({method:r.method(),body:r.postData()}));
  await boot(page);await fixture(page);
  await page.locator('#advanced-panel > summary').click();
  await page.locator('#apply-firmware').click();await expect(page.locator('#status')).toContainText('Select both');
  await page.locator('#bios').setInputFiles('dist/weave-msx2-boot.rom');
  await page.locator('#subrom').setInputFiles('dist/msx2-diagnostic-sub.rom');
  await expect(page.locator('#firmware-selection')).toContainText('weave-msx2-boot.rom');
  await page.locator('#apply-firmware').click();
  await expect(page.locator('#status')).toContainText('weave-msx2-boot.rom + msx2-diagnostic-sub.rom');
  await expect.poll(()=>pixel(page,10,10)).toEqual([0,255,255]);
  await page.locator('#reset').click();await expect(page.locator('#status')).toContainText('msx2-diagnostic-sub.rom');
  await expect(page.locator('#transcript')).toHaveText('');
  await page.locator('#subrom').setInputFiles({name:'bad-sub.rom',mimeType:'application/octet-stream',buffer:Buffer.alloc(16384)});
  await page.locator('#apply-firmware').click();await expect(page.locator('#status')).toContainText('CD-header');
  await expect.poll(()=>pixel(page,10,10)).toEqual([0,255,255]);
  await page.locator('#subrom').setInputFiles({name:'short-sub.rom',mimeType:'application/octet-stream',buffer:Buffer.alloc(7)});
  await page.locator('#apply-firmware').click();await expect(page.locator('#status')).toContainText('16 KiB');
  await page.locator('#builtin').click();await expect(page.locator('#status')).toContainText('Original cartridge loader');
  await expect(page.locator('#eject')).toBeDisabled();
  await expect(page.locator('#bios')).toHaveValue('');await expect(page.locator('#subrom')).toHaveValue('');
  await page.locator('#model').selectOption('0');await expect(page.locator('#transcript')).toContainText('READY');
  expect(requests.filter(r=>r.method!=='GET'||r.body)).toEqual([]);
});
test('MSX2 boots from the GitHub Pages project subpath with relative fixture links',async({page})=>{
  await boot(page,'http://127.0.0.1:41874/msx-rust/');await fixture(page);
  for(const file of ['msx2-test-cartridge.rom','weave-msx2-boot.rom','msx2-diagnostic-sub.rom']) expect((await page.request.get(`http://127.0.0.1:41874/msx-rust/${file}`)).ok()).toBe(true);
  await page.setViewportSize({width:320,height:720});
  expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(320);
  const box=await page.locator('#screen').boundingBox();expect(box.width).toBeLessThan(320);
});
if(process.env.MSX2_TEST_MAIN && process.env.MSX2_TEST_SUB) {
  test('local C-BIOS MSX2 pair boots the MSX2 diagnostic cartridge',async({page})=>{
    await boot(page);await page.locator('#advanced-panel > summary').click();
    await page.locator('#bios').setInputFiles(process.env.MSX2_TEST_MAIN);
    await page.locator('#subrom').setInputFiles(process.env.MSX2_TEST_SUB);
    await page.locator('#apply-firmware').click();
    await expect(page.locator('#status')).toContainText('cbios_sub.rom');
    await expect(page.locator('#open-rom')).toBeEnabled();
    // Original preamble exercises the external BIOS RDSLT/WRSLT entry points
    // against the expanded sub-ROM and RAM, then enters our V9938 diagnostic.
    const rom = await readFile('dist/msx2-test-cartridge.rom');
    rom[2]=0x00;rom[3]=0x5f;
    const preamble=[0xf3,0x31,0x00,0xf0,
      0x3e,0x83,0x21,0x00,0x00,0xcd,0x0c,0x00,0xfe,0x43,0xc2,0x40,0x5f,
      0x3e,0x8f,0x21,0x00,0xc2,0x1e,0x5a,0xcd,0x14,0x00,
      0x3e,0x8f,0x21,0x00,0xc2,0xcd,0x0c,0x00,0xfe,0x5a,0xc2,0x40,0x5f,
      0xc3,0x10,0x40];
    rom.set(preamble,0x1f00);rom.set([0x76,0xc3,0x40,0x5f],0x1f40);
    await page.locator('#cartridge').setInputFiles({name:'msx2-test-cartridge.rom',mimeType:'application/octet-stream',buffer:rom});
    await expect(page.locator('#status')).toContainText('msx2-test-cartridge.rom');
    await expect.poll(()=>pixel(page,10,10),{timeout:20000}).toEqual([0,255,255]);
    await expect(page.locator('#screen')).toHaveAttribute('width','512');
    await page.screenshot({path:'test-results/msx2-cbios.png',fullPage:true});
  });
}
