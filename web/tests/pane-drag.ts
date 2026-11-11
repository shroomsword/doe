// Dragging a pane with the mouse, for the tests: press on the pane's title, move to one of the eight positions of the area (or
// onto the middle of another pane, to swap them) and release.

import type { Page } from '@playwright/test';
import type { Zone } from '../src/dock.js';

/** The centre of each position, as fractions of the area that the three movable panes fill. */
const CENTRES: Record<Zone, [number, number]> = {
  tl: [0.13, 0.13],
  t: [0.5, 0.08],
  tr: [0.87, 0.13],
  l: [0.08, 0.5],
  r: [0.92, 0.5],
  bl: [0.13, 0.87],
  b: [0.5, 0.92],
  br: [0.87, 0.87],
};

/** The area above Status that the panes fill, in the window. */
export async function area(page: Page): Promise<{ x: number; y: number; width: number; height: number }> {
  const boxes = await page.evaluate(() =>
    ['pane-editor', 'pane-tree', 'pane-hex']
      .map((id) => document.getElementById(id) as HTMLElement)
      .filter((el) => !el.hidden)
      .map((el) => el.getBoundingClientRect())
      .map((r) => ({ left: r.left, top: r.top, right: r.right, bottom: r.bottom })),
  );
  const left = Math.min(...boxes.map((b) => b.left));
  const top = Math.min(...boxes.map((b) => b.top));
  return { x: left, y: top, width: Math.max(...boxes.map((b) => b.right)) - left, height: Math.max(...boxes.map((b) => b.bottom)) - top };
}

/** Where to release to drop on `zone`. */
export async function zonePoint(page: Page, zone: Zone): Promise<{ x: number; y: number }> {
  const box = await area(page);
  const [fx, fy] = CENTRES[zone];
  return { x: box.x + box.width * fx, y: box.y + box.height * fy };
}

/** Press on the title of `from` and move a little, so that the drag has begun. */
export async function startDrag(page: Page, from: string): Promise<void> {
  const title = (await page.locator(`#${from} .pane-title h2`).boundingBox())!;
  await page.mouse.move(title.x + 4, title.y + title.height / 2);
  await page.mouse.down();
  await page.mouse.move(title.x + 30, title.y + title.height / 2 + 10, { steps: 3 });
}

/** Drag the pane `from` to a position (`release: false` leaves the button down over it). */
export async function dragToZone(page: Page, from: string, zone: Zone, options: { release?: boolean } = {}): Promise<void> {
  await startDrag(page, from);
  const point = await zonePoint(page, zone);
  await page.mouse.move(point.x, point.y, { steps: 6 });
  if (options.release !== false) await page.mouse.up();
}

/** Drag the pane `from` onto the middle of the pane `onto`: the two swap. */
export async function dragOnto(page: Page, from: string, onto: string, options: { release?: boolean } = {}): Promise<void> {
  await startDrag(page, from);
  const box = (await page.locator(`#${onto}`).boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 6 });
  if (options.release !== false) await page.mouse.up();
}
