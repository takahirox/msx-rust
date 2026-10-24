import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
const cas = await readFile('dist/cassette-test.cas');
// Hand specified bytes: no parser/encoding helper or ROM readback supplies expected data.
const recovered = [
  208,208,208,208,208,208,208,208,208,208,84,65,80,69,32,32,
  0x16,0xc2,0x31,0xc2,0x16,0xc2,0x3e,0x22,0xd3,0x99,0x3e,0x87,0xd3,0x99,
  0x3e,0xa5,0x32,0xff,0xc1,0x3e,9,0xd3,0xab,0x76,0x18,0xfd,0,255,0x55,0xaa,0x80,0x81,0xfe,0x7f,
];
test.beforeEach(async ({page}) => {
  await page.addInitScript(() => {
    const instantiate = WebAssembly.instantiate;
    WebAssembly.instantiate = async (...args) => {
      const result = await instantiate(...args), w = result.instance.exports;
      window.tapeWasm = w;
      return {...result, instance:{exports:{...w,
        msx_frame(){if(!window.manualTapeFrames) w.msx_frame();},
        msx_tape_upload(n){return window.failTapeAllocation ? 0 : w.msx_tape_upload(n);},
      }}};
    };
    const read = File.prototype.arrayBuffer;
    File.prototype.arrayBuffer = async function(){
      if(this.name === 'read-error.cas') throw new RangeError('Synthetic file allocation failure');
      if(this.name === 'deferred.cas') await new Promise(resolve => window.finishTape = resolve);
      return read.call(this);
    };
  });
});
async function boot(page, site) {
  await page.goto(site); await expect(page.locator('#transcript')).toContainText('READY');
  await page.evaluate(() => window.manualTapeFrames = true);
}
async function advance(page, n) {
  await page.evaluate(async n => {
    for(let i=0;i<n;i++) window.tapeWasm.msx_frame();
    await new Promise(requestAnimationFrame); await new Promise(requestAnimationFrame);
  }, n);
}
async function openTape(page, file='dist/cassette-test.cas') {
  const chooser = page.waitForEvent('filechooser'); await page.locator('#open-cas').click();
  await (await chooser).setFiles(file);
  await expect(page.locator('#status')).toContainText('CAS mounted');
}
async function exactVisible(page) {
  const color=(await page.locator('#model').inputValue())==='0' ? [33,200,66] : [36,218,36];
  const actual = await page.locator('#screen').evaluate((canvas,{n,color}) => {
    const c = canvas.getContext('2d');
    return Array.from({length:n},(_,i) => {
      let v=0;
      for(let bit=0;bit<8;bit++) {
        const tile=i*8+bit, p=c.getImageData(tile%32*8,Math.floor(tile/32)*8,1,1).data;
        if(p[0]===255 && p[1]===255 && p[2]===255) v|=1<<bit;
        else if(!color.every((v,c)=>p[c]===v)) return -1;
      }
      return v;
    });
  }, {n:recovered.length,color});
  expect(actual).toEqual(recovered);
  const green = await page.locator('#screen').evaluate(c => [...c.getContext('2d').getImageData(0,160,1,1).data].slice(0,3));
  expect(green).toEqual(color);
}
for(const site of ['/', 'http://127.0.0.1:41874/msx-rust/']) {
  for(const model of ['0','1']) for(const rate of ['1200','2400']) {
    test(`Z80 cassette header/body/payload MSX${Number(model)+1} ${rate} at ${site}`,async({page}) => {
      test.setTimeout(60000);
      const errors=[], requests=[];page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>requests.push(r));
      await boot(page,site);
      // Trap any persistence attempted by a local media operation.
      await page.evaluate(() => {
        Storage.prototype.setItem=()=>{throw new Error('Unexpected storage');};
        indexedDB.open=()=>{throw new Error('Unexpected IndexedDB');};
        navigator.sendBeacon=()=>{throw new Error('Unexpected beacon');};
      });
      if(model==='0') await page.locator('#model').selectOption(model);
      const chooser=page.waitForEvent('filechooser');await page.locator('#open-rom').click();
      await (await chooser).setFiles('dist/cassette-test-cartridge.rom');
      await page.locator('#tape-rate').selectOption(rate);await openTape(page);
      await expect(page.locator('#tape-status')).toContainText(`${rate} baud`);
      await advance(page,1000);await exactVisible(page);
      await expect(page.locator('#tape-status')).toContainText('end of tape');
      await expect(page.locator('#tape-status')).toContainText('motor off');
      const end = await page.evaluate(() => window.tapeWasm.msx_tape_position());
      await advance(page,60);expect(await page.evaluate(() => window.tapeWasm.msx_tape_position())).toBe(end);
      await page.locator('#rewind-tape').click();
      expect(await page.evaluate(() => window.tapeWasm.msx_tape_position())).toBe(0);
      await page.locator('#reset').click();await advance(page,1000);await exactVisible(page);
      await page.screenshot({path:`test-results/cassette-${model}-${rate}-${site==='/'?'root':'pages'}.png`});
      await page.locator('#eject-tape').click();await expect(page.locator('#tape-status')).toHaveText('Cassette: empty');
      await exactVisible(page);await expect(page.locator('#status')).toContainText('Execution unchanged');
      expect(errors).toEqual([]);
      expect(requests.every(r=>r.method()==='GET' && !r.postData())).toBe(true);
    });
  }
  test(`CAS pause, failed imports, canceled staging and session preservation at ${site}`,async({page}) => {
    await boot(page,site);
    await page.locator('#screen').focus();await page.keyboard.type('10 PRINT 73');await page.keyboard.press('Enter');await advance(page,4);
    await page.locator('#disk').setInputFiles('dist/weave-disk-demo.dsk');
    await openTape(page);
    await page.evaluate(() => window.manualTapeFrames=false);
    await expect.poll(()=>page.evaluate(()=>window.tapeWasm.msx_tape_position())).toBeGreaterThan(0.1);
    await page.locator('#pause').click();await expect(page.locator('#tape-status')).toContainText('paused');
    const position=await page.evaluate(()=>window.tapeWasm.msx_tape_position());
    await page.waitForTimeout(150);expect(await page.evaluate(()=>window.tapeWasm.msx_tape_position())).toBe(position);
    for(const [name,buffer] of [['empty.cas',Buffer.alloc(0)],['wrong.cas',Buffer.from([1,2,3])],['bad.cas',cas.subarray(0,8)],['truncated.cas',cas.subarray(0,24)],['bad.tsx',cas],['read-error.cas',cas],['large.cas',Buffer.alloc(16777217)]]) {
      await page.locator('#tape').setInputFiles({name,mimeType:'application/octet-stream',buffer});
      await expect(page.locator('#status')).toContainText('Session and previous tape unchanged');
      expect(await page.evaluate(()=>window.tapeWasm.msx_tape_position())).toBe(position);
    }
    await page.evaluate(()=>window.failTapeAllocation=true);await page.locator('#tape').setInputFiles('dist/cassette-test.cas');
    await expect(page.locator('#status')).toContainText('Session and previous tape unchanged');
    await page.evaluate(()=>window.failTapeAllocation=false);
    expect(await page.evaluate(()=>{
      const w=window.tapeWasm, flags=w.msx_tape_flags(), pos=w.msx_tape_position();
      const invalid=[];
      for(const rate of [0,1,2401,0xffffffff]) {
        const p=w.msx_tape_upload(9);new Uint8Array(w.memory.buffer,p,9).set([31,166,222,186,204,19,125,116,85]);
        invalid.push(w.msx_mount_tape(9,rate));
      }
      w.msx_tape_upload(9);invalid.push(w.msx_mount_tape(8,1200));
      w.msx_tape_upload(9);w.msx_cancel_upload();invalid.push(w.msx_mount_tape(9,1200));
      invalid.push(w.msx_tape_upload(0),w.msx_tape_upload(16777217),w.msx_tape_upload(0xffffffff));
      return [invalid,w.msx_tape_flags()===flags,w.msx_tape_position()===pos,w.msx_firmware(),w.msx_disk_len()];
    })).toEqual([[0,0,0,0,0,0,0,0,0],true,true,0,737280]);
    // Mount/replacement and rewind leave paused execution and disk/firmware alone.
    await page.locator('#tape-rate').selectOption('2400');await openTape(page);
    expect(await page.evaluate(()=>window.tapeWasm.msx_tape_position())).toBe(0);
    await expect(page.locator('#pause')).toHaveText('Resume');
    await page.locator('#rewind-tape').click();await page.locator('#eject-tape').click();
    await expect(page.locator('#disk-status')).toContainText('weave-disk-demo.dsk');
    await page.locator('#pause').click();await page.locator('#screen').focus();await page.keyboard.type('RUN');await page.keyboard.press('Enter');
    await expect(page.locator('#transcript')).toContainText('73\nREADY');
    // All media controls remain reachable without squeezing the display in
    // landscape fullscreen; focusing controls scrolls the shared media strip.
    await page.setViewportSize({width:812,height:375});
    await page.locator('#fullscreen').click();
    for(const id of ['open-cas','tape-rate','open-dsk']) {
      await page.locator(`#${id}`).focus();
      const box=await page.locator(`#${id}`).boundingBox();
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x+box.width).toBeLessThanOrEqual(812);
    }
    const display=await page.locator('#screen').boundingBox();
    expect(display.width).toBeGreaterThan(180);
    expect(display.y+display.height).toBeLessThanOrEqual(375);
  });
  test(`stale CAS completion cannot replace tape or a newer session at ${site}`,async({page}) => {
    await boot(page,site);await openTape(page);
    const defer = async () => {
      await page.evaluate(()=>delete window.finishTape);
      await page.locator('#tape').setInputFiles({name:'deferred.cas',mimeType:'application/octet-stream',buffer:cas});
      await expect.poll(()=>page.evaluate(()=>typeof window.finishTape)).toBe('function');
    };
    const finish = async () => {await page.evaluate(()=>window.finishTape());await expect(page.locator('#open-cas')).toBeEnabled();};
    await defer();await page.locator('#eject-tape').click();await finish();
    await expect(page.locator('#tape-status')).toContainText('cassette-test.cas');
    await expect(page.locator('#status')).toContainText('Import canceled');
    await defer();
    // A programmatic second file-input event exercises stale completion even
    // though ordinary concurrent user selection is disabled during imports.
    await page.locator('#tape').setInputFiles({name:'new.cas',mimeType:'application/octet-stream',buffer:cas});
    await expect(page.locator('#tape-status')).toContainText('new.cas');await finish();
    await expect(page.locator('#tape-status')).toContainText('new.cas');
    await defer();await page.locator('#tape').dispatchEvent('cancel');await finish();
    await expect(page.locator('#tape-status')).toContainText('new.cas');
    await page.locator('#cartridge').setInputFiles('dist/cassette-test-cartridge.rom');
    await expect(page.locator('#status')).toContainText('C-BIOS');
    await defer();await page.locator('#eject').click();await finish();
    await expect(page.locator('#status')).toContainText('Weave BASIC');
    await expect(page.locator('#tape-status')).toContainText('new.cas');
  });
}
