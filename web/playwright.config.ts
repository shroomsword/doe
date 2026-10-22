import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: 'tests',
  webServer: [
    { command: 'node serve.mjs', url: 'http://127.0.0.1:4173/', reuseExistingServer: false },
    // doe --ide, with the page built into it (cargo build -p doe-cli after npm run build)
    { command: 'node local-server.mjs', url: 'http://127.0.0.1:4174/', reuseExistingServer: false, env: { PORT: '4174' } },
  ],
  use: { baseURL: 'http://127.0.0.1:4173/' },
  projects: [
    { name: 'chromium', testIgnore: /local\.spec\.ts/, use: { browserName: 'chromium' } },
    { name: 'firefox', testIgnore: /local\.spec\.ts/, use: { browserName: 'firefox' } },
    { name: 'local', testMatch: /local\.spec\.ts/, use: { browserName: 'chromium', baseURL: 'http://127.0.0.1:4174/' } },
  ],
});
