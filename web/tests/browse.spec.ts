// The browser of types: the list and filter without a page, and the dialog in a
// real browser.

import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, test } from '@playwright/test';
import { type Entry, compareEntries, filterEntries, firstLine, galleryEntries, hasSignature, libraryEntries, openEntries, shownPath, sorted } from '../src/browse.js';
import type { GalleryFormat } from '../src/detect.js';
import type { Row } from '../src/schemas.js';
import { systemTheme } from '../src/theme.js';
import { EXAMPLE_SCHEMA, loadExample } from './fixture.js';

/** `#rrggbb` as the browser writes a computed colour. */
function rgb(hex: string): string {
  const n = Number.parseInt(hex.slice(1), 16);
  return `rgb(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255})`;
}

const repo = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const hasGallery = existsSync(join(repo, 'conformance', 'kaitai_struct_formats', 'image', 'gif.ksy'));

function row(path: string, types: string[], extra: Partial<Row> = {}): Row {
  return { path, main: false, origin: 'added', id: null, types, error: null, errors: 0, editable: true, ...extra };
}

const format = (id: string, extra: Partial<GalleryFormat> = {}): GalleryFormat => ({ path: `image/${id}.ksy`, id, title: `The ${id} format`, imports: [], ...extra });

test('the types of the open schemas are listed with the one in use marked and a schema that does not read cannot be chosen', () => {
  const rows = [row('main.ksy', ['main', 'chunk'], { main: true, id: 'main' }), row('lib.ksy', ['piece'], { error: { severity: 'error', kind: 'schema', message: 'bad', position: null } })];
  const own = openEntries(rows, null);
  expect(own.map((e) => [e.id, e.current, e.error])).toEqual([
    ['main', true, null],
    ['chunk', false, null],
    ['piece', false, 'bad'],
  ]);
  const chosen = openEntries(rows, 'chunk');
  expect(chosen.filter((e) => e.current).map((e) => e.id)).toEqual(['chunk']);
});

test('the library and the gallery have their docs, signatures and the format in use', () => {
  const library = libraryEntries([{ id: 'sample', doc: '\n  Sample files\nmore', path: '/types/sample.yaml' }]);
  expect(library[0]?.doc).toBe('Sample files');
  expect(firstLine(null)).toBeNull();
  const gallery = galleryEntries([format('gif', { magic: [{ offset: 0, bytes: [71] }], verified: 'real' }), format('png')], 'image/png.ksy');
  expect(gallery.map((e) => [e.id, hasSignature(e), e.verified, e.current])).toEqual([
    ['gif', true, 'real', false],
    ['png', false, null, true],
  ]);
  // the signature of an open or library type is not known until it has been asked for
  expect(hasSignature(library[0] as Entry)).toBeNull();
});

test('the filter needs every word in the id, the path or the doc, in any case', () => {
  const entries: Entry[] = [...libraryEntries([{ id: 'sample', doc: 'Executable files', path: '/types/exe/sample.yaml' }]), ...galleryEntries([format('gif'), format('png')], null)];
  expect(filterEntries(entries, '').length).toBe(3);
  expect(filterEntries(entries, 'SAMPLE').map((e) => e.id)).toEqual(['sample']);
  expect(filterEntries(entries, 'exe files').map((e) => e.id)).toEqual(['sample']);
  expect(filterEntries(entries, 'image the').map((e) => e.id)).toEqual(['gif', 'png']);
  expect(filterEntries(entries, 'zzz')).toEqual([]);
});

test('the list is one list in alphabetical order by name, then path: case and accents do not matter, numbers count by value', () => {
  const entry = (id: string, source: Entry['source'], path: string): Entry => ({ ...galleryEntries([format('x')], null)[0]!, id, source, path });
  const names = (list: Entry[]) => list.map((e) => `${e.id} ${shownPath(e)}`);
  const shuffled = [
    entry('zip', 'gallery', 'archive/zip.ksy'),
    entry('Bee', 'gallery', 'a/Bee.ksy'),
    entry('apple', 'library', '/home/me/types/apple.yaml'),
    entry('élan', 'gallery', 'x/elan.ksy'),
    entry('t10', 'bundled', 'library/t10.yaml'),
    entry('t2', 'bundled', 'library/t2.yaml'),
    entry('tar', 'open', ''),
    entry('tar', 'bundled', 'library/tar.yaml'),
    entry('Tar', 'open', 'mine.ksy'),
  ];
  expect(names(sorted(shuffled))).toEqual([
    'apple local: types/apple.yaml',
    'Bee a/Bee.ksy',
    'élan x/elan.ksy',
    // 2 before 10, and a number before a letter
    't2 library/t2.yaml',
    't10 library/t10.yaml',
    // the same name: told apart by the path ("library/" before "open: ")
    'tar library/tar.yaml',
    'tar open: (editor)',
    'Tar open: mine.ksy',
    'zip archive/zip.ksy',
  ]);
  // total: the same list from any order
  expect(names(sorted([...shuffled].reverse()))).toEqual(names(sorted(shuffled)));
  expect(compareEntries(shuffled[0]!, shuffled[0]!)).toBe(0);
  // the filter finds a source by its path
  expect(filterEntries(shuffled, 'library/').map((e) => e.id).sort()).toEqual(['t10', 't2', 'tar']);
  expect(filterEntries(shuffled, 'open:').map((e) => e.id).sort()).toEqual(['Tar', 'tar']);
  expect(filterEntries(shuffled, '.ksy').length).toBe(4);
});

test("the types of doe's own library have a library/ path and are told from the others by it", () => {
  const bundled = (id: string): GalleryFormat => ({ path: `library/${id}.yaml`, id, title: `The ${id} type`, imports: [], bundled: true, verified: 'crafted' });
  const entries = [
    ...galleryEntries([format('gif'), bundled('tar'), bundled('wasm')], 'library/tar.yaml'),
    ...libraryEntries([{ id: 'sample', doc: null, path: 'sample.yaml' }]),
  ];
  expect(entries.map((e) => [e.id, e.source])).toEqual([
    ['gif', 'gallery'],
    ['tar', 'bundled'],
    ['wasm', 'bundled'],
    ['sample', 'library'],
  ]);
  expect(entries.filter((e) => shownPath(e).startsWith('library/')).map((e) => e.id)).toEqual(['tar', 'wasm']);
  // the type in use is marked, and the entries are found by the filter like any
  expect(entries.filter((e) => e.current).map((e) => e.id)).toEqual(['tar']);
  expect(filterEntries(entries, 'library wasm').map((e) => e.id)).toEqual(['wasm']);
  expect(entries.find((e) => e.id === 'tar')?.verified).toBe('crafted');
});

test('the dialog lists the open schemas, filters, reads one and parses with a type of them', async ({ page }) => {
  await page.goto('/');
  await page.waitForFunction(() => window.doe?.ready === true);
  await loadExample(page);
  // the list of the open schemas is there once the schema has been inspected
  await expect(page.locator('#schemas li.schema')).toHaveCount(1);
  await page.click('#browse');
  const dialog = page.locator('#browse-dialog');
  await expect(dialog).toBeVisible();
  // the type in use is the schema's own
  await expect(dialog.locator('li.current')).toHaveCount(1);
  await expect(dialog.locator('li.current')).toContainText('example');
  // filter: nothing matches
  await page.fill('#browse-filter', 'no such type anywhere');
  await expect(page.locator('.browse-empty')).toContainText('No type matches the filter');
  await page.fill('#browse-filter', '');
  // read the text of the first type, then go back (Esc goes back, then closes)
  await dialog.locator('li button', { hasText: 'View' }).first().click();
  await expect(page.locator('#browse-reader-text')).toContainText('seq');
  await page.keyboard.press('Escape');
  await expect(page.locator('#browse-reader')).toBeHidden();
  await expect(dialog).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
});

test('the viewer draws a schema with the colours of the editor, keeps its text and follows the theme', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.goto('/');
  await page.waitForFunction(() => window.doe?.ready === true);
  await loadExample(page);
  await expect(page.locator('#schemas li.schema')).toHaveCount(1);
  await page.click('#browse');
  const dialog = page.locator('#browse-dialog');
  await dialog.locator('li', { hasText: 'open: ' }).first().locator('button', { hasText: 'View' }).click();
  const text = page.locator('#browse-reader-text');
  await expect(text).toContainText('seq');
  // the text is the schema, unchanged
  expect(await text.evaluate((pre) => pre.textContent)).toBe(EXAMPLE_SCHEMA);
  // the colours of the theme for keys and strings, as the editor has them
  const colour = (needle: string) =>
    text.evaluate((pre, wanted) => {
      const span = [...pre.querySelectorAll('span')].find((s) => s.textContent === wanted);
      return span ? getComputedStyle(span).color : null;
    }, needle);
  const dark = systemTheme(true).colors;
  expect(await colour('seq')).toBe(rgb(dark.type));
  expect(await colour('"DOE"')).toBe(rgb(dark.string));
  // the editor draws the same key in the same colour
  const editorKey = await page.locator('.cm-content .cm-line', { hasText: /^seq:/ }).locator('span').first().evaluate((s) => getComputedStyle(s).color);
  expect(editorKey).toBe(rgb(dark.type));
  // the spans are made from the text, never HTML: no element but spans
  expect(await text.evaluate((pre) => [...pre.querySelectorAll('*')].every((e) => e.tagName === 'SPAN'))).toBe(true);
  // a change of theme while it is open re-colours it
  await page.emulateMedia({ colorScheme: 'light' });
  const light = systemTheme(false).colors;
  await expect.poll(() => colour('seq')).toBe(rgb(light.type));
  expect(await text.evaluate((pre) => pre.textContent)).toBe(EXAMPLE_SCHEMA);
});

test('the dialog is one list with no group headings, alphabetical, with the source in each path, and the type in use is in view', async ({ page }) => {
  await page.goto('/');
  await page.waitForFunction(() => window.doe?.ready === true);
  await loadExample(page);
  await expect(page.locator('#schemas li.schema')).toHaveCount(1);
  await page.click('#browse');
  const dialog = page.locator('#browse-dialog');
  await expect(dialog.locator('#browse-list ul')).toHaveCount(1);
  await expect(dialog.locator('#browse-list h4')).toHaveCount(0);
  const names = await dialog.locator('#browse-list li strong').allTextContents();
  expect(names.length).toBeGreaterThan(10);
  expect(names).toEqual([...names].sort((a, b) => a.localeCompare(b, 'en', { numeric: true, sensitivity: 'base' })));
  // the open schema's types say "open:", the library's say "library/"
  const paths = await dialog.locator('#browse-list .browse-path').allTextContents();
  expect(paths.some((p) => p.startsWith('open: '))).toBe(true);
  expect(paths.some((p) => p.startsWith('library/'))).toBe(true);
  expect(await dialog.locator('li.current').count()).toBe(1);
  await expect(dialog.locator('li.current')).toBeInViewport();
  // a path in the filter narrows the list to a source; the count is shown once
  await page.fill('#browse-filter', 'library/');
  const library = await dialog.locator('#browse-list .browse-path').allTextContents();
  expect(library.length).toBeGreaterThan(5);
  expect(library.every((p) => p.startsWith('library/'))).toBe(true);
  await expect(page.locator('#browse-count')).toHaveText(`${library.length} of ${names.length}`);
  await page.fill('#browse-filter', 'open:');
  expect((await dialog.locator('#browse-list .browse-path').allTextContents()).every((p) => p.startsWith('open: '))).toBe(true);
});

test("the dialog lists the types of doe's own library with a library/ path", async ({ page }) => {
  await page.goto('/');
  await page.waitForFunction(() => window.doe?.ready === true);
  await page.click('#browse');
  const dialog = page.locator('#browse-dialog');
  await page.fill('#browse-filter', 'tar');
  const item = dialog.locator('li', { hasText: 'library/tar.yaml' });
  await expect(item).toBeVisible();
  await expect(item).toContainText('tar archive');
  // it can be read without leaving the page: it is served with it
  await item.locator('button', { hasText: 'View' }).click();
  await expect(page.locator('#browse-reader-text')).toContainText('id: tar');
});

test('a gallery format is listed with its path and signature, and the dialog fits a phone', async ({ page }) => {
  test.skip(!hasGallery, 'the gallery is not checked out');
  await page.goto('/');
  await page.waitForFunction(() => window.doe?.ready === true);
  await page.click('#browse');
  await page.fill('#browse-filter', 'gif');
  const item = page.locator('#browse-dialog li', { hasText: 'image/gif.ksy' });
  await expect(item).toBeVisible();
  await expect(item).toContainText('signature');
  await expect(page.locator('#browse-count')).toContainText(' of ');
  // it fits a phone
  await page.setViewportSize({ width: 380, height: 700 });
  const box = await page.locator('#browse-dialog').boundingBox();
  expect(box?.width ?? 999).toBeLessThanOrEqual(380);
});

test('the types that start with a signature say so, and "Rank by the opened file" puts the ones that fit first', async ({ page }) => {
  await page.goto('/');
  await page.waitForFunction(() => window.doe?.ready === true);
  await loadExample(page);
  await expect(page.locator('#schemas li.schema')).toHaveCount(1);
  await page.click('#browse');
  const dialog = page.locator('#browse-dialog');
  // the example starts with "DOE": its signature is found out once the dialog is open
  await expect(dialog.locator('li.current')).toContainText('signature');
  // the example file is open, so the types can be ranked by it; the ranking waits for every signature
  const rank = page.locator('#browse-rank');
  await expect(rank).toBeEnabled();
  await rank.check();
  await expect(page.locator('#browse-note')).toContainText('ranked by the opened file', { timeout: 30_000 });
  const first = dialog.locator('#browse-list li').first();
  await expect(first).toContainText('example');
  await expect(first).toContainText('the file starts with its signature');
  await rank.uncheck();
});
