// Resizing the panes: the separators between them, by mouse, by keys, and as touch; the sizes kept per layout.

import { expect, test, type Page } from '@playwright/test';
import { LAYOUT_KEY, parseLayout } from '../src/layout.js';

const ready = async (page: Page): Promise<void> => {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
};

const stored = async (page: Page) => parseLayout(await page.evaluate((key) => window.localStorage.getItem(key), LAYOUT_KEY));
const bars = (page: Page) => page.locator('#splitters .splitter');
const box = async (page: Page, id: string) => {
  const b = await page.locator(`#${id}`).boundingBox();
  if (!b) throw new Error(`${id} has no box`);
  return b;
};
/** The widths (or heights) of the panes. */
const widths = async (page: Page, ids: string[]) => Promise.all(ids.map(async (id) => (await box(page, id)).width));
const heights = async (page: Page, ids: string[]) => Promise.all(ids.map(async (id) => (await box(page, id)).height));
const centre = async (page: Page, index: number) => {
  const b = await bars(page).nth(index).boundingBox();
  if (!b) throw new Error('no separator');
  return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
};
/** Drag a separator by a distance, with the mouse. */
const drag = async (page: Page, index: number, dx: number, dy: number, release = true): Promise<void> => {
  const from = await centre(page, index);
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + dx / 2, from.y + dy / 2, { steps: 3 });
  await page.mouse.move(from.x + dx, from.y + dy, { steps: 3 });
  if (release) await page.mouse.up();
};
const settle = (page: Page) => page.evaluate(() => new Promise<void>((done) => requestAnimationFrame(() => requestAnimationFrame(() => done()))));

test.describe('wide layout', () => {
  test.use({ viewport: { width: 1500, height: 900 } });
  test.beforeEach(async ({ page }) => ready(page));

  test('there is a separator between each two neighbouring panes, with the roles and values of a window splitter', async ({ page }) => {
    // two between the three columns, and one between them and the low pane
    await expect(bars(page)).toHaveCount(3);
    const first = bars(page).nth(0);
    await expect(first).toHaveAttribute('role', 'separator');
    await expect(first).toHaveAttribute('aria-orientation', 'vertical');
    await expect(first).toHaveAttribute('aria-label', 'Resize Schema and Parsed');
    await expect(first).toHaveAttribute('tabindex', '0');
    await expect(bars(page).nth(1)).toHaveAttribute('aria-label', 'Resize Parsed and Bytes');
    const low = bars(page).nth(2);
    await expect(low).toHaveAttribute('aria-orientation', 'horizontal');
    await expect(low).toHaveAttribute('aria-label', 'Resize Bytes and Status');
    // about a third of the room of the two columns each side: 50% for equal columns
    const now = Number(await first.getAttribute('aria-valuenow'));
    expect(now).toBeGreaterThanOrEqual(48);
    expect(now).toBeLessThanOrEqual(52);
    expect(Number(await first.getAttribute('aria-valuemin'))).toBeGreaterThan(5);
    expect(Number(await first.getAttribute('aria-valuemax'))).toBeLessThan(95);
    // a separator is in the gutter: between the boxes of the two panes, not over them
    const a = await box(page, 'pane-editor');
    const b = await box(page, 'pane-tree');
    const s = (await first.boundingBox())!;
    expect(s.x).toBeGreaterThanOrEqual(a.x + a.width - 6);
    expect(s.x + s.width).toBeLessThanOrEqual(b.x + 6);
  });

  test('dragging a separator makes one pane wider and the neighbour narrower by as much, and it is kept', async ({ page }) => {
    const ids = ['pane-editor', 'pane-tree', 'pane-hex'];
    const before = await widths(page, ids);
    await drag(page, 0, 120, 0);
    await settle(page);
    const after = await widths(page, ids);
    expect(after[0]! - before[0]!).toBeGreaterThan(100);
    expect(after[1]! - before[1]!).toBeLessThan(-100);
    expect(Math.abs(after[2]! - before[2]!)).toBeLessThan(2);
    expect(Math.abs(after[0]! + after[1]! - (before[0]! + before[1]!))).toBeLessThan(2);
    const saved = await stored(page);
    expect(saved?.sizes.wide?.cols?.length).toBe(3);
    expect(saved?.sizes.medium).toBeUndefined();
    await expect(page.locator('#layout-reset')).toBeVisible();
    await expect(page.locator('#layout-live')).toHaveText(/^Schema \d+%, Parsed \d+%$/);
    // and after a reload
    await page.reload();
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
    const again = await widths(page, ids);
    expect(Math.abs(again[0]! - after[0]!)).toBeLessThan(2);
    expect(Math.abs(again[1]! - after[1]!)).toBeLessThan(2);
  });

  test('a pane keeps a minimum width however far the drag goes', async ({ page }) => {
    const rem = await page.evaluate(() => parseFloat(getComputedStyle(document.documentElement).fontSize));
    await drag(page, 0, -2000, 0);
    await settle(page);
    expect((await box(page, 'pane-editor')).width).toBeGreaterThanOrEqual(12 * rem - 2);
    await drag(page, 0, 4000, 0);
    await settle(page);
    expect((await box(page, 'pane-tree')).width).toBeGreaterThanOrEqual(12 * rem - 2);
  });

  test('Esc during a drag puts the sizes back and keeps nothing', async ({ page }) => {
    const before = await widths(page, ['pane-editor', 'pane-tree']);
    await drag(page, 0, 150, 0, false);
    await settle(page);
    expect((await box(page, 'pane-editor')).width).toBeGreaterThan(before[0]! + 100);
    await page.keyboard.press('Escape');
    await page.mouse.up();
    await settle(page);
    const after = await widths(page, ['pane-editor', 'pane-tree']);
    expect(Math.abs(after[0]! - before[0]!)).toBeLessThan(2);
    expect(await stored(page)).toBeNull();
  });

  test('during a drag the page has the resize cursor and nothing is selected', async ({ page }) => {
    await drag(page, 0, 40, 0, false);
    expect(await page.evaluate(() => document.body.classList.contains('pane-resizing-col'))).toBe(true);
    expect(await page.evaluate(() => getComputedStyle(document.body).cursor)).toBe('col-resize');
    expect(await page.evaluate(() => window.getSelection()?.toString() ?? '')).toBe('');
    await page.mouse.up();
    expect(await page.evaluate(() => document.body.classList.contains('pane-resizing-col'))).toBe(false);
  });

  test('the arrow keys move a separator, Shift moves it more, Home and End give the room to a side, Enter puts it back', async ({ page }) => {
    const bar = bars(page).nth(0);
    await bar.focus();
    const start = Number(await bar.getAttribute('aria-valuenow'));
    await page.keyboard.press('ArrowRight');
    const one = Number(await bar.getAttribute('aria-valuenow'));
    expect(one).toBeGreaterThan(start);
    await expect(page.locator('#layout-live')).toHaveText(/^Schema \d+%, Parsed \d+%$/);
    await page.keyboard.press('Shift+ArrowRight');
    const big = Number(await bar.getAttribute('aria-valuenow'));
    expect(big - one).toBeGreaterThan(one - start);
    await page.keyboard.press('ArrowLeft');
    expect(Number(await bar.getAttribute('aria-valuenow'))).toBeLessThan(big);
    // the separator is still focused after each change
    expect(await page.evaluate(() => document.activeElement?.getAttribute('role'))).toBe('separator');
    await page.keyboard.press('End');
    expect(Number(await bar.getAttribute('aria-valuenow'))).toBe(Number(await bar.getAttribute('aria-valuemax')));
    await page.keyboard.press('Home');
    expect(Number(await bar.getAttribute('aria-valuenow'))).toBe(Number(await bar.getAttribute('aria-valuemin')));
    await page.keyboard.press('Enter');
    expect(Math.abs(Number(await bar.getAttribute('aria-valuenow')) - start)).toBeLessThanOrEqual(1);
    expect(await stored(page)).toBeNull();
    // a letter key is not taken by the separator: the page's own keys still work
    await page.keyboard.press('Shift+?');
    await expect(page.locator('#help-dialog')).toBeVisible();
  });

  test('a double-click on a separator puts that split back, and only that one', async ({ page }) => {
    await drag(page, 0, 100, 0);
    await settle(page);
    await drag(page, 1, -80, 0);
    await settle(page);
    const sized = await widths(page, ['pane-editor', 'pane-tree', 'pane-hex']);
    const point = await centre(page, 0);
    await page.mouse.dblclick(point.x, point.y);
    await settle(page);
    const after = await widths(page, ['pane-editor', 'pane-tree', 'pane-hex']);
    // the first two are equal again, the third is as it was
    expect(Math.abs(after[0]! - after[1]!)).toBeLessThan(2);
    expect(after[0]!).toBeLessThan(sized[0]! - 50);
  });

  test('the low pane can be made taller or shorter than its content, and the top row gives and takes the room', async ({ page }) => {
    const top = await heights(page, ['pane-editor']);
    const low = await heights(page, ['pane-status']);
    await drag(page, 2, 0, -150);
    await settle(page);
    const topAfter = await heights(page, ['pane-editor']);
    const lowAfter = await heights(page, ['pane-status']);
    expect(lowAfter[0]!).toBeGreaterThan(low[0]! + 100);
    expect(topAfter[0]!).toBeLessThan(top[0]! - 100);
    const saved = await stored(page);
    expect(saved?.sizes.wide?.rows?.length).toBe(2);
    await drag(page, 2, 0, 3000);
    await settle(page);
    const rem = await page.evaluate(() => parseFloat(getComputedStyle(document.documentElement).fontSize));
    expect((await box(page, 'pane-status')).height).toBeGreaterThanOrEqual(6 * rem - 2);
  });

  test('closing panes gives their room to the others, and reopening takes it back with the sizes', async ({ page }) => {
    await drag(page, 0, 150, 0);
    await settle(page);
    const sized = await widths(page, ['pane-editor', 'pane-tree', 'pane-hex']);
    // the panes that are left fill the places in order: closing Bytes puts Status in the third column
    await page.locator('#pane-hex .pane-close').click();
    await settle(page);
    await expect(bars(page)).toHaveCount(2);
    const third = await widths(page, ['pane-editor', 'pane-tree', 'pane-status']);
    expect(Math.abs(third[0]! - sized[0]!)).toBeLessThan(3);
    expect(Math.abs(third[2]! - sized[2]!)).toBeLessThan(3);
    // two panes: the two columns fill the width in the proportion they had
    await page.locator('#pane-status .pane-close').click();
    await settle(page);
    await expect(bars(page)).toHaveCount(1);
    const [a, b] = await widths(page, ['pane-editor', 'pane-tree']);
    expect(Math.abs(a! + b! + 16 - (1500 - 32))).toBeLessThan(4);
    expect(Math.abs(a! / b! - sized[0]! / sized[1]!)).toBeLessThan(0.05);
    // reopening both: the sizes are the ones they were
    await page.locator('#panes-button').click();
    await page.locator('#panes-dialog input[type=checkbox]').nth(2).check();
    await page.locator('#panes-dialog input[type=checkbox]').nth(3).check();
    await page.keyboard.press('Escape');
    await settle(page);
    const back = await widths(page, ['pane-editor', 'pane-tree', 'pane-hex']);
    for (const i of [0, 1, 2]) expect(Math.abs(back[i]! - sized[i]!)).toBeLessThan(3);
  });

  test('maximizing hides the separators and restoring brings the sizes back', async ({ page }) => {
    await drag(page, 0, 100, 0);
    await settle(page);
    const sized = await widths(page, ['pane-editor', 'pane-tree']);
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('f');
    await expect(bars(page).first()).toBeHidden();
    await page.keyboard.press('f');
    await expect(bars(page).first()).toBeVisible();
    await settle(page);
    const after = await widths(page, ['pane-editor', 'pane-tree']);
    expect(Math.abs(after[0]! - sized[0]!)).toBeLessThan(2);
  });

  test('Reset layout puts the default sizes back', async ({ page }) => {
    await drag(page, 0, 100, 0);
    await settle(page);
    expect((await stored(page))?.sizes.wide).toBeDefined();
    await page.locator('#layout-reset').click();
    await settle(page);
    expect(await stored(page)).toBeNull();
    const [a, b] = await widths(page, ['pane-editor', 'pane-tree']);
    expect(Math.abs(a! - b!)).toBeLessThan(2);
    await expect(page.locator('#layout-reset')).toBeHidden();
  });

  test('old records, newer records and junk sizes load without error and keep the rest', async ({ page }) => {
    const set = (value: string) => page.evaluate(([key, text]) => window.localStorage.setItem(key as string, text as string), [LAYOUT_KEY, value]);
    const order = ['schema', 'parsed', 'bytes', 'status'];
    for (const record of [
      { v: 2, order, hidden: [] },
      { v: 3, order, hidden: [], sizes: 'junk' },
      { v: 3, order, hidden: [], sizes: { wide: { cols: [1, 'x'] } } },
      { v: 9, order, hidden: [], sizes: { wide: { cols: [2, 1, 1] } } },
    ]) {
      await set(JSON.stringify(record));
      await page.reload();
      await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
      await expect(bars(page)).toHaveCount(3);
      const [a, b] = await widths(page, ['pane-editor', 'pane-tree']);
      if (record.v === 9) expect(a! / b!).toBeGreaterThan(1.5);
      else expect(Math.abs(a! - b!)).toBeLessThan(2);
    }
  });
});

test.describe('medium and narrow layouts have their own sizes', () => {
  test('in two columns the columns and the rows can be sized, and the wide sizes are not touched', async ({ page }) => {
    await page.setViewportSize({ width: 1500, height: 900 });
    await ready(page);
    await drag(page, 0, 100, 0);
    await settle(page);
    const wide = (await stored(page))?.sizes.wide?.cols;
    expect(wide).toBeDefined();
    await page.setViewportSize({ width: 1000, height: 900 });
    await settle(page);
    // a column separator, one under the first row, one above the low pane
    await expect(bars(page)).toHaveCount(3);
    await expect(bars(page).nth(0)).toHaveAttribute('aria-orientation', 'vertical');
    await expect(bars(page).nth(1)).toHaveAttribute('aria-orientation', 'horizontal');
    // the wide sizes are not applied here: two equal columns
    const [a, b] = await widths(page, ['pane-editor', 'pane-tree']);
    expect(Math.abs(a! - b!)).toBeLessThan(2);
    const before = (await heights(page, ['pane-editor']))[0]!;
    await drag(page, 1, 0, 120);
    await settle(page);
    const after = (await heights(page, ['pane-editor']))[0]!;
    expect(after - before).toBeGreaterThan(100);
    // the second pane of the row is as high
    expect(Math.abs((await heights(page, ['pane-tree']))[0]! - after)).toBeLessThan(2);
    await drag(page, 0, -100, 0);
    await settle(page);
    const saved = await stored(page);
    expect(saved?.sizes.wide?.cols).toEqual(wide);
    expect(saved?.sizes.medium?.cols).toBeDefined();
    expect(saved?.sizes.medium?.rows?.[0]).toBeGreaterThan(0.5);
    // back in the wide layout: its own sizes
    await page.setViewportSize({ width: 1500, height: 900 });
    await settle(page);
    const [x, y] = await widths(page, ['pane-editor', 'pane-tree']);
    expect(x! / y!).toBeGreaterThan(1.2);
  });

  test('in one column each separator sets the height of the pane above it, and the page gets longer', async ({ page }) => {
    await page.setViewportSize({ width: 500, height: 900 });
    await ready(page);
    await expect(bars(page)).toHaveCount(3);
    await expect(bars(page).nth(0)).toHaveAttribute('aria-orientation', 'horizontal');
    const before = (await heights(page, ['pane-editor']))[0]!;
    const length = await page.evaluate(() => document.documentElement.scrollHeight);
    await drag(page, 0, 0, 150);
    await settle(page);
    const after = (await heights(page, ['pane-editor']))[0]!;
    expect(after - before).toBeGreaterThan(120);
    expect(await page.evaluate(() => document.documentElement.scrollHeight)).toBeGreaterThan(length + 100);
    expect((await stored(page))?.sizes.narrow?.rows?.[0]).toBeGreaterThan(0.6);
    // the keys: the same separators, the up and down arrows
    const bar = bars(page).nth(1);
    await bar.focus();
    const high = (await heights(page, ['pane-tree']))[0]!;
    await page.keyboard.press('ArrowDown');
    await settle(page);
    expect((await heights(page, ['pane-tree']))[0]!).toBeGreaterThan(high);
    await page.keyboard.press('ArrowUp');
    await page.keyboard.press('ArrowUp');
    await settle(page);
    expect((await heights(page, ['pane-tree']))[0]!).toBeLessThan(high);
  });
});

test.describe('touch', () => {
  test('a finger drags a separator, and the page does not scroll', async ({ page, browserName }) => {
    test.skip(browserName !== 'chromium', 'the touch events are sent through the Chrome DevTools protocol');
    await page.setViewportSize({ width: 500, height: 900 });
    await ready(page);
    const target = bars(page).nth(0);
    const b = (await target.boundingBox())!;
    expect(b.height).toBeGreaterThanOrEqual(16);
    const before = (await heights(page, ['pane-editor']))[0]!;
    const scroll = await page.evaluate(() => window.scrollY);
    const client = await page.context().newCDPSession(page);
    const x = b.x + b.width / 2;
    const y = b.y + b.height / 2;
    await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
    for (let i = 1; i <= 6; i += 1) await client.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x, y: y + i * 20 }] });
    await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await settle(page);
    expect((await heights(page, ['pane-editor']))[0]!).toBeGreaterThan(before + 80);
    expect(await page.evaluate(() => window.scrollY)).toBe(scroll);
  });
});

test.describe('the content keeps its place', () => {
  test.use({ viewport: { width: 1500, height: 900 } });
  test('the schema editor keeps its text and cursor while a separator is dragged', async ({ page }) => {
    await ready(page);
    await page.locator('.cm-content').click();
    await page.keyboard.type('# a note');
    const text = await page.locator('.cm-content').innerText();
    await drag(page, 0, 120, 0);
    await settle(page);
    expect(await page.locator('.cm-content').innerText()).toBe(text);
    await page.keyboard.type('!');
    expect(await page.locator('.cm-content').innerText()).toContain('# a note!');
  });
});
