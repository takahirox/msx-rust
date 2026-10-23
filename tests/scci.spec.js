import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

// Independently authored MIT fixtures shared with native tests. Observe real
// compiled Wasm/AudioContext; no debug bus ABI, copyrighted ROMs or game assets.
async function fixture(name, size, fill = 0) {
  const bytes = Buffer.alloc(size, fill);
  for (const line of (await readFile(`tests/fixtures/philips-${name}.hex`, 'utf8')).split('\n')) {
    if (!line || line.startsWith('#')) continue;
    const [offset, data] = line.split(':');
    Buffer.from(data.trim().split(/\s+/).map(v => parseInt(v, 16))).copy(bytes, parseInt(offset, 16));
  }
  return bytes;
}
const system = await fixture('system', 32768);
const diskRom = await fixture('disk', 16384, 255);
const file = (name, buffer) => ({name, buffer, mimeType: 'application/octet-stream'});
const sites = ['http://127.0.0.1:41873/', 'http://127.0.0.1:41874/msx-rust/'];
const names = {1: 'Snatcher', 2: 'SD Snatcher'};
async function green(page, model) {
  await expect.poll(() => page.locator('#screen').evaluate(c => [...c.getContext('2d').getImageData(20,20,1,1).data]))
    .toEqual(model ? [36,218,36,255] : [33,200,66,255]);
}
async function install(page, profile) {
  await page.locator('#sound-cartridge').selectOption(String(profile));
  await page.locator('#install-sound-cartridge').click();
  await expect(page.locator('#sound-cartridge-status')).toHaveText(`${names[profile]} · 64 KiB RAM · SCC-I · slot 1`);
  await expect(page.locator('#status')).toContainText(`${names[profile]} sound cartridge`);
}
async function exported(page) {
  const promise = page.waitForEvent('download'); await page.locator('#export-disk').click();
  return readFile(await (await promise).path());
}
async function state(page) {
  return page.evaluate(() => {
    const w=window.scciWasm;
    return [w.msx_sound_cartridge_profile(), w.msx_firmware(), w.msx_boot_policy(), w.msx_disk_profile(), w.msx_disk_len(), w.msx_disk_flags()];
  });
}
for (const site of sites) for (const model of [0,1]) for (const profile of [1,2]) {
  test(`SCC-I ${names[profile]} CPU disk/RAM/audio and transactional lifecycle MSX${model+1} ${site}`, async ({page}) => {
    test.setTimeout(60000);
    await page.addInitScript(() => {
      const instantiate = WebAssembly.instantiate;
      WebAssembly.instantiate = async (...args) => {
        const result=await instantiate(...args), w=result.instance.exports;
        window.scciWasm=w;
        return {...result, instance: {exports: {...w,
          msx_install_sound_cartridge(profile) {return window.failScci ? 0 : w.msx_install_sound_cartridge(profile);},
        }}};
      };
      const read=File.prototype.arrayBuffer;
      File.prototype.arrayBuffer=async function () {
        if (this.name==='deferred.rom') await new Promise(resolve => window.finishScciImport=resolve);
        return read.call(this);
      };
      const NativeContext=window.AudioContext;
      window.scciScheduled=[];
      window.AudioContext=class extends NativeContext {
        constructor(...args) {
          super(...args); const create=this.createBufferSource.bind(this);
          this.createBufferSource=()=>{
            const source=create(), start=source.start.bind(source);
            source.start=(...args)=>{
              const pcm=source.buffer.getChannelData(0);
              window.scciScheduled.push({peak:Math.max(...pcm.map(Math.abs)),square:pcm.every(v=>Math.abs(Math.abs(v)-0.045)<1e-6)});
              if (window.scciScheduled.length>120) window.scciScheduled.shift();
              return start(...args);
            };
            return source;
          };
        }
      };
    });
    const requests=[], errors=[];
    page.on('request',r=>requests.push(r)); page.on('pageerror',e=>errors.push(e.message));
    await page.goto(site); await expect(page.locator('#transcript')).toContainText('READY');
    if (!model) await page.locator('#model').selectOption('0');
    await page.locator('#advanced-panel > summary').click();
    // Replacing ordinary SRAM hardware runs the existing outgoing save safeguard.
    await page.locator('#mapper').selectOption('6');
    await page.locator('#cartridge').setInputFiles('dist/mapper-6.rom');
    await expect(page.locator('#export-save')).toBeEnabled();
    await page.evaluate(()=>{const w=window.scciWasm;new Uint8Array(w.memory.buffer,w.msx_sram(),w.msx_sram_len()).fill(0x79);});
    await install(page,profile);
    expect((await state(page)).slice(0,3)).toEqual([profile,1,0]); // automatic C-BIOS retained
    expect(await page.evaluate(()=>Object.keys(localStorage).filter(k=>k.startsWith('msx-rust:sram:v1:')).map(k=>atob(JSON.parse(localStorage[k]).data).charCodeAt(2047)))).toEqual([0x79]);
    await expect(page.locator('#export-save')).toBeDisabled();
    // External firmware application retains the accessory, with no network I/O.
    const main=Buffer.from(system);main[0x2d]=model;
    await page.locator('#bios').setInputFiles(file('original-system.rom',main));
    if (model) {
      const sub=Buffer.alloc(16384);sub.write('CD');sub[0x100]=0x7b;
      await page.locator('#subrom').setInputFiles(file('original-sub.rom',sub));
      await page.locator('#apply-firmware').click();
    }
    await expect(page.locator('#firmware')).toHaveValue('local');
    expect((await state(page)).slice(0,3)).toEqual([profile,3,1]);
    const disk=await fixture('scci-sector',368640,0x35); disk[510]=profile===1 ? 0 : 8;
    const expected=Buffer.from(disk);expected.fill(0x6c,8*512,9*512);
    await page.locator('#disk').setInputFiles(file('original-scci.dsk',disk));
    await expect(page.locator('#disk-status')).toContainText('original-scci.dsk');
    await page.locator('#disk-protected').uncheck();
    await page.locator('#disk-rom').setInputFiles(file('original-disk.rom',diskRom));
    await green(page,model); // CPU checked RAM, fifth readbacks, disk write; red on failure
    await expect(page.locator('#disk-status')).toContainText('unsaved modifications');
    expect(await state(page)).toEqual([profile,3,1,1,368640,5]);
    // Independently expected signed DAC and 32*200-cycle pitch of voice 5.
    const pcm=await page.evaluate(()=>{
      const w=window.scciWasm, result=[];
      for(let i=0;i<6;i++){w.msx_frame();result.push(...new Float32Array(w.memory.buffer,w.msx_audio(),w.msx_audio_len()));}
      return result;
    });
    expect(pcm.every(v=>Math.abs(Math.abs(v)-0.045)<1e-6)).toBe(true);
    const rises=pcm.flatMap((v,i)=>i && pcm[i-1]<0 && v>0 ? [i] : []);
    expect(rises.length).toBeGreaterThan(40);
    for(let i=1;i<rises.length;i++) expect(Math.abs(rises[i]-rises[0]-i*32*200*44100/3579545)).toBeLessThan(1);
    await page.locator('#audio').click();
    await expect.poll(()=>page.evaluate(()=>window.scciScheduled.filter(r=>r.square).length)).toBeGreaterThan(5);
    await page.locator('#pause').click();
    const count=await page.evaluate(()=>window.scciScheduled.length);
    await page.waitForTimeout(150);
    expect(await page.evaluate(()=>window.scciScheduled.length)).toBe(count);
    await page.locator('#pause').click(); await page.locator('#audio').click();
    await expect(page.locator('#audio-state')).toHaveText('Audio off · enable to hear sound');
    // Failed requests preserve running sound, profile, firmware and modified media.
    const before=await state(page);
    await page.locator('#cartridge').setInputFiles(file('invalid.rom',Buffer.alloc(32768)));
    await expect(page.locator('#status')).toContainText('ROM import failed');
    expect(await state(page)).toEqual(before);
    expect(await page.evaluate(()=>window.scciWasm.msx_install_sound_cartridge(99))).toBe(0);
    await page.evaluate(()=>{window.failScci=true;});
    await page.locator('#install-sound-cartridge').click();
    expect(await state(page)).toEqual(before);
    await page.evaluate(()=>{window.failScci=false;});
    await page.locator('#cartridge').setInputFiles(file('deferred.rom',await readFile('dist/test-cartridge.rom')));
    await expect(page.locator('#remove-sound-cartridge')).toHaveText('Cancel import');
    await page.locator('#remove-sound-cartridge').click();
    await page.evaluate(()=>window.finishScciImport());
    await page.waitForTimeout(100); expect(await state(page)).toEqual(before);
    await green(page,model);
    for (const button of ['reset','boot-disk']) {
      await page.locator(`#${button}`).click(); await green(page,model);
      expect(await state(page)).toEqual(before);
    }
    // Replacement/removal retain the selected local firmware and exact dirty disk.
    await install(page,profile===1 ? 2 : 1);
    expect((await state(page)).slice(1)).toEqual(before.slice(1));
    await page.locator('#remove-sound-cartridge').click();
    expect(await state(page)).toEqual([0,...before.slice(1)]);
    await expect(page.locator('#sound-cartridge-status')).toHaveText('No sound cartridge installed');
    expect(await exported(page)).toEqual(expected);
    // Ordinary ROM replaces the accessory; bundled/model clearing remains explicit.
    await install(page,profile);
    await page.locator('#mapper').selectOption('1');
    await page.locator('#cartridge').setInputFiles('dist/test-cartridge.rom');
    await expect(page.locator('#status')).toContainText('test-cartridge.rom (Plain)');
    expect((await state(page))[0]).toBe(0);
    await install(page,profile); await page.locator('#builtin').click();
    expect((await state(page))[0]).toBe(0);
    await install(page,profile); await page.locator('#firmware').selectOption('cbios');
    expect((await state(page))[0]).toBe(0);
    await install(page,profile); await page.locator('#model').selectOption(String(1-model));
    expect((await state(page))[0]).toBe(0);
    expect((await state(page))[3]).toBe(1);
    expect(errors).toEqual([]);
    expect(requests.some(r=>r.method()!=='GET' || !r.url().startsWith(site))).toBe(false);
    expect(await page.evaluate(()=>Object.keys(localStorage).filter(k=>!k.startsWith('msx-rust:sram:v1:')))).toEqual([]);
  });
}
