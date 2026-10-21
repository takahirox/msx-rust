import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const sites = ['http://127.0.0.1:41873/', 'http://127.0.0.1:41874/msx-rust/'];
const names = {6:'ASCII8SRAM2',7:'ASCII8SRAM8',8:'ASCII16SRAM2',9:'ASCII16SRAM8',10:'R-Type'};
const file = (name, buffer) => ({ name, buffer, mimeType:'application/octet-stream' });
test.beforeEach(async ({page}) => {
  await page.addInitScript(() => {
    const instantiate = WebAssembly.instantiate;
    WebAssembly.instantiate = async (...args) => {
      const result = await instantiate(...args), exports = result.instance.exports;
      window.mapperWasm = exports;
      return {...result, instance:{exports:{...exports,
        msx_frame(){ if (!window.manualFrames) exports.msx_frame(); },
        msx_upload(n){ return window.failUpload ? 0 : exports.msx_upload(n); },
      }}};
    };
  });
});
async function start(page, site, model='1') {
  await page.goto(site);
  await expect(page.locator('#transcript')).toContainText('READY');
  await expect(page.locator('#model')).toHaveValue('1');
  if (model==='0') await page.locator('#model').selectOption('0');
  await page.locator('#advanced-panel').evaluate(el=>{el.open=true;});
  await page.evaluate(()=>{window.manualFrames=true;});
}
async function advance(page, n=2) {
  await page.evaluate(async n=>{
    for(let i=0;i<n;i++) window.mapperWasm.msx_frame();
    await new Promise(requestAnimationFrame); await new Promise(requestAnimationFrame);
  },n);
}
async function sram(page) {
  return page.evaluate(()=>Array.from(new Uint8Array(window.mapperWasm.memory.buffer,window.mapperWasm.msx_sram(),window.mapperWasm.msx_sram_len())));
}
async function pixel(page) {
  return page.locator('#screen').evaluate(c=>Array.from(c.getContext('2d').getImageData(0,0,1,1).data).slice(0,3));
}
async function open(page,id,rom=`dist/mapper-${id}.rom`) {
  await page.locator('#mapper').selectOption(String(id));
  await page.locator('#cartridge').setInputFiles(rom);
  await expect(page.locator('#status')).toContainText(`(${names[id]})`);
}
async function boot(page, value=4) {
  await advance(page,600);
  expect((await sram(page))[0]).toBe(value);
}
async function increment(page) {
  await page.locator('#screen').focus();await page.keyboard.down('Space');await advance(page);
  await page.keyboard.up('Space');await advance(page);
}
async function backup(page) {
  const downloadPromise=page.waitForEvent('download');
  await page.locator('#export-save').click();
  const download=await downloadPromise;
  return JSON.parse(await readFile(await download.path(),'utf8'));
}
async function importSave(page, save) {
  await page.locator('#import-save').setInputFiles(file('backup.json',Buffer.from(typeof save==='string'?save:JSON.stringify(save))));
}
for (const site of sites) {
  for (const id of [6,7,8,9]) {
    test(`${names[id]} real Z80 SRAM, reload, identity isolation and invalid saves at ${site}`,async({page})=>{
      test.setTimeout(90000);
      const errors=[],requests=[];
      page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>requests.push(r));
      await start(page,site);
      await open(page,id);await boot(page);
      expect(await pixel(page)).toEqual([36,36,255]);
      await increment(page);expect((await sram(page))[0]).toBe(5);
      const savedPixel=await pixel(page);expect(savedPixel).toEqual([72,109,255]);
      const save=await backup(page),bytes=await readFile(`dist/mapper-${id}.rom`);
      expect(save.rom).toBe(createHash('sha256').update(bytes).digest('hex'));
      expect(save.mapper).toBe(id);expect(save.size).toBe(id%2===0?2048:8192);
      const key=`msx-rust:sram:v1:${save.rom}:${id}`;
      // External raw-ABI callers get atomic exact-length validation too.
      const raw = await page.evaluate(() => {
        const w=window.mapperWasm, n=w.msx_sram_len();
        const pointer=w.msx_upload(n-1);
        new Uint8Array(w.memory.buffer,pointer>>>0,n-1).fill(0x66);
        const wrongSize=w.msx_restore_sram(n-1);
        w.msx_upload(n);
        const wrongTransfer=w.msx_restore_sram(n+1);
        const valid=w.msx_upload(n);
        new Uint8Array(w.memory.buffer,valid>>>0,n).set(new Uint8Array(w.memory.buffer,w.msx_sram()>>>0,n));
        return [wrongSize,wrongTransfer,w.msx_restore_sram(n)];
      });
      expect(raw).toEqual([0,0,1]);expect((await sram(page))[0]).toBe(5);
      await expect.poll(()=>page.evaluate(k=>JSON.parse(localStorage.getItem(k)).data,key)).toBe(save.data);
      await page.locator('#reset').click();await boot(page,5);expect(await pixel(page)).toEqual(savedPixel);
      for(const bad of [file('bad-header.rom',Buffer.alloc(bytes.length)),file('partial.rom',bytes.subarray(0,-1)),file('overlarge.rom',Buffer.alloc(2*1024*1024))]) {
        await page.locator('#cartridge').setInputFiles(bad);
        await expect(page.locator('#status')).toContainText('ROM import failed');expect((await sram(page))[0]).toBe(5);
      }
      await page.evaluate(()=>{window.failUpload=true;});
      await page.locator('#cartridge').setInputFiles(`dist/mapper-${id}.rom`);
      await expect(page.locator('#status')).toContainText('unchanged');expect((await sram(page))[0]).toBe(5);
      await page.evaluate(()=>{window.failUpload=false;});
      for(const bad of ['{', {...save,version:2},{...save,rom:'0'.repeat(64)},{...save,mapper:99},{...save,size:1},{...save,checksum:'00000000'}, {...save,data:save.data.slice(4)},'x'.repeat(17000)]) {
        await importSave(page,bad);await expect(page.locator('#save-status')).toContainText('rejected');
        expect((await sram(page))[0]).toBe(5);
      }
      await start(page,site);await open(page,id);await expect(page.locator('#save-status')).toContainText('restored');await boot(page,5);
      expect(await pixel(page)).toEqual(savedPixel);
      // Same filename, different local ROM bytes: separate identity and new SRAM.
      const other=Buffer.from(bytes);other[other.length-1]^=1;
      await open(page,id,file(`mapper-${id}.rom`,other));await boot(page,4);
      await open(page,id,file('renamed.rom',bytes));await boot(page,5);
      // Mapper is part of the key even with identical ROM bytes.
      await open(page,id%2===0?id+1:id-1,file('renamed.rom',bytes));
      expect((await sram(page)).every(b=>b===255)).toBe(true);
      await open(page,id,file('renamed.rom',bytes));await boot(page,5);
      // A corrupt durable record is rejected, left intact, and never silently overwritten.
      const corrupt=JSON.stringify({...save,checksum:'broken'});
      await page.evaluate(([key,text])=>localStorage.setItem(key,text),[key,corrupt]);
      await start(page,site);await open(page,id);await expect(page.locator('#save-status')).toContainText('restore unavailable');await boot(page,4);
      await page.waitForTimeout(1100);
      expect(await page.evaluate(k=>localStorage.getItem(k),key)).toBe(corrupt);
      await importSave(page,save);await expect(page.locator('#save-status')).toContainText('Imported');
      await page.locator('#reset').click();await boot(page,5);
      await page.screenshot({path:`test-results/sram-${id}-${site.includes('msx-rust')?'pages':'root'}.png`,fullPage:true});
      await page.locator('#eject').click();await expect(page.locator('#export-save')).toBeDisabled();
      await expect(page.locator('#status')).toContainText('Weave BASIC');
      expect(requests.every(r=>r.method()==='GET'&&r.url().startsWith(site))).toBe(true);
      expect(errors).toEqual([]);
    });
  }
  test(`R-Type chip layout and MSX1 SRAM manual choices at ${site}`,async({page})=>{
    test.setTimeout(60000);
    await start(page,site,'0');
    for (const id of [6,7,8,9]) {
      await open(page,id);await boot(page);expect(await pixel(page)).toEqual([84,85,237]);
      await increment(page);expect((await sram(page))[0]).toBe(5);
    }
    await open(page,10);await advance(page,600);
    expect(await pixel(page)).toEqual([212,193,84]);expect(await sram(page)).toEqual([]);
    await expect(page.locator('#export-save')).toBeDisabled();
    await page.locator('#reset').click();await advance(page,600);expect(await pixel(page)).toEqual([212,193,84]);
    await page.locator('#model').selectOption('1');
    await open(page,10);await advance(page,600);expect(await pixel(page)).toEqual([218,218,36]);
    const invalid=Buffer.alloc(512*1024);invalid.set([65,66]);
    await page.locator('#cartridge').setInputFiles(file('wrong-layout.rom',invalid));
    await expect(page.locator('#status')).toContainText('exactly 384 KiB');expect(await pixel(page)).toEqual([218,218,36]);
    await page.locator('#mapper').selectOption('0');await page.locator('#cartridge').setInputFiles('dist/mapper-10.rom');
    await expect(page.locator('#status')).toContainText('manual R-Type');expect(await pixel(page)).toEqual([218,218,36]);
  });
  test(`SRAM survives demo, firmware and model transitions; stale save reads are ignored at ${site}`,async({page})=>{
    test.setTimeout(60000);
    await start(page,site);await open(page,7);await boot(page);await increment(page);
    const saved=await backup(page);
    // Replacement saves outgoing data before the demo's declared-model change.
    await expect(page.locator('#play-demo')).toBeEnabled();
    await page.locator('#demo').selectOption('plustaker');await page.locator('#play-demo').click();
    await expect(page.locator('#status')).toContainText('plustaker (Plain)');
    await expect(page.locator('#export-save')).toBeDisabled();
    await open(page,7);await boot(page,5);
    await page.locator('#firmware').selectOption('cbios');await expect(page.locator('#export-save')).toBeDisabled();
    await open(page,7);await boot(page,5);
    // Local BIOS imports reset while retaining the active cartridge and SRAM.
    await page.locator('#bios').setInputFiles('dist/cbios_main_msx1.rom');
    await expect(page.locator('#status')).toContainText('user BIOS');await boot(page,5);
    await page.locator('#model').selectOption('1');await expect(page.locator('#export-save')).toBeDisabled();
    await open(page,7);await boot(page,5);
    await page.evaluate(()=>{
      const text=File.prototype.text;
      File.prototype.text=function(){
        if(this.name==='slow.json') return new Promise(resolve=>{window.resolveSave=resolve;});
        return text.call(this);
      };
    });
    await page.locator('#import-save').setInputFiles(file('slow.json',Buffer.from(JSON.stringify(saved))));
    await expect.poll(()=>page.evaluate(()=>!!window.resolveSave)).toBe(true);
    await open(page,9);
    await page.evaluate(text=>window.resolveSave(text),JSON.stringify(saved));
    await boot(page,4);expect((await sram(page))[0]).toBe(4);
    await page.locator('#builtin').click();await open(page,7);await boot(page,5);
  });
  test(`storage permissions/quota failures and cross-tab conflicts preserve execution at ${site}`,async({page})=>{
    test.setTimeout(60000);
    await start(page,site);
    await page.evaluate(()=>{window.storageGet=Storage.prototype.getItem;Storage.prototype.getItem=()=>{throw new Error('Storage permission denied');};});
    await open(page,6);await boot(page);await expect(page.locator('#save-status')).toContainText('permission denied');
    await increment(page);expect((await sram(page))[0]).toBe(5);
    const backupSave=await backup(page);
    await page.evaluate(()=>{Storage.prototype.getItem=window.storageGet;window.storageSet=Storage.prototype.setItem;Storage.prototype.setItem=()=>{throw new Error('Quota exceeded');};});
    await importSave(page,backupSave);await expect(page.locator('#save-status')).toContainText('Quota exceeded');
    expect((await sram(page))[0]).toBe(5);
    // An identical reimport retains unsaved SRAM even without usable persistence.
    await open(page,6);await boot(page,5);
    await page.evaluate(()=>{Storage.prototype.setItem=window.storageSet;});
    await importSave(page,backupSave);await expect(page.locator('#save-status')).toContainText('Imported');
    const key=`msx-rust:sram:v1:${backupSave.rom}:6`;
    const conflicting=JSON.stringify({...backupSave,otherTab:true});
    await page.evaluate(([key,text])=>localStorage.setItem(key,text),[key,conflicting]);
    await increment(page);expect((await sram(page))[0]).toBe(6);
    await expect(page.locator('#save-status')).toContainText('Another tab');
    expect(await page.evaluate(k=>localStorage.getItem(k),key)).toBe(conflicting);
    const exported=await backup(page);expect(Buffer.from(exported.data,'base64')[0]).toBe(6);
    await page.locator('#builtin').click();await expect(page.locator('#export-save')).toBeDisabled();
  });
}
