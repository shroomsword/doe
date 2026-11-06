// The layout of the panes: moving them, and keeping where they are.

import { expect, test, type Page } from '@playwright/test';
import { dragPane } from './pane-drag.js';
import { LAYOUT_KEY, defaultLayout, isDefault, layoutText, parseLayout, shown, swapped, withSizes, withVisible } from '../src/layout.js';

test('a saved layout is read only when it is four different panes of this version', () => {
  const order = ['parsed', 'schema', 'bytes', 'status'] as const;
  expect(parseLayout(layoutText({ order: [...order], hidden: ['bytes'], sizes: {} }))).toEqual({ order: [...order], hidden: ['bytes'], sizes: {} });
  // a record of version 1 has the order only: all the panes are shown
  expect(parseLayout(JSON.stringify({ v: 1, order }))).toEqual({ order: [...order], hidden: [], sizes: {} });
  // a pane cannot be closed twice or be unknown
  expect(parseLayout(JSON.stringify({ v: 2, order, hidden: ['schema'] }))).toEqual({ order: [...order], hidden: ['schema'], sizes: {} });
  expect(parseLayout(JSON.stringify({ v: 2, order, hidden: ['bytes', 'bytes'] }))).toBeNull();
  expect(parseLayout(JSON.stringify({ v: 2, order, hidden: ['nothing'] }))).toBeNull();
  expect(parseLayout(JSON.stringify({ v: 2, order, hidden: 'bytes' }))).toBeNull();
  expect(parseLayout(null)).toBeNull();
  expect(parseLayout('not json')).toBeNull();
  expect(parseLayout('42')).toBeNull();
  expect(parseLayout('null')).toBeNull();
  expect(parseLayout(JSON.stringify({ v: 0, order }))).toBeNull();
  expect(parseLayout(JSON.stringify({ v: '3', order }))).toBeNull();
  expect(parseLayout(JSON.stringify({ v: 1, order: ['parsed', 'parsed', 'bytes', 'status'] }))).toBeNull();
  expect(parseLayout(JSON.stringify({ v: 1, order: ['parsed', 'schema', 'bytes'] }))).toBeNull();
  expect(parseLayout(JSON.stringify({ v: 1, order: ['parsed', 'schema', 'bytes', 'nothing'] }))).toBeNull();
});

test('the sizes of a layout are read when they are valid, and only the sizes are lost when they are not', () => {
  const order = ['schema', 'parsed', 'bytes', 'status'];
  const sizes = { wide: { cols: [1.5, 1, 0.5], rows: [0.7, 0.3] }, narrow: { rows: [null, 0.4, null, 0.2] } };
  expect(parseLayout(JSON.stringify({ v: 3, order, hidden: [], sizes }))).toEqual({ order, hidden: [], sizes });
  // a record of version 1 or 2 has no sizes
  expect(parseLayout(JSON.stringify({ v: 2, order, hidden: [], sizes }))?.sizes).toEqual({});
  // a mode with junk loses its sizes, the others stay; so does everything when sizes are not an object
  const junk = { wide: { cols: [1, 'x', 1] }, medium: { cols: [1, 1], rows: [0.5, null] }, narrow: { cols: [0], rows: 7 } };
  expect(parseLayout(JSON.stringify({ v: 3, order, hidden: ['bytes'], sizes: junk }))).toEqual({ order, hidden: ['bytes'], sizes: { medium: { cols: [1, 1], rows: [0.5, null] } } });
  for (const bad of [null, 5, 'wide', [1, 2], { wide: null }, { wide: { cols: [1e9] } }, { wide: { cols: [] } }, { wide: { cols: [1, 1, 1, 1] } }, { narrow: { rows: [null, null] } }, { other: { cols: [1] } }]) {
    expect(parseLayout(JSON.stringify({ v: 3, order, hidden: [], sizes: bad }))).toEqual({ order, hidden: [], sizes: {} });
  }
  // a newer version is read as far as it is understood
  expect(parseLayout(JSON.stringify({ v: 4, order, hidden: ['status'], sizes, extra: true }))).toEqual({ order, hidden: ['status'], sizes });
  // sizes make the layout not the default, and withSizes sets or clears one mode
  const sized = withSizes(defaultLayout(), 'wide', { cols: [2, 1, 1] });
  expect(isDefault(sized)).toBe(false);
  expect(isDefault(withSizes(sized, 'wide', undefined))).toBe(true);
  expect(isDefault(withSizes(sized, 'wide', {}))).toBe(true);
});

test('two panes swap places, and the default is the order the panes are written in', () => {
  expect(swapped(defaultLayout(), 'schema', 'bytes').order).toEqual(['bytes', 'parsed', 'schema', 'status']);
  expect(isDefault(defaultLayout())).toBe(true);
  expect(isDefault(swapped(defaultLayout(), 'schema', 'bytes'))).toBe(false);
  // closing: the shown panes keep the order
  const closed = withVisible(withVisible(defaultLayout(), 'parsed', false), 'schema', false);
  expect(closed.hidden).toEqual(['schema', 'parsed']);
  expect(shown(closed)).toEqual(['bytes', 'status']);
  expect(isDefault(closed)).toBe(false);
  expect(isDefault(withVisible(withVisible(closed, 'parsed', true), 'schema', true))).toBe(true);
});

test.describe('in the page', () => {
  test.use({ viewport: { width: 1400, height: 800 } });

  const ready = async (page: Page): Promise<void> => {
    await page.goto('/');
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  };
  /** The ids of the panes in the order they are laid out: by the left edge, the bottom row last. */
  const places = (page: Page) =>
    page.evaluate(() =>
      [...document.querySelectorAll<HTMLElement>('main > section')]
        .map((section) => ({ id: section.id, slot: section.dataset.slot, ...section.getBoundingClientRect().toJSON() }))
        .sort((a, b) => Number(a.slot) - Number(b.slot))
        .map((section) => section.id),
    );
  const stored = (page: Page) => page.evaluate((key) => window.localStorage.getItem(key), LAYOUT_KEY);

  test.beforeEach(async ({ page }) => {
    await ready(page);
  });

  test('the default layout has nothing stored and no reset button', async ({ page }) => {
    expect(await places(page)).toEqual(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status']);
    expect(await stored(page)).toBeNull();
    await expect(page.locator('#layout-reset')).toBeHidden();
  });

  test('dragging a title onto another pane with Shift held swaps them; it is kept after a reload, and the button puts it back', async ({ page }) => {
    await dragPane(page, 'pane-editor', 'pane-hex', 'before', { shift: true });
    expect(await places(page)).toEqual(['pane-hex', 'pane-tree', 'pane-editor', 'pane-status']);
    // on the screen the bytes are the left column now
    const left = await page.locator('#pane-hex').boundingBox();
    const right = await page.locator('#pane-editor').boundingBox();
    expect(left!.x).toBeLessThan(right!.x);
    await expect(page.locator('#layout-reset')).toBeVisible();
    expect(parseLayout(await stored(page))?.order).toEqual(['bytes', 'parsed', 'schema', 'status']);
    await expect(page.locator('#layout-live')).toHaveText('Schema and Bytes swapped places');

    await page.reload();
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
    expect(await places(page)).toEqual(['pane-hex', 'pane-tree', 'pane-editor', 'pane-status']);
    await expect(page.locator('#layout-reset')).toBeVisible();

    await page.locator('#layout-reset').click();
    expect(await places(page)).toEqual(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status']);
    expect(await stored(page)).toBeNull();
    await expect(page.locator('#layout-reset')).toBeHidden();
  });

  test('Alt+Shift+h, j, k, l swap the focused pane with the one beside it, the focus stays, and R puts them back', async ({ page }) => {
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('Alt+Shift+KeyL');
    expect(await places(page)).toEqual(['pane-editor', 'pane-hex', 'pane-tree', 'pane-status']);
    expect(await page.evaluate(() => document.activeElement?.closest('section')?.id)).toBe('pane-tree');
    await expect(page.locator('#layout-live')).toHaveText('Parsed and Bytes swapped places');
    // at the right edge nothing moves
    await page.keyboard.press('Alt+Shift+KeyL');
    expect(await places(page)).toEqual(['pane-editor', 'pane-hex', 'pane-tree', 'pane-status']);
    await page.keyboard.press('Alt+Shift+KeyJ');
    expect(await places(page)).toEqual(['pane-editor', 'pane-hex', 'pane-status', 'pane-tree']);
    await page.keyboard.press('Alt+Shift+KeyK');
    await page.keyboard.press('Alt+Shift+KeyH');
    await page.keyboard.press('Alt+Shift+KeyH');
    expect(await page.evaluate(() => document.activeElement?.closest('section')?.id)).toBe('pane-tree');
    // h j k l after a move follow the new places
    await page.keyboard.press('Escape');
    await page.keyboard.press('l');
    expect(await page.evaluate(() => document.activeElement?.closest('section')?.id)).not.toBe('pane-tree');
    // that may be the editor, where the letters are text
    await page.keyboard.press('Escape');
    await page.keyboard.press('Shift+R');
    expect(await places(page)).toEqual(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status']);
    expect(await stored(page)).toBeNull();
  });

  test('Tab goes through the panes in the order they are on the screen', async ({ page }) => {
    await dragPane(page, 'pane-editor', 'pane-hex', 'before', { shift: true });
    await page.locator('#pane-hex').focus();
    const order: string[] = [];
    for (let i = 0; i < 2; i += 1) {
      await page.keyboard.press('Tab');
      order.push((await page.evaluate(() => document.activeElement?.closest('section')?.id)) ?? '');
    }
    // the third place is the editor, where Tab is the browser's again
    expect(order).toEqual(['pane-tree', 'pane-editor']);
  });

  test('junk in the storage, and a storage that throws, give the default layout', async ({ page }) => {
    await page.evaluate((key) => window.localStorage.setItem(key, '{"v":1,"order":["schema","schema","bytes","status"]}'), LAYOUT_KEY);
    await page.reload();
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
    expect(await places(page)).toEqual(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status']);
    await page.addInitScript(() => {
      Object.defineProperty(window, 'localStorage', {
        get() {
          throw new Error('blocked');
        },
      });
    });
    await page.reload();
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
    expect(await places(page)).toEqual(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status']);
    // a move still works, for as long as the page is open
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('Alt+Shift+KeyH');
    expect(await places(page)).toEqual(['pane-tree', 'pane-editor', 'pane-hex', 'pane-status']);
  });

  test('in one column the saved order is the order from top to bottom', async ({ page }) => {
    await dragPane(page, 'pane-editor', 'pane-status', 'before', { shift: true });
    await page.setViewportSize({ width: 500, height: 900 });
    const tops = await page.evaluate(() => [...document.querySelectorAll('main > section')].map((section) => [section.id, section.getBoundingClientRect().top]));
    const ids = tops.sort((a, b) => Number(a[1]) - Number(b[1])).map((entry) => entry[0]);
    expect(ids).toEqual(['pane-status', 'pane-tree', 'pane-hex', 'pane-editor']);
  });
});
