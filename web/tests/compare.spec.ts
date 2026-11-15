// Comparing two files with one schema: the marks, the counts, the rows of what only the other file has, the keys that go
// from one difference to the next, and the two ways to draw it.

import { type Page, expect, test } from '@playwright/test';

const SCHEMA =
  'meta:\n  id: s\n  endian: le\nseq:\n  - id: version\n    type: u1\n  - id: count\n    type: u1\n  - id: items\n    type: item\n    repeat: expr\n    repeat-expr: count\n  - id: tail\n    type: u2\ntypes:\n  item:\n    seq:\n      - id: a\n        type: u1\n      - id: b\n        type: u1\n';

const A = Buffer.from([1, 2, 10, 11, 12, 13, 7, 0]);

test.use({ viewport: { width: 1500, height: 900 } });

/** The page with the schema, the first file, and the second one to compare it with. */
async function compare(page: Page, first: Buffer, second: Buffer): Promise<void> {
  await page.goto('/');
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText(SCHEMA);
  await page.setInputFiles('#open-input', { name: 'a.bin', mimeType: 'application/octet-stream', buffer: first });
  await expect(page.locator('#status')).toContainText('a.bin');
  await page.setInputFiles('#compare-input', { name: 'b.bin', mimeType: 'application/octet-stream', buffer: second });
  await expect(page.locator('#compare-summary')).toBeVisible();
}

const changed = (second: number[]): Buffer => {
  const b = Buffer.from(A);
  second.forEach((value, index) => {
    if (value >= 0) b[index] = value;
  });
  return b;
};

const selectedName = (page: Page): Promise<string | null> => page.locator('.tree-row.selected .name').first().textContent();

test('the tree is marked with what differs and the title says how many', async ({ page }) => {
  await compare(page, A, changed([-1, -1, -1, 99]));
  await expect(page.locator('#compare-summary')).toHaveText('A: 1 of 11 fields differ');
  await expect(page.locator('#status')).toContainText('compared with b.bin');
  // `>` goes to the field that differs, opening the way to it
  await page.locator('.tree').first().focus();
  await page.keyboard.press('Shift+>');
  await expect(page.locator('.tree-row.diff-changed')).toHaveCount(1);
  expect(await selectedName(page)).toBe('b');
  await expect(page.locator('.tree-row.diff-changed .mark')).toHaveText('≠');
  // the containers around it say that something below them differs
  await expect(page.locator('.tree-row.diff-inside').first()).toContainText('1 differ');
  // the field's value in the other file is in the line under the pane
  await expect(page.locator('#selection')).toContainText('B: 99');
  // the byte is underlined in the bytes pane
  await expect(page.locator('.hex .b.chg')).toHaveCount(1);
  await expect(page.locator('.hex .b.chg')).toHaveText('0b');
});

test('identical files say so', async ({ page }) => {
  await compare(page, A, A);
  await expect(page.locator('#compare-summary')).toHaveText('A: identical');
  await expect(page.locator('.hex .b.chg')).toHaveCount(0);
  await page.locator('.tree').first().focus();
  await page.keyboard.press('Shift+>');
  await expect(page.locator('#layout-live')).toHaveText('nothing differs');
});

test('what only the second file has is a row under its parent, and the keys go from difference to difference and round', async ({ page }) => {
  const one = Buffer.from([1, 1, 10, 11, 7, 0]);
  await compare(page, one, A);
  await expect(page.locator('#compare-summary')).toContainText('1 only in B');
  await page.locator('.tree').first().focus();
  // version, count, items (changed: its length), then the ghost after its children
  const seen: string[] = [];
  for (let i = 0; i < 4; i += 1) {
    await page.keyboard.press('Shift+>');
    seen.push((await selectedName(page)) ?? '');
  }
  expect(seen[0]).toBe('count');
  expect(seen[1]).toBe('items');
  expect(seen[2]).toBe('[1]');
  await expect(page.locator('.tree-row.ghost')).toHaveCount(1);
  await expect(page.locator('.tree-row.ghost .mark')).toHaveText('+');
  // and back at the first difference
  await expect(page.locator('#layout-live')).toHaveText('back at the first difference');
  await page.keyboard.press('Shift+<');
  expect(await selectedName(page)).toBe('[1]');
});

test('= leaves out the fields that are the same', async ({ page }) => {
  await compare(page, A, changed([-1, -1, -1, 99]));
  await page.locator('.tree').first().focus();
  await page.keyboard.press('Shift+>');
  const all = await page.locator('.tree-row').count();
  await page.keyboard.press('=');
  const only = await page.locator('.tree-row').count();
  expect(only).toBeLessThan(all);
  await expect(page.locator('.tree-row .name', { hasText: /^tail$/ })).toHaveCount(0);
  await expect(page.locator('.tree-row .name', { hasText: /^b$/ })).toHaveCount(1);
  await page.keyboard.press('=');
  expect(await page.locator('.tree-row').count()).toBe(all);
});

test('V makes the other file the one that is shown, at the same field', async ({ page }) => {
  await compare(page, A, changed([-1, -1, -1, 99]));
  await page.locator('.tree').first().focus();
  await page.keyboard.press('Shift+>');
  await expect(page.locator('.tree-row.selected .summary')).toHaveText('11');
  await page.keyboard.press('Shift+V');
  await expect(page.locator('#compare-summary')).toHaveText('B: 1 of 11 fields differ');
  await expect(page.locator('#status')).toContainText('b.bin');
  await expect(page.locator('.tree-row.selected .summary')).toHaveText('99');
  expect(await selectedName(page)).toBe('b');
  await expect(page.locator('#selection')).toContainText('A: 11');
  await page.keyboard.press('Shift+V');
  await expect(page.locator('#compare-summary')).toHaveText('A: 1 of 11 fields differ');
  await expect(page.locator('.tree-row.selected .summary')).toHaveText('11');
});

test('| puts the two trees side by side under the names of the files, with the same field selected', async ({ page }) => {
  await compare(page, A, changed([-1, -1, -1, 99]));
  await page.locator('.tree').first().focus();
  await page.keyboard.press('Shift+>');
  await expect(page.locator('#tree-other')).toBeHidden();
  await page.keyboard.press('Shift+|');
  await expect(page.locator('#tree-other')).toBeVisible();
  await expect(page.locator('#tree')).toHaveAttribute('data-title', 'A · a.bin');
  await expect(page.locator('#tree-other')).toHaveAttribute('data-title', 'B · b.bin');
  // the same field is selected in the second tree, with the value of the second file
  await expect(page.locator('#tree-other .tree-row.selected .name')).toHaveText('b');
  await expect(page.locator('#tree-other .tree-row.selected .summary')).toHaveText('99');
  await expect(page.locator('#tree .tree-row.selected .summary')).toHaveText('11');
  // a click in the second tree selects the same field in the first
  await page.locator('#tree-other .tree-row', { has: page.locator('.name', { hasText: /^a$/ }) }).first().click();
  expect(await selectedName(page)).toBe('a');
  // the ghosts are not in the first tree: the second has its own rows
  await page.keyboard.press('Shift+|');
  await expect(page.locator('#tree-other')).toBeHidden();
});

test('the buttons: the view, and stopping', async ({ page }) => {
  await compare(page, A, changed([-1, -1, -1, 99]));
  await expect(page.locator('#compare-view')).toBeVisible();
  await page.locator('#compare-view').click();
  await expect(page.locator('#tree-other')).toBeVisible();
  await page.locator('#compare-stop').click();
  await expect(page.locator('#compare-summary')).toBeHidden();
  await expect(page.locator('#tree-other')).toBeHidden();
  await expect(page.locator('.tree-row.diff-changed')).toHaveCount(0);
  await expect(page.locator('.hex .b.chg')).toHaveCount(0);
  // and the keys are not for a page that is not comparing
  await page.locator('.tree').first().focus();
  await page.keyboard.press('Shift+>');
  await expect(page.locator('#layout-live')).not.toHaveText('nothing differs');
});

test('in the bytes pane > goes to the next run of bytes that differ', async ({ page }) => {
  const b = changed([-1, -1, -1, 99, -1, -1, 8]);
  await compare(page, A, b);
  await page.locator('#hex').click({ position: { x: 5, y: 5 } });
  await page.keyboard.press('Shift+>');
  await expect(page.locator('#selection')).toContainText('b');
  await page.keyboard.press('Shift+>');
  await expect(page.locator('#selection')).toContainText('tail');
});

test('the keys of a comparison are listed, and say when they need two files', async ({ page }) => {
  await page.goto('/');
  await page.keyboard.press('Shift+K');
  const row = page.locator('tr', { has: page.locator('kbd', { hasText: /^>$/ }) });
  await expect(row).toContainText('When comparing two files');
  await expect(row).toHaveClass(/unavailable/);
});
