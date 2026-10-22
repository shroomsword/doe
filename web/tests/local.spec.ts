// The page served by `doe --ide`: the types of the machine, and a file too
// large for the browser that stays where it is. (local-server.mjs starts it.)

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
  await expect(page.locator('#files code')).toHaveCount(1);
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
