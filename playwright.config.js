import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests',
  timeout: 30000,
  workers: 1,
  use: { baseURL: 'http://127.0.0.1:41873', headless: true, viewport: { width: 1120, height: 1100 } },
  webServer: { command: 'python3 -m http.server 41873 --bind 127.0.0.1 --directory dist', url: 'http://127.0.0.1:41873', reuseExistingServer: false },
});
