// The browser of types: the list and filter without a page, and the dialog in a
// real browser.

import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, test } from '@playwright/test';
import { type Entry, filterEntries, firstLine, galleryEntries, grouped, hasSignature, libraryEntries, openEntries } from '../src/browse.js';
import type { GalleryFormat } from '../src/detect.js';
import type { Row } from '../src/schemas.js';

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
  const library = libraryEntries([{ id: 'elf', doc: '\n  ELF files\nmore', path: '/types/elf.yaml' }]);
  expect(library[0]?.doc).toBe('ELF files');
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
  const entries: Entry[] = [...libraryEntries([{ id: 'elf', doc: 'Executable files', path: '/types/exe/elf.yaml' }]), ...galleryEntries([format('gif'), format('png')], null)];
  expect(filterEntries(entries, '').length).toBe(3);
  expect(filterEntries(entries, 'ELF').map((e) => e.id)).toEqual(['elf']);
  expect(filterEntries(entries, 'exe files').map((e) => e.id)).toEqual(['elf']);
  expect(filterEntries(entries, 'image the').map((e) => e.id)).toEqual(['gif', 'png']);
  expect(filterEntries(entries, 'zzz')).toEqual([]);
});

test('the list is grouped by where the types come from, the open schemas first', () => {
  const entries = [...galleryEntries([format('gif')], null), ...openEntries([row('m.ksy', ['m'], { main: true, id: 'm' })], null), ...libraryEntries([{ id: 'elf', doc: null, path: 'elf.yaml' }])];
  expect(grouped(entries).map((g) => g.source)).toEqual(['open', 'library', 'gallery']);
});

test('the dialog lists the open schemas, filters, reads one and parses with a type of them', async ({ page }) => {
  await page.goto('/');
  await page.waitForFunction(() => window.doe?.ready === true);
  // the list of the open schemas is there once the schema has been inspected
  await expect(page.locator('#schemas li.schema')).toHaveCount(1);
  await page.click('#browse');
  const dialog = page.locator('#browse-dialog');
  await expect(dialog).toBeVisible();
  await expect(dialog.locator('h4').first()).toContainText('Open schemas');
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
  await expect(dialog.locator('h4').first()).toContainText('Open schemas');
});
