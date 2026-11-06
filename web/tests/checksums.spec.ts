// The checksum column of the Files view: the MD5 of each file, computed in the worker, with the dashes for what has none.

import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { type Page, expect, test } from '@playwright/test';
import { batchesOf, checksumLine } from '../src/files.js';
import { tarOf } from './fixture.js';

const repo = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const sample = (name: string): Buffer => readFileSync(join(repo, 'types', 'samples', name));

async function ready(page: Page): Promise<void> {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
}

async function open(page: Page, name: string, buffer: Buffer, parsedAs: string): Promise<void> {
  // a file on disk when it is large (setInputFiles takes at most 50 MB in memory)
  if (buffer.length > 30 * 1024 * 1024) {
    const path = join(mkdtempSync(join(tmpdir(), 'doe-checksums-')), name);
    writeFileSync(path, buffer);
    await page.setInputFiles('#open-input', path);
  } else await page.setInputFiles('#open-input', { name, mimeType: 'application/octet-stream', buffer });
  await expect(page.locator('#detect')).toBeEnabled();
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText(`${parsedAs} parses the file`, { timeout: 30_000 });
}

const row = (page: Page, name: string) => page.locator('#files-list .file-row', { hasText: name });
const hash = (page: Page, name: string) => row(page, name).locator('.file-hash');

test('a line of md5sum, and the batches the work is cut into', () => {
  expect(checksumLine('d41d8cd98f00b204e9800998ecf8427e', 'a/b.txt')).toBe('d41d8cd98f00b204e9800998ecf8427e  a/b.txt\n');
  // md5sum escapes a name with a backslash or a new line, and marks the line with a backslash
  expect(checksumLine('d41d8cd98f00b204e9800998ecf8427e', 'a\\b')).toBe('\\d41d8cd98f00b204e9800998ecf8427e  a\\\\b\n');
  expect(checksumLine('d41d8cd98f00b204e9800998ecf8427e', 'a\nb')).toBe('\\d41d8cd98f00b204e9800998ecf8427e  a\\nb\n');
  const size = (n: number): number => n;
  expect(batchesOf([1, 2, 3, 4, 5], size, 6, 100)).toEqual([[1, 2, 3], [4], [5]]);
  // an item over the limit is a batch of its own; the count limits too
  expect(batchesOf([10, 1, 1], size, 5, 100)).toEqual([[10], [1, 1]]);
  expect(batchesOf([1, 1, 1], size, 100, 2)).toEqual([[1, 1], [1]]);
  expect(batchesOf([], size)).toEqual([]);
});

test('the button and m show the MD5 of each file of a tar, as md5sum has it, and none for a directory', async ({ page }) => {
  await ready(page);
  await open(page, 'sample.tar', sample('sample.tar'), 'tar');
  const toggle = page.locator('#hash-toggle');
  await expect(toggle).toBeVisible();
  await expect(toggle).toHaveAttribute('aria-pressed', 'false');
  await expect(toggle).toHaveAccessibleName('Checksums');
  await expect(page.locator('.file-hash')).toHaveCount(0);
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('#files-hash-bar')).toBeVisible();
  await expect(page.locator('#files-hash-head')).toHaveText('MD5');
  // docs is a directory; open it
  await row(page, 'docs').locator('.file-twisty').click();
  await expect(hash(page, 'data.bin')).toHaveText('c56bd5480f6e5413cb62a0ad9666613a');
  await expect(hash(page, 'hello.txt')).toHaveText('b1946ac92492d2347c6235b4d2611184');
  await expect(row(page, 'docs').first().locator('.file-hash')).toHaveText('');
  // m turns it off and on again, with the focus in the page
  await page.locator('#files-list').focus();
  await page.keyboard.press('m');
  await expect(page.locator('.file-hash')).toHaveCount(0);
  await expect(page.locator('#files-hash-bar')).toBeHidden();
  await page.keyboard.press('m');
  await expect(hash(page, 'hello.txt')).toHaveText('b1946ac92492d2347c6235b4d2611184');
});

test('the checksum of the selected file is in the preview bar, can be copied, and the list is saved for md5sum -c', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']).catch(() => undefined);
  await ready(page);
  await open(page, 'sample.tar', sample('sample.tar'), 'tar');
  await page.locator('#hash-toggle').click();
  await row(page, 'docs').locator('.file-twisty').click();
  await expect(hash(page, 'hello.txt')).toHaveText('b1946ac92492d2347c6235b4d2611184');
  await row(page, 'hello.txt').click();
  await expect(page.locator('#files-hash-value')).toHaveText('MD5 b1946ac92492d2347c6235b4d2611184');
  await expect(page.locator('#files-hash-copy')).toBeVisible();
  if (test.info().project.name === 'chromium') {
    await page.locator('#files-hash-copy').click();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('b1946ac92492d2347c6235b4d2611184');
  }
  // the list: one line per file (not for the directory), in the order of the archive, as md5sum writes it
  await expect(page.locator('#files-hash-save')).toBeEnabled();
  const [download] = await Promise.all([page.waitForEvent('download'), page.locator('#files-hash-save').click()]);
  expect(download.suggestedFilename()).toBe('md5sums.txt');
  const text = readFileSync((await download.path()) ?? '', 'utf8');
  expect(text).toBe('b1946ac92492d2347c6235b4d2611184  docs/hello.txt\nc56bd5480f6e5413cb62a0ad9666613a  data.bin\n');
});

test('an empty file has the MD5 of nothing, and a zip has it for stored entries and a dash with the reason for deflated ones', async ({ page }) => {
  await ready(page);
  await open(page, 'e.tar', tarOf({ 'empty.txt': '', 'a.txt': 'abc' }), 'tar');
  await page.locator('#hash-toggle').click();
  await expect(hash(page, 'empty.txt')).toHaveText('d41d8cd98f00b204e9800998ecf8427e');
  await expect(hash(page, 'a.txt')).toHaveText('900150983cd24fb0d6963f7d28e17f72');
  await open(page, 'sample.zip', sample('sample.zip'), 'zip_archive');
  await row(page, 'docs').locator('.file-twisty').click();
  await expect(hash(page, 'hello.txt')).toHaveText('b1946ac92492d2347c6235b4d2611184');
  // deflated: no digest of the compressed bytes
  await expect(hash(page, 'data.bin')).toHaveText('—');
  await expect(hash(page, 'data.bin')).toHaveAttribute('title', /compressed/);
  await expect(hash(page, 'stream.txt')).toHaveText('—');
  // and no preview or Save of the compressed stream as if it were the file
  await row(page, 'data.bin').click();
  await expect(page.locator('#files-save')).toBeHidden();
  await expect(page.locator('#files-hash-value')).toBeHidden();
});

test('opening another input drops the checksums of the old one', async ({ page }) => {
  await ready(page);
  await open(page, 'one.tar', tarOf({ 'f.txt': 'abc' }), 'tar');
  await page.locator('#hash-toggle').click();
  await expect(hash(page, 'f.txt')).toHaveText('900150983cd24fb0d6963f7d28e17f72');
  // the same name and size, other contents: the checksum is of the new bytes
  await open(page, 'two.tar', tarOf({ 'f.txt': 'xyz' }), 'tar');
  await expect(hash(page, 'f.txt')).toHaveText('d16fb36f0911f878998c136191af705e');
});

test('an archive of several megabytes keeps the page responsive while the checksums fill in', async ({ page }) => {
  await ready(page);
  const files: Record<string, Uint8Array> = {};
  for (let i = 0; i < 4; i += 1) files[`big${i}.bin`] = new Uint8Array(2 * 1024 * 1024).fill(i + 1);
  await open(page, 'big.tar', tarOf(files), 'tar');
  await page.locator('#hash-toggle').click();
  // the page answers while the worker hashes: a click on a row selects it at once
  await row(page, 'big0.bin').click();
  await expect(row(page, 'big0.bin')).toHaveClass(/selected/);
  await expect(hash(page, 'big3.bin')).toHaveText(/^[0-9a-f]{32}$/, { timeout: 60_000 });
  await expect(page.locator('#files-hash-save')).toBeEnabled();
  // and each one is the MD5 of its bytes (a different one for each fill)
  const all = await page.locator('.file-hash').allTextContents();
  expect(new Set(all).size).toBe(4);
});
