// Copy as…: a field as its path, value, JSON, bytes, place or schema line. The formats are tested without a browser on
// examples that the terminal viewer's tests (`doe::copy`) have too, then in the page.

import { type Page, expect, test } from '@playwright/test';
import { COPY_LIMIT, cArray, cName, copyText, hexContinuous, hexSpaced, pathText, placeText, resolvePath } from '../src/copyas.js';
import type { TraceNode } from '../src/doe.js';
import { loadExample } from './fixture.js';

function node(name: string, extra: Partial<TraceNode> = {}, children: TraceNode[] = []): TraceNode {
  return { name, type: null, start: null, end: null, instance: false, failed: false, problem: null, processed: false, summary: null, integer: null, children, ...extra };
}

const sample = node('file', {}, [
  node('magic'),
  node('entries', {}, [node('[0]', {}, [node('len')]), node('[1]', {}, [node('len'), node('odd name')]), node('[2]', {}, [node('len')])]),
]);

test('a path is written as an expression, and every path finds its node again', () => {
  expect(pathText(sample, [])).toBe('$');
  expect(pathText(sample, [0])).toBe('magic');
  expect(pathText(sample, [1])).toBe('entries');
  expect(pathText(sample, [1, 2, 0])).toBe('entries[2].len');
  expect(pathText(sample, [1, 1, 1])).toBe('entries[1]["odd name"]');
  const walk = (path: number[], at: TraceNode): void => {
    expect(resolvePath(sample, pathText(sample, path)), pathText(sample, path)).toEqual(path);
    at.children.forEach((child, index) => walk([...path, index], child));
  };
  walk([], sample);
  for (const bad of ['', '.magic', 'magic.', 'entries[', 'entries[x]', 'entries[9]', 'nothing', 'entries.len', 'a b', 'entries[1]odd']) {
    expect(resolvePath(sample, bad), JSON.stringify(bad)).toBeNull();
  }
  expect(resolvePath(sample, ' entries[0].len ')).toEqual([1, 0, 0]);
});

test('bytes are hex and a C array, and a place is an offset and a length', () => {
  expect(hexSpaced(new Uint8Array([1, 2, 255]))).toBe('01 02 ff');
  expect(hexContinuous(new Uint8Array([1, 2, 255]))).toBe('0102ff');
  expect(cArray('entries[2].len', new Uint8Array([1, 2]))).toBe('unsigned char entries_2_len[] = { 0x01, 0x02 };');
  expect(cArray('$', new Uint8Array())).toBe('unsigned char data[] = { };');
  expect(cName('[0].x')).toBe('_0_x');
  expect(cName('entries[1]["odd name"]')).toBe('entries_1_odd_name');
  const at = (start: number, end: number, extra: Partial<TraceNode> = {}): TraceNode => node('len', { start, end, ...extra });
  expect(placeText(node('len'))).toBeNull();
  expect(placeText(at(0x1f4 * 8, 0x1f6 * 8))).toBe('0x1f4+2');
  expect(placeText(at(0x1f4 * 8 + 3, 0x1f4 * 8 + 7))).toBe('0x1f4 bits 3..7');
  expect(placeText(at(0x1f4 * 8 + 3, 0x1f4 * 8 + 7, { processed: true }))).toBe('0x1f4 bits 3..7 in the decoded bytes');
});

test('copyText refuses what it cannot make, with the reason, and never cuts', () => {
  const tree = node('file', {}, [node('a', { start: 0, end: 16, summary: '513', integer: { value: '513', name: null } }), node('b', { start: 16, end: 19 }), node('c', { start: 0, end: 8, processed: true }), node('d')]);
  const context = (path: number[], bytes: Uint8Array | null = new Uint8Array([1, 2, 3])) => ({ root: tree, path, value: { a: 513 }, bytes, shown: null });
  expect(copyText('x', context([0]))).toEqual({ text: '01 02' });
  expect(copyText('c', context([0]))).toEqual({ text: 'unsigned char a[] = { 0x01, 0x02 };' });
  expect(copyText('s', context([0]))).toEqual({ text: 'a: u2' });
  expect(copyText('j', context([0]))).toEqual({ text: '513' });
  expect(copyText('x', context([0], null))).toHaveProperty('error');
  expect(copyText('x', context([2]))).toHaveProperty('error');
  expect(copyText('x', context([3]))).toHaveProperty('error');
  expect(copyText('o', context([3]))).toHaveProperty('error');
  expect(copyText('v', context([3]))).toHaveProperty('error');
  expect(copyText('q', context([0]))).toHaveProperty('error');
  const huge = node('file', {}, [node('big', { start: 0, end: (COPY_LIMIT + 1) * 8 })]);
  expect(copyText('X', { root: huge, path: [0], value: null, bytes: new Uint8Array(COPY_LIMIT + 1), shown: null })).toHaveProperty('error');
});

test.describe('in the page', () => {
  test.use({ viewport: { width: 1400, height: 800 } });

  /** Record what the page writes to the clipboard, or refuse it. */
  const clipboard = (page: Page, allowed: boolean) =>
    page.addInitScript((allow) => {
      const w = window as unknown as { __copied: string[] };
      w.__copied = [];
      Object.defineProperty(navigator, 'clipboard', {
        configurable: true,
        value: {
          writeText: (text: string) => (allow ? Promise.resolve(void w.__copied.push(text)) : Promise.reject(new Error('refused'))),
        },
      });
    }, allowed);
  const copied = (page: Page) => page.evaluate(() => (window as unknown as { __copied: string[] }).__copied);

  /** Select `entries[1].len` of the example. */
  const selectLen = async (page: Page): Promise<void> => {
    await page.locator('.tree-row', { hasText: 'entries' }).first().click();
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('ArrowDown');
  };

  test('the button and Y ask how, a letter copies, and the status and the live region say so', async ({ page }) => {
    await clipboard(page, true);
    await page.goto('/');
    await expect(page.locator('#copy-as-button')).toBeDisabled();
    await loadExample(page);
    await expect(page.locator('#copy-as-button')).toBeEnabled();
    // nothing selected: Y says to select a field
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('Shift+Y');
    await expect(page.locator('#status')).toContainText('select a field');
    await selectLen(page);
    await page.keyboard.press('Shift+Y');
    await expect(page.locator('#copy-dialog')).toBeVisible();
    await expect(page.locator('#copy-name')).toHaveText('entries[1].len');
    await page.keyboard.press('p');
    await expect(page.locator('#copy-dialog')).toBeHidden();
    await expect(page.locator('#status')).toContainText('copied path of len');
    await expect(page.locator('#layout-live')).toContainText('copied path of len');
    const formats: Record<string, string> = {
      v: '3',
      j: '3',
      x: '03',
      X: '03',
      c: 'unsigned char entries_1_len[] = { 0x03 };',
      o: '0xc+1',
      s: 'len: u1 in entry',
    };
    for (const [letter, text] of Object.entries(formats)) {
      await page.locator('#copy-as-button').click();
      await page.locator(`#copy-menu [data-copy="${letter}"]`).click();
      await expect(page.locator('#copy-dialog')).toBeHidden();
      expect((await copied(page)).at(-1), letter).toBe(text);
    }
    expect((await copied(page))[0]).toBe('entries[1].len');
  });

  test('Esc and a click outside cancel; a refused clipboard shows the text selected to copy by hand', async ({ page }) => {
    await clipboard(page, false);
    await page.goto('/');
    await loadExample(page);
    await selectLen(page);
    await page.keyboard.press('Shift+Y');
    await page.keyboard.press('Escape');
    await expect(page.locator('#copy-dialog')).toBeHidden();
    await page.keyboard.press('Shift+Y');
    await page.locator('#copy-menu [data-copy="s"]').click();
    await expect(page.locator('#copy-dialog')).toBeVisible();
    await expect(page.locator('#copy-note')).toContainText('did not let the page write');
    await expect(page.locator('#copy-text')).toHaveValue('len: u1 in entry');
    await page.keyboard.press('Escape');
    await expect(page.locator('#copy-dialog')).toBeHidden();
  });
});
