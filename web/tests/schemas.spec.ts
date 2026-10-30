// The schemas that are open: the list with where each came from and its errors,
// the imports that are missing, showing and editing a file, read-only files that
// were fetched, and the choice of the type to parse with.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { type Page, expect, test } from '@playwright/test';
import { linkOf, useType } from './fixture.js';
import { type InspectResponse } from '../src/doe.js';
import { isKnownType, listing, parsedAs } from '../src/schemas.js';

const repo = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

async function ready(page: Page, url = '/'): Promise<void> {
  await page.goto(url);
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
}

async function typeText(page: Page, text: string): Promise<void> {
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText(text);
}

const MAIN = 'meta:\n  id: top\n  imports:\n    - extra\nseq:\n  - id: first\n    type: u1\n  - id: second\n    type: extra\n';
const EXTRA = 'meta:\n  id: extra\nseq:\n  - id: b\n    type: u1\ntypes:\n  piece:\n    seq:\n      - id: p\n        type: u2le\n';

const file = (name: string, text: string) => ({ name, mimeType: 'text/plain', buffer: Buffer.from(text) });

/** The entry of the list for a path. */
const entry = (page: Page, path: string) => page.locator('#schemas li', { has: page.locator('code', { hasText: new RegExp(`^${path.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`) }) });

/** The row of the tree for a field. */
const treeRow = (page: Page, name: string) => page.locator('.tree-row', { has: page.locator('.name', { hasText: new RegExp(`^${name}$`) }) });

// ── no browser ───────────────────────────────────────────────────────────────

test('the list has the main schema first, then the imports, and what is added but not named last', () => {
  const found: InspectResponse = {
    ok: true,
    schemas: [
      { path: 'main.ksy', id: 'top', imports: ['a'], types: ['top', 'top::t'], error: null },
      { path: 'a.ksy', id: 'a', imports: [], types: ['a'], error: null },
      { path: 'broken.ksy', id: null, imports: [], types: [], error: { severity: 'error', kind: 'yaml', message: 'bad', position: null } },
    ],
    missing: [{ name: 'gone', from: 'a.ksy', message: 'not found' }],
    diagnostics: [],
  };
  const files = new Map([
    ['a.ksy', ''],
    ['broken.ksy', ''],
    ['unused.ksy', ''],
  ]);
  const origins = new Map([['a.ksy', 'gallery' as const]]);
  const { rows, missing } = listing(found, 'main.ksy', 'file', files, origins, 2);
  expect(rows.map((r) => r.path)).toEqual(['main.ksy', 'a.ksy', 'broken.ksy', 'unused.ksy']);
  expect(rows.map((r) => r.origin)).toEqual(['file', 'gallery', 'added', 'added']);
  expect(rows.map((r) => r.editable)).toEqual([true, false, true, true]);
  expect(rows.map((r) => r.errors)).toEqual([2, 0, 1, 0]);
  expect(missing).toEqual([{ name: 'gone', from: 'a.ksy', message: 'not found' }]);
  expect(isKnownType(rows, 'top::t')).toBe(true);
  expect(isKnownType(rows, 'a')).toBe(true);
  expect(isKnownType(rows, 'nope')).toBe(false);
  // before anything is known: the main schema and the files
  expect(listing(null, 'main.ksy', 'typed', files, origins, 0).rows.map((r) => r.path)).toEqual(['main.ksy', 'a.ksy', 'broken.ksy', 'unused.ksy']);
});

// ── in the page ──────────────────────────────────────────────────────────────

test('the list shows the main schema and the imports, each with where it came from', async ({ page }) => {
  await ready(page);
  await typeText(page, MAIN);
  await page.setInputFiles('#add-files', file('extra.ksy', EXTRA));
  await expect(page.locator('#status')).toContainText('parsed as top', { timeout: 10_000 });
  const rows = page.locator('#schemas li');
  await expect(rows).toHaveCount(2);
  await expect(rows.nth(0)).toHaveClass(/main/);
  await expect(rows.nth(0)).toContainText('main.ksy');
  await expect(rows.nth(0)).toContainText('top');
  await expect(rows.nth(0)).toContainText('main schema · typed here');
  await expect(rows.nth(1)).toContainText('extra.ksy');
  await expect(rows.nth(1)).toContainText('extra');
  await expect(rows.nth(1)).toContainText('added file');
  await expect(page.locator('#schemas .count')).toHaveCount(0);
  // an import is taken out of the list, and the schema then lacks it
  await page.locator('#schemas [data-remove="extra.ksy"]').click();
  await expect(page.locator('#schemas li.missing')).toContainText('extra');
  await expect(page.locator('#schemas li:not(.missing)')).toHaveCount(1);
});

test('an import that is not there is listed with the file that asked for it', async ({ page }) => {
  await ready(page);
  await typeText(page, 'meta:\n  id: top\n  imports:\n    - gone\n    - there\nseq: []\n');
  await page.setInputFiles('#add-files', file('there.ksy', 'meta:\n  id: there\n  imports:\n    - also_gone\nseq: []\n'));
  const missing = page.locator('#schemas li.missing');
  await expect(missing).toHaveCount(2, { timeout: 10_000 });
  await expect(missing.nth(0)).toContainText('gone');
  await expect(missing.nth(0)).toContainText('imported by main.ksy');
  await expect(missing.nth(1)).toContainText('also_gone');
  await expect(missing.nth(1)).toContainText('imported by there.ksy');
  await expect(missing.first().locator('.count')).toContainText('not found');
});

test('a file that cannot be read has a mark, and the problems of the main schema are counted', async ({ page }) => {
  await ready(page);
  await typeText(page, 'meta:\n  id: top\n  imports:\n    - broken\nseq:\n  - id: a\n    type: nope\n');
  await page.setInputFiles('#add-files', file('broken.ksy', 'id: broken\nseq:\n  - id: [\n'));
  await expect(entry(page, 'broken.ksy').locator('.count')).toContainText('✗ 1', { timeout: 10_000 });
  await expect(entry(page, 'main.ksy').locator('.count')).toContainText('✗');
  await expect(entry(page, 'broken.ksy')).toHaveClass(/bad/);
  // showing the broken file underlines its error
  await entry(page, 'broken.ksy').locator('.pick').click();
  await expect(page.locator('.cm-lintRange-error').first()).toBeVisible();
});

test('a file of the list is shown in the editor, can be edited when it was added, and the main schema is kept', async ({ page }) => {
  await ready(page);
  await typeText(page, MAIN);
  await page.setInputFiles('#add-files', file('extra.ksy', EXTRA));
  await page.setInputFiles('#open-input', { name: 'in.bin', mimeType: 'application/octet-stream', buffer: Buffer.from([1, 2, 3, 4]) });
  await expect(page.locator('#status')).toContainText('parsed as top', { timeout: 10_000 });

  await entry(page, 'extra.ksy').locator('.pick').click();
  await expect(page.locator('.cm-content')).toContainText('piece');
  await expect(page.locator('.cm-content')).not.toContainText('second');
  await expect(page.locator('#editing')).toContainText('extra.ksy');
  await expect(entry(page, 'extra.ksy')).toHaveClass(/active/);
  await expect(page.locator('.cm-content')).toHaveAttribute('contenteditable', 'true');
  await expect(page.locator('#edit-note')).toBeHidden();

  // a change to the import is a change to the schema: the field is now a u2le
  await typeText(page, EXTRA.replace('id: b\n    type: u1', 'id: b\n    type: u2le'));
  await expect(page.locator('#status')).toContainText('parsed as top', { timeout: 10_000 });
  await treeRow(page, 'second').locator('.toggle').click();
  await expect(treeRow(page, 'b')).toContainText('770');

  await entry(page, 'main.ksy').locator('.pick').click();
  await expect(page.locator('.cm-content')).toContainText('second');
  await expect(page.locator('#editing')).toHaveText('main.ksy');
  // the import kept its change
  await entry(page, 'extra.ksy').locator('.pick').click();
  await expect(page.locator('.cm-content')).toContainText('u2le');
});

test('a format of the gallery is shown with its imports read-only, until a copy is made', async ({ page }) => {
  await page.route('https://raw.githubusercontent.com/**', async (route) => {
    const [, , , , ...path] = new URL(route.request().url()).pathname.split('/');
    await route.fulfill({ body: readFileSync(join(repo, 'conformance/kaitai_struct_formats', ...path)), contentType: 'text/plain' });
  });
  await ready(page);
  const gallery = (await (await page.request.get('/gallery.json')).json()) as { formats: { path: string }[] };
  await useType(page, 'jpeg', 'image/jpeg.ksy');
  await expect(page.locator('#schemas li:not(.main) code').first()).toHaveText('image/exif.ksy');
  const exif = entry(page, 'image/exif.ksy');
  await expect(exif).toContainText('Kaitai Struct gallery');
  await expect(exif).toContainText('read-only');
  await expect(entry(page, 'image/jpeg.ksy')).toContainText('main schema · Kaitai Struct gallery');

  await exif.locator('.pick').click();
  await expect(page.locator('.cm-content')).toContainText('exif');
  await expect(page.locator('.cm-content')).toHaveAttribute('contenteditable', 'false');
  await expect(page.locator('#edit-note')).toContainText('read-only');
  // the main schema of the gallery can be edited, as any
  await entry(page, 'image/jpeg.ksy').locator('.pick').click();
  await expect(page.locator('.cm-content')).toHaveAttribute('contenteditable', 'true');
  await expect(page.locator('#edit-note')).toBeHidden();

  await exif.locator('.pick').click();
  await page.locator('#edit-note button').click();
  await expect(page.locator('#edit-note')).toBeHidden();
  await expect(page.locator('.cm-content')).toHaveAttribute('contenteditable', 'true');
  await expect(exif).toContainText('added file');
  await expect(exif).not.toContainText('read-only');
});

/** The example with an import, an input, and the type of the import chosen. */
async function withImportedType(page: Page): Promise<void> {
  await ready(page);
  await typeText(page, MAIN);
  await page.setInputFiles('#add-files', file('extra.ksy', EXTRA));
  await page.setInputFiles('#open-input', { name: 'in.bin', mimeType: 'application/octet-stream', buffer: Buffer.from([7, 1, 0, 9]) });
  await expect(page.locator('#status')).toContainText('parsed as top', { timeout: 10_000 });
  await page.selectOption('#parse-type', 'extra::piece');
  await expect(page.locator('#status')).toContainText('parsed as extra::piece', { timeout: 10_000 });
}

test('a type of any open schema can be chosen to parse with', async ({ page }) => {
  await ready(page);
  await typeText(page, MAIN);
  await page.setInputFiles('#add-files', file('extra.ksy', EXTRA));
  await page.setInputFiles('#open-input', { name: 'in.bin', mimeType: 'application/octet-stream', buffer: Buffer.from([7, 1, 0, 9]) });
  await expect(page.locator('#status')).toContainText('parsed as top', { timeout: 10_000 });

  const options = await page.locator('#parse-type option').evaluateAll((items) => items.map((item) => (item as HTMLOptionElement).value));
  expect(options).toEqual(['', 'top', 'extra', 'extra::piece']);
  await expect(page.locator('#parse-type optgroup').nth(1)).toHaveAttribute('label', 'extra.ksy (extra)');

  await page.selectOption('#parse-type', 'extra::piece');
  await expect(page.locator('#status')).toContainText('parsed as extra::piece', { timeout: 10_000 });
  await expect(treeRow(page, 'p')).toContainText('263');
  await expect(page.locator('#parse-type')).toHaveValue('extra::piece');

  // the schema's own again
  await page.selectOption('#parse-type', '');
  await expect(page.locator('#status')).toContainText('parsed as top', { timeout: 10_000 });
});

test('the type that is chosen is saved with the work in the browser', async ({ page }) => {
  await withImportedType(page);
  await page.reload();
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await expect(page.locator('#parse-type')).toHaveValue('extra::piece');
});

test('the type that is chosen is in a link that an earlier version made', async ({ page }) => {
  // nothing is remembered in the browser: only the link has the schema, its import and the type
  const link = linkOf({ schema: MAIN, files: { 'extra.ksy': EXTRA }, path: 'main.ksy', type: 'extra::piece' });
  await ready(page, link);
  await expect(page.locator('#parse-type')).toHaveValue('extra::piece');
  await page.setInputFiles('#open-input', { name: 'in.bin', mimeType: 'application/octet-stream', buffer: Buffer.from([7, 1, 0, 9]) });
  await expect(page.locator('#status')).toContainText('parsed as extra::piece', { timeout: 10_000 });
});

test('a type that is no longer there is dropped', async ({ page }) => {
  await ready(page);
  await typeText(page, MAIN);
  await page.setInputFiles('#add-files', file('extra.ksy', EXTRA));
  await page.setInputFiles('#open-input', { name: 'in.bin', mimeType: 'application/octet-stream', buffer: Buffer.from([7, 1, 0, 9]) });
  await page.selectOption('#parse-type', 'extra::piece');
  await expect(page.locator('#status')).toContainText('parsed as extra::piece', { timeout: 10_000 });
  await page.locator('#schemas [data-remove="extra.ksy"]').click();
  await expect(page.locator('#parse-type')).toHaveValue('', { timeout: 10_000 });
});

test('the id in the title of the Parsed pane is the chosen type, else the main schema\'s, else the last parse\'s, and nothing without a schema', () => {
  const rows = listing(null, 'main.ksy', 'typed', new Map(), new Map(), 0).rows;
  expect(parsedAs(null, rows, 'from_parse', true)).toBe('from_parse');
  const read = [{ ...rows[0]!, id: 'zip' }];
  expect(parsedAs(null, read, 'from_parse', true)).toBe('zip');
  expect(parsedAs('extra::piece', read, 'zip', true)).toBe('extra::piece');
  expect(parsedAs('extra::piece', read, 'zip', false)).toBe('');
  expect(parsedAs(null, [], null, true)).toBe('');
});

test('the title of the Parsed pane says which type the file is parsed as, and follows Parse as', async ({ page }) => {
  await ready(page);
  await expect(page.locator('#parsed-as')).toHaveText('');
  await withImportedType(page);
  // withImportedType chose extra::piece
  await expect(page.locator('#parsed-as')).toHaveText('extra::piece');
  await expect(page.locator('#h-parsed')).toContainText('Parsed');
  await page.selectOption('#parse-type', '');
  await expect(page.locator('#parsed-as')).toHaveText('top');
  await expect(page.locator('#parsed-as')).toHaveAttribute('title', 'the file is parsed as top');
});
