// Follow a pointer: from a field that a `pos:` or `size:` reads to the value it places or sizes, and back.

import { type Page, expect, test } from '@playwright/test';
import type { TraceNode } from '../src/doe.js';
import { History, follow, startingAt } from '../src/pointers.js';

function node(name: string, extra: Partial<TraceNode> = {}, children: TraceNode[] = []): TraceNode {
  return { name, type: null, start: null, end: null, instance: false, failed: false, problem: null, processed: false, summary: null, integer: null, children, ...extra };
}

test('places are walked back and forward; a jump after going back drops what was ahead; near hops and old places go', () => {
  const at = (offset: number) => ({ offset, path: [] });
  const h = new History();
  h.leave(at(0));
  h.arrive(at(1000), 100);
  h.leave(at(1000));
  h.arrive(at(5000), 100);
  expect(h.length).toBe(3);
  h.remember(at(5000));
  expect(h.walk(false)?.offset).toBe(1000);
  expect(h.walk(false)?.offset).toBe(0);
  expect(h.walk(false)).toBeNull();
  expect(h.walk(true)?.offset).toBe(1000);
  h.leave(at(1000));
  h.arrive(at(9000), 100);
  expect(h.walk(true)).toBeNull();
  expect(h.walk(false)?.offset).toBe(1000);
  // a hop within a screen replaces the place; from the very first place it is no jump
  const near = new History();
  near.leave(at(10));
  near.arrive(at(20), 100);
  expect(near.length).toBe(0);
  const long = new History();
  long.leave(at(0));
  for (let n = 1; n <= 150; n += 1) {
    long.arrive(at(n * 1000), 100);
    long.leave(at(n * 1000));
  }
  expect(long.length).toBe(100);
});

test('a field is followed to what it places, or by a guess to what starts at its number, or it says why not', () => {
  const root = node('file', { start: 0, end: 48 }, [
    node('ofs', { start: 0, end: 16, integer: { value: '4', name: null }, points_to: { path: [2], start: 32, more: 1 } }),
    node('count', { start: 16, end: 32, integer: { value: '4', name: null } }),
    node('table', { start: 32, end: 48, instance: true }),
    node('name', { start: 24, end: 32, summary: '"x"' }),
  ]);
  expect(follow(root, [0], 6)).toEqual({ path: [2], offset: 4, more: 1, guess: false });
  // no link: the outermost node that starts at the number
  expect(follow(root, [1], 6)).toEqual({ path: [2], offset: 4, more: 0, guess: true });
  expect(startingAt(root, 3)).toEqual([3]);
  expect(startingAt(root, 0)).toEqual([0]);
  expect(follow(root, [3], 6)).toEqual({ error: 'this field does not point at anything' });
  const far = node('file', {}, [node('n', { start: 0, end: 8, integer: { value: '999', name: null } })]);
  expect(follow(far, [0], 10)).toEqual({ error: '0x3e7 is past the end of the file (10 bytes)' });
  const none = node('file', {}, [node('n', { start: 0, end: 8, integer: { value: '5', name: null } })]);
  expect(follow(none, [0], 10)).toHaveProperty('error');
  const negative = node('file', {}, [node('n', { start: 0, end: 8, integer: { value: '-2', name: null } })]);
  expect(follow(negative, [0], 10)).toEqual({ error: '-2 is not an offset' });
});

test.describe('in the page', () => {
  test.use({ viewport: { width: 1400, height: 800 } });

  const SCHEMA = 'meta:\n  id: p\n  endian: le\nseq:\n  - id: ofs\n    type: u2\n  - id: gap\n    size: 0x7fe\ninstances:\n  table:\n    pos: ofs\n    type: u1\n';
  const data = (): Buffer => {
    const bytes = Buffer.alloc(0x801);
    bytes[0] = 0x00;
    bytes[1] = 0x08;
    return bytes;
  };
  const load = async (page: Page): Promise<void> => {
    await page.goto('/');
    await page.locator('.cm-content').click();
    await page.keyboard.press('ControlOrMeta+a');
    await page.keyboard.insertText(SCHEMA);
    await page.setInputFiles('#open-input', { name: 'p.bin', mimeType: 'application/octet-stream', buffer: data() });
    await expect(page.locator('#status')).toContainText('p.bin');
    await expect(page.locator('.tree-row .pointer')).toHaveText('→ 0x800');
  };
  const selectedName = (page: Page) => page.locator('.tree-row.selected .name').textContent();

  test('the arrow shows the target; clicking it or J jumps there, and Alt+← and Alt+→ walk the places', async ({ page }) => {
    await load(page);
    await page.locator('.tree-row', { hasText: 'ofs' }).first().click();
    await page.keyboard.press('Shift+J');
    await expect(page.locator('#status')).toContainText('→ 0x800: table');
    expect(await selectedName(page)).toBe('table');
    await page.keyboard.press('Alt+ArrowLeft');
    await expect(page.locator('#status')).toContainText('back to 0x0 (ofs)');
    expect(await selectedName(page)).toBe('ofs');
    await page.keyboard.press('Alt+ArrowLeft');
    await expect(page.locator('#status')).toContainText('oldest');
    await page.keyboard.press('Alt+ArrowRight');
    expect(await selectedName(page)).toBe('table');
    await page.keyboard.press('Alt+ArrowRight');
    await expect(page.locator('#status')).toContainText('newest');
    // the arrow is a button too
    await page.keyboard.press('Alt+ArrowLeft');
    await page.locator('.tree-row .pointer').click();
    expect(await selectedName(page)).toBe('table');
    await expect(page.locator('#layout-live')).toContainText('→ 0x800: table');
  });

  test('a field that is not a number says so, and nothing moves', async ({ page }) => {
    await load(page);
    await page.locator('.tree-row', { hasText: 'gap' }).first().click();
    await page.keyboard.press('Shift+J');
    await expect(page.locator('#status')).toContainText('does not point at anything');
    expect(await selectedName(page)).toBe('gap');
  });
});
