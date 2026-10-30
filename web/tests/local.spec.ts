// The page served by `doe --ide`: the types of the machine, and a file too
// large for the browser that stays where it is. (local-server.mjs starts it.)

import { readFileSync } from 'node:fs';
import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
});

test('the types of the machine are offered and open with their imports', async ({ page }) => {
  await expect(page.locator('#types-box')).toBeVisible();
  const options = await page.locator('#types option').allTextContents();
  expect(options).toEqual(['choose…', 'common — two bytes', 'pair']);
  await page.locator('#types').selectOption({ label: 'pair' });
  await expect(page.locator('.cm-content')).toContainText('imports');
  // the import comes from the machine too: no problems, though "common" is not in the page
  await expect(page.locator('.problems')).toContainText('No problems.');
  await expect(page.locator('#schemas li:not(.main) code')).toHaveCount(1);
});

test('a file larger than the browser limit is read and parsed by the server', async ({ page }) => {
  const use = page.locator('#use-input');
  await expect(use).toHaveText('Use big.bin');
  await page.locator('#types').selectOption({ label: 'pair' });
  await use.click();
  await expect(page.locator('#status')).toContainText('big.bin, 314572800 bytes (on this machine)', { timeout: 20_000 });
  await expect(page.locator('#status')).toContainText('parsed as pair');
  // the fields, from the first bytes of the file
  const a = page.locator('.tree-row', { has: page.locator('.name', { hasText: /^a$/ }) });
  await expect(a).toContainText('1');
  // the bytes come in as they are shown
  await expect(page.locator('.hex-row').first().locator('.hex-bytes')).toContainText('0001');
  await page.locator('.hex').evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await expect(page.locator('.hex-row').last().locator('.hex-bytes')).toContainText('deadbeef');
});

test('the gallery is allowed by the page policy, and export works on the file that stays on the machine', async ({ page }) => {
  // The request is answered here; what is tested is that the policy of the server lets the page make it.
  await page.route('https://raw.githubusercontent.com/**', (route) =>
    route.fulfill({ body: 'meta:\n  id: from_gallery\nseq:\n  - id: a\n    type: u2be\n', contentType: 'text/plain' }),
  );
  const gallery = (await (await page.request.get('/gallery.json')).json()) as { formats: { bundled?: boolean }[] };
  // the first format of the Kaitai Struct gallery: the types of doe's own library come before and are served by the page
  const first = gallery.formats.findIndex((format) => !format.bundled);
  if (first >= 0) {
    await page.locator('#gallery').selectOption(String(first));
    await expect(page.locator('.cm-content')).toContainText('from_gallery');
  }
  // and the page may fetch a type of the library from the server that serves it
  const bundled = gallery.formats.findIndex((format) => format.bundled);
  expect(bundled).toBeGreaterThanOrEqual(0);
  await page.locator('#gallery').selectOption(String(bundled));
  await expect(page.locator('.cm-content')).toContainText('id: ar');

  await page.locator('#types').selectOption({ label: 'pair' });
  await page.locator('#use-input').click();
  await expect(page.locator('#status')).toContainText('parsed as pair', { timeout: 20_000 });
  await page.locator('#export-format').selectOption('json');
  await expect(page.locator('#export')).toBeEnabled();
  const [download] = await Promise.all([page.waitForEvent('download'), page.locator('#export').click()]);
  const text = readFileSync((await download.path()) ?? '', 'utf8');
  expect(JSON.parse(text)).toMatchObject({ a: 1 });
});
