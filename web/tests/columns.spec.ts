// The columns of the Bytes pane: the dialog (by key and by button), the bytes in a row, the number kept, the view kept.

import { expect, test, type Page } from '@playwright/test';
import { COLUMNS_KEY, DEFAULT_COLUMNS, MAX_COLUMNS, columnsError, parseColumns } from '../src/columns.js';

test('a number of columns is a whole number from 1 to the largest', () => {
  expect(parseColumns('16')).toBe(16);
  expect(parseColumns(' 8 ')).toBe(8);
  expect(parseColumns('64')).toBe(64);
  for (const bad of ['0', '65', '-1', '1.5', 'two', '', ' ', '1e1', '03x', '99999', null, undefined]) expect(parseColumns(bad as string), String(bad)).toBeNull();
  expect(parseColumns('007')).toBe(7);
  expect(columnsError('')).toMatch(/enter a number/);
  expect(columnsError('x')).toMatch(/whole number/);
  expect(columnsError(String(MAX_COLUMNS + 1))).toMatch(/from 1 to 64/);
  expect(columnsError('2')).toBeNull();
  expect(DEFAULT_COLUMNS).toBe(16);
});

const SCHEMA = 'meta:\n  id: raw\nseq:\n  - id: a\n    type: u1\n  - id: b\n    type: u1\n  - id: rest\n    size-eos: true\n';

async function setUp(page: Page, size = 1000): Promise<void> {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText(SCHEMA);
  await page.setInputFiles('#open-input', { name: 'raw.bin', mimeType: 'application/octet-stream', buffer: Buffer.from(Array.from({ length: size }, (_, i) => i % 256)) });
  await expect(page.locator('#status')).toContainText(`raw.bin, ${size} bytes`);
  await expect(page.locator('.hex-row').first()).toBeVisible();
  await page.locator('#pane-hex').focus();
}

const stored = (page: Page) => page.evaluate((key) => window.localStorage.getItem(key), COLUMNS_KEY);
const perRow = (page: Page) => page.locator('.hex-row').first().locator('.b').count();
const offsets = (page: Page) => page.locator('.hex-row .hex-off').allTextContents();

test.describe('the Bytes pane', () => {
  test.use({ viewport: { width: 1600, height: 900 } });
  test.beforeEach(async ({ page }) => setUp(page));

  test('c and the button in the title bar open the same dialog; the button is an icon with a name and a tooltip naming the key', async ({ page }) => {
    const button = page.getByRole('button', { name: 'Columns of the bytes' });
    await expect(button).toHaveCount(1);
    expect((await button.textContent())?.trim()).toBe('');
    await expect(button.locator('svg')).toHaveCount(1);
    await expect(button).toHaveAttribute('title', 'Columns of the bytes (c)');
    await expect(button).toHaveAttribute('aria-haspopup', 'dialog');
    // in the title bar of the Bytes pane, before the maximize and close buttons
    const bar = (await page.locator('#pane-hex .pane-title').boundingBox())!;
    const box = (await button.boundingBox())!;
    expect(box.y).toBeGreaterThanOrEqual(bar.y);
    expect(box.y + box.height).toBeLessThanOrEqual(bar.y + bar.height);
    expect(box.x).toBeLessThan((await page.locator('#pane-hex .pane-maximize').boundingBox())!.x);
    await page.keyboard.press('c');
    const dialog = page.locator('#columns-dialog');
    await expect(dialog).toBeVisible();
    await expect(page.locator('#columns-input')).toBeFocused();
    await expect(page.locator('#columns-input')).toHaveValue('16');
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    await button.click();
    await expect(dialog).toBeVisible();
    await page.locator('#columns-cancel').click();
    await expect(dialog).toBeHidden();
    // c is a letter: it is text in a field
    await page.locator('.cm-content').click();
    await page.keyboard.press('c');
    await expect(dialog).toBeHidden();
  });

  test('Enter applies and the rows have that many bytes; Esc and Cancel change nothing; the quick choices fill the box; Reset goes back to 16', async ({ page }) => {
    expect(await perRow(page)).toBe(16);
    await page.keyboard.press('c');
    await page.locator('#columns-input').fill('8');
    await page.keyboard.press('Enter');
    await expect(page.locator('#columns-dialog')).toBeHidden();
    await expect(page.locator('#layout-live')).toHaveText('Bytes pane: 8 columns');
    expect(await perRow(page)).toBe(8);
    // the offset of each row follows, and so does the text beside the bytes
    expect((await offsets(page)).slice(0, 3)).toEqual(['00000000', '00000008', '00000010']);
    expect(await page.locator('.hex-row').first().locator('.a').count()).toBe(8);
    // Esc and Cancel change nothing
    await page.keyboard.press('c');
    await page.locator('#columns-input').fill('5');
    await page.keyboard.press('Escape');
    expect(await perRow(page)).toBe(8);
    await page.keyboard.press('c');
    await expect(page.locator('#columns-input')).toHaveValue('8');
    await page.locator('#columns-input').fill('4');
    await page.locator('#columns-cancel').click();
    expect(await perRow(page)).toBe(8);
    // the quick choices fill the box, Apply applies
    await page.keyboard.press('c');
    await page.locator('#columns-quick button', { hasText: /^24$/ }).click();
    await expect(page.locator('#columns-input')).toHaveValue('24');
    await page.locator('#columns-apply').click();
    expect(await perRow(page)).toBe(24);
    // Reset
    await page.keyboard.press('c');
    await page.locator('#columns-reset').click();
    expect(await perRow(page)).toBe(16);
    expect(await stored(page)).toBeNull();
    await expect(page.locator('#layout-live')).toHaveText('Bytes pane: 16 columns');
    await page.keyboard.press('c');
    await page.locator('#columns-input').fill('1');
    await page.keyboard.press('Enter');
    await expect(page.locator('#layout-live')).toHaveText('Bytes pane: 1 column');
    expect(await perRow(page)).toBe(1);
  });

  test('a number that is not a whole number in range is refused with a message, and the dialog stays open', async ({ page }) => {
    await page.keyboard.press('c');
    for (const [text, message] of [['0', /from 1 to 64/], ['65', /from 1 to 64/], ['', /enter a number/], ['2.5', /whole number/]] as const) {
      await page.locator('#columns-input').fill(text);
      await page.keyboard.press('Enter');
      await expect(page.locator('#columns-error')).toBeVisible();
      await expect(page.locator('#columns-error')).toHaveText(message);
      await expect(page.locator('#columns-input')).toHaveAttribute('aria-invalid', 'true');
      await expect(page.locator('#columns-dialog')).toBeVisible();
    }
    expect(await perRow(page)).toBe(16);
    await page.locator('#columns-input').fill('12');
    await expect(page.locator('#columns-error')).toBeHidden();
    await page.keyboard.press('Enter');
    await expect(page.locator('#columns-dialog')).toBeHidden();
    expect(await perRow(page)).toBe(12);
  });

  test('the number is kept after a reload, and junk in the storage is the default', async ({ page }) => {
    await page.keyboard.press('c');
    await page.locator('#columns-input').fill('20');
    await page.keyboard.press('Enter');
    expect(await stored(page)).toBe('20');
    await page.reload();
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
    await page.setInputFiles('#open-input', { name: 'raw.bin', mimeType: 'application/octet-stream', buffer: Buffer.alloc(100, 1) });
    await expect(page.locator('.hex-row').first()).toBeVisible();
    expect(await perRow(page)).toBe(20);
    for (const junk of ['0', '999', 'x', '2.5', '']) {
      await page.evaluate(([key, value]) => window.localStorage.setItem(key as string, value as string), [COLUMNS_KEY, junk]);
      await page.reload();
      await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
      await page.setInputFiles('#open-input', { name: 'raw.bin', mimeType: 'application/octet-stream', buffer: Buffer.alloc(100, 1) });
      await expect(page.locator('.hex-row').first()).toBeVisible();
      expect(await perRow(page), junk).toBe(16);
    }
  });

  test('the first byte in view stays in view, and the selected field stays selected, when the number changes', async ({ page }) => {
    // scroll down, then change the number: the top row still has the byte that was at the top
    await page.locator('#hex .hex').evaluate((el) => {
      el.scrollTop = 20 * 20;
    });
    await expect.poll(async () => (await offsets(page))[0]).not.toBe('00000000');
    // the offset of the first row that is whole in the view
    const topOffset = () =>
      page.locator('#hex .hex').evaluate((el) => {
        const top = el.getBoundingClientRect().top;
        const row = [...el.querySelectorAll('.hex-row')].find((r) => r.getBoundingClientRect().top >= top - 1);
        return row?.querySelector('.hex-off')?.textContent ?? '';
      });
    const before = await topOffset();
    await page.keyboard.press('c');
    await page.locator('#columns-input').fill('8');
    await page.keyboard.press('Enter');
    // the byte that was first in view is in the first row or the one above it, now that the rows are 8 bytes
    const after = Number.parseInt(await topOffset(), 16);
    const was = Number.parseInt(before, 16);
    expect(after).toBeLessThanOrEqual(was + 8);
    expect(after).toBeGreaterThanOrEqual(was - 8);
    // a field selected in the tree is marked at every number
    await page.locator('#tree .tree-row').nth(1).click();
    await expect(page.locator('.hex .b.sel')).toHaveCount(1);
    await page.keyboard.press('c');
    await page.locator('#columns-input').fill('3');
    await page.keyboard.press('Enter');
    await expect(page.locator('.hex .b.sel')).toHaveCount(1);
    await expect(page.locator('.hex .b.sel')).toBeVisible();
  });

  test('C cancels a parse that is running (c is the columns), and the Cancel button says C', async ({ page }) => {
    await expect(page.locator('#cancel')).toHaveAttribute('title', /\(C\)$/);
    await expect(page.locator('#cancel')).toHaveAttribute('aria-keyshortcuts', 'C');
    await page.keyboard.press('Shift+K');
    await expect(page.locator('#keys-table tr', { hasText: 'Cancel the parse' }).locator('kbd')).toHaveText('C');
    await expect(page.locator('#keys-table tr', { hasText: 'columns of the Bytes pane' }).locator('kbd')).toHaveText('c');
  });
});

test('a pane too narrow for the number scrolls sideways, and the row is not wrapped', async ({ page }) => {
  await page.setViewportSize({ width: 700, height: 900 });
  await setUp(page);
  await page.keyboard.press('c');
  await page.locator('#columns-input').fill('64');
  await page.keyboard.press('Enter');
  expect(await perRow(page)).toBe(64);
  const wide = await page.locator('#hex .hex').evaluate((el) => el.scrollWidth > el.clientWidth);
  expect(wide).toBe(true);
  // one line for each row
  const heights = await page.locator('.hex-row').evaluateAll((rows) => rows.map((row) => Math.round(row.getBoundingClientRect().height)));
  expect(new Set(heights).size).toBe(1);
});
