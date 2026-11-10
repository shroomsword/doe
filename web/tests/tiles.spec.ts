// The tiles: the panes in rows that the user arranges (layout.ts), chosen in the Panes dialog, kept in the browser, resized
// between neighbours, and put one under the other in a narrow window.

import { expect, test, type Page } from '@playwright/test';
import {
  LAYOUT_KEY,
  TILE_PRESETS,
  defaultLayout,
  isDefault,
  layoutText,
  moved,
  movedAcross,
  movedInRow,
  parseLayout,
  swapped,
  tileGrid,
  tilesOf,
  withTiles,
  withVisible,
} from '../src/layout.js';

const ROWS = [['schema'], ['parsed', 'bytes'], ['status']] as const;
const rows = () => ROWS.map((row) => [...row]);

test('tiles are read when they hold each pane once, and only the tiles are lost when they do not', () => {
  const layout = withTiles(defaultLayout(), rows() as never);
  expect(layout.order).toEqual(['schema', 'parsed', 'bytes', 'status']);
  expect(layout.tiles).toEqual(rows());
  expect(parseLayout(layoutText(layout))).toEqual(layout);
  expect(isDefault(layout)).toBe(false);
  expect(isDefault(withTiles(layout, undefined))).toBe(true);
  // the order is the rows one after the other, whatever order was written
  const text = JSON.parse(layoutText(layout)) as Record<string, unknown>;
  expect(parseLayout(JSON.stringify({ ...text, order: ['status', 'bytes', 'parsed', 'schema'] }))?.order).toEqual(['schema', 'parsed', 'bytes', 'status']);
  for (const bad of [[], [[]], [['schema'], ['schema', 'parsed', 'bytes', 'status']], [['schema', 'parsed'], ['bytes']], [['schema', 'nothing', 'bytes', 'status', 'parsed']], 'rows', null, [[['schema']]]]) {
    const read = parseLayout(JSON.stringify({ ...text, order: ['parsed', 'schema', 'bytes', 'status'], tiles: bad }));
    expect(read).toEqual({ order: ['parsed', 'schema', 'bytes', 'status'], hidden: [], sizes: {} });
  }
  // a record of version 3 has none: it is the arrangement of the width of the window
  expect(parseLayout(JSON.stringify({ v: 3, order: ['schema', 'parsed', 'bytes', 'status'], hidden: [], sizes: {}, tiles: rows() }))?.tiles).toBeUndefined();
});

test('a pane moves along its row and across the rows, and swaps and drops keep the rows', () => {
  expect(movedInRow(rows() as never, 'parsed', 1)).toEqual([['schema'], ['bytes', 'parsed'], ['status']]);
  expect(movedInRow(rows() as never, 'parsed', -1)).toBeNull();
  expect(movedInRow(rows() as never, 'schema', 1)).toBeNull();
  // into the row above or under, at the end of it
  expect(movedAcross(rows() as never, 'bytes', -1)).toEqual([['schema', 'bytes'], ['parsed'], ['status']]);
  expect(movedAcross(rows() as never, 'schema', 1)).toEqual([['parsed', 'bytes', 'schema'], ['status']]);
  // alone in the first or last row: nowhere to go; sharing it: a row of its own beyond it
  expect(movedAcross(rows() as never, 'schema', -1)).toBeNull();
  expect(movedAcross(rows() as never, 'status', 1)).toBeNull();
  expect(movedAcross([['schema', 'parsed'], ['bytes', 'status']], 'parsed', -1)).toEqual([['parsed'], ['schema'], ['bytes', 'status']]);
  expect(movedAcross([['schema', 'parsed'], ['bytes', 'status']], 'status', 1)).toEqual([['schema', 'parsed'], ['bytes'], ['status']]);
  const layout = withTiles(defaultLayout(), rows() as never);
  expect(swapped(layout, 'schema', 'status').tiles).toEqual([['status'], ['parsed', 'bytes'], ['schema']]);
  expect(swapped(layout, 'schema', 'status').order).toEqual(['status', 'parsed', 'bytes', 'schema']);
  // dropping a pane next to another puts it in that row (and an emptied row goes)
  expect(moved(layout, 'schema', 'bytes', true).tiles).toEqual([['parsed', 'bytes', 'schema'], ['status']]);
  expect(moved(layout, 'status', 'parsed', false).tiles).toEqual([['schema'], ['status', 'parsed', 'bytes']]);
  // without tiles, the window's arrangement is made explicit to start from
  expect(tilesOf(defaultLayout())).toEqual([['schema', 'parsed', 'bytes'], ['status']]);
  for (const preset of TILE_PRESETS) if (preset.rows) expect(preset.rows.flat().sort()).toEqual(['bytes', 'parsed', 'schema', 'status']);
});

test('the grid of the tiles: a pane spans the stretches between its edges, closed panes leave their rows', () => {
  const layout = withTiles(defaultLayout(), rows() as never);
  const grid = tileGrid(layout, undefined);
  expect(grid?.place).toEqual({
    schema: { column: '1 / 3', row: '1 / 2' },
    parsed: { column: '1 / 2', row: '2 / 3' },
    bytes: { column: '2 / 3', row: '2 / 3' },
    status: { column: '1 / 3', row: '3 / 4' },
  });
  // the Status pane alone in a row is low, unless the user sized it
  expect(grid?.rows).toBe('minmax(0, 1.0000fr) minmax(0, 1.0000fr) minmax(0, 0.4000fr)');
  // rows with different widths share one grid
  const uneven = tileGrid(withTiles(defaultLayout(), [['schema', 'parsed', 'bytes'], ['status']]), { tileCols: [[1, 1, 2], [1]], tileRows: [1, 1] });
  expect(uneven?.place.schema?.column).toBe('1 / 2');
  expect(uneven?.place.bytes?.column).toBe('3 / 4');
  expect(uneven?.place.status?.column).toBe('1 / 4');
  const crossing = tileGrid(withTiles(defaultLayout(), [['schema', 'parsed'], ['bytes', 'status']]), { tileCols: [[1, 1], [1, 3]], tileRows: [1, 1] });
  expect(crossing?.columns.match(/minmax/g)).toHaveLength(3);
  expect(crossing?.place.schema?.column).toBe('1 / 3');
  expect(crossing?.place.parsed?.column).toBe('3 / 4');
  expect(crossing?.place.bytes?.column).toBe('1 / 2');
  expect(crossing?.place.status?.column).toBe('2 / 4');
  // sizes of another shape are not used
  expect(tileGrid(layout, { tileRows: [1, 1], tileCols: [[1]] })?.rows).toBe(grid?.rows);
  // a closed pane leaves its row, and a row with no pane shown is gone
  const closed = withVisible(withVisible(layout, 'schema', false), 'status', false);
  const rest = tileGrid(closed, undefined);
  expect(Object.keys(rest?.place ?? {})).toEqual(['parsed', 'bytes']);
  expect(rest?.place.parsed?.row).toBe('1 / 2');
  expect(tileGrid(withVisible(withVisible(withVisible(withVisible(layout, 'schema', false), 'status', false), 'parsed', false), 'bytes', false), undefined)).toBeNull();
});

const ready = async (page: Page): Promise<void> => {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
};
const stored = async (page: Page) => parseLayout(await page.evaluate((key) => window.localStorage.getItem(key), LAYOUT_KEY));
const box = async (page: Page, id: string) => {
  const b = await page.locator(`#${id}`).boundingBox();
  if (!b) throw new Error(`${id} has no box`);
  return b;
};
const choose = async (page: Page, id: string): Promise<void> => {
  await page.locator('#panes-button').click();
  await page.locator('#tiles-preset').selectOption(id);
  await page.locator('#panes-close').click();
};

test.describe('in a wide window', () => {
  test.use({ viewport: { width: 1500, height: 900 } });
  test.beforeEach(async ({ page }) => ready(page));

  test('schema on top, parsed beside bytes, status at the bottom', async ({ page }) => {
    await choose(page, 'rows');
    const [schema, parsed, bytes, status] = await Promise.all(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status'].map((id) => box(page, id)));
    if (!schema || !parsed || !bytes || !status) throw new Error('no boxes');
    // the first row is the Schema pane across the window
    expect(schema.width).toBeGreaterThan(1300);
    expect(parsed.y).toBeGreaterThanOrEqual(schema.y + schema.height);
    // the middle row has two panes of one width, side by side, and the last is the Status pane across
    expect(Math.abs(parsed.y - bytes.y)).toBeLessThan(2);
    expect(bytes.x).toBeGreaterThan(parsed.x + parsed.width - 2);
    expect(Math.abs(parsed.width - bytes.width)).toBeLessThan(4);
    expect(status.y).toBeGreaterThanOrEqual(parsed.y + parsed.height);
    expect(status.width).toBeGreaterThan(1300);
    // the Status pane is the low row; the others share the rest
    expect(status.height).toBeLessThan(schema.height);
    // kept
    expect((await stored(page))?.tiles).toEqual(rows());
    await page.reload();
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
    expect((await box(page, 'pane-editor')).width).toBeGreaterThan(1300);
    expect(await page.locator('main').getAttribute('data-tiled')).toBe('1');
  });

  test('the editor moves a pane by buttons, announces it, and keeps the focus', async ({ page }) => {
    await choose(page, 'rows');
    await page.locator('#panes-button').click();
    await expect(page.locator('#tiles-preset')).toHaveValue('rows');
    const up = page.locator('#tiles-editor button[data-key="bytes-up"]');
    await up.focus();
    await page.keyboard.press('Enter');
    await expect(page.locator('#layout-live')).toContainText('Bytes moved to the row above: row 1: Schema and Bytes; row 2: Parsed; row 3: Status');
    expect((await stored(page))?.tiles).toEqual([['schema', 'bytes'], ['parsed'], ['status']]);
    await expect(page.locator('#tiles-preset')).toHaveValue('custom');
    // the focus is on a button of the same pane
    expect(await page.evaluate(() => (document.activeElement as HTMLElement | null)?.dataset.key?.split('-')[0])).toBe('bytes');
    // a button that cannot move is disabled
    await expect(page.locator('#tiles-editor button[data-key="schema-left"]')).toBeDisabled();
    await expect(page.locator('#tiles-editor button[data-key="bytes-left"]')).toBeEnabled();
    await page.locator('#tiles-editor button[data-key="bytes-left"]').click();
    expect((await stored(page))?.tiles).toEqual([['bytes', 'schema'], ['parsed'], ['status']]);
    await page.locator('#panes-close').click();
    const [bytes, schema] = await Promise.all([box(page, 'pane-hex'), box(page, 'pane-editor')]);
    expect(bytes.x).toBeLessThan(schema.x);
    // by the width of the window again
    await choose(page, 'window');
    expect((await stored(page))?.tiles).toBeUndefined();
    expect(await page.locator('main').getAttribute('data-tiled')).toBeNull();
  });

  test('the separators resize the tiles of a row and the rows, by keys, and the sizes are kept', async ({ page }) => {
    await choose(page, 'rows');
    const bars = page.locator('#splitters .splitter');
    // one between Parsed and Bytes, and two between the rows
    await expect(bars).toHaveCount(3);
    const before = await Promise.all(['pane-tree', 'pane-hex'].map((id) => box(page, id)));
    const column = bars.filter({ has: page.locator('xpath=self::*[@aria-orientation="vertical"]') });
    await expect(column).toHaveCount(1);
    await column.focus();
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press('ArrowRight');
    const after = await Promise.all(['pane-tree', 'pane-hex'].map((id) => box(page, id)));
    expect(after[0]!.width).toBeGreaterThan(before[0]!.width + 20);
    expect(after[1]!.width).toBeLessThan(before[1]!.width - 20);
    // the Schema row above does not move sideways: it spans the window
    expect((await box(page, 'pane-editor')).width).toBeGreaterThan(1300);
    const saved = (await stored(page))?.sizes;
    const mode = Object.keys(saved ?? {})[0] as 'wide';
    expect(saved?.[mode]?.tileCols?.[1]?.[0]).toBeGreaterThan(1);
    // a row separator moves the rows
    const rowBar = bars.filter({ has: page.locator('xpath=self::*[@aria-orientation="horizontal"]') }).first();
    const top = (await box(page, 'pane-editor')).height;
    await rowBar.focus();
    await page.keyboard.press('ArrowDown');
    expect((await box(page, 'pane-editor')).height).toBeGreaterThan(top + 20);
    // Enter puts the pair back to the default
    await page.keyboard.press('Enter');
    expect(Math.abs((await box(page, 'pane-editor')).height - top)).toBeLessThan(3);
  });

  test('a closed pane leaves its row, and the rows that are left share the window', async ({ page }) => {
    await choose(page, 'rows');
    await page.locator('#panes-button').click();
    await page.locator('#panes-list input[data-pane="status"]').uncheck();
    await page.locator('#panes-close').click();
    await expect(page.locator('#pane-status')).toBeHidden();
    // two rows are left, and the Parsed and Bytes row has the room under the Schema row
    const [schema, parsed] = await Promise.all([box(page, 'pane-editor'), box(page, 'pane-tree')]);
    expect(parsed.y).toBeGreaterThanOrEqual(schema.y + schema.height);
    expect(parsed.y + parsed.height).toBeGreaterThan(800);
    expect((await stored(page))?.hidden).toEqual(['status']);
  });

  test('a maximized pane fills the page and the tiles come back', async ({ page }) => {
    await choose(page, 'rows');
    await page.locator('#pane-tree .pane-maximize').click();
    expect((await box(page, 'pane-tree')).width).toBeGreaterThan(1300);
    await page.locator('#pane-tree .pane-maximize').click();
    expect((await box(page, 'pane-tree')).width).toBeLessThan(800);
    expect(await page.locator('main').getAttribute('data-tiled')).toBe('1');
  });
});

test.describe('in other windows', () => {
  test('a narrow window puts the panes one under the other, in the order of the rows, and the tiles return when it is wide', async ({ page }) => {
    await page.setViewportSize({ width: 1500, height: 900 });
    await ready(page);
    await choose(page, 'stack');
    await page.setViewportSize({ width: 500, height: 900 });
    await expect.poll(async () => page.locator('main').getAttribute('data-tiled')).toBeNull();
    const boxes = await Promise.all(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status'].map((id) => box(page, id)));
    for (let i = 1; i < boxes.length; i += 1) expect(boxes[i]!.y).toBeGreaterThan(boxes[i - 1]!.y);
    expect(boxes[0]!.width).toBeGreaterThan(400);
    await page.setViewportSize({ width: 1500, height: 900 });
    await expect.poll(async () => page.locator('main').getAttribute('data-tiled')).toBe('1');
  });

  test('a medium window draws the tiles in the height of the window', async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 900 });
    await ready(page);
    await choose(page, 'rows');
    const [parsed, bytes, status] = await Promise.all(['pane-tree', 'pane-hex', 'pane-status'].map((id) => box(page, id)));
    expect(Math.abs(parsed!.y - bytes!.y)).toBeLessThan(2);
    expect(status!.y).toBeGreaterThan(parsed!.y);
    expect(status!.y + status!.height).toBeLessThan(900 * 1.05);
  });

  test('the layout is reset with the key R, and a stored layout of the old version still opens', async ({ page }) => {
    await page.setViewportSize({ width: 1500, height: 900 });
    await page.addInitScript((key) => {
      if (!window.localStorage.getItem(key)) window.localStorage.setItem(key, JSON.stringify({ v: 3, order: ['parsed', 'schema', 'bytes', 'status'], hidden: [], sizes: {} }));
    }, LAYOUT_KEY);
    await ready(page);
    expect(await page.locator('main').getAttribute('data-tiled')).toBeNull();
    // the stored order is kept: Parsed is first
    expect((await box(page, 'pane-tree')).x).toBeLessThan((await box(page, 'pane-editor')).x);
    await choose(page, 'line');
    expect((await stored(page))?.tiles).toEqual([['schema', 'parsed', 'bytes', 'status']]);
    await page.locator('body').click({ position: { x: 5, y: 5 } });
    await page.keyboard.press('Shift+R');
    expect(await stored(page)).toBeNull();
    expect(await page.locator('main').getAttribute('data-tiled')).toBeNull();
  });
});
