import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
const sites=['/','http://127.0.0.1:41874/msx-rust/'];
const file=(name,buffer)=>({name,buffer,mimeType:'application/octet-stream'});
const firmware=await readFile('dist/music-diagnostic-firmware.rom');
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
  const chooser=page.waitForEvent('filechooser');await page.locator('#open-music-rom').click();
  await(await chooser).setFiles(file('diagnostic-fm.rom',firmware));
  await expect(page.locator('#music-status')).toContainText('diagnostic-fm.rom');
  await expect(page.locator('#emulator')).toHaveAttribute('aria-busy','false');
}
async function advance(page,n) {return page.evaluate(n=>{const w=window.musicWasm;for(let i=0;i<n;i++)w.msx_frame();return [...new Float32Array(w.memory.buffer,w.msx_audio(),w.msx_audio_len())];},n);}
async function green(page) {await expect.poll(()=>page.locator('#screen').evaluate(c=>[...c.getContext('2d').getImageData(20,20,1,1).data])).toEqual([36,218,36,255]);}
for(const site of sites) {
  test(`MSX-MUSIC real firmware/cart CPU, scheduled PCM, mute/pause/reset/removal ${site}`,async({page})=>{
    const errors=[],requests=[];page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>requests.push(r));
    await boot(page,site);
    await page.evaluate(()=>{
      Storage.prototype.setItem=()=>{throw new Error('Unexpected storage');};indexedDB.open=()=>{throw new Error('Unexpected IndexedDB');};navigator.sendBeacon=()=>{throw new Error('Unexpected beacon');};
    });
    await install(page);
    await page.locator('#firmware').selectOption('diagnostic');await page.locator('#mapper').selectOption('1');
    await page.locator('#cartridge').setInputFiles('dist/music-test-cartridge.rom');
    await green(page);
    await page.getByRole('button',{name:'Enable audio',exact:true}).click();
    await expect.poll(()=>page.evaluate(()=>window.musicScheduled.some(({rate,pcm})=>rate===44100 && pcm.some(v=>Math.abs(v)>0.01) && pcm.every(v=>Number.isFinite(v)&&Math.abs(v)<=1)))).toBe(true);
    // Replay identical emulated frames while muted and enabled: playback never gates chip phase.
    await page.evaluate(()=>window.manualMusic=true);
    const pcm=await page.evaluate(()=>{const w=window.musicWasm,pcm=[];for(let i=0;i<8;i++){w.msx_frame();pcm.push(...new Float32Array(w.memory.buffer,w.msx_audio(),w.msx_audio_len()));}return pcm;});
    const crossings=pcm.flatMap((v,i)=>i && pcm[i-1]<=0 && v>0 ? [i]:[]);
    expect(crossings.length).toBeGreaterThan(40);
    const period=44100*72*2**19/(290*16*3579545);
    for(let n=1;n<crossings.length;n++) expect(Math.abs(crossings[n]-crossings[0]-n*period)).toBeLessThan(2);
    expect(Math.max(...pcm.map(Math.abs))).toBeGreaterThan(0.015);
    expect(Math.max(...pcm.map(Math.abs))).toBeLessThan(0.016);
    await page.getByRole('button',{name:'Mute audio',exact:true}).click();
    await page.locator('#reset').click();const muted=await advance(page,30);
    await page.getByRole('button',{name:'Enable audio',exact:true}).click();
    await page.locator('#reset').click();const audible=await advance(page,30);
    expect(audible).toEqual(muted);expect(audible.some(v=>Math.abs(v)>0.01)).toBe(true);
    // Actual animation-loop pause leaves PCM unchanged; resume continues CPU/chip clocks.
    await page.evaluate(()=>window.manualMusic=false);
    await page.locator('#pause').click();await page.waitForTimeout(50);
    const snapshot=await advance(page,0);await page.waitForTimeout(100);expect(await advance(page,0)).toEqual(snapshot);
    await page.locator('#pause').click();await green(page);
    await page.evaluate(()=>window.manualMusic=true);
    await page.locator('#remove-music-rom').click();expect(await page.evaluate(()=>window.musicWasm.msx_music_profile())).toBe(0);
    const removed=await advance(page,5);expect(removed.every(v=>v===0)).toBe(true);
    expect(errors).toEqual([]);expect(requests.filter(r=>r.method()!=='GET'||new URL(r.url()).origin!==new URL(page.url()).origin)).toEqual([]);
    expect(await page.evaluate(()=>({local:localStorage.length,session:sessionStorage.length}))).toEqual({local:0,session:0});
  });
  test(`MSX-MUSIC failed/canceled/stale imports, retained SRAM and inactive MSX1 ${site}`,async({page})=>{
    await boot(page,site);
    await page.locator('#mapper').selectOption('6');await page.locator('#cartridge').setInputFiles('dist/mapper-6.rom');
    await expect(page.locator('#export-save')).toBeEnabled();
    await page.evaluate(()=>{const w=window.musicWasm;new Uint8Array(w.memory.buffer,w.msx_sram(),w.msx_sram_len()).fill(0x79);});
    await install(page);
    expect(await page.evaluate(()=>{const w=window.musicWasm;return [...new Uint8Array(w.memory.buffer,w.msx_sram(),w.msx_sram_len())].every(v=>v===0x79);})).toBe(true);
    expect(await page.evaluate(()=>Object.keys(localStorage).filter(k=>k.startsWith('msx-rust:sram:v1:')).map(k=>atob(JSON.parse(localStorage[k]).data).charCodeAt(2047)))).toEqual([0x79]);
    for(const [name,bytes] of [['short.rom',Buffer.alloc(16383)],['wrong.txt',firmware],['allocation.rom',firmware]]) {
      await page.locator('#music-rom').setInputFiles(file(name,bytes));await expect(page.locator('#status')).toContainText('MSX-MUSIC import failed');
      await expect(page.locator('#music-status')).toContainText('diagnostic-fm.rom');
    }
    await page.locator('#music-rom').setInputFiles(file('deferred.rom',firmware));
    await expect(page.locator('#remove-music-rom')).toHaveText('Cancel import');await page.locator('#remove-music-rom').click();
    await page.evaluate(()=>window.finishMusic());await expect(page.locator('#music-status')).toContainText('diagnostic-fm.rom');
    await page.locator('#music-rom').setInputFiles(file('deferred.rom',firmware));
    await page.locator('#eject').click();await page.evaluate(()=>window.finishMusic());
    await expect(page.locator('#music-status')).toContainText('diagnostic-fm.rom');
    await page.locator('#model').selectOption('0');await expect(page.locator('#music-status')).toContainText('inactive on MSX1');
    await page.locator('#model').selectOption('1');await expect(page.locator('#music-status')).toContainText('active');
    await page.locator('#builtin').click();await expect(page.locator('#music-status')).toContainText('diagnostic-fm.rom');
    await page.locator('#remove-music-rom').click();await expect(page.locator('#music-status')).toHaveText('MSX-MUSIC not installed');
    await expect(page.locator('#transcript')).toContainText('READY');
  });
  test(`MSX-MUSIC plus SCC-I/PSG and exact Philips disk transfers ${site}`,async({page})=>{
    await boot(page,site);await install(page);
    await page.locator('#sound-cartridge').selectOption('1');await page.locator('#install-sound-cartridge').click();
    const main=await hex('philips-system',32768);main[0x2d]=1;
    const sub=Buffer.alloc(16384);sub.write('CD');sub[0x100]=0x7b;
    await page.locator('#bios').setInputFiles(file('original-main.rom',main));await page.locator('#subrom').setInputFiles(file('original-sub.rom',sub));await page.locator('#apply-firmware').click();await expect(page.locator('#firmware')).toHaveValue('local');await expect(page.locator('#emulator')).toHaveAttribute('aria-busy','false');
    await page.locator('#disk-rom').setInputFiles(file('original-disk.rom',await hex('music-disk',16384,255)));await expect(page.locator('#disk-profile-status')).toContainText('original-disk.rom');await expect(page.locator('#emulator')).toHaveAttribute('aria-busy','false');
    const disk=await hex('philips-scci-sector',368640,0x35);disk[510]=0;
    await page.locator('#disk').setInputFiles(file('original.dsk',disk));await expect(page.locator('#disk-status')).toContainText('original.dsk');await page.locator('#disk-protected').uncheck();
    await page.locator('#boot-disk').click();await green(page);
    await expect(page.locator('#disk-status')).toContainText('unsaved modifications');
    const downloading=page.waitForEvent('download');await page.locator('#export-disk').click();const exported=await readFile(await(await downloading).path());
    const expected=Buffer.from(disk);expected.fill(0x6c,8*512,9*512);expect(exported).toEqual(expected);
    await page.getByRole('button',{name:'Enable audio',exact:true}).click();
    await expect.poll(()=>page.evaluate(()=>window.musicScheduled.some(({pcm})=>pcm.some(v=>Math.abs(v)>0.05)&&pcm.every(v=>Number.isFinite(v)&&Math.abs(v)<=1)))).toBe(true);
    expect(await page.evaluate(()=>{const w=window.musicWasm;return [w.msx_music_profile(),w.msx_sound_cartridge_profile(),w.msx_disk_profile(),w.msx_firmware()];})).toEqual([1,1,1,3]);
    await page.locator('#remove-music-rom').click();
    expect(await page.evaluate(()=>{const w=window.musicWasm;return [w.msx_sound_cartridge_profile(),w.msx_disk_profile(),w.msx_disk_len()];})).toEqual([1,1,368640]);
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
for(const site of sites) test(`MSX-MUSIC real Wasm allocation failure preserves installed bytes and CPU audio ${site}`,async({page})=>{
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
  await page.locator('#firmware').selectOption('diagnostic');await page.locator('#mapper').selectOption('1');await page.locator('#cartridge').setInputFiles('dist/music-test-cartridge.rom');await green(page);
  await page.evaluate(()=>window.failMusicAllocation=true);
  await page.locator('#music-rom').setInputFiles(file('replacement.rom',firmware));await expect(page.locator('#status')).toContainText('MSX-MUSIC import failed');
  expect(await page.evaluate(()=>window.musicReservation)).toBe(0);
  await expect(page.locator('#music-status')).toContainText('diagnostic-fm.rom');
  expect((await advance(page,10)).some(v=>Math.abs(v)>0.01)).toBe(true);await green(page);
});
