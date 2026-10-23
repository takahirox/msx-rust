import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const manifest = JSON.parse(await readFile('demos/manifest.json'));
const sites = ['http://127.0.0.1:41873/', 'http://127.0.0.1:41874/msx-rust/'];
// Deterministic stepping still executes the real Wasm CPU/VDP. It keeps games
// from reaching GAME OVER while browser assertions/screenshot capture run.
test.beforeEach(async ({page}) => {
  await page.addInitScript(() => {
    const instantiate=WebAssembly.instantiate;
    WebAssembly.instantiate=async (...args)=>{
      const result=await instantiate(...args), exports=result.instance.exports;
      window.demoWasm=exports;
      return {...result,instance:{exports:{...exports,msx_frame(){if(!window.manualFrames) exports.msx_frame();}}}};
    };
  });
});
async function advance(page, count) {
  await page.evaluate(async count=>{
    for(let i=0;i<count;i++) window.demoWasm.msx_frame();
    await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);
  },count);
}

async function pixel(page, x, y) {
  return page.locator('#screen').evaluate((c, [x,y]) => Array.from(c.getContext('2d').getImageData(x,y,1,1).data).slice(0,3), [x,y]);
}
async function whiteCount(page, rect) {
  return page.locator('#screen').evaluate((c, rect) => {
    const d = c.getContext('2d').getImageData(...rect).data;
    return Array.from({length: d.length / 4}, (_, i) => d[i*4] === 255 && d[i*4+1] === 255 && d[i*4+2] === 255).filter(Boolean).length;
  }, rect);
}
// Largest connected white component below the HUD: a ship/character rather than
// a scrolling one-pixel star, yellow enemy block or C-BIOS boot message.
async function playerSprite(page) {
  return page.locator('#screen').evaluate(c => {
    const d = c.getContext('2d').getImageData(0,32,256,128).data;
    const remaining = new Set();
    for (let i=0;i<256*128;i++) if(d[i*4]===255 && d[i*4+1]===255 && d[i*4+2]===255) remaining.add(i);
    let largest = [];
    for (const first of remaining) {
      const component = [], queue = [first]; remaining.delete(first);
      while(queue.length) {
        const p=queue.pop(); component.push(p);
        const x=p%256, y=Math.floor(p/256);
        for (const [nx,ny] of [[x-1,y],[x+1,y],[x,y-1],[x,y+1]]) {
          const next=ny*256+nx;
          if(nx>=0 && nx<256 && ny>=0 && ny<128 && remaining.delete(next)) queue.push(next);
        }
      }
      if(component.length>largest.length) largest=component;
    }
    return largest.length < 10 ? null : {x:Math.min(...largest.map(p=>p%256)),y:32+Math.min(...largest.map(p=>Math.floor(p/256))),size:largest.length};
  });
}
async function choose(page, id) {
  await page.locator('#demo').selectOption(id);
  await page.locator('#play-demo').click();
  const demo = manifest.demos.find(d=>d.id===id);
  await expect(page.locator('#status')).toContainText(`${demo.title} (${demo.mapper})`);
  await expect(page.locator('#model')).toHaveValue(demo.model==='MSX2'?'1':'0');
}
async function gameReady(page, id) {
  await page.evaluate(()=>{window.manualFrames=true;});
  // Fixed HUD regions distinguish the game's Z80 output from startup text.
  const rect=id==='plustaker'?[64,168,128,24]:[0,0,256,8];
  for(let i=0;i<60;i++) {
    await advance(page,10);
    if(await whiteCount(page,rect)>150 && await playerSprite(page)) {
      if(id==='yoko') {
        // Upstream intentionally boots into its GAME OVER/start prompt.
        await page.locator('#screen').focus();
        // The HUD can appear before initialization starts polling input. Retry
        // the start press until the ship appears, within 64 emulated frames.
        let ship;
        for(let start=0;start<8;start++) {
          await page.keyboard.down('Space');
          try { await advance(page,4); }
          finally { await page.keyboard.up('Space'); }
          await advance(page,4);
          ship=await playerSprite(page);
          if(ship && ship.x<64) break;
        }
        expect(ship).not.toBeNull();expect(ship.x).toBeLessThan(64);
      }
      return;
    }
  }
  throw new Error(`${id} failed to draw its game scene/player within 600 emulated frames`);
}
async function basicCommand(page, text) {
  await page.locator('#screen').focus();
  await page.keyboard.type(text);await page.keyboard.press('Enter');
}
for (const site of sites) {
  const prefix=site.includes('msx-rust')?'pages':'root';
  test(`curated games and MSX2 diagnostic execute with input, switching, reset and eject at ${prefix}`, async ({page}) => {
    test.setTimeout(60000);
    const errors=[], requests=[];
    page.on('pageerror',e=>errors.push(e.message));
    page.on('response',r=>{if(r.status()>=400)errors.push(`${r.status()} ${r.url()}`);});
    page.on('request',r=>requests.push({url:r.url(),method:r.method(),body:r.postData()}));
    await page.route('**/*',route=>route.request().url().startsWith(site)?route.continue():route.abort());
    await page.addInitScript(()=>{
      Storage.prototype.setItem=()=>{throw new Error('Unexpected persistence');};
      indexedDB.open=()=>{throw new Error('Unexpected IndexedDB');};
      navigator.sendBeacon=()=>{throw new Error('Unexpected analytics');};

    });
    await page.goto(site);
    await expect(page.locator('#transcript')).toContainText('READY');
    // Starting in MSX2 also proves a game selects its declared MSX1 machine.
    await expect(page.locator('#model')).toHaveValue('1');
    for (const id of ['plustaker','yoko']) {
      await choose(page,id);await gameReady(page,id);
      await expect(page.locator('#firmware')).toHaveValue('auto');
      await expect(page.locator('#status')).toContainText('MSX1 C-BIOS');
      expect(await page.evaluate(()=>[window.demoWasm.msx_firmware(),window.demoWasm.msx_detected_mapper(),window.demoWasm.msx_boot_policy()])).toEqual([1,1,0]);
      const before=await playerSprite(page);
      await page.locator('#screen').focus();
      await page.keyboard.down('ArrowUp');
      await advance(page,6);
      await expect.poll(async()=>{const after=await playerSprite(page);return after && after.y<before.y-6;}).toBe(true);
      await page.keyboard.up('ArrowUp');
      // Keep fire held through a few emulated frames; the screenshot records shots.
      await page.keyboard.down('Space');
      await advance(page,8);
      await page.keyboard.up('Space');
      await page.screenshot({path:`test-results/demo-${id}-${prefix}.png`,fullPage:true});
      await page.locator('#reset').click();await gameReady(page,id);
      await expect.poll(async()=>{const after=await playerSprite(page);return after && Math.abs(after.y-before.y)<3;}).toBe(true);
    }
    await page.evaluate(()=>{window.manualFrames=false;});
    await page.locator('#eject').click();
    await expect(page.locator('#status')).toHaveText('Running · Weave BASIC');
    await choose(page,'plustaker');
    await choose(page,'msx2-diagnostic');
    await expect(page.locator('#status')).toContainText('MSX2 C-BIOS');
    await expect(page.locator('#screen')).toHaveAttribute('width','512');
    await expect.poll(()=>pixel(page,10,10)).toEqual([0,255,255]);
    await page.keyboard.down('8');await expect(page.locator('#screen')).toHaveAttribute('width','256');await page.keyboard.up('8');
    await expect.poll(()=>pixel(page,10,10)).toEqual([0,255,0]);
    await page.screenshot({path:`test-results/demo-msx2-${prefix}.png`,fullPage:true});
    await page.locator('#reset').click();await expect(page.locator('#screen')).toHaveAttribute('width','512');
    await expect.poll(()=>pixel(page,10,10)).toEqual([0,255,255]);
    await page.locator('#eject').click();
    await expect(page.locator('#status')).toHaveText('Running · MSX2 · Weave BASIC');
    await basicCommand(page,'PRINT 73');await expect(page.locator('#transcript')).toContainText('73\nREADY');
    await choose(page,'plustaker');
    await page.locator('#advanced-panel > summary').click();await page.locator('#builtin').click();
    await expect(page.locator('#status')).toHaveText('Running · Weave BASIC');
    // Manual cartridges still use their chosen mapper and ordinary file picker.
    await page.locator('#mapper').selectOption('1');
    await page.locator('#cartridge').setInputFiles('dist/cbios-msx1-test-cartridge.rom');
    await expect(page.locator('#status')).toContainText('cbios-msx1-test-cartridge.rom (Plain)');
    await page.locator('#eject').click();
    for(const demo of manifest.demos) for(const file of [demo.file,...demo.notices]) expect((await page.request.get(new URL(file,site).href)).ok()).toBe(true);
    expect(requests.every(r=>r.method==='GET'&&!r.body&&r.url.startsWith(site))).toBe(true);
    expect(errors).toEqual([]);
    await page.setViewportSize({width:320,height:720});
    expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(320);
    await page.screenshot({path:`test-results/demo-mobile-${prefix}.png`,fullPage:true});
  });

  test(`failed and superseded demo fetches preserve the running session at ${prefix}`,async({page})=>{
    test.setTimeout(60000);
    await page.goto(site);await expect(page.locator('#demo')).toBeEnabled();
    await basicCommand(page,'10 PRINT 91');
    await page.route('**/demos/yokosht.rom',r=>r.fulfill({status:404,body:'Missing'}));
    await page.locator('#demo').selectOption('yoko');await page.locator('#play-demo').click();
    await expect(page.locator('#status')).toContainText('HTTP 404');
    await expect(page.locator('#model')).toHaveValue('1');await expect(page.locator('#firmware')).toHaveValue('auto');
    await basicCommand(page,'LIST');await expect(page.locator('#transcript')).toContainText('10 PRINT 91');
    await page.unroute('**/demos/yokosht.rom');
    await choose(page,'msx2-diagnostic');await expect(page.locator('#screen')).toHaveAttribute('width','512');
    await expect.poll(()=>pixel(page,10,10)).toEqual([0,255,255]);
    // Same-size corruption must fail SHA-256 validation before changing MSX2.
    const corrupt=Buffer.from(await readFile('demos/plustakermsx.rom'));corrupt[32]^=1;
    await page.route('**/demos/plustakermsx.rom',r=>r.fulfill({body:corrupt,contentType:'application/octet-stream'}));
    await page.locator('#demo').selectOption('plustaker');await page.locator('#play-demo').click();
    await expect(page.locator('#status')).toContainText('integrity check failed');
    await expect(page.locator('#model')).toHaveValue('1');await expect.poll(()=>pixel(page,10,10)).toEqual([0,255,255]);
    await page.unroute('**/demos/plustakermsx.rom');
    await page.route('**/demos/plustakermsx.rom',r=>r.abort('failed'));
    await page.locator('#play-demo').click();await expect(page.locator('#status')).toContainText('Demo load failed');
    await expect(page.locator('#model')).toHaveValue('1');await expect.poll(()=>pixel(page,10,10)).toEqual([0,255,255]);
    await page.unroute('**/demos/plustakermsx.rom');
    let finish;
    const pending=new Promise(resolve=>{finish=resolve;});
    let requested=false, resolveHandled;
    const handled=new Promise(resolve=>{resolveHandled=resolve;});
    await page.route('**/demos/plustakermsx.rom',async r=>{requested=true;await pending;await r.fulfill({body:await readFile('demos/plustakermsx.rom')});resolveHandled();});
    await page.locator('#play-demo').click();await expect.poll(()=>requested).toBe(true);
    await expect(page.locator('#play-demo')).toBeDisabled();
    // Synthetic repeated clicks and local imports cannot race the active upload.
    await page.locator('#play-demo').dispatchEvent('click');
    await page.locator('#cartridge').setInputFiles('dist/test-cartridge.rom');
    await page.locator('#eject').click();
    await choose(page,'yoko');
    finish();await handled;await page.unroute('**/demos/plustakermsx.rom');
    await gameReady(page,'yoko');
    await expect(page.locator('#status')).toContainText('YOKO (yokosht) (Plain)');
    await expect(page.locator('#model')).toHaveValue('0');
  });

  test(`explicit firmware remains authoritative for demos at ${prefix}`,async({page})=>{
    await page.goto(site);await expect(page.locator('#demo')).toBeEnabled();
    await page.locator('#model').selectOption('0');
    await page.locator('#advanced-panel > summary').click();
    await page.locator('#bios').setInputFiles('dist/cbios_main_msx1.rom');
    await choose(page,'plustaker');
    await expect(page.locator('#firmware')).toHaveValue('local');
    await page.locator('#demo').selectOption('msx2-diagnostic');await page.locator('#play-demo').click();
    await expect(page.locator('#status')).toContainText('override');
    await expect(page.locator('#model')).toHaveValue('0');await expect(page.locator('#firmware')).toHaveValue('local');
    await page.locator('#builtin').click();await page.locator('#firmware').selectOption('cbios');
    await choose(page,'msx2-diagnostic');await expect(page.locator('#firmware')).toHaveValue('cbios');
    await page.locator('#eject').click();await expect(page.locator('#firmware')).toHaveValue('cbios');
    await page.locator('#firmware').selectOption('diagnostic');
    await choose(page,'msx2-diagnostic');await expect(page.locator('#firmware')).toHaveValue('diagnostic');
    await page.locator('#demo').selectOption('yoko');await page.locator('#play-demo').click();
    await expect(page.locator('#status')).toContainText('override');
    await expect(page.locator('#model')).toHaveValue('1');await expect(page.locator('#firmware')).toHaveValue('diagnostic');
    await page.locator('#builtin').click();await choose(page,'yoko');
  });
}

test('missing/invalid catalogs can be retried, and core validation failures do not change the model',async({page})=>{
  await page.route('**/demos/manifest.json',r=>r.fulfill({status:404,body:'Missing'}));
  await page.goto('/');await expect(page.locator('#demo-message')).toContainText('Demos unavailable');
  await basicCommand(page,'10 PRINT 84');
  await page.unroute('**/demos/manifest.json');
  const unsupported=structuredClone(manifest);unsupported.demos[0].mapper='Unsupported';
  await page.route('**/demos/manifest.json',r=>r.fulfill({json:unsupported}));
  await page.locator('#play-demo').click();await expect(page.locator('#demo-message')).toContainText('Invalid demo metadata');
  await page.unroute('**/demos/manifest.json');
  // A hash-valid but malformed MSX1 cartridge must leave the default MSX2 unchanged.
  const invalid=Buffer.alloc(16384), changed=structuredClone(manifest);
  changed.demos[0].sha256=createHash('sha256').update(invalid).digest('hex');
  await page.route('**/demos/manifest.json',r=>r.fulfill({json:changed}));
  await page.route('**/demos/plustakermsx.rom',r=>r.fulfill({body:invalid}));
  await page.locator('#play-demo').click();await expect(page.locator('#demo')).toBeEnabled();
  await page.locator('#play-demo').click();await expect(page.locator('#status')).toContainText('Demo load failed');
  await expect(page.locator('#model')).toHaveValue('1');
  await basicCommand(page,'LIST');await expect(page.locator('#transcript')).toContainText('10 PRINT 84');
});
