// The description (doc:) of a field next to the field in the Parsed pane: the row, the whole text, the toggle, the search.

import { type Page, expect, test } from '@playwright/test';
import type { Docs, TraceNode } from '../src/doe.js';
import { docOf, findNode, firstLine, nodeMatches } from '../src/model.js';

const SCHEMA = `meta:
  id: d
  endian: le
doc: The description of the whole type, which a field does not repeat.
seq:
  - id: count
    type: u1
    doc: |
      How many entries follow.

      A second paragraph with the word zebra that the row does not show.
  - id: entries
    type: entry
    repeat: expr
    repeat-expr: count
    doc: The entries.
  - id: plain
    type: u1
  - id: markup
    type: u1
    doc: "<b>bold</b> & <img src=x onerror=alert(1)>"
instances:
  twice:
    value: count * 2
    doc: Twice the count.
types:
  entry:
    doc: One entry.
    seq:
      - id: kind
        type: u1
        doc: What the entry is.
`;

const node = (name: string, type: string | null, children: TraceNode[] = [], extra: Partial<TraceNode> = {}): TraceNode => ({
  name,
  type,
  start: null,
  end: null,
  instance: false,
  failed: false,
  problem: null,
  processed: false,
  summary: null,
  integer: null,
  children,
  ...extra,
});

test('a description is looked up by the type of the parent and the name; the first line is the row', () => {
  const docs: Docs = { d: { count: 'How many.\n\nMore.' }, 'd::entry': { kind: 'What it is.' } };
  expect(docOf(docs, 'd', 'count')).toBe('How many.\n\nMore.');
  expect(docOf(docs, 'd', 'plain')).toBeNull();
  expect(docOf(docs, 'd::entry', 'kind')).toBe('What it is.');
  expect(docOf(docs, null, 'count')).toBeNull();
  expect(docOf(undefined, 'd', 'count')).toBeNull();
  expect(firstLine('How many.\n\nMore.')).toBe('How many.');
  expect(firstLine('  one  ')).toBe('one');
  expect(firstLine('')).toBe('');
  // the search sees a word of the description, and only when it is given
  const plain = node('count', null, [], { summary: '2' });
  expect(nodeMatches(plain, 'zebra')).toBe(false);
  expect(nodeMatches(plain, 'ZEBRA', 'a zebra here')).toBe(true);
  const root = node('d', 'd', [node('count', null, [], { summary: '2' }), node('other', null)]);
  expect(findNode(root, 'many', null, 1, docs)?.path).toEqual([0]);
  expect(findNode(root, 'many', null, 1)).toBeNull();
});

async function load(page: Page, schema = SCHEMA, bytes: number[] = [2, 10, 11, 12, 13]): Promise<void> {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText(schema);
  await page.setInputFiles('#open-input', { name: 'd.bin', mimeType: 'application/octet-stream', buffer: Buffer.from(bytes) });
  await expect(page.locator('#status')).toContainText('d.bin');
  await expect(page.locator('.tree-row').first()).toBeVisible();
}

const row = (page: Page, name: string) => page.locator('.tree-row', { has: page.locator(`.name:text-is("${name}")`) }).first();

test('a field with a description shows its first line after the value, and the whole text is the tooltip', async ({ page }) => {
  await load(page);
  const count = row(page, 'count');
  await expect(count.locator('.doc')).toHaveText('How many entries follow.');
  await expect(count.locator('.doc')).toHaveAttribute('title', /second paragraph with the word zebra/);
  // the instance, the repeated field once, a field without one
  await expect(row(page, 'twice').locator('.doc')).toHaveText('Twice the count.');
  await expect(row(page, 'entries').locator('.doc')).toHaveText('The entries.');
  await expect(row(page, 'plain').locator('.doc')).toHaveCount(0);
  // the description of the type is not on the row of the root, and an element has none
  await expect(row(page, 'd').locator('.doc')).toHaveCount(0);
  await row(page, 'entries').locator('.toggle').click();
  await expect(row(page, '[0]').locator('.doc')).toHaveCount(0);
  // a field of the type of the element has the description of that type's field
  await row(page, '[0]').locator('.toggle').click();
  await expect(row(page, 'kind').first().locator('.doc')).toHaveText('What the entry is.');
  // the description comes after the value and before the range
  const order = await count.evaluate((el) => [...el.children].map((c) => c.className));
  expect(order.indexOf('summary number')).toBeLessThan(order.indexOf('doc'));
  expect(order.indexOf('doc')).toBeLessThan(order.indexOf('range'));
});

test('the description is text: markup in it is shown as written and nothing is made of it', async ({ page }) => {
  await load(page);
  const doc = row(page, 'markup').locator('.doc');
  await expect(doc).toHaveText('<b>bold</b> & <img src=x onerror=alert(1)>');
  expect(await row(page, 'markup').locator('b, img').count()).toBe(0);
  await row(page, 'markup').click();
  await expect(page.locator('#field-doc')).toHaveText('<b>bold</b> & <img src=x onerror=alert(1)>');
  expect(await page.locator('#field-doc b, #field-doc img').count()).toBe(0);
});

test('the whole description of the selected field is under the tree, and a field without one shows nothing there', async ({ page }) => {
  await load(page);
  await expect(page.locator('#field-doc')).toBeHidden();
  await row(page, 'count').click();
  await expect(page.locator('#field-doc')).toBeVisible();
  await expect(page.locator('#field-doc')).toContainText('How many entries follow.');
  await expect(page.locator('#field-doc')).toContainText('A second paragraph with the word zebra');
  await row(page, 'plain').click();
  await expect(page.locator('#field-doc')).toBeHidden();
});

test('a long description is cut and never makes the row wrap or the pane scroll sideways', async ({ page }) => {
  const long = `${'word '.repeat(300)}end`;
  await load(page, SCHEMA.replace('doc: The entries.', `doc: "${long}"`));
  const entries = row(page, 'entries');
  await expect(entries.locator('.doc')).toBeVisible();
  const box = await page.locator('#tree-box').evaluate((el) => ({ scroll: el.scrollWidth, client: el.clientWidth }));
  expect(box.scroll).toBeLessThanOrEqual(box.client + 1);
  // the row is one line high, the same as a row without a description
  const heights = await Promise.all([entries, row(page, 'plain')].map((r) => r.evaluate((el) => el.getBoundingClientRect().height)));
  expect(Math.abs((heights[0] ?? 0) - (heights[1] ?? 0))).toBeLessThan(2);
  await page.setViewportSize({ width: 420, height: 800 });
  const narrow = await page.locator('#tree-box').evaluate((el) => ({ scroll: el.scrollWidth, client: el.clientWidth }));
  expect(narrow.scroll).toBeLessThanOrEqual(narrow.client + 1);
});

test('the button and D hide and show the descriptions, and the choice is kept', async ({ page }) => {
  await load(page);
  const button = page.locator('#docs-toggle');
  await expect(button).toHaveAttribute('aria-pressed', 'true');
  await row(page, 'count').click();
  await expect(page.locator('#field-doc')).toBeVisible();
  await button.click();
  await expect(button).toHaveAttribute('aria-pressed', 'false');
  await expect(page.locator('.tree-row .doc')).toHaveCount(0);
  await expect(page.locator('#field-doc')).toBeHidden();
  // the key, with the focus in the tree
  await page.locator('.tree').focus();
  await page.keyboard.press('Shift+D');
  await expect(button).toHaveAttribute('aria-pressed', 'true');
  await expect(row(page, 'count').locator('.doc')).toBeVisible();
  await page.keyboard.press('Shift+D');
  await expect(page.locator('.tree-row .doc')).toHaveCount(0);
  // off is kept
  await page.reload();
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await expect(page.locator('#docs-toggle')).toHaveAttribute('aria-pressed', 'false');
  await page.locator('#docs-toggle').click();
  await expect(page.locator('#docs-toggle')).toHaveAttribute('aria-pressed', 'true');
});

test('the search of the Parsed pane finds a word of a description while the descriptions are shown', async ({ page }) => {
  await load(page);
  await page.locator('#pane-tree').focus();
  await page.keyboard.press('/');
  const find = page.locator('#pane-tree .pane-search input');
  await find.fill('zebra');
  await expect(page.locator('#pane-tree .pane-search-count')).toContainText('1 match');
  await expect(page.locator('.tree-row.selected .name')).toHaveText('count');
  // hidden descriptions are not searched: the match would have no reason on the screen
  await page.keyboard.press('Escape');
  await page.locator('#docs-toggle').click();
  await page.locator('#pane-tree').focus();
  await page.keyboard.press('/');
  await page.locator('#pane-tree .pane-search input').fill('zebra');
  await expect(page.locator('#pane-tree .pane-search-count')).toContainText('no match');
});

test('a schema with no descriptions looks as it did', async ({ page }) => {
  await load(page, 'meta:\n  id: n\nseq:\n  - id: a\n    type: u1\n', [1]);
  await expect(row(page, 'a')).toBeVisible();
  await expect(page.locator('.tree-row .doc')).toHaveCount(0);
  await row(page, 'a').click();
  await expect(page.locator('#field-doc')).toBeHidden();
});
