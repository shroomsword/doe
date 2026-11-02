// Maximizing a pane: f, the button in the title bar, a double-click on the title bar; Esc, f and the button put it back.

import { expect, test, type Page } from '@playwright/test';
import { LAYOUT_KEY } from '../src/layout.js';

const ready = async (page: Page): Promise<void> => {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
};
const shownIds = (page: Page) =>
  page.evaluate(() => [...document.querySelectorAll<HTMLElement>('main > section')].filter((s) => s.getBoundingClientRect().width > 0).map((s) => s.id));
const active = (page: Page) => page.evaluate(() => document.activeElement?.closest('section')?.id ?? document.activeElement?.tagName ?? '');

test.describe('wide window', () => {
  test.use({ viewport: { width: 1400, height: 800 } });
  test.beforeEach(async ({ page }) => ready(page));

  test('every pane has a maximize button with an icon (no text) and a name that says what it does', async ({ page }) => {
    for (const name of ['Schema', 'Parsed', 'Bytes', 'Status']) {
      const button = page.getByRole('button', { name: `Maximize the ${name} pane` });
      await expect(button).toHaveCount(1);
      await expect(button).toHaveAttribute('title', `Maximize the ${name} pane (f)`);
      expect((await button.textContent())?.trim()).toBe('');
      await expect(button.locator('svg')).toHaveCount(1);
    }
  });

  test('f maximizes the focused pane and f restores it; the pane fills the page and the others are hidden, not closed', async ({ page }) => {
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('f');
    expect(await shownIds(page)).toEqual(['pane-tree']);
    const main = await page.locator('main').boundingBox();
    const pane = await page.locator('#pane-tree').boundingBox();
    expect(pane!.width).toBeGreaterThan(main!.width - 40);
    expect(pane!.height).toBeGreaterThan(main!.height - 40);
    await expect(page.locator('#layout-live')).toHaveText('Parsed maximized');
    // the others are not closed: the layout in storage is the default, and they come back as they were
    expect(await page.evaluate((key) => window.localStorage.getItem(key), LAYOUT_KEY)).toBeNull();
    expect(await active(page)).toBe('pane-tree');
    // the button says restore, with another icon
    const restore = page.getByRole('button', { name: 'Restore the Parsed pane' });
    await expect(restore).toHaveAttribute('aria-pressed', 'true');
    await expect(restore).toHaveAttribute('title', 'Restore the Parsed pane (f)');
    await page.keyboard.press('f');
    expect(await shownIds(page)).toEqual(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status']);
    await expect(page.locator('#layout-live')).toHaveText('Parsed restored');
    expect(await active(page)).toBe('pane-tree');
  });

  test('the button maximizes and restores', async ({ page }) => {
    await page.getByRole('button', { name: 'Maximize the Bytes pane' }).click();
    expect(await shownIds(page)).toEqual(['pane-hex']);
    await page.getByRole('button', { name: 'Restore the Bytes pane' }).click();
    expect(await shownIds(page)).toHaveLength(4);
  });

  test('a double-click on the title bar maximizes and restores, but not on a button, a box or the search bar', async ({ page }) => {
    await page.locator('#pane-hex .pane-title h2').dblclick();
    expect(await shownIds(page)).toEqual(['pane-hex']);
    await page.locator('#pane-hex .pane-title h2').dblclick();
    expect(await shownIds(page)).toHaveLength(4);
    // an empty part of the bar
    const bar = await page.locator('#pane-hex .pane-title').boundingBox();
    await page.mouse.dblclick(bar!.x + bar!.width / 2, bar!.y + bar!.height / 2);
    expect(await shownIds(page)).toEqual(['pane-hex']);
    await page.getByRole('button', { name: 'Restore the Bytes pane' }).click();
    // controls in a title bar do not
    await page.locator('#pane-tree #radix').dblclick();
    await page.locator('#pane-tree #detect-auto-box').dblclick();
    await page.locator('#pane-tree #detect').dblclick({ force: true }).catch(() => undefined);
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('/');
    await page.locator('#pane-tree .pane-search input').dblclick();
    expect(await shownIds(page)).toHaveLength(4);
  });

  test('Esc restores, but first leaves the editor, and a dialog or a search keeps its own Esc', async ({ page }) => {
    await page.getByRole('button', { name: 'Maximize the Schema pane' }).click();
    await page.locator('.cm-content').click();
    await page.keyboard.press('Escape');
    expect(await shownIds(page)).toEqual(['pane-editor']);
    await page.keyboard.press('Escape');
    expect(await shownIds(page)).toHaveLength(4);
    // a search bar is closed by Esc and the pane stays maximized
    await page.getByRole('button', { name: 'Maximize the Bytes pane' }).click();
    await page.keyboard.press('/');
    await expect(page.locator('#pane-hex .pane-search')).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.locator('#pane-hex .pane-search')).toBeHidden();
    expect(await shownIds(page)).toEqual(['pane-hex']);
    // a dialog too
    await page.keyboard.press('Shift+K');
    await expect(page.locator('#keys-dialog')).toBeVisible();
    await page.keyboard.press('Escape');
    expect(await shownIds(page)).toEqual(['pane-hex']);
    await page.keyboard.press('Escape');
    expect(await shownIds(page)).toHaveLength(4);
  });

  test('while a pane is maximized Tab and h j k l do nothing, a key for another pane restores first, and closing the pane restores', async ({ page }) => {
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('f');
    await page.keyboard.press('Tab');
    await page.keyboard.press('l');
    await page.keyboard.press('Alt+Shift+KeyL');
    expect(await shownIds(page)).toEqual(['pane-tree']);
    expect(await active(page)).toBe('pane-tree');
    await expect(page.locator('#layout-live')).toContainText('press f or Esc');
    // its own number keeps it, another pane's restores
    await page.keyboard.press('2');
    expect(await shownIds(page)).toEqual(['pane-tree']);
    await page.keyboard.press('3');
    expect(await shownIds(page)).toHaveLength(4);
    expect(await active(page)).toBe('pane-hex');
    await page.keyboard.press('f');
    await page.getByRole('button', { name: 'Close the Bytes pane' }).click();
    expect(await shownIds(page)).toEqual(['pane-editor', 'pane-tree', 'pane-status']);
  });

  test('the other panes keep what they show: the parse goes on and the tree is there after a restore', async ({ page }) => {
    await page.getByRole('button', { name: 'Maximize the Schema pane' }).click();
    await page.locator('.cm-content').click();
    await page.keyboard.press('Control+A');
    await page.keyboard.type('id: x\nseq:\n  - id: a\n    type: nosuchtype\n');
    await page.keyboard.press('Escape');
    await page.keyboard.press('Escape');
    expect(await shownIds(page)).toHaveLength(4);
    await expect(page.locator('#pane-status')).toContainText(/nosuchtype|failed|problem/i);
  });
});

test.describe('other widths', () => {
  for (const width of [1000, 500]) {
    test(`a maximized pane fills the width at ${width}px and the others come back`, async ({ page }) => {
      await page.setViewportSize({ width, height: 900 });
      await ready(page);
      await page.getByRole('button', { name: 'Maximize the Parsed pane' }).click();
      expect(await shownIds(page)).toEqual(['pane-tree']);
      const pane = await page.locator('#pane-tree').boundingBox();
      expect(pane!.width).toBeGreaterThan(width - 40);
      expect(pane!.height).toBeGreaterThan(600);
      await page.keyboard.press('Escape');
      expect(await shownIds(page)).toHaveLength(4);
    });
  }
});
