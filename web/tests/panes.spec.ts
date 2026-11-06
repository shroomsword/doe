// Closing panes and choosing which are shown.

import { expect, test, type Page } from '@playwright/test';
import { LAYOUT_KEY, parseLayout } from '../src/layout.js';

const ids = { schema: 'pane-editor', parsed: 'pane-tree', bytes: 'pane-hex', status: 'pane-status' } as const;

const ready = async (page: Page): Promise<void> => {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
};
const stored = (page: Page) => page.evaluate((key) => window.localStorage.getItem(key), LAYOUT_KEY);
/** The panes that are on the screen, left to right and top to bottom. */
const visible = (page: Page) =>
  page.evaluate(() =>
    [...document.querySelectorAll<HTMLElement>('main > section')]
      .filter((section) => section.getBoundingClientRect().width > 0)
      .map((section) => ({ id: section.id, ...section.getBoundingClientRect().toJSON() }))
      .sort((a, b) => Math.round(a.y) - Math.round(b.y) || a.x - b.x)
      .map((section) => section.id),
  );
const active = (page: Page) => page.evaluate(() => document.activeElement?.closest('section')?.id ?? document.activeElement?.tagName ?? '');

test.describe('wide window', () => {
  test.use({ viewport: { width: 1400, height: 800 } });
  test.beforeEach(async ({ page }) => ready(page));

  test('every pane has an X at the top right of its title bar; it closes the pane', async ({ page }) => {
    for (const name of ['schema', 'parsed', 'bytes', 'status'] as const) {
      await expect(page.locator(`#${ids[name]} .pane-title .pane-close`)).toHaveCount(1);
    }
    const close = page.getByRole('button', { name: 'Close the Bytes pane' });
    const title = await page.locator('#pane-hex .pane-title').boundingBox();
    const box = await close.boundingBox();
    // the button is the last thing at the right of the title bar
    expect(box!.x + box!.width).toBeGreaterThan(title!.x + title!.width - 24);
    await close.click();
    await expect(page.locator('#pane-hex')).toBeHidden();
    expect(await visible(page)).toEqual(['pane-editor', 'pane-tree', 'pane-status']);
    expect(parseLayout(await stored(page))).toEqual({ order: ['schema', 'parsed', 'bytes', 'status'], hidden: ['bytes'], sizes: {} });
    await expect(page.locator('#layout-live')).toHaveText('The Bytes pane is closed');
    await expect(page.locator('#layout-reset')).toBeVisible();
  });

  test('the X of the Schema pane closes it; the others fill the window, and v or Reset brings it back; with every pane closed a note says how to open them', async ({ page }) => {
    await page.getByRole('button', { name: 'Close the Schema pane' }).click();
    await expect(page.locator('#pane-editor')).toBeHidden();
    expect(await visible(page)).toEqual(['pane-tree', 'pane-hex', 'pane-status']);
    expect(parseLayout(await stored(page))?.hidden).toEqual(['schema']);
    await page.reload();
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
    await expect(page.locator('#pane-editor')).toBeHidden();
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('p');
    await page.locator('#panes-list li').nth(0).locator('input').check();
    await page.keyboard.press('Escape');
    await expect(page.locator('#pane-editor')).toBeVisible();
    // every pane closed
    for (const name of ['Schema', 'Parsed', 'Bytes', 'Status']) await page.getByRole('button', { name: `Close the ${name} pane` }).click();
    expect(await visible(page)).toEqual([]);
    expect(await page.evaluate(() => getComputedStyle(document.querySelector('main')!, '::after').content)).toContain('All the panes are closed');
    await page.getByRole('button', { name: 'Panes' }).click();
    await page.locator('#panes-list li').nth(2).locator('input').check();
    await expect(page.locator('#pane-hex')).toBeVisible();
    await page.keyboard.press('Escape');
    await page.keyboard.press('Shift+R');
    expect(await visible(page)).toEqual(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status']);
  });

  test('x closes the focused pane, but not Schema (its button does), and not in a text field', async ({ page }) => {
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('x');
    await expect(page.locator('#pane-tree')).toBeHidden();
    // the focus went to a pane that is shown
    expect(['pane-hex', 'pane-status', 'pane-editor']).toContain(await active(page));
    await page.locator('#pane-editor').focus();
    await page.keyboard.press('x');
    await expect(page.locator('#pane-editor')).toBeVisible();
    await expect(page.locator('#layout-live')).toHaveText('x does not close the Schema pane: use its X button, or p');
    // in the editor x is text
    await page.locator('.cm-content').click();
    await page.keyboard.press('x');
    await expect(page.locator('#pane-editor')).toBeVisible();
  });

  test('the remaining panes fill the window: 3, 2 and 1 panes are side by side with no hole', async ({ page }) => {
    const widthOf = async (id: string) => (await page.locator(`#${id}`).boundingBox())!.width;
    await page.locator('#pane-status').focus();
    await page.keyboard.press('x');
    // three tall panes in a row
    const three = await Promise.all(['pane-editor', 'pane-tree', 'pane-hex'].map(widthOf));
    expect(Math.max(...three) - Math.min(...three)).toBeLessThan(3);
    await page.locator('#pane-hex').focus();
    await page.keyboard.press('x');
    const two = await Promise.all(['pane-editor', 'pane-tree'].map(widthOf));
    expect(Math.abs(two[0]! - two[1]!)).toBeLessThan(3);
    expect(two[0]!).toBeGreaterThan(three[0]! * 1.4);
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('x');
    expect(await visible(page)).toEqual(['pane-editor']);
    const main = await page.locator('main').boundingBox();
    expect((await widthOf('pane-editor')) + 2 * 16).toBeGreaterThan(main!.width - 4);
  });

  test('p opens a list of the four panes with their state; the boxes show and close them, Schema too', async ({ page }) => {
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('p');
    const dialog = page.locator('#panes-dialog');
    await expect(dialog).toBeVisible();
    const rows = dialog.locator('#panes-list li');
    await expect(rows).toHaveCount(4);
    await expect(rows.nth(0)).toContainText('Schema');
    await expect(rows.nth(0).locator('input')).toBeEnabled();
    await expect(rows.nth(0).locator('input')).toBeChecked();
    await expect(rows.nth(1)).toContainText('shown');
    await rows.nth(1).locator('input').uncheck();
    await expect(page.locator('#pane-tree')).toBeHidden();
    await expect(rows.nth(1)).toContainText('closed');
    await expect(rows.nth(1).locator('input')).not.toBeChecked();
    await rows.nth(1).locator('input').check();
    await expect(page.locator('#pane-tree')).toBeVisible();
    await rows.nth(3).locator('input').uncheck();
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    expect(parseLayout(await stored(page))?.hidden).toEqual(['status']);
    // the header button opens it too
    await page.getByRole('button', { name: 'Panes' }).click();
    await expect(dialog).toBeVisible();
    await expect(dialog.locator('li').nth(3)).toContainText('closed');
    await page.locator('#panes-close').click();
  });

  test('v no longer opens the list; p does not either in a text field, with Ctrl or while a dialog is open', async ({ page }) => {
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('v');
    await expect(page.locator('#panes-dialog')).toBeHidden();
    await page.keyboard.press('Control+p');
    await expect(page.locator('#panes-dialog')).toBeHidden();
    await page.locator('.cm-content').click();
    await page.keyboard.press('p');
    await expect(page.locator('#panes-dialog')).toBeHidden();
    await expect(page.locator('.cm-content')).toContainText('p');
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('Shift+K');
    await expect(page.locator('#keys-dialog')).toBeVisible();
    await page.keyboard.press('p');
    await expect(page.locator('#panes-dialog')).toBeHidden();
    await page.keyboard.press('Escape');
    // and the list names its key
    await expect(page.getByRole('button', { name: 'Panes' })).toHaveAttribute('title', 'Choose the panes that are shown (p)');
    await page.keyboard.press('Shift+K');
    await expect(page.locator('#keys-table tr', { hasText: 'Choose the panes' }).locator('kbd')).toHaveText('p');
  });

  test('the closed panes are kept after a reload, and Reset shows every pane again', async ({ page }) => {
    await page.locator('#pane-hex').focus();
    await page.keyboard.press('x');
    await page.locator('#pane-status').focus();
    await page.keyboard.press('x');
    await page.reload();
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
    expect(await visible(page)).toEqual(['pane-editor', 'pane-tree']);
    await page.locator('#layout-reset').click();
    expect(await visible(page)).toEqual(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status']);
    expect(await stored(page)).toBeNull();
  });

  test('1 to 4 show a closed pane and focus it; Tab, h j k l and the search use the panes that are shown', async ({ page }) => {
    await page.locator('#pane-hex').focus();
    await page.keyboard.press('x');
    await page.locator('#pane-tree').focus();
    // Tab visits only what is shown: tree, status, (then the editor, where Tab is the browser's)
    await page.keyboard.press('Tab');
    expect(await active(page)).toBe('pane-status');
    // h j k l never land on the closed pane
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('l');
    expect(await active(page)).not.toBe('pane-hex');
    await page.keyboard.press('Escape');
    await page.keyboard.press('3');
    await expect(page.locator('#pane-hex')).toBeVisible();
    expect(await active(page)).toBe('pane-hex');
    expect(await stored(page)).toBeNull();
  });

  test('a parse that fails opens the Status pane, so that the message is not lost', async ({ page }) => {
    await page.locator('#pane-status').focus();
    await page.keyboard.press('x');
    await expect(page.locator('#pane-status')).toBeHidden();
    await page.locator('.cm-content').click();
    await page.keyboard.press('Control+A');
    await page.keyboard.type('id: x\nseq:\n  - id: a\n    type: nosuchtype\n');
    await expect(page.locator('#pane-status')).toBeVisible({ timeout: 15000 });
  });

  test('junk in the storage is the default, and a record of the first version shows every pane', async ({ page }) => {
    await page.evaluate((key) => window.localStorage.setItem(key, '{"v":2,"order":["schema","parsed","bytes","status"],"hidden":["nothing"]}'), LAYOUT_KEY);
    await page.reload();
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
    expect(await visible(page)).toEqual(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status']);
    await page.evaluate((key) => window.localStorage.setItem(key, '{"v":1,"order":["parsed","schema","bytes","status"]}'), LAYOUT_KEY);
    await page.reload();
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
    expect(await visible(page)).toEqual(['pane-tree', 'pane-editor', 'pane-hex', 'pane-status']);
  });
});

test.describe('medium and narrow windows', () => {
  test('two columns with the third across; one column with the panes that are shown', async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 900 });
    await ready(page);
    await page.locator('#pane-status').focus();
    await page.keyboard.press('x');
    const widths = await Promise.all(['pane-editor', 'pane-tree', 'pane-hex'].map(async (id) => (await page.locator(`#${id}`).boundingBox())!.width));
    expect(widths[2]!).toBeGreaterThan(widths[0]! * 1.8);
    await page.setViewportSize({ width: 500, height: 900 });
    const tops = await page.evaluate(() => [...document.querySelectorAll('main > section')].filter((s) => s.getBoundingClientRect().width > 0).map((s) => s.id));
    expect(tops).toEqual(['pane-editor', 'pane-tree', 'pane-hex']);
  });
});
