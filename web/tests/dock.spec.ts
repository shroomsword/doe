// The dock: the record that keeps it (version 5, and version 4 read as a tree), the grid it is drawn on, the separators that
// resize it, and how a narrow window and a closed pane treat it.

import { expect, test, type Page } from '@playwright/test';
import { cleanTree, defaultTree, dockGrid, dockTo, leaves, tidy, view, without } from '../src/dock.js';
import { LAYOUT_KEY, defaultLayout, dockedOrder, isDefault, layoutText, parseLayout, swapped, withDock } from '../src/layout.js';
import { dragToZone } from './pane-drag.js';

const stored = async (page: Page) => parseLayout(await page.evaluate((key) => window.localStorage.getItem(key), LAYOUT_KEY));
const box = async (page: Page, id: string) => (await page.locator(`#${id}`).boundingBox())!;
const ready = async (page: Page): Promise<void> => {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
};

test('a dock is kept in the record of version 5, and a bad one loses the dock and not the order', () => {
  const tree = dockTo(defaultTree(), 'parsed', 'tr')!;
  const layout = withDock(defaultLayout(), { tree, status: 0.6 });
  expect(layout.order).toEqual(['schema', 'parsed', 'bytes', 'status']);
  expect(isDefault(layout)).toBe(false);
  expect(isDefault(withDock(layout, undefined))).toBe(true);
  expect(parseLayout(layoutText(layout))).toEqual(layout);
  const text = JSON.parse(layoutText(layout)) as Record<string, unknown>;
  expect(text.v).toBe(5);
  const bad = (dock: unknown) => parseLayout(JSON.stringify({ ...text, order: ['bytes', 'schema', 'parsed', 'status'], dock }));
  for (const dock of [null, 5, {}, { tree: 'schema', status: 0.4 }, { tree, status: 0 }, { tree, status: 'low' }, { tree: { dir: 'row', children: ['schema', 'schema', 'bytes'], weights: [1, 1, 1] }, status: 0.4 }, { tree: { dir: 'diagonal', children: ['schema', 'parsed'], weights: [1, 1] }, status: 0.4 }, { tree: { dir: 'row', children: ['schema', 'parsed', 'bytes', 'status'], weights: [1, 1, 1, 1] }, status: 0.4 }]) {
    expect(bad(dock), JSON.stringify(dock)).toEqual({ order: ['bytes', 'schema', 'parsed', 'status'], hidden: [], sizes: {} });
  }
  // a nested tree that goes too deep is junk
  let deep: unknown = 'schema';
  for (let i = 0; i < 8; i += 1) deep = { dir: i % 2 ? 'row' : 'col', children: [deep, 'parsed'], weights: [1, 1] };
  expect(cleanTree(deep)).toBeNull();
});

test('a record of version 4 (rows of panes) is read as the nearest tree, with Status fixed along the bottom', () => {
  const order = ['schema', 'parsed', 'bytes', 'status'];
  const read = (tiles: unknown) => parseLayout(JSON.stringify({ v: 4, order, hidden: [], sizes: {}, tiles }));
  // schema on top, parsed beside bytes, status at the bottom
  const rows = read([['schema'], ['parsed', 'bytes'], ['status']]);
  expect(rows?.dock?.tree).toEqual({ dir: 'col', children: ['schema', { dir: 'row', children: ['parsed', 'bytes'], weights: [1, 1] }], weights: [1, 1] });
  expect(rows?.order).toEqual(['schema', 'parsed', 'bytes', 'status']);
  // one row of three with status under: the row
  expect(read([['schema', 'parsed', 'bytes'], ['status']])?.dock?.tree).toEqual({ dir: 'row', children: ['schema', 'parsed', 'bytes'], weights: [1, 1, 1] });
  // status in the middle of the rows is moved out of them
  expect(leaves(read([['bytes'], ['status'], ['schema', 'parsed']])!.dock!.tree)).toEqual(['bytes', 'schema', 'parsed']);
  // junk rows lose the dock, not the order
  expect(read([['schema'], ['schema', 'parsed', 'bytes', 'status']])).toEqual({ order, hidden: [], sizes: {} });
  expect(read('rows')).toEqual({ order, hidden: [], sizes: {} });
  // older records have no dock
  for (const v of [1, 2, 3]) expect(parseLayout(JSON.stringify({ v, order, hidden: [], sizes: {} }))?.dock).toBeUndefined();
});

test('swapping keeps the dock, Status is not swapped, and a pane leaves the tree it is taken out of', () => {
  const layout = withDock(defaultLayout(), { tree: dockTo(defaultTree(), 'parsed', 'tr')!, status: null });
  expect(swapped(layout, 'schema', 'bytes').order).toEqual(['bytes', 'parsed', 'schema', 'status']);
  expect(dockedOrder(swapped(layout, 'schema', 'bytes').dock!)).toEqual(['bytes', 'parsed', 'schema', 'status']);
  expect(swapped(layout, 'schema', 'status')).toBe(layout);
  expect(without(layout.dock!.tree, 'schema')).toEqual({ dir: 'col', children: ['parsed', 'bytes'], weights: [1, 1] });
  expect(tidy({ dir: 'row', children: [{ dir: 'row', children: ['schema', 'parsed'], weights: [1, 1] }, 'bytes'], weights: [2, 1] })).toEqual({ dir: 'row', children: ['schema', 'parsed', 'bytes'], weights: [1, 1, 1] });
});

test('the grid has a column for every place where a pane ends, closed panes leave it, and Status is the last row', () => {
  const tree = dockTo(defaultTree(), 'parsed', 'tr')!;
  const all = new Set(['schema', 'parsed', 'bytes'] as const);
  const grid = dockGrid(tree, all, true, 0.4)!;
  expect(grid.place).toEqual({
    schema: { column: '1 / 2', row: '1 / 3' },
    parsed: { column: '2 / 3', row: '1 / 2' },
    bytes: { column: '2 / 3', row: '2 / 3' },
    status: { column: '1 / 3', row: '3 / 4' },
  });
  expect(grid.rows).toBe('minmax(0, 500000fr) minmax(0, 500000fr) minmax(0, 400000fr)');
  // Bytes closed: the right half is Parsed alone, the whole height
  const rest = dockGrid(tree, new Set(['schema', 'parsed'] as const), true, 0.4)!;
  expect(rest.place.parsed).toEqual({ column: '2 / 3', row: '1 / 2' });
  expect(rest.place.schema).toEqual({ column: '1 / 2', row: '1 / 2' });
  // nothing set: Status is as high as it is
  expect(dockGrid(tree, all, true, null)!.rows).toBe('minmax(0, 500000fr) minmax(0, 500000fr) auto');
  // Status closed: no row for it
  expect(dockGrid(tree, all, false, 0.4)!.place.status).toBeUndefined();
  // a closed pane leaves the view, and a split with one child left is that child
  expect(view(tree, new Set(['schema'] as const))).toEqual({ pane: 'schema' });
});

test.describe('the separators of the dock', () => {
  test.use({ viewport: { width: 1500, height: 900 } });
  test.beforeEach(async ({ page }) => ready(page));
  const bars = (page: Page) => page.locator('#splitters .splitter');

  test('between the panes of a split and above Status, by the keys, and the sizes are kept', async ({ page }) => {
    await dragToZone(page, 'pane-tree', 'tr');
    // Schema | (Parsed over Bytes), and Status under them
    await expect(bars(page)).toHaveCount(3);
    const column = bars(page).filter({ has: page.locator('xpath=self::*[@aria-orientation="vertical"]') });
    await expect(column).toHaveCount(1);
    const [schema0, parsed0] = await Promise.all([box(page, 'pane-editor'), box(page, 'pane-tree')]);
    await column.focus();
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press('ArrowRight');
    const [schema1, parsed1, bytes1] = await Promise.all([box(page, 'pane-editor'), box(page, 'pane-tree'), box(page, 'pane-hex')]);
    expect(schema1.width).toBeGreaterThan(schema0.width + 20);
    expect(parsed1.width).toBeLessThan(parsed0.width - 20);
    // Parsed and Bytes share the right half, so they changed together
    expect(Math.abs(parsed1.width - bytes1.width)).toBeLessThan(2);
    // the one between Parsed and Bytes moves only them
    const row = bars(page).filter({ has: page.locator('xpath=self::*[@aria-orientation="horizontal"]') });
    await expect(row).toHaveCount(2);
    const parsedHeight = parsed1.height;
    await row.first().focus();
    await page.keyboard.press('ArrowDown');
    expect((await box(page, 'pane-tree')).height).toBeGreaterThan(parsedHeight + 20);
    expect(Math.abs((await box(page, 'pane-editor')).height - schema1.height)).toBeLessThan(2);
    // above Status: the panes and Status share the height
    const statusHeight = (await box(page, 'pane-status')).height;
    await row.last().focus();
    await page.keyboard.press('ArrowUp');
    expect((await box(page, 'pane-status')).height).toBeGreaterThan(statusHeight + 20);
    // kept in the dock, and Enter puts the pair back
    const saved = await stored(page);
    expect(saved?.dock?.status).toBeGreaterThan(0.1);
    await page.keyboard.press('Enter');
    expect(Math.abs((await box(page, 'pane-status')).height - statusHeight)).toBeLessThan(3);
    await page.reload();
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
    expect((await box(page, 'pane-editor')).width).toBeGreaterThan(schema0.width + 20);
  });

  test('closed panes leave the dock and come back to where they were', async ({ page }) => {
    await dragToZone(page, 'pane-tree', 'tr');
    await page.locator('#pane-hex .pane-close').click();
    // Parsed has the whole right half now
    const [schema, parsed] = await Promise.all([box(page, 'pane-editor'), box(page, 'pane-tree')]);
    expect(Math.abs(schema.height - parsed.height)).toBeLessThan(3);
    await page.locator('#panes-button').click();
    await page.locator('#panes-list input[data-pane="bytes"]').check();
    await page.locator('#panes-close').click();
    const [parsedAgain, bytes] = await Promise.all([box(page, 'pane-tree'), box(page, 'pane-hex')]);
    expect(bytes.y).toBeGreaterThan(parsedAgain.y + parsedAgain.height - 2);
    // and Status can be closed and shown, and the others use its room
    await page.locator('#pane-status .pane-close').click();
    expect((await box(page, 'pane-hex')).y + (await box(page, 'pane-hex')).height).toBeGreaterThan(850);
  });

  test('a narrow window stacks the panes in the order of the dock and the dock returns when it is wide', async ({ page }) => {
    await dragToZone(page, 'pane-tree', 'tr');
    await page.setViewportSize({ width: 500, height: 900 });
    await expect.poll(async () => page.locator('main').getAttribute('data-docked')).toBeNull();
    const tops = await page.evaluate(() => [...document.querySelectorAll('main > section')].map((s) => [s.id, s.getBoundingClientRect().top] as const));
    expect(tops.sort((a, b) => a[1] - b[1]).map((t) => t[0])).toEqual(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status']);
    await page.setViewportSize({ width: 1500, height: 900 });
    await expect.poll(async () => page.locator('main').getAttribute('data-docked')).toBe('1');
  });

  test('the layout is reset with R, and a saved record of version 4 still opens as a dock', async ({ page }) => {
    await page.evaluate((key) => window.localStorage.setItem(key, JSON.stringify({ v: 4, order: ['schema', 'parsed', 'bytes', 'status'], hidden: [], sizes: {}, tiles: [['schema'], ['parsed', 'bytes'], ['status']] })), LAYOUT_KEY);
    await page.reload();
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
    expect(await page.locator('main').getAttribute('data-docked')).toBe('1');
    // Schema across the top, Parsed beside Bytes under it, Status at the bottom
    const [schema, parsed, bytes, status] = await Promise.all(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status'].map((id) => box(page, id)));
    expect(schema!.width).toBeGreaterThan(1300);
    expect(Math.abs(parsed!.y - bytes!.y)).toBeLessThan(2);
    expect(status!.y).toBeGreaterThan(parsed!.y);
    await page.locator('body').click({ position: { x: 5, y: 5 } });
    await page.keyboard.press('Shift+R');
    expect(await page.locator('main').getAttribute('data-docked')).toBeNull();
  });
});
