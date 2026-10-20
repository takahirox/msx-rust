import { test, expect } from '@playwright/test';

async function boot(page, path = '/') {
  await page.goto(path);
  await expect(page.locator('#status')).toHaveText('Running · Weave BASIC');
  await page.locator('#model').selectOption('1');
  await expect(page.locator('#status')).toContainText('MSX2 · Weave BASIC');
}
async function pixel(page, x, y) {
  return page.locator('#screen').evaluate((canvas, [x,y]) => Array.from(canvas.getContext('2d').getImageData(x,y,1,1).data).slice(0,3), [x,y]);
}
async function servicePattern(page) {
  // A whole original tile distinguishes cartridge completion from transient
  // white pixels in the C-BIOS startup text, especially after reset/reload.
  await expect.poll(() => page.locator('#screen').evaluate(canvas => {
    const data = canvas.getContext('2d').getImageData(0,16,8,8).data;
    return Array.from({length:64}, (_,i) => {
      const color = i % 2 === 0 ? 255 : 0;
      return data[i*4] === color && data[i*4+1] === color && data[i*4+2] === color;
    }).every(Boolean);
  }), {timeout:15000}).toBe(true);
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
  await page.locator('#builtin').click();await expect(page.locator('#status')).toContainText('Weave BASIC');
  await expect(page.locator('#eject')).toBeDisabled();
  await expect(page.locator('#bios')).toHaveValue('');await expect(page.locator('#subrom')).toHaveValue('');
  await page.locator('#model').selectOption('0');await expect(page.locator('#transcript')).toContainText('READY');
  expect(requests.filter(r=>r.method!=='GET'||r.body)).toEqual([]);
});
test('MSX2 boots from the GitHub Pages project subpath with relative fixture links',async({page})=>{
  await boot(page,'http://127.0.0.1:41874/msx-rust/');await fixture(page);
  for(const file of ['msx2-test-cartridge.rom','weave-msx2-boot.rom','msx2-diagnostic-sub.rom','cbios-test-cartridge.rom','cbios_main_msx2.rom','cbios_sub.rom','CBIOS-LICENSE.txt','CBIOS-PROVENANCE.txt']) expect((await page.request.get(`http://127.0.0.1:41874/msx-rust/${file}`)).ok()).toBe(true);
  await page.setViewportSize({width:320,height:720});
  await page.locator('#advanced-panel > summary').click();
  await expect(page.locator('#firmware')).toBeVisible();
  expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(320);
  const box=await page.locator('#screen').boundingBox();expect(box.width).toBeLessThan(320);
});

for (const path of ['/', 'http://127.0.0.1:41874/msx-rust/']) {
  test(`default bundled C-BIOS runs standard BIOS cartridge services locally at ${path}`, async ({page}) => {
    const errors=[], requests=[];
    page.on('pageerror', error => errors.push(error.message));
    page.on('requestfailed', request => errors.push(`Failed request: ${request.url()}`));
    page.on('response', response => { if (response.status() >= 400) errors.push(`HTTP ${response.status()}: ${response.url()}`); });
    page.on('request', request => requests.push({url:request.url(), method:request.method(), body:request.postData()}));
    // Reject external access: the normal bundled path must work with local assets alone.
    await page.route('**/*', route => new URL(route.request().url()).hostname === '127.0.0.1' ? route.continue() : route.abort());
    await page.addInitScript(() => {
      window.cbiosAudio = [];
      const Native = window.AudioContext;
      window.AudioContext = class extends Native {
        createBufferSource() {
          const source = super.createBufferSource(), start = source.start.bind(source);
          source.start = (...args) => {
            window.cbiosAudio.push(Math.max(...source.buffer.getChannelData(0).map(Math.abs)));
            return start(...args);
          };
          return source;
        }
      };
      // Fail on attempted persistence even if it is later removed.
      Storage.prototype.setItem = () => { throw new Error('Unexpected ROM storage'); };
      indexedDB.open = () => { throw new Error('Unexpected IndexedDB storage'); };
      navigator.sendBeacon = () => { throw new Error('Unexpected beacon'); };
    });
    await boot(page, path);
    await expect(page.locator('#transcript')).toContainText('READY');
    await page.locator('#advanced-panel > summary').click();
    await expect(page.locator('#firmware')).toHaveValue('auto');
    await expect(page.locator('#msx2-builtins')).toContainText('no original MSX BASIC');
    await page.locator('#cartridge').setInputFiles('dist/cbios-test-cartridge.rom');
    await expect(page.locator('#status')).toContainText('Auto: Plain');
    // Only reached after C-BIOS slot discovery, RDSLT/WRSLT, EXTROM,
    // CHGMOD/CHPUT/FILVRM/WRTVRM. These pixels are drawn by emulated Z80 code.
    await servicePattern(page);
    await expect.poll(() => pixel(page, 1, 16)).toEqual([0,0,0]);
    await expect.poll(() => pixel(page, 8, 16)).toEqual([0,0,0]);
    await expect(page.locator('#transcript')).toHaveText('');
    await page.getByRole('button', {name:'Enable audio',exact:true}).click();
    await expect.poll(() => page.evaluate(() => window.cbiosAudio.slice(-5).length === 5 && window.cbiosAudio.slice(-5).every(v=>v<0.0001))).toBe(true);
    await page.keyboard.down('Space');
    await expect.poll(() => pixel(page, 8, 16)).toEqual([255,255,255]);
    await expect.poll(() => page.evaluate(() => Math.max(0,...window.cbiosAudio))).toBeGreaterThan(0.05);
    await page.keyboard.up('Space');
    await expect.poll(() => pixel(page, 8, 16)).toEqual([0,0,0]);
    await expect.poll(() => page.evaluate(() => window.cbiosAudio.slice(-5).every(v=>v<0.0001))).toBe(true);
    await page.locator('#reset').click();
    await servicePattern(page);
    await page.screenshot({path:`test-results/cbios-services-${path === '/' ? 'root' : 'pages'}.png`,fullPage:true});
    // A local alternative uses the same real file inputs and core boot path.
    await page.locator('#bios').setInputFiles('dist/cbios_main_msx2.rom');
    await page.locator('#subrom').setInputFiles('dist/cbios_sub.rom');
    await page.locator('#apply-firmware').click();
    await expect(page.locator('#firmware')).toHaveValue('local');
    await servicePattern(page);
    await page.locator('#subrom').setInputFiles({name:'invalid-local.rom',mimeType:'application/octet-stream',buffer:Buffer.alloc(16384)});
    await page.locator('#apply-firmware').click();
    await expect(page.locator('#status')).toContainText('unchanged');
    await expect(page.locator('#firmware')).toHaveValue('local');
    await expect.poll(() => pixel(page, 0, 16)).toEqual([255,255,255]);
    await page.locator('#firmware').selectOption('diagnostic');
    await expect(page.locator('#status')).toContainText('Original cartridge loader');
    await expect(page.locator('#eject')).toBeDisabled();
    await expect(page.locator('#bios')).toHaveValue('');
    await expect(page.locator('#subrom')).toHaveValue('');
    await fixture(page);
    await page.locator('#firmware').selectOption('cbios');
    await expect(page.locator('#status')).toContainText('C-BIOS 0.29a');
    await expect(page.locator('#eject')).toBeDisabled();
    await expect(page.locator('#screen')).toHaveAttribute('width','256');
    await page.locator('#cartridge').setInputFiles('dist/cbios-test-cartridge.rom');
    await servicePattern(page);
    await page.locator('#eject').click();
    await expect(page.locator('#status')).toHaveText('Running · MSX2 C-BIOS 0.29a · cartridge BIOS (no BASIC)');
    await expect(page.locator('#transcript')).toHaveText('');
    await page.locator('#model').selectOption('0');
    await expect(page.locator('#status')).toHaveText('Running · Weave BASIC');
    await expect(page.locator('#transcript')).toContainText('READY');
    await page.keyboard.type('PRINT 42'); await page.keyboard.press('Enter');
    await expect(page.locator('#transcript')).toContainText('42');
    expect(await page.evaluate(() => [localStorage.length, sessionStorage.length])).toEqual([0,0]);
    const pageUrl = new URL(path, 'http://127.0.0.1:41873');
    const assets = ['', 'app.js', 'style.css', 'msx_wasm.wasm'].map(file => new URL(file, pageUrl).href);
    expect(requests.filter(r => r.method !== 'GET' || r.body || !assets.includes(r.url))).toEqual([]);
    expect(errors).toEqual([]);
  });
}
