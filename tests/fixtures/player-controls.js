import { expect } from '@playwright/test';

export async function settingsControl(page, id) {
  if (!await page.locator('#advanced-panel:popover-open').count()) {
    await page.locator('#settings').click();
  }
  return page.locator(`#${id}`);
}

export async function mediaControl(page, id) {
  // Settings occupies the media side of the player while open.
  if (await page.locator('#advanced-panel:popover-open').count()) {
    await page.getByRole('button', { name: 'Close Settings' }).click();
  }
  return page.locator(`#${id}`);
}

export async function startSound(page) {
  const button = page.locator('#mute');
  if (await button.getAttribute('aria-label') === 'Start sound') await button.click();
  await expect(button).toHaveAccessibleName('Mute');
  await page.locator('#screen').focus();
}
