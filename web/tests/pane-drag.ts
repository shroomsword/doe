// Dragging a pane with the mouse, for the tests: press on the pane's title, move, and release over the first or the
// last quarter of the target (before it or after it; the low or the spanning panes are before / after it up and down).

import type { Page } from '@playwright/test';

export type Edge = 'before' | 'after';

/** Where to release over a pane: in its half that is before or after, by the axis that the page uses for it. */
export async function pointOver(page: Page, id: string, edge: Edge): Promise<{ x: number; y: number }> {
  const box = (await page.locator(`#${id}`).boundingBox())!;
  const main = (await page.locator('main').boundingBox())!;
  const across = box.width > main.width * 0.85;
  const f = edge === 'before' ? 0.25 : 0.75;
  return across ? { x: box.x + box.width / 2, y: box.y + box.height * f } : { x: box.x + box.width * f, y: box.y + box.height / 2 };
}

/** Press on the title of `from`, drag to `to` and release (`release: false` leaves the button down). */
export async function dragPane(page: Page, from: string, to: string, edge: Edge, options: { release?: boolean; shift?: boolean } = {}): Promise<void> {
  const title = (await page.locator(`#${from} .pane-title h2`).boundingBox())!;
  await page.mouse.move(title.x + 4, title.y + title.height / 2);
  await page.mouse.down();
  await page.mouse.move(title.x + 30, title.y + title.height / 2 + 10, { steps: 3 });
  const point = await pointOver(page, to, edge);
  await page.mouse.move(point.x, point.y, { steps: 6 });
  if (options.release === false) return;
  if (options.shift) await page.keyboard.down('Shift');
  await page.mouse.up();
  if (options.shift) await page.keyboard.up('Shift');
}
