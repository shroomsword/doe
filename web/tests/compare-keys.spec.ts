// Matching the elements of a list by a key field when two files are compared (#444): the schema's `compare-by:`, the choice
// made in the page with `#` or Match by…, what is said when a key cannot match, and what is kept for the next visit.

import { type Page, expect, test } from '@playwright/test';

/** A table of records up to the end of the file: an id, a letter and a value in each. */
const TABLE =
  'meta:\n  id: t\n  endian: le\nseq:\n  - id: records\n    type: record\n    repeat: eos\n__KEY__types:\n  record:\n    seq:\n      - id: id\n        type: u1\n      - id: letter\n        type: u1\n      - id: value\n        type: u1\n';

const A = Buffer.from([1, 97, 10, 2, 98, 20, 3, 99, 30]);
/** The same with a record at the front. */
const B = Buffer.from([9, 122, 90, 1, 97, 10, 2, 98, 20, 3, 99, 30]);

test.use({ viewport: { width: 1500, height: 900 } });

const schema = (key = ''): string => TABLE.replace('__KEY__', key ? `    compare-by: ${key}\n` : '');

async function compare(page: Page, text: string, first = A, second = B): Promise<void> {
  await page.goto('/');
  // the page is ready when the worker has answered (a second visit restores what the first left, which must settle first)
  await expect(page.locator('body')).toHaveAttribute('data-help', 'ready');
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText(text);
  await page.setInputFiles('#open-input', { name: 'a.bin', mimeType: 'application/octet-stream', buffer: first });
  await expect(page.locator('#status')).toContainText(`a.bin, ${first.length} bytes`);
  await page.setInputFiles('#compare-input', { name: 'b.bin', mimeType: 'application/octet-stream', buffer: second });
  await expect(page.locator('#compare-summary')).toBeVisible();
}

const records = (page: Page) => page.locator('.tree-row', { has: page.locator('.name', { hasText: /^records$/ }) });

async function choose(page: Page, field: string): Promise<void> {
  await records(page).first().click();
  await page.keyboard.press('#');
  await expect(page.locator('#match-dialog')).toBeVisible();
  await page.locator('#match-field').fill(field);
  await page.locator('#match-field').press('Enter');
  await expect(page.locator('#match-dialog')).toBeHidden();
}

test('by index an inserted record shifts every later one, and # matches the list by a field so that it is one difference', async ({ page }) => {
  await compare(page, schema());
  await expect(page.locator('#compare-summary')).not.toContainText('matched by key');
  const before = await page.locator('#compare-summary').textContent();
  expect(before).toMatch(/[4-9] of \d+ fields differ|\d\d+ of/);
  await choose(page, 'id');
  await expect(page.locator('#compare-summary')).toContainText('matched by key: records by id');
  await expect(page.locator('#compare-summary')).toHaveText(/^A: 0 of /);
  await expect(page.locator('#compare-summary')).toContainText('1 only in B');
  // the record that only B has is a row under the list
  await records(page).first().click();
  await page.keyboard.press('ArrowRight');
  await expect(page.locator('.tree-row.ghost')).toHaveCount(1);
});

test('the dialog says which fields could match, and nothing typed goes back to the index', async ({ page }) => {
  await compare(page, schema('id'));
  // the schema's key is used without being asked
  await expect(page.locator('#compare-summary')).toContainText('matched by key: records by id');
  await records(page).first().click();
  await page.keyboard.press('#');
  await expect(page.locator('#match-field')).toHaveValue('id');
  await expect(page.locator('#match-where')).toContainText('id, letter, value');
  await page.locator('#match-index').click();
  await expect(page.locator('#match-dialog')).toBeHidden();
  await expect(page.locator('#compare-summary')).not.toContainText('matched by key');
  await expect(page.locator('#compare-summary')).not.toHaveText(/^A: 0 of /);
});

test('a field that cannot match is said, and the list is matched by index', async ({ page }) => {
  await compare(page, schema());
  await choose(page, 'nothing');
  await expect(page.locator('#status')).toContainText('no field nothing');
  await expect(page.locator('#status')).toContainText('by index');
  await expect(page.locator('#compare-summary')).not.toContainText('matched by key');
});

test('the choice is kept for the schema in the browser', async ({ page }) => {
  await compare(page, schema());
  await choose(page, 'id');
  await expect(page.locator('#compare-summary')).toContainText('records by id');
  // the page again, the same schema and files: the list is matched at once
  await compare(page, schema());
  await expect(page.locator('#compare-summary')).toContainText('records by id');
});

test('Match by… is only there while comparing, and # without a list selected says so', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('#compare-match')).toBeHidden();
  await compare(page, schema());
  await expect(page.locator('#compare-match')).toBeVisible();
  // nothing selected: the list is not known
  await page.locator('.tree').first().focus();
  await page.locator('#compare-match').click();
  await expect(page.locator('#match-dialog')).toBeHidden();
  await expect(page.locator('#layout-live')).toContainText('Select a list');
  // with a record selected, the button opens the dialog for its list
  await records(page).first().click();
  await page.locator('#compare-match').click();
  await expect(page.locator('#match-title')).toHaveText('Match the elements of records');
  await page.locator('#match-cancel').click();
  await expect(page.locator('#match-dialog')).toBeHidden();
});

test('the key is in the table of keys and says that it needs two files', async ({ page }) => {
  await page.goto('/');
  await page.keyboard.press('Shift+K');
  const row = page.locator('tr', { has: page.locator('kbd', { hasText: /^#$/ }) });
  await expect(row).toContainText('When comparing two files');
  await expect(row).toHaveClass(/unavailable/);
});
