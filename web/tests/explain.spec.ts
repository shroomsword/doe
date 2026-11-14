// Parse errors that point: the message names the byte and the field, the failed field is selected, and a suggestion
// that was tried can be applied with a click.

import { expect, test } from '@playwright/test';

const SCHEMA = 'id: p\nmeta:\n  endian: le\nseq:\n  - id: len\n    type: u2\n  - id: body\n    size: len\n';

test.use({ viewport: { width: 1400, height: 800 } });

test('a failed parse names the byte and the field, selects it, and a suggestion is applied with a click', async ({ page }) => {
  await page.goto('/');
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText(SCHEMA);
  // 0x0300 bytes of body in little-endian, 3 in big-endian
  await page.setInputFiles('#open-input', { name: 'p.bin', mimeType: 'application/octet-stream', buffer: Buffer.from([0x00, 0x03, 0x61, 0x62, 0x63]) });
  const message = page.locator('.problem', { has: page.locator('.kind', { hasText: /^parse$/ }) });
  await expect(message.locator('.message')).toHaveText('byte 0x2, in body: needs 768 bytes, 3 are left');
  // the failed field is selected, and the bytes are at it
  await expect(page.locator('.tree-row.selected .name')).toHaveText('body');
  // the suggestion was tried: the file does parse with the other byte order
  const fix = page.locator('.problem', { has: page.locator('.kind', { hasText: /^fix$/ }) });
  await expect(fix.locator('.message')).toContainText('big-endian');
  await fix.click();
  await expect(page.locator('#status')).toContainText('parsed as');
  await expect(page.locator('.cm-content')).toContainText('endian: be');
  await expect(page.locator('.problem')).toHaveCount(0);
});

test('a failure that has no known cause gets the plain message and no suggestion', async ({ page }) => {
  await page.goto('/');
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText(SCHEMA);
  // too long in either order
  await page.setInputFiles('#open-input', { name: 'q.bin', mimeType: 'application/octet-stream', buffer: Buffer.from([0x01, 0x01, 0x61]) });
  await expect(page.locator('.problem .message').first()).toHaveText('byte 0x2, in body: needs 257 bytes, 1 is left');
  await expect(page.locator('.problem', { has: page.locator('.kind', { hasText: /^fix$/ }) })).toHaveCount(0);
});
