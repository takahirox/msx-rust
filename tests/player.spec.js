import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

test.use({ launchOptions: { args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'] } });

const display = page => page.locator('#screen');
const picker = (page, media) => page.getByRole('region', { name: `Insert ${media}` });
const status = page => page.locator('#status');

async function boot(page, url = '/') {
  await page.goto(url);
  await expect(status(page)).toHaveText('Running · MSX2 · Weave BASIC');
  await expect(page.getByLabel('Display', { exact: true })).toHaveValue('crt');
}

async function chooseFile(page, button, files) {
  const chooser = page.waitForEvent('filechooser');
  await button.click();
  await (await chooser).setFiles(files);
}

test('tabbing out of Help on a phone reveals the next page controls', async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 812 });
  await boot(page);
  const help = page.getByRole('button', { name: 'Help', exact: true });
  const panel = page.getByRole('region', { name: 'Help' });
  await help.focus();
  await page.keyboard.press('Enter');
  await page.keyboard.press('Tab');
  await expect(panel.getByRole('button', { name: 'Close Help' })).toBeFocused();

  const lastLink = panel.getByRole('link', { name: 'Documentation' });
  await lastLink.focus();
  await page.keyboard.press('Tab');
  await expect(panel).toBeHidden();
  await expect(page.getByRole('link', { name: 'GitHub' })).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(display(page)).toBeFocused();
  await expect(display(page)).toBeInViewport({ ratio: 1 });
});

test('sound cartridge changes keep keyboard focus in phone Settings', async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 812 });
  await boot(page);
  const settings = page.getByRole('button', { name: 'Settings', exact: true });
  const panel = page.getByRole('region', { name: 'Settings' });
  await settings.click();
  const install = panel.getByRole('button', { name: 'Install sound cartridge' });
  const remove = panel.getByRole('button', { name: 'Remove sound cartridge' });
  await install.focus();
  await page.keyboard.press('Enter');
  await expect(status(page)).toContainText('Snatcher sound cartridge');
  await expect(panel).toBeVisible();
  await expect(install).toBeFocused();
  await expect(install).toBeInViewport({ ratio: 1 });

  await page.keyboard.press('Tab');
  await expect(remove).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(status(page)).toHaveText('Running · MSX2 · Weave BASIC');
  await expect(remove).toBeDisabled();
  await expect(install).toBeFocused();
  await expect(install).toBeInViewport({ ratio: 1 });
  await page.keyboard.press('Escape');
  await expect(panel).toBeHidden();
  await expect(settings).toBeFocused();
});

test('keyboard dismissal, demo and local ROM share the Pages cartridge picker', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await boot(page, 'http://127.0.0.1:41874/msx-rust/');
  await expect(page.locator('#crt-screen')).toBeVisible();
  await page.screenshot({ path: 'test-results/player-desktop-crt.png' });
  const screenBounds = await display(page).boundingBox();
  const cartridge = page.getByRole('button', { name: 'Insert cartridge', exact: true });
  const insert = picker(page, 'cartridge');
  await cartridge.focus();
  await page.keyboard.press('Enter');
  await expect(insert).toBeVisible();
  await page.keyboard.press('Tab');
  await expect(insert.getByRole('button', { name: 'Close cartridge picker' })).toBeFocused();
  await page.screenshot({ path: 'test-results/player-cartridge-picker.png' });
  await page.keyboard.press('Escape');
  await expect(insert).toBeHidden();
  await expect(cartridge).toBeFocused();
  await expect(page.locator('#transcript')).not.toContainText('BREAK');

  await cartridge.click();
  await chooseFile(page, insert.getByRole('button', { name: 'Open local ROM…' }), []);
  await expect(insert).toBeVisible();
  await expect(status(page)).toHaveText('Running · MSX2 · Weave BASIC');
  await insert.getByRole('button', { name: 'plustaker MSX1' }).click();
  await expect(status(page)).toContainText('MSX1 C-BIOS');
  await expect(page.locator('#cartridge-status')).toHaveText('plustaker (Plain)');
  await expect(page.getByRole('region', { name: 'BASIC starters' })).toBeHidden();
  await expect(display(page)).toBeFocused();

  await cartridge.click();
  const filename = `local-${'long-name-'.repeat(18)}.rom`;
  await chooseFile(page, insert.getByRole('button', { name: 'Open local ROM…' }), {
    name: filename, mimeType: 'application/octet-stream', buffer: await readFile('dist/cbios-msx1-test-cartridge.rom'),
  });
  await expect(page.locator('#cartridge-status')).toContainText(filename);
  await expect(display(page)).toBeFocused();
  await cartridge.click();
  await chooseFile(page, insert.getByRole('button', { name: 'Open local ROM…' }), {
    name: 'invalid.rom', mimeType: 'application/octet-stream', buffer: Buffer.alloc(17),
  });
  await expect(status(page)).toContainText('Selected ROMs are unchanged');
  await expect(page.locator('#cartridge-message')).toBeVisible();
  await expect(page.locator('#cartridge-message')).toContainText('ROM import failed');
  await expect(page.locator('#cartridge-status')).toContainText(filename);
  expect(await display(page).boundingBox()).toEqual(screenBounds);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(1280);

  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: 'Use Weave BASIC' }).click();
  await expect(page.getByRole('region', { name: 'Settings' })).toBeHidden();
  await expect(status(page)).toHaveText('Running · Weave BASIC');
  await expect(page.locator('#cartridge-status')).toHaveText('Empty');
  await page.getByRole('button', { name: 'BASIC example' }).click();
  await expect(page.locator('#transcript')).toContainText('1\n2\n3\n4\n5\nREADY');
});

test('a canceled import can retry the same file while its original read is pending', async ({ page }) => {
  await page.addInitScript(() => {
    const read = File.prototype.arrayBuffer;
    window.pendingFileReads = [];
    File.prototype.arrayBuffer = async function () {
      const bytes = await read.call(this);
      return new Promise(resolve => window.pendingFileReads.push(() => resolve(bytes)));
    };
  });
  await boot(page);
  const cases = [
    ['insert-cartridge', 'open-rom', 'cartridge', 'cartridge-status', 'cbios-test-cartridge.rom'],
    ['insert-disk', 'open-dsk', 'disk', 'disk-status', 'weave-disk-demo.dsk'],
    ['insert-cassette', 'open-cas', 'cassette', 'tape-status', 'cassette-test.cas'],
    ['settings', 'open-disk-rom', 'disk-rom', 'disk-profile-status', 'weave-disk.rom'],
    ['settings', 'open-music-rom', 'music-rom', 'music-status', 'music-diagnostic-firmware.rom'],
    ['settings', 'open-pac-rom', 'pac-rom', 'pac-status', 'fm-pac-diagnostic-firmware.rom'],
  ];
  for (const [trigger, open, target, slot, filename] of cases) {
    await test.step(target, async () => {
      const slotStatus = page.locator(`#${slot}`);
      const initial = await slotStatus.textContent();
      const cancel = page.locator(`#cancel-${target}-import`);
      const selectFile = async () => {
        const openFile = page.locator(`#${open}`);
        if (!await openFile.isVisible()) await page.locator(`#${trigger}`).click();
        // A repeated filesystem path exercises Chromium's native same-file behavior.
        await chooseFile(page, openFile, `dist/${filename}`);
      };
      await selectFile();
      const pendingReads = () => page.evaluate(() => window.pendingFileReads.length);
      await expect.poll(pendingReads).toBe(1);
      await cancel.click();
      await expect(status(page)).toContainText('Import canceled');
      await expect(slotStatus).toHaveText(initial);

      await selectFile();
      await expect.poll(pendingReads).toBe(2);
      await page.evaluate(() => window.pendingFileReads.shift()());
      await expect(cancel).toBeVisible();
      await expect(slotStatus).toHaveText(initial);

      await page.evaluate(() => window.pendingFileReads.shift()());
      await expect(slotStatus).toContainText(filename);
      await expect(cancel).toBeHidden();
    });
  }
});

test('mobile media, scale and browser output gain preserve the paused machine', async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 812 });
  await page.addInitScript(() => {
    const Audio = window.AudioContext;
    window.playerAudio = [];
    window.AudioContext = class extends Audio {
      constructor(...args) {
        super(...args);
        const evidence = { context: this, output: undefined, routed: false };
        window.playerAudio.push(evidence);
        const createGain = this.createGain.bind(this);
        this.createGain = () => (evidence.output = createGain());
        const createSource = this.createBufferSource.bind(this);
        this.createBufferSource = () => {
          const source = createSource();
          const connect = source.connect.bind(source);
          source.connect = target => {
            evidence.routed ||= target === evidence.output;
            return connect(target);
          };
          return source;
        };
      }
    };
  });
  await boot(page);
  const volume = page.getByRole('slider', { name: 'Volume' });
  await volume.fill('35');
  expect(await page.evaluate(() => window.playerAudio.length)).toBe(0);
  await page.getByRole('button', { name: 'Sound demo' }).click();
  await expect(page.locator('#transcript')).toContainText('>BEEP');
  await expect.poll(() => page.evaluate(() => window.playerAudio[0].routed)).toBe(true);
  expect(await page.evaluate(() => window.playerAudio[0].output.gain.value)).toBeCloseTo(0.35);
  await volume.focus();
  await page.keyboard.press('ArrowRight');
  await expect(volume).toHaveValue('36');
  await expect.poll(() => page.evaluate(() => window.playerAudio[0].output.gain.value)).toBeCloseTo(0.36, 3);

  await page.getByRole('button', { name: 'Pause', exact: true }).click();
  await page.getByRole('button', { name: 'Mute audio' }).click();
  await expect.poll(() => page.evaluate(() => window.playerAudio[0].context.state)).toBe('suspended');
  await page.getByRole('button', { name: 'Enable audio' }).click();
  expect(await page.evaluate(() => window.playerAudio.length)).toBe(1);
  await page.getByRole('combobox', { name: 'Display scale' }).selectOption('1');
  expect((await display(page).boundingBox()).width).toBe(256);
  await page.getByRole('combobox', { name: 'Display scale' }).selectOption('2');
  expect((await display(page).boundingBox()).width).toBeGreaterThan(256);
  expect((await display(page).boundingBox()).width).toBeLessThan(375);
  await page.getByRole('combobox', { name: 'Display', exact: true }).selectOption('clean');
  await expect(page.locator('#crt-screen')).toBeHidden();
  await page.getByRole('combobox', { name: 'Display', exact: true }).selectOption('crt');
  await expect(page.locator('#crt-screen')).toBeVisible();
  expect(await page.locator('#crt-screen').boundingBox()).toEqual(await display(page).boundingBox());

  const help = page.getByRole('button', { name: 'Help', exact: true });
  await help.click();
  await page.locator('#transcript-panel > summary').click();
  await expect(page.getByLabel('BASIC transcript')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(help).toBeFocused();
  await expect(page.getByRole('region', { name: 'Help' })).toBeHidden();

  await page.getByRole('button', { name: 'Insert disk', exact: true }).click();
  await picker(page, 'disk').getByRole('button', { name: 'Weave disk demo' }).click();
  await expect(status(page)).toContainText('Choose Boot disk');
  await expect(page.locator('#disk-status')).toContainText('weave-disk-demo.dsk');
  await expect(page.getByRole('button', { name: 'Resume', exact: true })).toBeVisible();
  await expect(page.locator('#transcript')).toContainText('>BEEP');

  await page.getByRole('button', { name: 'Insert cassette', exact: true }).click();
  const tape = picker(page, 'cassette');
  await tape.getByLabel('CAS rate').selectOption('2400');
  await chooseFile(page, tape.getByRole('button', { name: 'Open local CAS…' }), 'dist/cassette-test.cas');
  await expect(page.locator('#tape-status')).toContainText('cassette-test.cas · 2400 baud · paused');
  await page.getByRole('button', { name: 'Rewind tape' }).click();
  await page.getByRole('button', { name: 'Eject tape' }).click();
  await expect(page.locator('#tape-status')).toHaveText('Empty');
  await expect(page.getByRole('button', { name: 'Insert cassette', exact: true })).toBeFocused();
  await expect(page.getByRole('button', { name: 'Resume', exact: true })).toBeVisible();

  await page.getByRole('button', { name: 'Boot disk' }).click();
  await expect(status(page)).toContainText('Weave disk firmware');
  await page.getByLabel('Read-only', { exact: true }).uncheck();
  await display(page).focus();
  await page.keyboard.down('Space');
  await expect(page.locator('#disk-status')).toContainText('unsaved modifications — export to keep');
  await page.keyboard.up('Space');
  await page.getByRole('button', { name: 'Eject disk' }).click();
  await expect(status(page)).toContainText('Export modified DSK before ejecting');
  await expect(page.locator('#disk-message')).toBeVisible();
  await expect(page.locator('#disk-status')).toHaveAttribute('data-warning', 'true');
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export DSK' }).click();
  expect((await download).suggestedFilename()).toBe('weave-disk-demo-modified.dsk');
  await page.getByRole('button', { name: 'Eject disk' }).click();
  await expect(page.locator('#disk-status')).toHaveText('Empty');
});
