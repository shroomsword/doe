// The columns of the Parsed pane: the dialog (by key and by button), the flow of the rows, the number kept, a narrow pane.

import { expect, test, type Page } from '@playwright/test';
import { COLUMNS_KEY, MAX_COLUMNS, columnsError, parseColumns, shownColumns } from '../src/columns.js';

test('a number of columns is a whole number from 1 to the largest, and a narrow pane shows fewer', () => {
  expect(parseColumns('3')).toBe(3);
  expect(parseColumns(' 8 ')).toBe(8);
  for (const bad of ['0', '9', '-1', '1.5', 'two', '', ' ', '1e1', '03x', null, undefined]) expect(parseColumns(bad as string), String(bad)).toBeNull();
  expect(parseColumns('007')).toBe(7);
  expect(columnsError('')).toMatch(/enter a number/);
  expect(columnsError('x')).toMatch(/whole number/);
  expect(columnsError(String(MAX_COLUMNS + 1))).toMatch(/from 1 to 8/);
  expect(columnsError('2')).toBeNull();
  // 20rem of 16px is 320px; with a gap of 16: 3 columns need 3 * 320 + 2 * 16
  expect(shownColumns(3, 3 * 320 + 2 * 16, 16, 16)).toBe(3);
  expect(shownColumns(3, 3 * 320 + 2 * 16 - 1, 16, 16)).toBe(2);
  expect(shownColumns(8, 100, 16, 16)).toBe(1);
  expect(shownColumns(1, 5000, 16, 16)).toBe(1);
});

const SCHEMA = `meta:\n  id: many\nseq:\n${Array.from({ length: 60 }, (_, i) => `  - id: f${i}\n    type: u1`).join('\n')}\n`;

async function setUp(page: Page): Promise<void> {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText(SCHEMA);
  await page.setInputFiles('#open-input', { name: 'many.bin', mimeType: 'application/octet-stream', buffer: Buffer.alloc(60, 7) });
  await expect(page.locator('#status')).toContainText('many.bin, 60 bytes');
  await expect(page.locator('#tree .tree-row').nth(40)).toBeAttached();
  // a column keeps at least 20rem: the Parsed pane is maximized, so that it is wide enough for several
  await page.getByRole('button', { name: 'Maximize the Parsed pane' }).click();
  await page.locator('#pane-tree').focus();
}

/** The distinct left edges of the rows of the tree: one for each column that is drawn. */
const columnEdges = (page: Page) =>
  page.evaluate(() => [...new Set([...document.querySelectorAll('#tree .tree-row')].map((row) => Math.round(row.getBoundingClientRect().left)))].sort((a, b) => a - b));

const stored = (page: Page) => page.evaluate((key) => window.localStorage.getItem(key), COLUMNS_KEY);

test.describe('wide window', () => {
  test.use({ viewport: { width: 1600, height: 900 } });
  test.beforeEach(async ({ page }) => setUp(page));

  test('c and the button in the title bar open the same dialog; the button is an icon with a name and a tooltip naming the key', async ({ page }) => {
    const button = page.getByRole('button', { name: 'Columns of the parsed value' });
    await expect(button).toHaveCount(1);
    expect((await button.textContent())?.trim()).toBe('');
    await expect(button.locator('svg')).toHaveCount(1);
    await expect(button).toHaveAttribute('title', 'Columns of the parsed value (c)');
    await expect(button).toHaveAttribute('aria-haspopup', 'dialog');
    // in the title bar of the Parsed pane, before the maximize and close buttons
    const bar = await page.locator('#pane-tree .pane-title').boundingBox();
    const box = (await button.boundingBox())!;
    expect(box.y).toBeGreaterThanOrEqual(bar!.y);
    expect(box.x).toBeLessThan((await page.locator('#pane-tree .pane-maximize').boundingBox())!.x);
    await page.keyboard.press('c');
    const dialog = page.locator('#columns-dialog');
    await expect(dialog).toBeVisible();
    await expect(page.locator('#columns-input')).toBeFocused();
    await expect(page.locator('#columns-input')).toHaveValue('1');
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    await button.click();
    await expect(dialog).toBeVisible();
    await page.locator('#columns-cancel').click();
    await expect(dialog).toBeHidden();
    // c is a letter: it is text in a field (the editor is hidden while the Parsed pane is maximized), and does nothing while a dialog is open
    await page.getByRole('button', { name: 'Restore the Parsed pane' }).click();
    await page.locator('.cm-content').click();
    await page.keyboard.press('c');
    await expect(dialog).toBeHidden();
  });

  test('Enter applies, the rows flow in that many columns, Esc and Cancel change nothing, Reset goes back to one', async ({ page }) => {
    expect(await columnEdges(page)).toHaveLength(1);
    await page.keyboard.press('c');
    await page.locator('#columns-input').fill('3');
    await page.keyboard.press('Enter');
    await expect(page.locator('#columns-dialog')).toBeHidden();
    await expect(page.locator('#layout-live')).toHaveText('Parsed pane: 3 columns');
    const edges = await columnEdges(page);
    expect(edges).toHaveLength(3);
    // down the first column, then the next: the rows are in order, and a row is whole inside one column
    const tops = await page.evaluate(() =>
      [...document.querySelectorAll('#tree .tree-row')].slice(0, 60).map((row) => ({ x: Math.round(row.getBoundingClientRect().left), y: Math.round(row.getBoundingClientRect().top) })),
    );
    let column = 0;
    for (let i = 1; i < tops.length; i += 1) {
      const row = tops[i]!;
      const before = tops[i - 1]!;
      if (row.x !== before.x) {
        column += 1;
        expect(row.x, 'the next column is to the right').toBeGreaterThan(before.x);
      } else expect(row.y, 'down a column').toBeGreaterThan(before.y);
    }
    expect(column).toBe(2);
    // Esc and Cancel change nothing
    await page.keyboard.press('c');
    await page.locator('#columns-input').fill('5');
    await page.keyboard.press('Escape');
    expect(await columnEdges(page)).toHaveLength(3);
    await page.keyboard.press('c');
    await expect(page.locator('#columns-input')).toHaveValue('3');
    await page.locator('#columns-input').fill('4');
    await page.locator('#columns-cancel').click();
    expect(await columnEdges(page)).toHaveLength(3);
    // the quick choices fill the box, Apply applies
    await page.keyboard.press('c');
    await page.locator('#columns-quick button', { hasText: /^2$/ }).click();
    await expect(page.locator('#columns-input')).toHaveValue('2');
    await page.locator('#columns-apply').click();
    expect(await columnEdges(page)).toHaveLength(2);
    // Reset
    await page.keyboard.press('c');
    await page.locator('#columns-reset').click();
    expect(await columnEdges(page)).toHaveLength(1);
    expect(await stored(page)).toBeNull();
    await expect(page.locator('#layout-live')).toHaveText('Parsed pane: 1 column');
  });

  test('a number that is not a whole number in range is refused with a message, and the dialog stays open', async ({ page }) => {
    await page.keyboard.press('c');
    for (const [text, message] of [['0', /from 1 to 8/], ['9', /from 1 to 8/], ['', /enter a number/], ['2.5', /whole number/]] as const) {
      await page.locator('#columns-input').fill(text);
      await page.keyboard.press('Enter');
      await expect(page.locator('#columns-error')).toBeVisible();
      await expect(page.locator('#columns-error')).toHaveText(message);
      await expect(page.locator('#columns-input')).toHaveAttribute('aria-invalid', 'true');
      await expect(page.locator('#columns-dialog')).toBeVisible();
    }
    expect(await columnEdges(page)).toHaveLength(1);
    // fixing the number hides the message, and it applies
    await page.locator('#columns-input').fill('2');
    await expect(page.locator('#columns-error')).toBeHidden();
    await page.keyboard.press('Enter');
    await expect(page.locator('#columns-dialog')).toBeHidden();
    expect(await columnEdges(page)).toHaveLength(2);
  });

  test('the number is kept after a reload, and junk in the storage is one column', async ({ page }) => {
    await page.keyboard.press('c');
    await page.locator('#columns-input').fill('4');
    await page.keyboard.press('Enter');
    expect(await stored(page)).toBe('4');
    await page.reload();
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
    expect(await page.evaluate(() => getComputedStyle(document.querySelector('#tree-box')!).getPropertyValue('--tree-cols').trim())).toBe('4');
    for (const junk of ['0', '99', 'x', '2.5', '']) {
      await page.evaluate(([key, value]) => window.localStorage.setItem(key as string, value as string), [COLUMNS_KEY, junk]);
      await page.reload();
      await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
      expect(await page.evaluate(() => getComputedStyle(document.querySelector('#tree-box')!).getPropertyValue('--tree-cols').trim()), junk).toBe('1');
    }
  });

  test('the arrow keys follow the order of the rows, down a column and on to the next, and the selected row is in view', async ({ page }) => {
    await page.keyboard.press('c');
    await page.locator('#columns-input').fill('3');
    await page.keyboard.press('Enter');
    const rows = page.locator('#tree .tree-row');
    await rows.nth(1).click();
    const selected = page.locator('#tree .tree-row.selected');
    await expect(selected).toHaveCount(1);
    const left = () => selected.evaluate((row) => Math.round(row.getBoundingClientRect().left));
    const start = await left();
    const name = () => selected.locator('.name').first().textContent();
    const first = await name();
    for (let i = 0; i < 30; i += 1) await page.keyboard.press('ArrowDown');
    expect(await name()).not.toBe(first);
    // the thirtieth row after the second is in another column than the second, to its right, and in view
    expect(await left()).toBeGreaterThan(start);
    await expect(selected).toBeInViewport();
    // and the rows are drawn in three columns
    const lefts = await page.evaluate(() => [...document.querySelectorAll('#tree .tree-row')].map((r) => Math.round(r.getBoundingClientRect().left)));
    expect(new Set(lefts).size).toBe(3);
  });

  test('C cancels a parse that is running (c is the columns now), and the Cancel button says C', async ({ page }) => {
    await expect(page.locator('#cancel')).toHaveAttribute('title', /\(C\)$/);
    await expect(page.locator('#cancel')).toHaveAttribute('aria-keyshortcuts', 'C');
    await page.keyboard.press('Shift+K');
    const row = page.locator('#keys-table tr', { hasText: 'Cancel the parse' });
    await expect(row.locator('kbd')).toHaveText('C');
    await expect(page.locator('#keys-table tr', { hasText: 'columns of the Parsed pane' }).locator('kbd')).toHaveText('c');
  });
});

test.describe('a narrow pane', () => {
  test('fewer columns are drawn than the number, the dialog says how many fit, and the number is not changed', async ({ page }) => {
    await page.setViewportSize({ width: 700, height: 900 });
    await setUp(page);
    await page.keyboard.press('c');
    await page.locator('#columns-input').fill('6');
    await expect(page.locator('#columns-fit')).toContainText(/wide enough for \d of 6 columns/);
    await page.keyboard.press('Enter');
    expect(await stored(page)).toBe('6');
    expect((await columnEdges(page)).length).toBeLessThan(6);
    expect((await columnEdges(page)).length).toBeGreaterThanOrEqual(1);
    // the pane has room for them when it is wide
    await page.setViewportSize({ width: 2400, height: 900 });
    await page.waitForTimeout(200);
    expect(await columnEdges(page)).toHaveLength(6);
  });
});
