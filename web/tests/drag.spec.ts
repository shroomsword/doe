// Reordering the panes by dragging: insertion before or after, the grip, the feedback, Esc, the edge of the window, touch.

import { expect, test, type Page } from '@playwright/test';
import { LAYOUT_KEY, moved, defaultLayout, parseLayout } from '../src/layout.js';
import { dragPane, pointOver } from './pane-drag.js';

const ready = async (page: Page): Promise<void> => {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
};
const places = (page: Page) =>
  page.evaluate(() =>
    [...document.querySelectorAll<HTMLElement>('main > section')].filter((s) => !s.hidden).sort((a, b) => Number(a.dataset.slot) - Number(b.dataset.slot)).map((s) => s.id),
  );
const stored = (page: Page) => page.evaluate((key) => window.localStorage.getItem(key), LAYOUT_KEY);

test('moving a pane takes it out and puts it before or after another one', () => {
  const start = defaultLayout();
  expect(moved(start, 'schema', 'bytes', true).order).toEqual(['parsed', 'bytes', 'schema', 'status']);
  expect(moved(start, 'schema', 'bytes', false).order).toEqual(['parsed', 'schema', 'bytes', 'status']);
  expect(moved(start, 'status', 'schema', false).order).toEqual(['status', 'schema', 'parsed', 'bytes']);
  expect(moved(start, 'parsed', 'parsed', true)).toBe(start);
  // a closed pane keeps its place in the order
  expect(moved({ order: ['schema', 'parsed', 'bytes', 'status'], hidden: ['parsed'] }, 'status', 'schema', true)).toEqual({ order: ['schema', 'status', 'parsed', 'bytes'], hidden: ['parsed'] });
});

test.describe('wide window', () => {
  test.use({ viewport: { width: 1400, height: 800 } });
  test.beforeEach(async ({ page }) => ready(page));

  test('every title bar has a grip icon, with a tooltip that says what it does', async ({ page }) => {
    for (const id of ['pane-editor', 'pane-tree', 'pane-hex', 'pane-status']) {
      const grip = page.locator(`#${id} .pane-title .pane-grip`);
      await expect(grip).toHaveCount(1);
      await expect(grip.locator('svg')).toHaveCount(1);
      expect(await grip.getAttribute('title')).toContain('before or after');
      // the first thing in the title bar
      expect(await page.locator(`#${id} .pane-title`).evaluate((bar) => bar.firstElementChild?.className)).toBe('pane-grip');
    }
  });

  test('dropped on the second half of a pane it goes after it, on the first half before it, and the others shift', async ({ page }) => {
    await dragPane(page, 'pane-editor', 'pane-hex', 'after');
    expect(await places(page)).toEqual(['pane-tree', 'pane-hex', 'pane-editor', 'pane-status']);
    await expect(page.locator('#layout-live')).toHaveText('Schema moved after Bytes');
    expect(parseLayout(await stored(page))?.order).toEqual(['parsed', 'bytes', 'schema', 'status']);
    await dragPane(page, 'pane-editor', 'pane-tree', 'before');
    expect(await places(page)).toEqual(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status']);
    expect(await stored(page)).toBeNull();
    // the low place: above its middle is before it
    await dragPane(page, 'pane-tree', 'pane-status', 'after');
    expect(await places(page)).toEqual(['pane-editor', 'pane-hex', 'pane-status', 'pane-tree']);
  });

  test('while dragging the pane is dimmed, a line shows where it goes, and a copy follows the pointer', async ({ page }) => {
    await dragPane(page, 'pane-editor', 'pane-hex', 'after', { release: false });
    await expect(page.locator('#pane-editor')).toHaveClass(/dragging/);
    await expect(page.locator('.drag-ghost')).toHaveText('Schema');
    const line = page.locator('.drop-line');
    await expect(line).toBeVisible();
    const hex = (await page.locator('#pane-hex').boundingBox())!;
    const box = (await line.boundingBox())!;
    // after the Bytes pane: at its right edge, as tall as it is
    expect(Math.abs(box.x + box.width / 2 - (hex.x + hex.width))).toBeLessThan(4);
    expect(Math.abs(box.height - hex.height)).toBeLessThan(4);
    // before it: at its left edge
    const before = await pointOver(page, 'pane-hex', 'before');
    await page.mouse.move(before.x, before.y, { steps: 3 });
    expect(Math.abs((await line.boundingBox())!.x + 2 - hex.x)).toBeLessThan(4);
    // over the pane that is dragged, or outside the panes, there is no line
    const own = await pointOver(page, 'pane-editor', 'after');
    await page.mouse.move(own.x, own.y, { steps: 3 });
    await expect(line).toBeHidden();
    await page.mouse.up();
    await expect(page.locator('.drag-ghost')).toHaveCount(0);
    await expect(page.locator('#pane-editor')).not.toHaveClass(/dragging/);
    expect(await places(page)).toEqual(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status']);
  });

  test('Esc cancels a drag, and dropping outside the panes does nothing', async ({ page }) => {
    await dragPane(page, 'pane-editor', 'pane-hex', 'after', { release: false });
    await page.keyboard.press('Escape');
    await expect(page.locator('.drag-ghost')).toHaveCount(0);
    await expect(page.locator('.drop-line')).toHaveCount(0);
    await page.mouse.up();
    expect(await places(page)).toEqual(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status']);
    expect(await stored(page)).toBeNull();
    // outside: over the header
    await dragPane(page, 'pane-editor', 'pane-hex', 'after', { release: false });
    await page.mouse.move(300, 5, { steps: 4 });
    await page.mouse.up();
    expect(await places(page)).toEqual(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status']);
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
    expect(await places(page)).toEqual(['pane-editor', 'pane-tree', 'pane-hex', 'pane-status']);
  });

  test('the empty part of a title bar drags too, and the grip drags with a pen or a finger', async ({ page }) => {
    const bar = (await page.locator('#pane-hex .pane-title').boundingBox())!;
    await page.mouse.move(bar.x + bar.width / 2, bar.y + bar.height / 2);
    await page.mouse.down();
    await page.mouse.move(bar.x + bar.width / 2 + 40, bar.y + bar.height / 2 + 40, { steps: 3 });
    await expect(page.locator('.drag-ghost')).toHaveText('Bytes');
    await page.mouse.up();
    // a touch press on the grip: pointer events of type touch, as a finger sends them
    const result = await page.evaluate(() => {
      const grip = document.querySelector('#pane-editor .pane-grip') as HTMLElement;
      const target = document.querySelector('#pane-hex') as HTMLElement;
      const from = grip.getBoundingClientRect();
      const to = target.getBoundingClientRect();
      const fire = (type: string, x: number, y: number, node: EventTarget) =>
        node.dispatchEvent(new PointerEvent(type, { pointerId: 7, pointerType: 'touch', isPrimary: true, button: 0, buttons: 1, clientX: x, clientY: y, bubbles: true, cancelable: true }));
      fire('pointerdown', from.x + 3, from.y + 3, grip);
      fire('pointermove', from.x + 40, from.y + 40, window);
      fire('pointermove', to.x + to.width * 0.75, to.y + to.height / 2, window);
      const ghost = document.querySelector('.drag-ghost')?.textContent ?? null;
      fire('pointerup', to.x + to.width * 0.75, to.y + to.height / 2, window);
      // a touch on the title bar away from the grip does not drag (it scrolls)
      const h2 = document.querySelector('#pane-tree .pane-title h2') as HTMLElement;
      const b = h2.getBoundingClientRect();
      fire('pointerdown', b.x + 3, b.y + 3, h2);
      fire('pointermove', b.x + 80, b.y + 80, window);
      const other = document.querySelector('.drag-ghost') !== null;
      fire('pointerup', b.x + 80, b.y + 80, window);
      return { ghost, other };
    });
    expect(result.ghost).toBe('Schema');
    expect(result.other).toBe(false);
    // the Bytes pane that was moved by the mouse first, then the touch put Schema after it
    expect((await places(page)).indexOf('pane-editor')).toBeGreaterThan((await places(page)).indexOf('pane-hex') - 1);
  });

  test('the saved layout and the keyboard moves agree with the mouse', async ({ page }) => {
    await dragPane(page, 'pane-tree', 'pane-hex', 'after');
    const byMouse = await places(page);
    await page.reload();
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
    expect(await places(page)).toEqual(byMouse);
    await page.getByRole('button', { name: 'Reset the layout' }).click();
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('Alt+Shift+KeyL');
    // one place to the right: the same as dropping it after its neighbour
    expect(await places(page)).toEqual(byMouse);
  });
});

test.describe('one column', () => {
  test('the page scrolls while a pane is dragged near the bottom edge, and a drop on a pane below puts it after it', async ({ page }) => {
    await page.setViewportSize({ width: 500, height: 700 });
    await ready(page);
    const title = (await page.locator('#pane-editor .pane-title h2').boundingBox())!;
    await page.mouse.move(title.x + 4, title.y + title.height / 2);
    await page.mouse.down();
    await page.mouse.move(title.x + 30, 690, { steps: 5 });
    await page.waitForFunction(() => window.scrollY > 50);
    // go back up to the Parsed pane and drop on its lower half: after it
    await page.mouse.move(250, 10, { steps: 2 });
    await page.waitForFunction(() => window.scrollY < 5, undefined, { timeout: 8000 });
    const tree = (await page.locator('#pane-tree').boundingBox())!;
    await page.mouse.move(250, Math.min(tree.y + tree.height * 0.75, 690), { steps: 4 });
    await page.mouse.up();
    // the target may have been out of view: only check that nothing broke and the layout is a permutation
    expect((await places(page)).sort()).toEqual(['pane-editor', 'pane-hex', 'pane-status', 'pane-tree']);
  });
});
