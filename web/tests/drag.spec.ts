// Docking the panes by dragging: the eight positions of a drop, what each makes, the outline while dragging, a swap by dropping
// on the middle of another pane, Status that stays where it is, and the list of places for the keyboard.

import { expect, test, type Page } from '@playwright/test';
import { type Zone, ZONES, ZONE_NAMES, dockTo, defaultTree, leaves, rects, swapIn } from '../src/dock.js';
import { LAYOUT_KEY, parseLayout } from '../src/layout.js';
import { area, dragOnto, dragToZone, startDrag, zonePoint } from './pane-drag.js';

const ready = async (page: Page): Promise<void> => {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
};
const stored = async (page: Page) => parseLayout(await page.evaluate((key) => window.localStorage.getItem(key), LAYOUT_KEY));
const box = async (page: Page, id: string) => (await page.locator(`#${id}`).boundingBox())!;
const IDS = { schema: 'pane-editor', parsed: 'pane-tree', bytes: 'pane-hex' } as const;

test('docking a pane to a position makes the layout that the rules say (no browser)', () => {
  const start = defaultTree();
  const where = (zone: Zone, pane: 'schema' | 'parsed' | 'bytes') => {
    const tree = dockTo(start, pane, zone)!;
    return Object.fromEntries([...rects(tree)].map(([name, b]) => [name, [b.x0, b.y0, b.x1, b.y1].map((n) => Math.round(n * 100) / 100)]));
  };
  // your example: Parsed to the top right
  expect(where('tr', 'parsed')).toEqual({ schema: [0, 0, 0.5, 1], parsed: [0.5, 0, 1, 0.5], bytes: [0.5, 0.5, 1, 1] });
  // the other corners put the pane in that quarter and the others fill the rest in reading order, a column at a time
  expect(where('br', 'schema')).toEqual({ parsed: [0, 0, 0.5, 1], bytes: [0.5, 0, 1, 0.5], schema: [0.5, 0.5, 1, 1] });
  expect(where('tl', 'bytes')).toEqual({ bytes: [0, 0, 0.5, 0.5], schema: [0, 0.5, 0.5, 1], parsed: [0.5, 0, 1, 1] });
  expect(where('bl', 'parsed')).toEqual({ schema: [0, 0, 0.5, 0.5], parsed: [0, 0.5, 0.5, 1], bytes: [0.5, 0, 1, 1] });
  // a side makes the pane a strip of a third, and the others keep their arrangement in what is left
  expect(where('l', 'bytes')).toEqual({ bytes: [0, 0, 0.33, 1], schema: [0.33, 0, 0.67, 1], parsed: [0.67, 0, 1, 1] });
  expect(where('r', 'schema')).toEqual({ parsed: [0, 0, 0.33, 1], bytes: [0.33, 0, 0.67, 1], schema: [0.67, 0, 1, 1] });
  expect(where('t', 'parsed')).toEqual({ parsed: [0, 0, 1, 0.33], schema: [0, 0.33, 0.5, 1], bytes: [0.5, 0.33, 1, 1] });
  expect(where('b', 'schema')).toEqual({ parsed: [0, 0, 0.5, 0.67], bytes: [0.5, 0, 1, 0.67], schema: [0, 0.67, 1, 1] });
  // swapping
  expect(leaves(swapIn(start, 'schema', 'bytes'))).toEqual(['bytes', 'parsed', 'schema']);
  // a pane docked next to the only other one: sides only
  expect(dockTo('schema', 'schema', 'l')).toBeNull();
});

test.describe('in a wide window', () => {
  test.use({ viewport: { width: 1500, height: 900 } });
  test.beforeEach(async ({ page }) => ready(page));

  test('only Schema, Parsed and Bytes have a grip: Status is fixed', async ({ page }) => {
    await expect(page.locator('.pane-grip')).toHaveCount(3);
    await expect(page.locator('#pane-status .pane-grip')).toHaveCount(0);
    // dragging the title of Status does nothing
    const before = await box(page, 'pane-status');
    const title = (await page.locator('#pane-status .pane-title').boundingBox())!;
    await page.mouse.move(title.x + title.width / 2, title.y + title.height / 2);
    await page.mouse.down();
    await page.mouse.move(title.x + 100, title.y - 200, { steps: 4 });
    await expect(page.locator('.dock-overlay')).toHaveCount(0);
    await page.mouse.up();
    expect(await box(page, 'pane-status')).toEqual(before);
  });

  test('Parsed dragged to the top right: Schema takes the left half, Parsed the top right, Bytes the bottom right, Status stays', async ({ page }) => {
    const status = await box(page, 'pane-status');
    await dragToZone(page, 'pane-tree', 'tr');
    const [schema, parsed, bytes, statusAfter] = await Promise.all(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status'].map((id) => box(page, id)));
    const total = schema!.width + parsed!.width + 16;
    // the left half is Schema, the whole height of the area
    expect(schema!.x).toBeLessThan(parsed!.x);
    expect(Math.abs(schema!.width - parsed!.width)).toBeLessThan(6);
    expect(Math.abs(schema!.height - (parsed!.height + bytes!.height + 16))).toBeLessThan(4);
    // Parsed is above Bytes in the right half
    expect(Math.abs(parsed!.x - bytes!.x)).toBeLessThan(2);
    expect(bytes!.y).toBeGreaterThan(parsed!.y + parsed!.height - 2);
    expect(total).toBeGreaterThan(1200);
    // Status did not move
    expect(statusAfter!.y).toBeGreaterThanOrEqual(status.y - 2);
    expect(statusAfter!.width).toBeGreaterThan(1400);
    // kept, with the order of the places the panes are in now
    const layout = await stored(page);
    expect(layout?.dock).toBeDefined();
    expect(layout?.order).toEqual(['schema', 'parsed', 'bytes', 'status']);
    await expect(page.locator('#layout-live')).toContainText('Parsed docked at the top right: Schema left, Parsed top right, Bytes bottom right');
    await page.reload();
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
    expect(Math.abs((await box(page, 'pane-tree')).x - parsed!.x)).toBeLessThan(2);
  });

  for (const zone of ZONES) {
    test(`each position gives its layout: ${ZONE_NAMES[zone]}`, async ({ page }) => {
      await dragToZone(page, 'pane-tree', zone);
      const layout = await stored(page);
      expect(layout?.dock).toBeDefined();
      const expected = rects(dockTo(defaultTree(), 'parsed', zone)!);
      const main = await area(page);
      for (const [pane, id] of Object.entries(IDS)) {
        const e = expected.get(pane as 'schema')!;
        const b = await box(page, id);
        const fx = (b.x - main.x) / main.width;
        const fy = (b.y - main.y) / main.height;
        expect(Math.abs(fx - e.x0), `${pane} x`).toBeLessThan(0.03);
        expect(Math.abs(fy - e.y0), `${pane} y`).toBeLessThan(0.03);
        expect(Math.abs(b.width / main.width - (e.x1 - e.x0)), `${pane} width`).toBeLessThan(0.04);
        expect(Math.abs(b.height / main.height - (e.y1 - e.y0)), `${pane} height`).toBeLessThan(0.04);
      }
      expect((await box(page, 'pane-status')).width).toBeGreaterThan(1400);
    });
  }

  test('while dragging the eight positions show, the one under the pointer lights, and the layout it makes is outlined', async ({ page }) => {
    await dragToZone(page, 'pane-tree', 'tr', { release: false });
    await expect(page.locator('.drag-ghost')).toHaveText('Parsed');
    await expect(page.locator('#pane-tree')).toHaveClass(/dragging/);
    await expect(page.locator('.dock-zone')).toHaveCount(8);
    await expect(page.locator('.dock-zone.hot')).toHaveText('top right');
    // the outline has the three panes, the one that moves marked
    await expect(page.locator('.dock-ghost')).toHaveCount(3);
    await expect(page.locator('.dock-ghost.moving')).toHaveText('Parsed');
    const point = await zonePoint(page, 'bl');
    await page.mouse.move(point.x, point.y, { steps: 4 });
    await expect(page.locator('.dock-zone.hot')).toHaveText('bottom left');
    // over nothing (the middle of the dragged pane itself): no position is lit
    const own = await box(page, 'pane-tree');
    await page.mouse.move(own.x + own.width / 2, own.y + own.height / 2, { steps: 4 });
    await expect(page.locator('.dock-zone.hot')).toHaveCount(0);
    await page.mouse.up();
    await expect(page.locator('.dock-overlay')).toHaveCount(0);
    expect(await stored(page)).toBeNull();
  });

  test('Esc cancels a drag, and dropping outside the area does nothing', async ({ page }) => {
    await dragToZone(page, 'pane-hex', 'l', { release: false });
    await expect(page.locator('.dock-overlay')).toHaveCount(1);
    await page.keyboard.press('Escape');
    await expect(page.locator('.dock-overlay')).toHaveCount(0);
    await page.mouse.up();
    expect(await stored(page)).toBeNull();
    // released over the header
    await startDrag(page, 'pane-hex');
    await page.mouse.move(300, 20, { steps: 4 });
    await page.mouse.up();
    expect(await stored(page)).toBeNull();
  });

  test('a press on a control of the title bar, or a press that does not move, is not a drag', async ({ page }) => {
    const radix = (await page.locator('#pane-tree #radix').boundingBox())!;
    await page.mouse.move(radix.x + 4, radix.y + 4);
    await page.mouse.down();
    await page.mouse.move(radix.x + 200, radix.y + 200, { steps: 3 });
    await expect(page.locator('.drag-ghost')).toHaveCount(0);
    await page.mouse.up();
    const title = (await page.locator('#pane-hex .pane-title h2').boundingBox())!;
    await page.mouse.move(title.x + 4, title.y + 4);
    await page.mouse.down();
    await page.mouse.move(title.x + 6, title.y + 5);
    await expect(page.locator('.drag-ghost')).toHaveCount(0);
    await page.mouse.up();
    expect(await stored(page)).toBeNull();
  });

  test('dropped on the middle of another pane the two swap, and Status cannot be swapped', async ({ page }) => {
    await dragOnto(page, 'pane-editor', 'pane-hex', { release: false });
    await expect(page.locator('.dock-ghost')).toHaveCount(3);
    await page.mouse.up();
    const [schema, bytes] = await Promise.all([box(page, 'pane-editor'), box(page, 'pane-hex')]);
    expect(bytes.x).toBeLessThan(schema.x);
    expect((await stored(page))?.order).toEqual(['bytes', 'parsed', 'schema', 'status']);
    // Status is fixed: the keys that swap say so
    await page.locator('#pane-status').focus();
    await page.keyboard.press('Alt+Shift+KeyK');
    await expect(page.locator('#layout-live')).toContainText('Status pane is fixed');
    expect((await stored(page))?.order).toEqual(['bytes', 'parsed', 'schema', 'status']);
  });

  test('the title bar of a pane drags with a pen or a finger from its grip, and not from the rest of the bar', async ({ page }) => {
    const result = await page.evaluate(() => {
      const grip = document.querySelector('#pane-editor .pane-grip') as HTMLElement;
      const from = grip.getBoundingClientRect();
      const main = document.querySelector('#pane-tree')!.getBoundingClientRect();
      const fire = (type: string, x: number, y: number, node: EventTarget) =>
        node.dispatchEvent(new PointerEvent(type, { pointerId: 7, pointerType: 'touch', isPrimary: true, button: 0, buttons: 1, clientX: x, clientY: y, bubbles: true, cancelable: true }));
      fire('pointerdown', from.x + 3, from.y + 3, grip);
      fire('pointermove', from.x + 40, from.y + 40, window);
      fire('pointermove', main.x + main.width / 2, main.y + main.height / 2, window);
      const ghost = document.querySelector('.drag-ghost')?.textContent ?? null;
      const zones = document.querySelectorAll('.dock-zone').length;
      fire('pointerup', main.x + main.width / 2, main.y + main.height / 2, window);
      const h2 = document.querySelector('#pane-tree .pane-title h2') as HTMLElement;
      const b = h2.getBoundingClientRect();
      fire('pointerdown', b.x + 3, b.y + 3, h2);
      fire('pointermove', b.x + 80, b.y + 80, window);
      const other = document.querySelector('.drag-ghost') !== null;
      fire('pointerup', b.x + 80, b.y + 80, window);
      return { ghost, zones, other };
    });
    expect(result.ghost).toBe('Schema');
    expect(result.zones).toBe(8);
    expect(result.other).toBe(false);
    // dropped on the middle of Parsed: swapped
    expect((await stored(page))?.order).toEqual(['parsed', 'schema', 'bytes', 'status']);
  });

  test('the list of places moves a pane without the mouse, and says what it did', async ({ page }) => {
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('Shift+P');
    await expect(page.locator('#dock-dialog')).toBeVisible();
    await expect(page.locator('#dock-title')).toHaveText('Place the Parsed pane');
    await expect(page.locator('#dock-grid button')).toHaveCount(8);
    // the first place that can be chosen has the focus, and the arrows move over the map
    expect(await page.evaluate(() => (document.activeElement as HTMLElement | null)?.dataset.zone)).toBe('tl');
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press('ArrowRight');
    expect(await page.evaluate(() => (document.activeElement as HTMLElement | null)?.dataset.zone)).toBe('tr');
    await page.keyboard.press('Enter');
    await expect(page.locator('#dock-dialog')).toBeHidden();
    await expect(page.locator('#layout-live')).toContainText('Parsed docked at the top right: Schema left, Parsed top right, Bytes bottom right');
    const [schema, parsed, bytes] = await Promise.all(['pane-editor', 'pane-tree', 'pane-hex'].map((id) => box(page, id)));
    expect(Math.abs(parsed!.x - bytes!.x)).toBeLessThan(2);
    expect(schema!.x).toBeLessThan(parsed!.x);
    // a click on the grip of a title bar opens it too, for that pane
    await page.locator('#pane-hex .pane-grip').click();
    await expect(page.locator('#dock-title')).toHaveText('Place the Bytes pane');
    await page.keyboard.press('Escape');
    await expect(page.locator('#dock-dialog')).toBeHidden();
  });

  test('with a pane closed only the sides are offered, with one shown nothing is', async ({ page }) => {
    await page.locator('#pane-editor .pane-close').click();
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('Shift+P');
    await expect(page.locator('#dock-grid button:not(:disabled)')).toHaveCount(4);
    for (const zone of ['tl', 'tr', 'bl', 'br']) await expect(page.locator(`#dock-grid button[data-zone="${zone}"]`)).toBeDisabled();
    await page.locator('#dock-grid button[data-zone="r"]').click();
    const [parsed, bytes] = await Promise.all([box(page, 'pane-tree'), box(page, 'pane-hex')]);
    expect(bytes.x).toBeLessThan(parsed.x);
    await page.locator('#pane-hex .pane-close').click();
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('Shift+P');
    await expect(page.locator('#dock-dialog')).toBeHidden();
    await expect(page.locator('#layout-live')).toContainText('no other pane');
  });

  test('a maximized pane cannot be dragged, and the Panes dialog has no move buttons any more', async ({ page }) => {
    await page.locator('#pane-tree .pane-maximize').click();
    await startDrag(page, 'pane-tree');
    await expect(page.locator('.dock-overlay')).toHaveCount(0);
    await page.mouse.up();
    await page.locator('#pane-tree .pane-maximize').click();
    await page.locator('#panes-button').click();
    await expect(page.locator('#panes-list input')).toHaveCount(4);
    await expect(page.locator('#panes-dialog button', { hasText: /[◀▶▲▼]/ })).toHaveCount(0);
    await expect(page.locator('#tiles-editor, #tiles-preset')).toHaveCount(0);
  });
});

test.describe('in a narrow window', () => {
  test.use({ viewport: { width: 500, height: 900 } });
  test('the panes are one under the other: nothing is dragged to a position, and the list says so', async ({ page }) => {
    await ready(page);
    await startDrag(page, 'pane-tree');
    await expect(page.locator('.dock-overlay')).toHaveCount(0);
    await page.mouse.up();
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('Shift+P');
    await expect(page.locator('#dock-dialog')).toBeHidden();
    await expect(page.locator('#layout-live')).toContainText('narrow window');
  });
});
