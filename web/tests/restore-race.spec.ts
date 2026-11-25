// The page loads the parser and the gallery before it puts back the work that this browser remembers. What the person did
// meanwhile (typed a schema, opened a file) must not be taken away by it.

import { expect, test } from '@playwright/test';

test('what is done while the page is still loading is not replaced by the work that was remembered', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('body')).toHaveAttribute('data-help', 'ready');
  // a first visit leaves a schema and a file name behind
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText('meta:\n  id: old\nseq:\n  - id: x\n    type: u1\n');
  await page.setInputFiles('#open-input', { name: 'old.bin', mimeType: 'application/octet-stream', buffer: Buffer.from([1]) });
  await expect(page.locator('#status')).toContainText('old.bin, 1 bytes');
  // the second visit has a slow gallery: the work is put back only once it has come
  let release: () => void = () => undefined;
  const slow = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route('**/gallery.json*', async (route) => {
    await slow;
    await route.continue();
  });
  await page.reload();
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText('meta:\n  id: fresh\nseq:\n  - id: y\n    type: u1\n');
  await page.setInputFiles('#open-input', { name: 'fresh.bin', mimeType: 'application/octet-stream', buffer: Buffer.from([2]) });
  await expect(page.locator('#status')).toContainText('fresh.bin, 1 bytes');
  release();
  // let the page finish loading: its remembered work comes now, and must not replace what was just done
  await expect(page.locator('body')).toHaveAttribute('data-help', 'ready');
  await page.waitForTimeout(1500);
  await expect(page.locator('#status')).toContainText('fresh.bin, 1 bytes');
  await expect(page.locator('#status')).not.toContainText('again');
  await expect(page.locator('.cm-content')).toContainText('id: fresh');
  await expect(page.locator('.cm-content')).not.toContainText('id: old');
});
