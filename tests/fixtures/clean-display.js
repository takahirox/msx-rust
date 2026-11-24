import { test as base, expect } from '@playwright/test';
import { displayPreferenceKey } from '../../web/display.js';

// These tests inspect the exact Canvas framebuffer. Supply a Clean preference
// without writing anything into storage, which is itself under test.
const test = base.extend({
  context: async ({ context }, use) => {
    await context.addInitScript(key => {
      const getItem = Storage.prototype.getItem;
      Storage.prototype.getItem = function (name) {
        const value = getItem.call(this, name);
        return name === key && this === localStorage ? value ?? 'clean' : value;
      };
    }, displayPreferenceKey);
    await use(context);
  },
});

export { test, expect };
