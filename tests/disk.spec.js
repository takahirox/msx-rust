import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
const demo = await readFile('dist/weave-disk-demo.dsk');
const sites = ['http://127.0.0.1:41873/', 'http://127.0.0.1:41874/msx-rust/'];
// Observe the real framebuffer glyphs, independently of status/transcript strings.
async function textOnScreen(page, text, row) {
  const glyphs = [...text].map(ch => [...demo.subarray(2048 + ch.charCodeAt(0)*8, 2056 + ch.charCodeAt(0)*8)]);
  await expect.poll(() => page.locator('#screen').evaluate((canvas, {glyphs,row}) => {
    const pixels = canvas.getContext('2d').getImageData(0,row*8,glyphs.length*8,8).data;
    return glyphs.every((glyph,col) => glyph.every((bits,y) => Array.from({length:8},(_,x) => {
      const i = (y*glyphs.length*8 + col*8+x)*4;
      const white = pixels[i] > 240 && pixels[i+1] > 240 && pixels[i+2] > 240;
      return white === Boolean(bits & (0x80 >> x));
    }).every(Boolean)));
  }, {glyphs,row}), {timeout:15000}).toBe(true);
}
async function exportDisk(page) {
  const download = page.waitForEvent('download');
  await page.locator('#export-disk').click();
  return readFile(await (await download).path());
}
async function space(page) {
  await page.locator('#screen').focus(); await page.keyboard.down('Space');
  await page.waitForTimeout(300); await page.keyboard.up('Space');
}
for (const site of sites) {
  for (const model of ['1','0']) {
    test(`real disk boot, input, protect, exact export and lifecycle MSX${Number(model)+1} ${site}`, async ({page}) => {
      const errors=[]; page.on('pageerror',e=>errors.push(e.message));
      const requests=[]; page.on('request',r=>requests.push({method:r.method(),url:r.url()}));
      await page.goto(site); await expect(page.locator('#transcript')).toContainText('READY');
      await expect(page.locator('#model')).toHaveValue('1');
      if (model==='0') await page.locator('#model').selectOption('0');
      const picker=page.waitForEvent('filechooser'); await page.locator('#open-dsk').click();
      await (await picker).setFiles('dist/weave-disk-demo.dsk');
      await expect(page.locator('#disk-status')).toContainText('720 KiB · read-only');
      await expect(page.locator('#transcript')).toContainText('READY');
      await page.locator('#boot-disk').click();
      await expect(page.locator('#status')).toContainText('Weave disk firmware');
      await textOnScreen(page,'WEAVE DISK BOOT',4);
      await textOnScreen(page,'Z80 LOADED FROM DRIVE A',7);
      await expect(page.locator('#transcript')).toHaveText('');
      await space(page); await textOnScreen(page,'WRITE PROTECTED',16);
      expect(await exportDisk(page)).toEqual(demo);
      await page.locator('#disk-protected').uncheck();
      await space(page); await textOnScreen(page,'SAVED',16);
      await expect(page.locator('#disk-status')).toContainText('unsaved modifications');
      await page.screenshot({path:`test-results/disk-msx${Number(model)+1}-${site.includes('41874')?'pages':'root'}.png`});
      // Modified media cannot be discarded by Eject or a replacement upload.
      await page.locator('#eject-disk').click(); await expect(page.locator('#status')).toContainText('Export modified DSK before ejecting');
      await page.locator('#disk').setInputFiles('dist/weave-disk-demo.dsk');
      await expect(page.locator('#status')).toContainText('before replacing');
      await page.locator('#reset').click(); await textOnScreen(page,'WEAVE DISK BOOT',4);
      await expect(page.locator('#disk-status')).toContainText('unsaved modifications');
      const expected=Buffer.from(demo); expected[9216]=1;
      expect(await exportDisk(page)).toEqual(expected);
      await page.locator('#eject-disk').click(); await expect(page.locator('#transcript')).toContainText('READY');
      await expect(page.locator('#disk-status')).toHaveText('Drive A: empty');
      await expect(page.locator('#model')).toHaveValue(model);
      expect(requests.every(r=>r.method==='GET' && r.url.startsWith(site))).toBe(true);
      expect(errors).toEqual([]);
    });
  }
  test(`failed DSKs and explicit/local firmware preserve session ${site}`,async({page})=>{
    await page.addInitScript(()=>{
      const instantiate=WebAssembly.instantiate;
      WebAssembly.instantiate=async(...args)=>{
        const result=await instantiate(...args), exports=result.instance.exports;
        window.diskWasm=exports;
        return {...result, instance:{exports:{...exports, msx_upload(n){return window.failDiskUpload ? 0 : exports.msx_upload(n);}}}};
      };
    });
    await page.goto(site); await expect(page.locator('#transcript')).toContainText('READY');
    await page.locator('#disk').setInputFiles('dist/weave-disk-demo.dsk');
    await page.locator('#screen').focus(); await page.keyboard.type('10 PRINT 73'); await page.keyboard.press('Enter');
    for (const [name,buffer] of [['bad.dsk',Buffer.alloc(17)],['short.dsk',demo.subarray(0,-1)],['empty.dsk',Buffer.alloc(0)],['bad.fdi',demo]]) {
      await page.locator('#disk').setInputFiles({name,mimeType:'application/octet-stream',buffer});
      await expect(page.locator('#status')).toContainText('Session and previous disk unchanged');
      await expect(page.locator('#disk-status')).toContainText('weave-disk-demo.dsk');
    }
    await page.evaluate(()=>window.failDiskUpload=true);
    await page.locator('#disk').setInputFiles('dist/weave-disk-demo.dsk');
    await expect(page.locator('#status')).toContainText('Session and previous disk unchanged');
    await page.evaluate(()=>window.failDiskUpload=false);
    const raw=await page.evaluate(()=>{
      const w=window.diskWasm, n=w.msx_disk_len();
      w.msx_upload(n);
      const wrongLength=w.msx_mount_disk(n-1,0);
      w.msx_upload(n); w.msx_cancel_upload();
      return [wrongLength,w.msx_mount_disk(n,0),w.msx_disk_len(),w.msx_disk_flags()];
    });
    expect(raw).toEqual([0,0,737280,3]);
    await page.locator('#screen').focus(); await page.keyboard.type('RUN'); await page.keyboard.press('Enter');
    await expect(page.locator('#transcript')).toContainText('73\nREADY');
    await page.locator('#advanced-panel > summary').click(); await page.locator('#firmware').selectOption('cbios');
    await page.locator('#boot-disk').click(); await expect(page.locator('#status')).toContainText('explicit firmware override');
    await expect(page.locator('#firmware')).toHaveValue('cbios');
    await page.locator('#bios').setInputFiles('dist/cbios_main_msx2.rom');
    await page.locator('#subrom').setInputFiles('dist/cbios_sub.rom'); await page.locator('#apply-firmware').click();
    await page.locator('#boot-disk').click(); await expect(page.locator('#status')).toContainText('explicit firmware override');
    await expect(page.locator('#firmware')).toHaveValue('local');
    await page.locator('#builtin').click(); await page.locator('#boot-disk').click(); await textOnScreen(page,'WEAVE DISK BOOT',4);
    // A cartridge deliberately takes priority, retaining mounted media and normal C-BIOS policy.
    await page.locator('#cartridge').setInputFiles('dist/cbios-test-cartridge.rom');
    await expect(page.locator('#status')).toContainText('MSX2 C-BIOS'); await expect(page.locator('#disk-status')).toContainText('720 KiB');
    await page.locator('#boot-disk').click(); await textOnScreen(page,'WEAVE DISK BOOT',4);
    await page.locator('#builtin').click(); await expect(page.locator('#transcript')).toContainText('READY');
    await expect(page.locator('#disk-status')).toContainText('720 KiB');
    await page.locator('#model').selectOption('0'); await expect(page.locator('#disk-status')).toContainText('720 KiB');
    await expect(page.locator('#transcript')).toContainText('READY');
    // A standard 360 KiB raw image can be mounted and exported without reinterpretation.
    await page.locator('#disk').setInputFiles({name:'single-sided.dsk',mimeType:'application/octet-stream',buffer:Buffer.alloc(368640,0x5a)});
    await expect(page.locator('#disk-status')).toContainText('360 KiB'); expect(await exportDisk(page)).toEqual(Buffer.alloc(368640,0x5a));
    await page.locator('#boot-disk').click(); await expect(page.locator('#disk-status')).toContainText('requires a WV boot sector');
    await page.locator('#eject-disk').click(); await expect(page.locator('#transcript')).toContainText('READY');
  });
  test(`stale and concurrent disk imports cannot replace the session ${site}`,async({page})=>{
    await page.addInitScript(()=>{
      const read=File.prototype.arrayBuffer;
      File.prototype.arrayBuffer=async function(){
        if(window.deferDisk && this.name==='deferred.dsk') { await new Promise(resolve=>window.finishDisk=resolve); }
        return read.call(this);
      };
    });
    await page.goto(site); await expect(page.locator('#transcript')).toContainText('READY');
    await page.locator('#disk').setInputFiles('dist/weave-disk-demo.dsk');
    await page.evaluate(()=>window.deferDisk=true);
    await page.locator('#disk').setInputFiles({name:'deferred.dsk',mimeType:'application/octet-stream',buffer:demo});
    await expect.poll(()=>page.evaluate(()=>typeof window.finishDisk)).toBe('function');
    await expect(page.locator('#open-rom')).toBeDisabled();
    await page.locator('#cartridge').setInputFiles('dist/cbios-test-cartridge.rom');
    await page.locator('#eject-disk').click(); await page.evaluate(()=>window.finishDisk());
    await expect(page.locator('#disk-status')).toHaveText('Drive A: empty');
    await expect(page.locator('#transcript')).toContainText('READY');
    await expect(page.locator('#firmware')).toHaveValue('auto');
    await page.locator('#disk').setInputFiles('dist/weave-disk-demo.dsk');
    await page.locator('#boot-disk').click(); await textOnScreen(page,'WEAVE DISK BOOT',4);
  });
}
