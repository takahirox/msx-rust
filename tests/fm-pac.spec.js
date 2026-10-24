import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
const sites=['/','http://127.0.0.1:41874/msx-rust/'];
const file=(name,buffer)=>({name,buffer,mimeType:'application/octet-stream'});
const firmware=await readFile('dist/fm-pac-diagnostic-firmware.rom');
async function hex(name,size,fill=0) {
  const bytes=Buffer.alloc(size,fill);
  for(const line of (await readFile(`tests/fixtures/${name}.hex`,'utf8')).split('\n')) {
    if(!line || line.startsWith('#'))continue;
    const [offset,data]=line.split(':');Buffer.from(data.trim().split(/\s+/).map(v=>parseInt(v,16))).copy(bytes,parseInt(offset,16));
  }
  return bytes;
}
async function observe(page) {
  await page.addInitScript(()=>{
    const instantiate=WebAssembly.instantiate;
    WebAssembly.instantiate=async(...args)=>{
      const result=await instantiate(...args),w=result.instance.exports;window.musicWasm=w;
      return {...result,instance:{exports:{...w,
        msx_frame(){if(!window.manualMusic)w.msx_frame();},
      }}};
    };
    const read=File.prototype.arrayBuffer;
    File.prototype.arrayBuffer=async function(){
      if(this.name==='deferred.rom')await new Promise(resolve=>window.finishMusic=resolve);
      if(this.name==='allocation.rom')throw new RangeError('Test file allocation failure');
      return read.call(this);
    };
    const NativeContext=window.AudioContext;
    window.musicScheduled=[];
    window.AudioContext=class extends NativeContext {
      constructor(...args){super(...args);window.musicAudio=this;const create=this.createBufferSource.bind(this);
        this.createBufferSource=()=>{const source=create(),start=source.start.bind(source);
          source.start=(...args)=>{const pcm=source.buffer.getChannelData(0);window.musicScheduled.push({rate:source.buffer.sampleRate,pcm:[...pcm]});if(window.musicScheduled.length>20)window.musicScheduled.shift();return start(...args);};return source;};
      }
    };
  });
}
async function boot(page,site) {
  await observe(page);await page.goto(site);await expect(page.locator('#transcript')).toContainText('READY');
  await page.locator('#advanced-panel > summary').click();
}
async function install(page) {
  const chooser=page.waitForEvent('filechooser');await page.locator('#open-pac-rom').click();
  await(await chooser).setFiles(file('diagnostic-pac.rom',firmware));
  await expect(page.locator('#pac-status')).toContainText('diagnostic-pac.rom');
  await expect(page.locator('#emulator')).toHaveAttribute('aria-busy','false');
}
async function advance(page,n) {return page.evaluate(n=>{const w=window.musicWasm;for(let i=0;i<n;i++)w.msx_frame();return [...new Float32Array(w.memory.buffer,w.msx_audio(),w.msx_audio_len())];},n);}
async function green(page,model=1) {await expect.poll(()=>page.locator('#screen').evaluate(c=>[...c.getContext('2d').getImageData(20,20,1,1).data])).toEqual(model ? [36,218,36,255] : [33,200,66,255]);}

function expectSine(pcm, minimumCrossings=40) {
  const crossings=pcm.flatMap((v,i)=>i && pcm[i-1]<=0 && v>0 ? [i]:[]);
  expect(crossings.length).toBeGreaterThan(minimumCrossings);
  const period=44100*72*2**19/(290*16*3579545);
  for(let n=1;n<crossings.length;n++) expect(Math.abs(crossings[n]-crossings[0]-n*period)).toBeLessThan(2);
  const peak=Math.max(...pcm.map(Math.abs));expect(peak).toBeGreaterThan(0.015);expect(peak).toBeLessThan(0.016);
}
async function saved(page) {
  return page.evaluate(()=>{const w=window.musicWasm;return [...new Uint8Array(w.memory.buffer,w.msx_fm_pac_sram(),w.msx_fm_pac_sram_len())];});
}
async function backup(page) {
  const downloading=page.waitForEvent('download');await page.locator('#export-pac-save').click();
  return JSON.parse(await readFile(await(await downloading).path(),'utf8'));
}
const pattern=Array.from({length:8190},(_,i)=>(i*71+Math.floor(i/257))&255);
for(const site of sites) {
  for(const model of [0,1]) test(`FM-PAC Z80 banks/SRAM, independent pitch, scheduled PCM and lifecycle MSX${model+1} ${site}`,async({page})=>{
    await boot(page,site);await page.locator('#model').selectOption(String(model));await install(page);
    await page.locator('#mapper').selectOption('1');await page.locator('#cartridge').setInputFiles('dist/fm-pac-test-cartridge.rom');await green(page,model);
    await page.getByRole('button',{name:'Enable audio',exact:true}).click();
    await expect.poll(()=>page.evaluate(()=>window.musicScheduled.some(({rate,pcm})=>rate===44100 && pcm.some(v=>Math.abs(v)>0.015) && pcm.every(v=>Number.isFinite(v)&&Math.abs(v)<=1)))).toBe(true);
    const scheduled=await page.evaluate(()=>window.musicScheduled.find(({rate,pcm})=>rate===44100 && pcm.some(v=>Math.abs(v)>0.015)));
    expectSine(scheduled.pcm,4); // Independent pitch/gain on actual scheduled AudioBuffer.
    await page.evaluate(()=>window.manualMusic=true);
    const pcm=await page.evaluate(()=>{const w=window.musicWasm,pcm=[];for(let i=0;i<8;i++){w.msx_frame();pcm.push(...new Float32Array(w.memory.buffer,w.msx_audio(),w.msx_audio_len()));}return pcm;});expectSine(pcm);
    await page.getByRole('button',{name:'Mute audio',exact:true}).click();
    await page.locator('#reset').click();const muted=await advance(page,300);
    await page.getByRole('button',{name:'Enable audio',exact:true}).click();
    await page.locator('#reset').click();expect(await advance(page,300)).toEqual(muted);expect(muted.some(v=>Math.abs(v)>0.01)).toBe(true);
    await page.evaluate(()=>window.manualMusic=false);await page.locator('#pause').click();await page.waitForTimeout(50);
    const paused=await advance(page,0);await page.waitForTimeout(100);expect(await advance(page,0)).toEqual(paused);
    await page.locator('#pause').click();await green(page,model);
    await page.evaluate(()=>window.manualMusic=true);await page.locator('#remove-pac-rom').click();
    expect(await page.evaluate(()=>window.musicWasm.msx_fm_pac_profile())).toBe(0);
    expect((await advance(page,300)).every(v=>v===0)).toBe(true);
  });
  test(`FM-PAC separate shared save roundtrip/reload, exact validation, conflicts and recovery ${site}`,async({page})=>{
    await boot(page,site);await page.locator('#mapper').selectOption('6');await page.locator('#cartridge').setInputFiles('dist/mapper-6.rom');
    await install(page);
    await page.evaluate(pattern=>{const w=window.musicWasm;new Uint8Array(w.memory.buffer,w.msx_fm_pac_sram(),8190).set(pattern);new Uint8Array(w.memory.buffer,w.msx_sram(),w.msx_sram_len()).fill(0x79);},pattern);
    const container=await backup(page);expect(container.size).toBe(8190);expect(container.mapper).toBe('original-fm-pac');
    expect([...Buffer.from(container.data,'base64')]).toEqual(pattern);
    await page.locator('#reset').click();expect(await saved(page)).toEqual(pattern);
    await page.locator('#mapper').selectOption('1');await page.locator('#cartridge').setInputFiles('dist/test-cartridge.rom');expect(await saved(page)).toEqual(pattern);
    await page.locator('#model').selectOption('0');expect(await saved(page)).toEqual(pattern);
    await page.locator('#builtin').click();expect(await saved(page)).toEqual(pattern);
    // Exact checksum/version/firmware/profile/length validation before mutation.
    for(const patch of [{version:2},{rom:'0'.repeat(64)},{mapper:'1'},{size:8192},{checksum:'00000000'},{data:container.data.slice(4)}]) {
      await page.locator('#import-pac-save').setInputFiles(file('invalid.json',Buffer.from(JSON.stringify({...container,...patch}))));
      await expect(page.locator('#pac-save-status')).toContainText('Save import rejected');expect(await saved(page)).toEqual(pattern);
    }
    // Another writer changes the record: preserve it, retain live SRAM and export.
    await page.evaluate(()=>{const key=Object.keys(localStorage).find(k=>k.startsWith('msx-rust:fm-pac:v1:'));localStorage.setItem(key,'foreign-tab-record');const w=window.musicWasm;new Uint8Array(w.memory.buffer,w.msx_fm_pac_sram(),8190)[0]^=1;});
    const conflicted=await backup(page);await expect(page.locator('#pac-save-status')).toContainText('Another tab');
    expect(await page.evaluate(()=>localStorage.getItem(Object.keys(localStorage).find(k=>k.startsWith('msx-rust:fm-pac:v1:'))))).toBe('foreign-tab-record');
    await page.locator('#import-pac-save').setInputFiles(file('restore.json',Buffer.from(JSON.stringify(container))));await expect(page.locator('#pac-save-status')).toContainText('Imported SRAM');expect(await saved(page)).toEqual(pattern);
    expect(conflicted.data).not.toBe(container.data);
    await page.reload();await expect(page.locator('#transcript')).toContainText('READY');await page.locator('#advanced-panel > summary').click();await install(page);
    await expect(page.locator('#pac-save-status')).toContainText('restored');expect(await saved(page)).toEqual(pattern);
    // Cartridge SRAM namespace remains distinct, with its exact bytes intact.
    const records=await page.evaluate(()=>Object.keys(localStorage).map(k=>[k,JSON.parse(localStorage[k])]));
    expect(records.filter(([k])=>k.startsWith('msx-rust:sram:v1:'))).toHaveLength(1);
    const game=records.find(([k])=>k.startsWith('msx-rust:sram:v1:'))[1];expect([...Buffer.from(game.data,'base64')]).toEqual(Array(2048).fill(0x79));
    expect(records).toHaveLength(2);expect(records.every(([,v])=>!('firmware' in v))).toBe(true);
    // A different firmware digest gets its own accessory SRAM; a different filename
    // for the original bytes restores the original save independently of the game.
    const other=Buffer.from(firmware);other[0x1234]^=255;
    await page.locator('#pac-rom').setInputFiles(file('other-profile-firmware.rom',other));
    await expect(page.locator('#pac-status')).toContainText('other-profile-firmware.rom');
    await expect(page.locator('#emulator')).toHaveAttribute('aria-busy','false');
    expect(await saved(page)).toEqual(Array(8190).fill(255));
    expect((await backup(page)).rom).not.toBe(container.rom);
    await page.locator('#pac-rom').setInputFiles(file('renamed-original.rom',firmware));
    await expect(page.locator('#pac-status')).toContainText('renamed-original.rom');
    await expect(page.locator('#emulator')).toHaveAttribute('aria-busy','false');
    expect(await saved(page)).toEqual(pattern);

  });
  test(`FM-PAC failed/canceled/stale imports, corrupt saves and storage failure remain recoverable ${site}`,async({page})=>{
    await boot(page,site);await install(page);
    await page.evaluate(pattern=>{const w=window.musicWasm;new Uint8Array(w.memory.buffer,w.msx_fm_pac_sram(),8190).set(pattern);},pattern);
    const container=await backup(page);
    for(const [name,bytes] of [['short.rom',Buffer.alloc(65535)],['wrong.txt',firmware],['allocation.rom',firmware]]) {
      await page.locator('#pac-rom').setInputFiles(file(name,bytes));await expect(page.locator('#status')).toContainText('FM-PAC import failed');expect(await saved(page)).toEqual(pattern);
    }
    await page.locator('#pac-rom').setInputFiles(file('deferred.rom',firmware));await expect(page.locator('#remove-pac-rom')).toHaveText('Cancel import');await page.locator('#remove-pac-rom').click();await page.evaluate(()=>window.finishMusic());expect(await saved(page)).toEqual(pattern);
    await page.locator('#mapper').selectOption('1');await page.locator('#cartridge').setInputFiles('dist/test-cartridge.rom');await expect(page.locator('#emulator')).toHaveAttribute('aria-busy','false');
    await page.locator('#pac-rom').setInputFiles(file('deferred.rom',firmware));await page.locator('#eject').click();await page.evaluate(()=>window.finishMusic());expect(await saved(page)).toEqual(pattern);
    await page.evaluate(()=>{const key=Object.keys(localStorage).find(k=>k.startsWith('msx-rust:fm-pac:v1:'));localStorage[key]='corrupted';});
    await page.reload();await expect(page.locator('#transcript')).toContainText('READY');await page.locator('#advanced-panel > summary').click();await install(page);
    await expect(page.locator('#pac-save-status')).toContainText('restore unavailable');expect(await saved(page)).toEqual(Array(8190).fill(255));
    await page.locator('#import-pac-save').setInputFiles(file('recovery.json',Buffer.from(JSON.stringify(container))));await expect(page.locator('#pac-save-status')).toContainText('Imported SRAM');expect(await saved(page)).toEqual(pattern);
    await page.evaluate(()=>{Storage.prototype.setItem=()=>{throw new Error('Test quota exceeded');};const w=window.musicWasm;new Uint8Array(w.memory.buffer,w.msx_fm_pac_sram(),8190)[0]=0x37;});
    const live=await backup(page);await expect(page.locator('#pac-save-status')).toContainText('in memory only');expect(Buffer.from(live.data,'base64')[0]).toBe(0x37);
    await install(page);expect((await saved(page))[0]).toBe(0x37); // same firmware, unsaved RAM retained
    await page.locator('#remove-pac-rom').click();await expect(page.locator('#export-pac-save')).toBeDisabled();await install(page);
    await page.locator('#import-pac-save').setInputFiles(file('memory-recovery.json',Buffer.from(JSON.stringify(live))));await expect(page.locator('#pac-save-status')).toContainText('in memory only');expect((await saved(page))[0]).toBe(0x37);
  });
  test(`FM-PAC plus SCC-I/PSG and exact Philips disk transfers ${site}`,async({page})=>{
    await boot(page,site);await install(page);
    await page.locator('#sound-cartridge').selectOption('1');await page.locator('#install-sound-cartridge').click();
    const main=await hex('fm-pac-system',32768);main[0x2d]=1;
    const sub=Buffer.alloc(16384);sub.write('CD');sub[0x100]=0x7b;
    await page.locator('#bios').setInputFiles(file('original-main.rom',main));await page.locator('#subrom').setInputFiles(file('original-sub.rom',sub));await page.locator('#apply-firmware').click();await expect(page.locator('#firmware')).toHaveValue('local');await expect(page.locator('#emulator')).toHaveAttribute('aria-busy','false');
    await page.locator('#disk-rom').setInputFiles(file('original-disk.rom',await hex('music-disk',16384,255)));await expect(page.locator('#disk-profile-status')).toContainText('original-disk.rom');await expect(page.locator('#emulator')).toHaveAttribute('aria-busy','false');
    const disk=await hex('philips-scci-sector',368640,0x35);disk[510]=0;
    await page.locator('#disk').setInputFiles(file('original.dsk',disk));await expect(page.locator('#disk-status')).toContainText('original.dsk');await page.locator('#disk-protected').uncheck();await page.locator('#boot-disk').click();await green(page);
    await expect(page.locator('#disk-status')).toContainText('unsaved modifications');
    const downloading=page.waitForEvent('download');await page.locator('#export-disk').click();const exported=await readFile(await(await downloading).path());
    const expected=Buffer.from(disk);expected.fill(0x6c,8*512,9*512);expect(exported).toEqual(expected);
    await page.getByRole('button',{name:'Enable audio',exact:true}).click();await expect.poll(()=>page.evaluate(()=>window.musicScheduled.some(({pcm})=>pcm.some(v=>Math.abs(v)>0.05)&&pcm.every(v=>Number.isFinite(v)&&Math.abs(v)<=1)))).toBe(true);
    expect(await page.evaluate(()=>{const w=window.musicWasm;return [w.msx_fm_pac_profile(),w.msx_sound_cartridge_profile(),w.msx_disk_profile(),w.msx_firmware()];})).toEqual([1,1,1,3]);
    await page.locator('#remove-pac-rom').click();expect(await page.evaluate(()=>{const w=window.musicWasm;return [w.msx_sound_cartridge_profile(),w.msx_disk_profile(),w.msx_disk_len()];})).toEqual([1,1,368640]);
  });
}

// Same bounded-memory methodology as browser.spec.js: exercise the real Wasm
// allocator with an impossible reservation, without exhausting the host.
function boundedMemory(bytes,maxPages) {
  let cursor=8;
  function leb(){let value=0,shift=0,b;do{b=bytes[cursor++];value+=(b&127)*2**shift;shift+=7;}while(b&128);return value;}
  function encoded(value){const result=[];do{const b=value&127;value>>>=7;result.push(b|(value?128:0));}while(value);return Buffer.from(result);}
  while(cursor<bytes.length){const start=cursor,section=bytes[cursor++],length=leb(),end=cursor+length;
    if(section===5){expect(leb()).toBe(1);const flags=leb();expect([0,1]).toContain(flags);const minimum=leb();expect(minimum).toBeLessThan(maxPages);
      const payload=Buffer.concat([Buffer.from([1,1]),encoded(minimum),encoded(maxPages)]);
      return Buffer.concat([bytes.subarray(0,start),Buffer.from([5]),encoded(payload.length),payload,bytes.subarray(end)]);
    }cursor=end;
  }throw new Error('Missing memory section');
}
for(const site of sites) test(`FM-PAC real Wasm allocation failure preserves installed bytes and CPU audio ${site}`,async({page})=>{
  const bounded=boundedMemory(await readFile('dist/msx_wasm.wasm'),64);
  await page.addInitScript(base64=>{
    const instantiate=WebAssembly.instantiate;
    WebAssembly.instantiate=async(...args)=>{
      args[0]=Uint8Array.from(atob(base64),c=>c.charCodeAt(0));
      const result=await instantiate(...args),w=result.instance.exports;
      return {...result,instance:{exports:{...w,msx_upload(n){const p=w.msx_upload(window.failMusicAllocation ? 8*1024*1024:n);window.musicReservation=p;return p;}}}};
    };
  },bounded.toString('base64'));
  await boot(page,site);await install(page);
  await page.locator('#firmware').selectOption('diagnostic');await page.locator('#mapper').selectOption('1');await page.locator('#cartridge').setInputFiles('dist/fm-pac-test-cartridge.rom');await green(page);
  await page.evaluate(()=>window.failMusicAllocation=true);
  await page.locator('#pac-rom').setInputFiles(file('replacement.rom',firmware));await expect(page.locator('#status')).toContainText('FM-PAC import failed');
  expect(await page.evaluate(()=>window.musicReservation)).toBe(0);
  await expect(page.locator('#pac-status')).toContainText('diagnostic-pac.rom');
  expect((await advance(page,10)).some(v=>Math.abs(v)>0.01)).toBe(true);await green(page);
});
