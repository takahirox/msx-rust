import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

// The exact original CPU fixtures also run in the native bus tests. They contain
// no BIOS traps, proprietary bytes, WV signature or dependencies on Weave firmware.
async function fixture(name, size, fill = 0) {
  const bytes = Buffer.alloc(size, fill);
  for (const line of (await readFile(`tests/fixtures/philips-${name}.hex`, 'utf8')).split('\n')) {
    if (!line || line.startsWith('#')) continue;
    const [offset, data] = line.split(':');
    Buffer.from(data.trim().split(/\s+/).map(value => parseInt(value, 16))).copy(bytes, parseInt(offset, 16));
  }
  return bytes;
}
const diskRom = await fixture('disk', 16384, 255);
const systemRom = await fixture('system', 32768);
const sites = ['http://127.0.0.1:41873/', 'http://127.0.0.1:41874/msx-rust/'];
const file = (name, buffer) => ({name, buffer, mimeType: 'application/octet-stream'});
async function exportDisk(page) {
  const download = page.waitForEvent('download');
  await page.locator('#export-disk').click();
  return readFile(await (await download).path());
}
async function installSystem(page, model) {
  const main = Buffer.from(systemRom); main[0x2d] = Number(model);
  await page.locator('#bios').setInputFiles(file('original-system.rom', main));
  if (model === '1') {
    const sub = Buffer.alloc(16384); sub.write('CD'); sub[0x100] = 0x7b;
    await page.locator('#subrom').setInputFiles(file('original-sub.rom', sub));
    await page.locator('#apply-firmware').click();
  }
  await expect(page.locator('#firmware')).toHaveValue('local');
}
async function observeWasm(page) {
  await page.addInitScript(() => {
    const instantiate = WebAssembly.instantiate;
    WebAssembly.instantiate = async (...args) => {
      const result = await instantiate(...args), exports = result.instance.exports;
      window.philipsWasm = exports;
      return {...result, instance: {exports: {...exports, msx_upload(n) {
        return window.failPhilipsUpload ? 0 : exports.msx_upload(n);
      }}}};
    };
    const read = File.prototype.arrayBuffer;
    File.prototype.arrayBuffer = async function () {
      // Keep cartridge import pending long enough to exercise the BIOS import wait.
      if (this.name === 'mapper-6.rom') await new Promise(resolve => setTimeout(resolve, 300));
      if (this.name === 'deferred.rom') await new Promise(resolve => window.finishPhilips = resolve);
      if (this.name === 'unreadable.rom') throw new Error('fixture read failure');
      return read.call(this);
    };
  });
}
async function greenOutput(page, model) {
  await expect.poll(() => page.locator('#screen').evaluate(canvas => {
    const data = canvas.getContext('2d').getImageData(20, 20, 1, 1).data;
    return [...data];
  })).toEqual(model === '1' ? [36, 218, 36, 255] : [33, 200, 66, 255]);
}
for (const site of sites) {
  for (const model of ['0', '1']) {
    for (const size of [368640, 737280]) {
      test(`Philips CPU disk output, exact export, BIOS/SRAM and lifecycle MSX${Number(model)+1} ${size} ${site}`, async ({page}) => {
        await observeWasm(page);
        const requests = [], errors = [];
        page.on('request', request => requests.push(request)); page.on('pageerror', error => errors.push(error.message));
        await page.goto(site); await expect(page.locator('#transcript')).toContainText('READY');
        if (model === '0') await page.locator('#model').selectOption('0');
        await page.locator('#advanced-panel > summary').click();
        await page.locator('#mapper').selectOption('6');
        await page.locator('#cartridge').setInputFiles('dist/mapper-6.rom');
        await expect(page.locator('#status')).toContainText('mapper-6.rom (ASCII8SRAM2)');
        await expect(page.locator('#emulator')).toHaveAttribute('aria-busy', 'false');
        await expect(page.locator('#export-save')).toBeEnabled();
        await installSystem(page, model);
        const disk = await fixture('sector', size, 0x35);
        disk[0x1f1] = Number(size === 737280);
        expect(disk.subarray(510,512).toString()).not.toBe('WV');
        await page.locator('#disk').setInputFiles(file('original-non-wv.dsk', disk));
        await expect(page.locator('#disk-status')).toContainText('original-non-wv.dsk');
        const picker = page.waitForEvent('filechooser');
        await page.locator('#open-disk-rom').click();
        await (await picker).setFiles(file('original-disk.rom', diskRom));
        await expect(page.locator('#disk-profile-status')).toContainText('Philips NMS1200-style · original-disk.rom');
        await expect(page.locator('#firmware')).toHaveValue('local');
        await greenOutput(page, model); // sector executed real VDP writes despite write protection
        expect(await exportDisk(page)).toEqual(disk);
        await page.locator('#disk-protected').uncheck();
        await page.locator('#boot-disk').click();
        await greenOutput(page, model);
        await expect(page.locator('#disk-status')).toContainText('unsaved modifications');
        await expect(page.locator('#status')).toContainText('user BIOS');
        await expect(page.locator('#status')).toContainText('Philips NMS1200-style');
        const expected = Buffer.from(disk), sector = size === 737280 ? 17 : 8;
        expected.fill(0xa6, sector*512, (sector+1)*512);
        await page.locator('#eject-disk').click();
        await expect(page.locator('#status')).toContainText('Export modified DSK before ejecting');
        expect(await exportDisk(page)).toEqual(expected);
        const sram = await page.evaluate(() => {
          const w = window.philipsWasm;
          return new Uint8Array(w.memory.buffer, w.msx_sram(), w.msx_sram_len())[0x123];
        });
        expect(sram).toBe(0x6d);
        await page.locator('#reset').click(); await greenOutput(page, model);
        await page.locator('#eject-disk').click();
        await expect(page.locator('#disk-status')).toHaveText('Drive A: empty');
        await expect(page.locator('#firmware')).toHaveValue('local');
        await expect(page.locator('#disk-profile-status')).toContainText('Philips');
        // Removal keeps explicit system firmware and SRAM; legacy protection still applies.
        await page.locator('#remove-disk-rom').click();
        await expect(page.locator('#disk-profile-status')).toContainText('Weave disk/demo boot option');
        await expect(page.locator('#firmware')).toHaveValue('local');
        expect(await page.evaluate(() => {
          const w = window.philipsWasm;
          return new Uint8Array(w.memory.buffer, w.msx_sram(), w.msx_sram_len())[0x123];
        })).toBe(0x6d);
        await page.locator('#builtin').click();
        await expect(page.locator('#transcript')).toContainText('READY');
        await page.locator('#disk').setInputFiles('dist/weave-disk-demo.dsk');
        await page.locator('#boot-disk').click();
        await expect(page.locator('#status')).toContainText('Weave disk firmware');
        await expect.poll(() => page.evaluate(() => window.philipsWasm.msx_disk_boot_state())).toBe(1);
        expect(errors).toEqual([]);
        expect(requests.every(request => request.method() === 'GET' && request.url().startsWith(site))).toBe(true);
      });
    }
  }
  test(`Philips failed, canceled and stale imports preserve interface/session/media ${site}`, async ({page}) => {
    await observeWasm(page);
    await page.goto(site); await expect(page.locator('#transcript')).toContainText('READY');
    await page.locator('#advanced-panel > summary').click();
    await page.locator('#disk').setInputFiles('dist/weave-disk-demo.dsk');
    await expect(page.locator('#disk-status')).toContainText('weave-disk-demo.dsk');
    await page.locator('#disk-rom').setInputFiles(file('retained.rom', diskRom));
    await expect(page.locator('#disk-profile-status')).toContainText('retained.rom');
    await page.locator('#screen').focus(); await page.keyboard.type('10 PRINT 73'); await page.keyboard.press('Enter');
    for (const [name, bytes] of [['empty.rom', Buffer.alloc(0)], ['short.rom', diskRom.subarray(0,-1)], ['no-header.rom', Buffer.alloc(16384)], ['wrong.dsk', diskRom], ['unreadable.rom', diskRom]]) {
      await page.locator('#disk-rom').setInputFiles(file(name, bytes));
      await expect(page.locator('#status')).toContainText('Session, interface and media unchanged');
      await expect(page.locator('#disk-profile-status')).toContainText('retained.rom');
      await expect(page.locator('#disk-status')).toContainText('weave-disk-demo.dsk');
    }
    await page.evaluate(() => window.failPhilipsUpload = true);
    await page.locator('#disk-rom').setInputFiles(file('allocation.rom', diskRom));
    await expect(page.locator('#status')).toContainText('Session, interface and media unchanged');
    await page.evaluate(() => window.failPhilipsUpload = false);
    const rejected = await page.evaluate(() => {
      const w = window.philipsWasm;
      w.msx_upload(16384); w.msx_cancel_upload();
      return [w.msx_install_disk_rom(16384, 1), w.msx_disk_profile(), w.msx_disk_flags()];
    });
    expect(rejected).toEqual([0,1,3]);
    await page.locator('#screen').focus(); await page.keyboard.type('RUN'); await page.keyboard.press('Enter');
    await expect(page.locator('#transcript')).toContainText('73\nREADY');
    // A native picker cancellation has no file and must leave installation alone.
    await page.locator('#disk-rom').setInputFiles([]);
    await expect(page.locator('#disk-profile-status')).toContainText('retained.rom');
    await page.locator('#disk-rom').setInputFiles(file('deferred.rom', diskRom));
    await expect.poll(() => page.evaluate(() => typeof window.finishPhilips)).toBe('function');
    await expect(page.locator('#open-rom')).toBeDisabled();
    await page.locator('#remove-disk-rom').click();
    await page.evaluate(() => window.finishPhilips());
    await expect(page.locator('#disk-profile-status')).toContainText('retained.rom');
    await page.locator('#screen').focus(); await page.keyboard.type('RUN'); await page.keyboard.press('Enter');
    await expect(page.locator('#transcript')).toContainText('73\nREADY');
    await page.locator('#remove-disk-rom').click();
    await expect(page.locator('#disk-profile-status')).toContainText('no external interface');
    expect(await exportDisk(page)).toEqual(await readFile('dist/weave-disk-demo.dsk'));
    await expect(page.locator('#disk-status')).toContainText('weave-disk-demo.dsk');
    await page.locator('#disk-rom').setInputFiles(file('retained.rom', diskRom));
    await expect(page.locator('#disk-profile-status')).toContainText('retained.rom');
    await page.locator('#firmware').selectOption('cbios');
    await page.locator('#boot-disk').click();
    await expect(page.locator('#firmware')).toHaveValue('cbios');
    await expect(page.locator('#disk-profile-status')).toContainText('retained.rom');
    await page.locator('#model').selectOption('0');
    await expect(page.locator('#disk-profile-status')).toContainText('retained.rom');
    await page.locator('#cartridge').setInputFiles('dist/cbios-msx1-test-cartridge.rom');
    await expect(page.locator('#disk-profile-status')).toContainText('retained.rom');
    await page.locator('#eject').click();
    await expect(page.locator('#disk-profile-status')).toContainText('retained.rom');
    await page.locator('#remove-disk-rom').click(); await page.locator('#builtin').click();
    await expect(page.locator('#transcript')).toContainText('READY');
  });
}
