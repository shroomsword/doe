// The keys of the page.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, test, type Page } from '@playwright/test';
import { KEYS, byteQuery, findPattern, keyName } from '../src/keys.js';
import { findNode } from '../src/model.js';
import type { TraceNode } from '../src/doe.js';
import { loadExample } from './fixture.js';

const readme = readFileSync(join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'README.md'), 'utf8');

const node = (name: string, summary: string | null, children: TraceNode[] = []): TraceNode => ({
  name,
  type: null,
  start: null,
  end: null,
  instance: false,
  failed: false,
  processed: false,
  summary,
  integer: null,
  children,
});

test('every key of the table is documented in the README, once, and the table has no key twice for different things', () => {
  const section = readme.slice(readme.indexOf('### Keys'), readme.indexOf('## CLI reference'));
  for (const entry of KEYS) expect(section, entry.key).toContain(`| \`${entry.key}\` |`);
  const seen = new Map<string, string>();
  for (const entry of KEYS) {
    const before = seen.get(entry.key);
    expect(before === undefined || before === entry.action, `${entry.key} is two things`).toBe(true);
    seen.set(entry.key, entry.action);
  }
});

test('key names, queries of the bytes and the search of the tree', () => {
  expect(keyName({ key: 'o', shiftKey: false })).toBe('o');
  expect(keyName({ key: '?', shiftKey: true })).toBe('?');
  expect(keyName({ key: 'Tab', shiftKey: true })).toBe('Shift+Tab');
  expect(byteQuery('')).toBeNull();
  expect(byteQuery('@1f4')).toEqual({ kind: 'offset', offset: 0x1f4 });
  expect(byteQuery('@0x10')).toEqual({ kind: 'offset', offset: 16 });
  expect(byteQuery('@#500')).toEqual({ kind: 'offset', offset: 500 });
  expect(byteQuery('@zz')).toBeNull();
  expect(byteQuery('de ad BE ef')).toEqual({ kind: 'bytes', pattern: [0xde, 0xad, 0xbe, 0xef] });
  expect(byteQuery('0xdead')).toEqual({ kind: 'bytes', pattern: [0xde, 0xad] });
  expect(byteQuery('hello')).toEqual({ kind: 'bytes', pattern: [104, 101, 108, 108, 111] });
  const bytes = { length: 6, byteAt: (i: number) => [1, 2, 3, 1, 2, undefined][i] };
  expect(findPattern(bytes, [1, 2], -1, 1)).toBe(0);
  expect(findPattern(bytes, [1, 2], 0, 1)).toBe(3);
  expect(findPattern(bytes, [1, 2], 3, 1)).toBe(0);
  expect(findPattern(bytes, [1, 2], 0, -1)).toBe(3);
  expect(findPattern(bytes, [9], 0, 1)).toBeNull();
  const root = node('root', null, [node('a', '1'), node('b', '"hello"', [node('c', '2'), node('hello', null)]), node('d', '"hello again"')]);
  expect(findNode(root, 'hello', null, 1)).toMatchObject({ path: [1], count: 3 });
  expect(findNode(root, 'hello', [1], 1)?.path).toEqual([1, 1]);
  expect(findNode(root, 'hello', [1, 1], 1)?.path).toEqual([2]);
  expect(findNode(root, 'hello', [2], 1)?.path).toEqual([1]);
  expect(findNode(root, 'hello', [1], -1)?.path).toEqual([2]);
  expect(findNode(root, 'nothing', null, 1)).toBeNull();
});

test.describe('in the page', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  });

  /** The pane that has the focus. */
  const focused = (page: Page) => page.evaluate(() => document.activeElement?.closest('section')?.id ?? document.activeElement?.tagName ?? '');

  test('? opens the help and k the list of the keys, which is made from the table', async ({ page }) => {
    await page.keyboard.press('Shift+?');
    await expect(page.locator('#help-dialog')).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.locator('#help-dialog')).toBeHidden();
    await page.keyboard.press('k');
    await expect(page.locator('#keys-dialog')).toBeVisible();
    const keys = await page.locator('#keys-table th kbd').allTextContents();
    expect(keys).toEqual(KEYS.map((entry) => entry.key));
    // without a file, the keys that need one say so
    await expect(page.locator('#keys-table tr', { hasText: 'Detect the schema' })).toHaveClass(/unavailable/);
    await expect(page.locator('#keys-table tr', { hasText: 'Open a file to parse' })).not.toHaveClass(/unavailable/);
    await page.keyboard.press('Escape');
    await expect(page.locator('#keys-dialog')).toBeHidden();
  });

  test('the keys that open things ask for the file chooser or open the dialog', async ({ page }) => {
    const chooser = async (key: string, selector: string): Promise<void> => {
      const waiting = page.waitForEvent('filechooser');
      await page.keyboard.press(key);
      const asked = await waiting;
      expect(await asked.element().evaluate((element, wanted) => (element as Element).matches(wanted), selector)).toBe(true);
    };
    await chooser('o', '#open-input');
    await chooser('Shift+O', '#open-schema');
    await chooser('i', '#add-files');
    await page.keyboard.press('t');
    await expect(page.locator('#browse-dialog')).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.locator('#browse-dialog')).toBeHidden();
  });

  test('d and s act when there is a file and a value, and do nothing without', async ({ page }) => {
    await page.keyboard.press('s');
    await expect(page.locator('#save-dialog')).toBeHidden();
    await page.keyboard.press('d');
    await expect(page.locator('#detect-report')).toBeHidden();
    await loadExample(page);
    await page.locator('#pane-tree').focus();
    await expect(page.locator('#save')).toBeEnabled();
    await page.keyboard.press('s');
    await expect(page.locator('#save-dialog')).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.locator('#save-dialog')).toBeHidden();
    await page.keyboard.press('d');
    await expect(page.locator('#detect-report')).toBeVisible({ timeout: 30_000 });
  });

  test('typing the letters in the editor, a box or a select types and nothing else, and Ctrl and Alt are not taken', async ({ page }) => {
    await page.locator('.cm-content').click();
    await page.keyboard.type('o?kdsit/gar');
    await expect(page.locator('.cm-content')).toContainText('o?kdsit/gar');
    await expect(page.locator('dialog[open]')).toHaveCount(0);
    await page.locator('#browse').click();
    await page.locator('#browse-filter').fill('');
    await page.locator('#browse-filter').pressSequentially('kdo?');
    await expect(page.locator('#browse-filter')).toHaveValue('kdo?');
    await expect(page.locator('#keys-dialog')).toBeHidden();
    await page.locator('#browse-close').click();
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Alt+t');
    await expect(page.locator('dialog[open]')).toHaveCount(0);
    // a letter while a dialog is open does nothing to the page
    await page.keyboard.press('k');
    await expect(page.locator('#keys-dialog')).toBeVisible();
    await page.keyboard.press('t');
    await expect(page.locator('#browse-dialog')).toBeHidden();
  });

  test('Tab and Shift+Tab visit every pane and come back; F6 does from anywhere; Tab does not trap the editor', async ({ page }) => {
    await page.locator('#pane-tree').focus();
    const order = ['pane-tree'];
    for (let i = 0; i < 3; i += 1) {
      await page.keyboard.press('Tab');
      order.push(await focused(page));
    }
    // the fourth is the editor, where Tab is the browser's again
    expect(order).toEqual(['pane-tree', 'pane-hex', 'pane-status', 'pane-editor']);
    await page.locator('#pane-hex').focus();
    await page.keyboard.press('Shift+Tab');
    expect(await focused(page)).toBe('pane-tree');
    await page.keyboard.press('Shift+Tab');
    expect(await focused(page)).toBe('pane-editor');
    // inside the editor Tab is the browser's: it does not stay in the editor
    await page.locator('.cm-content').click();
    await page.keyboard.press('Tab');
    expect(await page.evaluate(() => document.activeElement?.closest('.cm-editor') !== null)).toBe(false);
    // F6 works in the editor
    await page.locator('.cm-content').click();
    await page.keyboard.press('F6');
    expect(await focused(page)).toBe('pane-tree');
    await page.keyboard.press('Shift+F6');
    expect(await focused(page)).toBe('pane-editor');
    // the focused pane shows it
    const bar = await page.locator('#pane-editor .pane-title').evaluate((element) => getComputedStyle(element).boxShadow);
    expect(bar).not.toBe('none');
  });

  test('1 to 4 focus the panes, and Esc leaves the editor for its pane so that the keys work again', async ({ page }) => {
    for (const [key, id] of [['2', 'pane-tree'], ['3', 'pane-hex'], ['4', 'pane-status'], ['1', 'pane-editor']] as const) {
      await page.keyboard.press(key);
      expect(await focused(page), key).toBe(id);
      await page.locator('#pane-tree').focus();
    }
    await page.keyboard.press('1');
    await page.keyboard.press('Escape');
    expect(await page.evaluate(() => document.activeElement?.id)).toBe('pane-editor');
    await page.keyboard.press('Shift+?');
    await expect(page.locator('#help-dialog')).toBeVisible();
  });

  test('a, r and c: automatic detection, the way integers are shown, and cancel', async ({ page }) => {
    await page.locator('#pane-tree').focus();
    const auto = page.locator('#detect-auto');
    const before = await auto.isChecked();
    await page.keyboard.press('a');
    expect(await auto.isChecked()).toBe(!before);
    await page.keyboard.press('a');
    await expect(page.locator('#radix')).toHaveValue('dec');
    await page.keyboard.press('r');
    await expect(page.locator('#radix')).toHaveValue('hex');
    await page.keyboard.press('r');
    await page.keyboard.press('r');
    await page.keyboard.press('r');
    await expect(page.locator('#radix')).toHaveValue('dec');
    // there is no parse to cancel
    await page.keyboard.press('c');
    await expect(page.locator('dialog[open]')).toHaveCount(0);
  });

  test('/ searches the editor with its own panel', async ({ page }) => {
    await page.keyboard.press('1');
    await page.locator('.cm-content').click();
    await page.keyboard.press('Escape');
    await page.keyboard.press('/');
    await expect(page.locator('.cm-search')).toBeVisible();
  });

  test('/ searches the tree, the bytes and the status, and Esc clears it', async ({ page }) => {
    await loadExample(page);
    // the tree: the next node that has the text
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('/');
    const find = page.locator('#pane-tree .pane-search input');
    await expect(find).toBeFocused();
    await find.fill('hello');
    await expect(page.locator('#pane-tree .pane-search-count')).toContainText('match');
    await expect(page.locator('.tree-row.selected')).toContainText('hello');
    await page.keyboard.press('Escape');
    await expect(page.locator('#pane-tree .pane-search')).toBeHidden();
    expect(await focused(page)).toBe('pane-tree');
    // the bytes: text, hexadecimal and an offset
    await page.locator('#pane-hex').focus();
    await page.keyboard.press('/');
    const bytes = page.locator('#pane-hex .pane-search input');
    await bytes.fill('hello');
    await expect(page.locator('#pane-hex .pane-search-count')).toContainText('at ');
    await expect(page.locator('.hex .b.sel').first()).toHaveText('68');
    await bytes.fill('62 79 65');
    await expect(page.locator('.hex .b.sel')).toHaveCount(3);
    await bytes.fill('@3');
    await expect(page.locator('.hex .b.sel').first()).toHaveAttribute('data-i', '3');
    await bytes.fill('zzzz zz');
    await expect(page.locator('#pane-hex .pane-search-count')).toContainText('not found');
    await page.keyboard.press('Escape');
    // g asks for an offset
    await page.locator('#pane-tree').focus();
    await page.keyboard.press('g');
    await expect(bytes).toHaveValue('@');
    await bytes.pressSequentially('5');
    await expect(page.locator('.hex .b.sel').first()).toHaveAttribute('data-i', '5');
    await page.keyboard.press('Escape');
    // the status: a mistake can be searched among the problems
    await page.locator('.cm-content').click();
    await page.keyboard.press('ControlOrMeta+a');
    await page.keyboard.insertText('meta:\n  id: broken\nseq:\n  - id: a\n    type: nope\n  - id: b\n    type: nada\n');
    await expect(page.locator('.problem')).toHaveCount(1, { timeout: 10_000 });
    await page.keyboard.press('Escape');
    await page.keyboard.press('4');
    await page.keyboard.press('/');
    await page.locator('#pane-status .pane-search input').fill('qqqq');
    await expect(page.locator('#pane-status .pane-search-count')).toContainText('0 of 1');
    await expect(page.locator('.problem:not(.search-miss)')).toHaveCount(0);
    await page.locator('#pane-status .pane-search input').fill('nope');
    await expect(page.locator('.problem:not(.search-miss)')).toHaveCount(1);
    await page.keyboard.press('Escape');
    await expect(page.locator('.problem.search-miss')).toHaveCount(0);
  });

  test('the buttons say their keys', async ({ page }) => {
    await expect(page.locator('#help')).toHaveAttribute('aria-keyshortcuts', '?');
    await expect(page.locator('label.button:has(#open-input)')).toHaveAttribute('title', /\(o\)$/);
    await expect(page.locator('#browse')).toHaveAttribute('title', /\(t\)$/);
    await expect(page.locator('#detect')).toHaveAttribute('aria-keyshortcuts', 'd');
    await expect(page.locator('#save')).toHaveAttribute('aria-keyshortcuts', 's');
  });
});
