import {test, expect} from '@playwright/test';
async function pixel(page,x,y) {
  return page.locator('#screen').evaluate((c,[x,y])=>Array.from(c.getContext('2d').getImageData(x,y,1,1).data).slice(0,3),[x,y]);
}
async function enter(page,text) {
  await page.locator('#screen').focus(); await page.keyboard.type(text); await page.keyboard.press('Enter');
}
async function pattern(page) {
  await expect.poll(()=>page.locator('#screen').evaluate(c=>{
    const d=c.getContext('2d').getImageData(0,16,8,8).data;
    return Array.from({length:64},(_,i)=>d[i*4]===(i%2===0?255:0)&&d[i*4+1]===d[i*4]&&d[i*4+2]===d[i*4]).every(Boolean);
  }),{timeout:15000}).toBe(true);
}
for(const path of ['/', 'http://127.0.0.1:41874/msx-rust/']) {
  for(const model of ['0','1']) {
    test(`automatic BASIC, BIOS cartridge, eject and explicit firmware: model ${model} at ${path}`,async({page})=>{
      const errors=[]; page.on('pageerror',e=>errors.push(e.message));
      await page.goto(path); await expect(page.locator('#transcript')).toContainText('READY');
      await expect(page.locator('#model')).toHaveValue('1');
      if(model==='0') await page.locator('#model').selectOption(model);
      await expect(page.locator('#status')).toContainText('Weave BASIC');
      await expect(page.locator('#example')).toBeEnabled();
      await enter(page,'10 INPUT N$'); await enter(page,'20 PRINT N$;42'); await enter(page,'RUN');
      await expect(page.locator('#transcript')).toContainText('? ');
      await enter(page,'ORBIT'); await expect(page.locator('#transcript')).toContainText('ORBIT42\nREADY');
      // Invalid/ambiguous imports retain the stored program and usable interpreter.
      for(const [index,buffer] of [Buffer.alloc(0),Buffer.alloc(17),Buffer.alloc(65536)].entries()) {
        await page.locator('#cartridge').setInputFiles({name:'invalid.rom',mimeType:'application/octet-stream',buffer});
        await expect(page.locator('#status')).toContainText('unchanged');
        await enter(page,'RUN');
        await expect.poll(()=>page.locator('#transcript').textContent()).toMatch(/\? $/);
        await enter(page,`SAVED${index}`);
        await expect(page.locator('#transcript')).toContainText(`SAVED${index}42\nREADY`);
      }
      // SCREEN 0 visibly prints; 1 writes a white tile through emulated VRAM.
      await expect.poll(()=>page.locator('#screen').evaluate(c=>{
        const d=c.getContext('2d').getImageData(0,0,256,192).data;
        return Array.from({length:256*192},(_,i)=>d[i*4]===255&&d[i*4+1]===255&&d[i*4+2]===255).filter(Boolean).length;
      })).toBeGreaterThan(100);
      // Pause/resume refreshes the status after the deliberately failed imports.
      await page.locator('#pause').click(); await page.locator('#pause').click();
      await page.screenshot({path:`test-results/basic-msx${Number(model)+1}-screen0-${path==='/'?'root':'pages'}.png`});
      for(const mode of [1,2,3,...(model==='1'?[5,6,7,8]:[])]) {
        const color=mode===6?3:mode===8?28:15;
        const draw=mode===1?'VPOKE 0,255:VPOKE 6144,0':`LINE (4,4)-(120,180),${color},B:PAINT (5,5),${color}:CIRCLE (180,96),40,${color}:PSET (250,190),${color}`;
        // SCREEN 3 shares the existing drawing primitives; SCREEN 1 uses tile VRAM.
        await enter(page,`SCREEN ${mode}:${draw}`);
        await expect(page.locator('#screen')).toHaveAttribute('width',[6,7].includes(mode)?'512':'256');
        await expect(page.locator('#screen')).toHaveAttribute('height',mode>=5?'212':'192');
        const expected=mode===8?[255,0,0]:mode===6?[109,255,109]:[255,255,255];
        await expect.poll(()=>pixel(page,mode===1?0:4,mode===1?0:4)).toEqual(expected);
        if(mode!==1) await expect.poll(()=>pixel(page,220,96)).toEqual(expected);
        await page.screenshot({path:`test-results/basic-msx${Number(model)+1}-screen${mode}-${path==='/'?'root':'pages'}.png`});
        await page.keyboard.press('Enter'); await enter(page,'PRINT 73');
        await expect(page.locator('#transcript')).toContainText('73\nREADY');
      }
      // Open ROM is the real file picker, with an independently authored BIOS service fixture.
      const fixture=model==='0'?'dist/cbios-msx1-test-cartridge.rom':'dist/cbios-test-cartridge.rom';
      const chooser=page.waitForEvent('filechooser'); await page.locator('#open-rom').click();
      await (await chooser).setFiles(fixture); await pattern(page);
      await expect(page.locator('#status')).toContainText(`MSX${Number(model)+1} C-BIOS`);
      await expect(page.locator('#example')).toBeDisabled();
      await expect(page.locator('#transcript')).toHaveText('');
      await page.locator('#screen').focus(); await page.keyboard.down('Space');
      await expect.poll(()=>pixel(page,8,16)).toEqual([255,255,255]);
      await page.keyboard.up('Space'); await expect.poll(()=>pixel(page,8,16)).toEqual([0,0,0]);
      await page.locator('#reset').click(); await pattern(page);
      await expect(page.locator('#model')).toHaveValue(model);
      await page.locator('#eject').click(); await enter(page,'PRINT 91');
      await expect(page.locator('#model')).toHaveValue(model);
      await expect(page.locator('#status')).toContainText('Weave BASIC');
      await expect(page.locator('#transcript')).toContainText('91\nREADY');
      await enter(page,'LIST'); await expect(page.locator('#transcript')).not.toContainText('20 PRINT');
      await page.locator('#advanced-panel > summary').click();
      await page.locator('#firmware').selectOption('cbios');
      await page.locator('#cartridge').setInputFiles(fixture); await pattern(page);
      await page.locator('#eject').click(); await expect(page.locator('#firmware')).toHaveValue('cbios');
      await expect(page.locator('#transcript')).toHaveText('');
      // Locally selected legal C-BIOS is a user override, retained on import/eject.
      await page.locator('#bios').setInputFiles(model==='0'?'dist/cbios_main_msx1.rom':'dist/cbios_main_msx2.rom');
      if(model==='1') { await page.locator('#subrom').setInputFiles('dist/cbios_sub.rom'); await page.locator('#apply-firmware').click(); }
      await expect(page.locator('#status')).toContainText('user BIOS');
      await page.locator('#cartridge').setInputFiles(fixture); await pattern(page);
      await page.locator('#eject').click(); await expect(page.locator('#firmware')).toHaveValue('local');
      await expect(page.locator('#status')).toContainText('user BIOS');
      await page.locator('#builtin').click(); await enter(page,'PRINT 86');
      await expect(page.locator('#model')).toHaveValue(model);
      await expect(page.locator('#firmware')).toHaveValue('auto');
      await expect(page.locator('#transcript')).toContainText('86\nREADY');
      expect(errors).toEqual([]);
    });
  }
}

test('a deferred paired firmware import cannot replace BASIC after Eject or accept a concurrent ROM',async({page})=>{
  await page.addInitScript(()=>{
    const read=File.prototype.arrayBuffer;
    File.prototype.arrayBuffer=async function(){
      if(window.deferMain && this.size===32768) {
        window.deferMain=false; await new Promise(resolve=>{window.finishMain=resolve;});
      }
      return read.call(this);
    };
  });
  await page.goto('/'); await expect(page.locator('#transcript')).toContainText('READY');
  await page.locator('#model').selectOption('1');
  await page.locator('#cartridge').setInputFiles('dist/cbios-test-cartridge.rom'); await pattern(page);
  await page.locator('#advanced-panel > summary').click();
  await page.locator('#bios').setInputFiles('dist/cbios_main_msx2.rom');
  await page.locator('#subrom').setInputFiles('dist/cbios_sub.rom');
  await page.evaluate(()=>{window.deferMain=true;});
  await page.locator('#apply-firmware').click();
  await expect.poll(()=>page.evaluate(()=>typeof window.finishMain)).toBe('function');
  await expect(page.locator('#open-rom')).toBeDisabled();
  // Even a synthetic second file-change event cannot commit a concurrent upload.
  await page.locator('#cartridge').setInputFiles('dist/msx2-test-cartridge.rom');
  await expect.poll(()=>pixel(page,1,16)).toEqual([0,0,0]);
  await page.locator('#eject').click();
  await page.evaluate(()=>window.finishMain());
  await enter(page,'PRINT 64');
  await expect(page.locator('#status')).toHaveText('Running · MSX2 · Weave BASIC');
  await expect(page.locator('#firmware')).toHaveValue('auto');
  await expect(page.locator('#transcript')).toContainText('64\nREADY');
  await expect(page.locator('#example')).toBeEnabled();
});
