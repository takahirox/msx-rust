import { test, expect } from '@playwright/test';

// Observe only the cartridge's actual Canvas pixels. No emulated RAM getter or
// JavaScript input stub can make these pass without Z80/PSG/C-BIOS/VDP execution.
async function visible(page) {
  return page.locator('#screen').evaluate(canvas => {
    const context = canvas.getContext('2d');
    return Array.from({ length: 15 }, (_, row) => {
      let value = 0;
      for (let bit = 0; bit < 8; bit++) {
        const p = context.getImageData((20 + bit) * 8, (4 + row) * 8, 1, 1).data;
        if (p[0] === 255 && p[1] === 255 && p[2] === 255) value |= 1 << bit;
        else if (p[0] !== 0 || p[1] !== 0 || p[2] !== 0) return -1;
      }
      return value;
    });
  });
}
const released = [0x7f,0x7f,0x7f,0x7f,0x7f,0x7f,0,0,0,0,0,0,255,255,255];
async function assertInputs(page, expected = released) {
  await expect.poll(() => visible(page)).toEqual(expected);
}
// Expected bytes are derived from the documented input/BIOS contracts. Direction
// numbers are explicit test inputs, not obtained from the emulator under test.
function expected(one, two, direction1 = 0, direction2 = 0, matrix = [255,255,255]) {
  return [0x40|one,0x40|two,0x7f,0x7f,0x40|one,0x40|two,direction1,direction2,
    one & 16 ? 0 : 255,two & 16 ? 0 : 255,one & 32 ? 0 : 255,two & 32 ? 0 : 255,...matrix];
}
async function boot(page, path, model) {
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  // Retain the real compiled instance for the legacy ABI compatibility check.
  await page.addInitScript(() => {
    const instantiate = WebAssembly.instantiate;
    WebAssembly.instantiate = async (...args) => {
      const result = await instantiate(...args);
      window.joystickWasm = result.instance.exports;
      return result;
    };
  });
  await page.goto(path);
  await expect(page.locator('#status')).toContainText('Running · MSX2 · Weave BASIC');
  if (model === '0') await page.locator('#model').selectOption(model);
  await page.locator('#cartridge').setInputFiles('dist/joystick-test-cartridge.rom');
  await expect(page.locator('#status')).toContainText('C-BIOS');
  await assertInputs(page);
  await page.locator('#screen').focus();
  return errors;
}

for (const path of ['/', 'http://127.0.0.1:41874/msx-rust/']) {
  for (const model of ['0', '1']) {
    test(`two joysticks and real BIOS calls, model ${model} at ${path}`, async ({ page }) => {
      test.setTimeout(90000);
      const errors = await boot(page, path, model);
      // Every direction and both triggers, preserving keyboard matrix delivery.
      for (const [key, connector, mask, direction, matrix] of [
        ['ArrowUp',0,0x3e,1,[255,255,255]], ['ArrowDown',0,0x3d,5,[255,255,255]],
        ['ArrowLeft',0,0x3b,7,[255,255,255]], ['ArrowRight',0,0x37,3,[255,255,255]],
        ['KeyZ',0,0x2f,0,[255,255,127]], ['KeyX',0,0x1f,0,[255,255,223]],
        ['KeyI',1,0x3e,1,[191,255,255]], ['KeyK',1,0x3d,5,[255,254,255]],
        ['KeyJ',1,0x3b,7,[127,255,255]], ['KeyL',1,0x37,3,[255,253,255]],
        ['KeyN',1,0x2f,0,[255,247,255]], ['KeyM',1,0x1f,0,[255,251,255]],
      ]) {
        await page.keyboard.down(key);
        await assertInputs(page, expected(connector === 0 ? mask : 63, connector === 1 ? mask : 63,
          connector === 0 ? direction : 0, connector === 1 ? direction : 0, matrix));
        await page.keyboard.up(key);
        await assertInputs(page);
      }
      // Up-right + both triggers on one, down-left + both on two.
      for (const key of ['ArrowUp','ArrowRight','KeyZ','KeyX','KeyK','KeyJ','KeyN','KeyM']) await page.keyboard.down(key);
      await assertInputs(page, expected(0x06,0x09,2,6,[127,242,95]));
      await page.keyboard.up('KeyZ');
      await assertInputs(page, expected(0x16,0x09,2,6,[127,242,223]));
      await page.keyboard.up('KeyK');
      await assertInputs(page, expected(0x16,0x0b,2,7,[127,243,223]));
      for (const key of ['ArrowUp','ArrowRight','KeyX','KeyJ','KeyN','KeyM']) await page.keyboard.up(key);
      await assertInputs(page);
      // Shift changes event.key but physical KeyI still drives up on connector 2.
      await page.keyboard.down('Shift');
      await page.keyboard.down('KeyI');
      await assertInputs(page, expected(63,0x3e,0,1,[191,255,255]));
      await page.keyboard.up('KeyI');
      await page.keyboard.up('Shift');
      await assertInputs(page);
      // Legacy setter remains connector one; upper host bits are masked and
      // invalid indexed calls cannot alter either connector in the compiled ABI.
      expect(await page.evaluate(() => {
        const w = window.joystickWasm;
        w.msx_joystick(0xffffffee);
        const ok = w.msx_joystick_indexed(1, 0xffffffd7);
        return [ok, w.msx_joystick_indexed(2, 0), w.msx_joystick_indexed(0xffffffff, 0)];
      })).toEqual([1,0,0]);
      await assertInputs(page, expected(0x2e,0x17,1,3));
      await page.evaluate(() => window.joystickWasm.msx_joystick(63));
      await assertInputs(page, expected(63,0x17,0,3));
      await page.evaluate(() => window.joystickWasm.msx_release_keys());
      await assertInputs(page);
      await page.screenshot({ path: `test-results/joysticks-${model}-${path === '/' ? 'root' : 'subpath'}.png` });
      expect(errors).toEqual([]);
    });

    test(`joystick focus and lifecycle releases, model ${model} at ${path}`, async ({ page }) => {
      test.setTimeout(90000);
      const errors = await boot(page, path, model);
      const hold = async () => {
        await page.locator('#screen').focus();
        await page.keyboard.down('ArrowUp');
        await page.keyboard.down('KeyN');
        await assertInputs(page, expected(0x3e,0x2f,1,0,[255,247,255]));
      };
      const up = async () => { await page.keyboard.up('ArrowUp'); await page.keyboard.up('KeyN'); };
      await hold();
      await page.locator('#focus-screen').focus(); // actual Canvas blur
      await assertInputs(page);
      await up();
      // Keyboard events outside the Canvas cannot drive either connector.
      await page.keyboard.down('ArrowRight');
      await page.keyboard.down('KeyM');
      await assertInputs(page);
      await page.keyboard.up('ArrowRight');
      await page.keyboard.up('KeyM');
      await hold();
      await page.evaluate(() => window.dispatchEvent(new Event('blur')));
      await assertInputs(page);
      // Unrelated release must not resurrect a stale held key after window blur.
      await page.keyboard.press('a');
      await assertInputs(page);
      await up();
      await hold();
      // Browser lifecycle notification (keyboard events above are real). Override
      // hidden only while notifying the app, then restore it to allow rendering.
      await page.evaluate(() => {
        Object.defineProperty(document, 'hidden', { configurable: true, value: true });
        document.dispatchEvent(new Event('visibilitychange'));
        delete document.hidden;
        document.dispatchEvent(new Event('visibilitychange'));
      });
      await assertInputs(page);
      await up();
      await hold();
      await page.locator('#pause').click();
      await expect(page.locator('#pause')).toHaveText('Resume');
      await up();
      // Resume releases keys pressed while paused as well as earlier holds.
      await page.keyboard.down('KeyM');
      await page.locator('#pause').click();
      await assertInputs(page);
      await page.keyboard.up('KeyM');
      await hold();
      await page.locator('#reset').click();
      await assertInputs(page);
      await page.keyboard.up('ArrowUp');
      await assertInputs(page); // releasing one cannot reassert the other
      await page.keyboard.up('KeyN');
      await hold();
      // Replacing a cartridge clears host-held and matrix state as well as core input.
      await page.locator('#cartridge').setInputFiles('dist/joystick-test-cartridge.rom');
      await assertInputs(page);
      await up();
      await assertInputs(page);
      await hold();
      await page.locator('#eject').click();
      await expect(page.locator('#transcript')).toContainText('READY');
      await page.locator('#cartridge').setInputFiles('dist/joystick-test-cartridge.rom');
      await assertInputs(page);
      await up();
      await assertInputs(page);
      await hold();
      await page.locator('#model').selectOption(model === '0' ? '1' : '0');
      await expect(page.locator('#transcript')).toContainText('READY');
      await page.locator('#cartridge').setInputFiles('dist/joystick-test-cartridge.rom');
      await assertInputs(page);
      await up();
      await assertInputs(page);
      expect(errors).toEqual([]);
    });
  }
}
