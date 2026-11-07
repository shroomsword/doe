// The checksum column of the Files view: the MD5 of each file, computed in the worker, with the dashes for what has none.

import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { type Page, expect, test } from '@playwright/test';
import { ALGORITHMS, batchesOf, checksumLine, cycleAlgorithm, shownDigest } from '../src/files.js';
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

/** Choose a checksum in the menu of the hash button ('None' hides the column). */
async function choose(page: Page, label: string): Promise<void> {
  await page.locator('#hash-toggle').click();
  await expect(page.locator('#hash-menu')).toBeVisible();
  await page.locator('#hash-menu button', { hasText: new RegExp(`^${label}$`) }).click();
  await expect(page.locator('#hash-menu')).toBeHidden();
}

const row = (page: Page, name: string) => page.locator('#files-list .file-row', { hasText: name });
const hash = (page: Page, name: string) => row(page, name).locator('.file-hash');

test('the algorithms go round: none, MD5, SHA-1, SHA-256, none, and back', () => {
  expect(ALGORITHMS.map((a) => [a.id, a.label, a.digits, a.weak])).toEqual([
    ['md5', 'MD5', 32, true],
    ['sha1', 'SHA-1', 40, true],
    ['sha256', 'SHA-256', 64, false],
  ]);
  expect(cycleAlgorithm(null, 1)).toBe('md5');
  expect(cycleAlgorithm('md5', 1)).toBe('sha1');
  expect(cycleAlgorithm('sha1', 1)).toBe('sha256');
  expect(cycleAlgorithm('sha256', 1)).toBeNull();
  expect(cycleAlgorithm(null, -1)).toBe('sha256');
  expect(cycleAlgorithm('sha256', -1)).toBe('sha1');
  expect(cycleAlgorithm('sha1', -1)).toBe('md5');
  expect(cycleAlgorithm('md5', -1)).toBeNull();
  // a long digest is cut to its first 16 digits in a narrow pane, a short one never
  const long = '5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03';
  expect(shownDigest(long, true)).toBe('5891b5b522d5df08…');
  expect(shownDigest(long, false)).toBe(long);
  expect(shownDigest('b1946ac92492d2347c6235b4d2611184', true)).toBe('b1946ac92492d2347c6235b4d2611184');
});

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
  await choose(page, 'MD5');
  await expect(toggle).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('#files-hash-bar')).toBeVisible();
  await expect(page.locator('#files-hash-head')).toHaveText('MD5');
  // docs is a directory; open it
  await row(page, 'docs').locator('.file-twisty').click();
  await expect(hash(page, 'data.bin')).toHaveText('c56bd5480f6e5413cb62a0ad9666613a');
  await expect(hash(page, 'hello.txt')).toHaveText('b1946ac92492d2347c6235b4d2611184');
  await expect(row(page, 'docs').first().locator('.file-hash')).toHaveText('');
  // m goes on to SHA-1 and then off, and on again, with the focus in the page
  await page.locator('#files-list').focus();
  await page.keyboard.press('m');
  await expect(page.locator('#files-hash-head')).toHaveText('SHA-1');
  await page.keyboard.press('m');
  await expect(page.locator('#files-hash-head')).toHaveText('SHA-256');
  await page.keyboard.press('m');
  await expect(page.locator('.file-hash')).toHaveCount(0);
  await expect(page.locator('#files-hash-bar')).toBeHidden();
  await expect(toggle).toHaveAttribute('aria-pressed', 'false');
  await page.keyboard.press('m');
  await expect(hash(page, 'hello.txt')).toHaveText('b1946ac92492d2347c6235b4d2611184');
});

test('the checksum of the selected file is in the preview bar, can be copied, and the list is saved for md5sum -c', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']).catch(() => undefined);
  await ready(page);
  await open(page, 'sample.tar', sample('sample.tar'), 'tar');
  await choose(page, 'MD5');
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
  expect(download.suggestedFilename()).toBe('checksums.md5');
  const text = readFileSync((await download.path()) ?? '', 'utf8');
  expect(text).toBe('b1946ac92492d2347c6235b4d2611184  docs/hello.txt\nc56bd5480f6e5413cb62a0ad9666613a  data.bin\n');
});

test('an empty file has the MD5 of nothing, and a zip has it for stored entries and a dash with the reason for deflated ones', async ({ page }) => {
  await ready(page);
  await open(page, 'e.tar', tarOf({ 'empty.txt': '', 'a.txt': 'abc' }), 'tar');
  await choose(page, 'MD5');
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
  await choose(page, 'MD5');
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
  await choose(page, 'MD5');
  // the page answers while the worker hashes: a click on a row selects it at once
  await row(page, 'big0.bin').click();
  await expect(row(page, 'big0.bin')).toHaveClass(/selected/);
  await expect(hash(page, 'big3.bin')).toHaveText(/^[0-9a-f]{32}$/, { timeout: 60_000 });
  await expect(page.locator('#files-hash-save')).toBeEnabled();
  // and each one is the MD5 of its bytes (a different one for each fill)
  const all = await page.locator('.file-hash').allTextContents();
  expect(new Set(all).size).toBe(4);
});

test('the menu lists MD5 and SHA-1 with the one in use marked, and SHA-1 has 40 digits, a header, a copy and a list for sha1sum', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']).catch(() => undefined);
  await ready(page);
  await open(page, 'sample.tar', sample('sample.tar'), 'tar');
  await page.locator('#hash-toggle').click();
  const items = page.locator('#hash-menu button');
  await expect(items).toHaveText(['MD5', 'SHA-1', 'SHA-256', 'None']);
  await expect(items.nth(3)).toHaveAttribute('aria-checked', 'true');
  await expect(page.locator('#hash-toggle')).toHaveAttribute('aria-expanded', 'true');
  // the keys of the menu: up twice from None, Enter
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('Enter');
  await expect(page.locator('#hash-menu')).toBeHidden();
  await expect(page.locator('#files-hash-head')).toHaveText('SHA-1');
  await row(page, 'docs').locator('.file-twisty').click();
  await expect(hash(page, 'hello.txt')).toHaveText('f572d396fae9206628714fb2ce00f72e94f2258f');
  await expect(hash(page, 'data.bin')).toHaveText('494179714a6cd627239dfededf2de9ef994caf03');
  await expect(page.locator('#files-hash-head')).toHaveAttribute('title', /SHA-1.*not a security measure/);
  await row(page, 'hello.txt').click();
  await expect(page.locator('#files-hash-value')).toHaveText('SHA-1 f572d396fae9206628714fb2ce00f72e94f2258f');
  if (test.info().project.name === 'chromium') {
    await page.locator('#files-hash-copy').click();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('f572d396fae9206628714fb2ce00f72e94f2258f');
  }
  const [download] = await Promise.all([page.waitForEvent('download'), page.locator('#files-hash-save').click()]);
  expect(download.suggestedFilename()).toBe('checksums.sha1');
  expect(readFileSync((await download.path()) ?? '', 'utf8')).toBe(
    'f572d396fae9206628714fb2ce00f72e94f2258f  docs/hello.txt\n494179714a6cd627239dfededf2de9ef994caf03  data.bin\n',
  );
  // Escape closes the menu and the focus goes back to the button
  await page.locator('#hash-toggle').click();
  await page.keyboard.press('Escape');
  await expect(page.locator('#hash-menu')).toBeHidden();
  await expect(page.locator('#hash-toggle')).toBeFocused();
});

test('m goes MD5, SHA-1, SHA-256, none and M goes back; the digests of the other algorithm are kept; the choice is remembered', async ({ page }) => {
  await ready(page);
  await open(page, 'sample.tar', sample('sample.tar'), 'tar');
  await page.locator('#files-list').focus();
  const head = page.locator('#files-hash-head');
  await page.keyboard.press('m');
  await expect(head).toHaveText('MD5');
  await row(page, 'docs').locator('.file-twisty').click();
  await expect(hash(page, 'hello.txt')).toHaveText('b1946ac92492d2347c6235b4d2611184');
  await page.locator('#files-list').focus();
  await page.keyboard.press('m');
  await expect(head).toHaveText('SHA-1');
  await expect(hash(page, 'hello.txt')).toHaveText('f572d396fae9206628714fb2ce00f72e94f2258f');
  // back to MD5 with Shift+M: its digests are there at once, never the dots of a pending one
  await page.keyboard.press('Shift+M');
  await expect(head).toHaveText('MD5');
  await expect(hash(page, 'hello.txt')).toHaveText('b1946ac92492d2347c6235b4d2611184');
  await page.keyboard.press('Shift+M');
  await expect(page.locator('.file-hash')).toHaveCount(0);
  await expect(page.locator('#files-hash-bar')).toBeHidden();
  // from none, M goes to SHA-256 and then to SHA-1
  await page.keyboard.press('Shift+M');
  await expect(head).toHaveText('SHA-256');
  await page.keyboard.press('Shift+M');
  await expect(head).toHaveText('SHA-1');
  // the choice is remembered for the next page
  await page.reload();
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await open(page, 'sample.tar', sample('sample.tar'), 'tar');
  await expect(page.locator('#files-hash-head')).toHaveText('SHA-1');
  await row(page, 'docs').locator('.file-twisty').click();
  await expect(hash(page, 'hello.txt')).toHaveText('f572d396fae9206628714fb2ce00f72e94f2258f');
  // None forgets it
  await choose(page, 'None');
  await page.reload();
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await open(page, 'sample.tar', sample('sample.tar'), 'tar');
  await expect(page.locator('.file-hash')).toHaveCount(0);
});

test('SHA-256 has 64 digits, is not called weak, is cut to 16 in a narrow pane with the whole in the tooltip and the bar, and is saved for sha256sum', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']).catch(() => undefined);
  await page.setViewportSize({ width: 2600, height: 900 });
  await ready(page);
  await open(page, 'sample.tar', sample('sample.tar'), 'tar');
  await choose(page, 'SHA-256');
  await expect(page.locator('#files-hash-head')).toHaveText('SHA-256');
  await expect(page.locator('#files-hash-head')).not.toHaveAttribute('title', /not a security measure/);
  await row(page, 'docs').locator('.file-twisty').click();
  // the Parsed pane has the whole window for the row
  await page.locator('#files-list').focus();
  await page.keyboard.press('f');
  const whole = '5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03';
  await expect(hash(page, 'hello.txt')).toHaveText(whole);
  await expect(hash(page, 'data.bin')).toHaveText('1f825aa2f0020ef7cf91dfa30da4668d791c5d4824fc8e41354b89ec05795ab3');
  // a narrow window: the first 16 digits and an ellipsis, the whole in the tooltip
  await page.setViewportSize({ width: 560, height: 900 });
  await expect(hash(page, 'hello.txt')).toHaveText('5891b5b522d5df08…');
  await expect(hash(page, 'hello.txt')).toHaveAttribute('title', `SHA-256 ${whole}`);
  // the preview bar has the whole digest, and Copy copies it
  await row(page, 'hello.txt').click();
  await expect(page.locator('#files-hash-value')).toHaveText(`SHA-256 ${whole}`);
  if (test.info().project.name === 'chromium') {
    await page.locator('#files-hash-copy').click();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(whole);
  }
  // wide again: the whole digest
  await page.setViewportSize({ width: 2600, height: 900 });
  await expect(hash(page, 'hello.txt')).toHaveText(whole);
  const [download] = await Promise.all([page.waitForEvent('download'), page.locator('#files-hash-save').click()]);
  expect(download.suggestedFilename()).toBe('checksums.sha256');
  expect(readFileSync((await download.path()) ?? '', 'utf8')).toBe(
    `${whole}  docs/hello.txt\n1f825aa2f0020ef7cf91dfa30da4668d791c5d4824fc8e41354b89ec05795ab3  data.bin\n`,
  );
});

test('S copies the list of the shown algorithm in the format of the checksum tool, and says when there is none', async ({ page, context }) => {
  test.skip(test.info().project.name !== 'chromium', 'the clipboard needs a permission that only Chromium has here');
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await ready(page);
  await open(page, 'sample.tar', sample('sample.tar'), 'tar');
  await page.locator('#files-list').focus();
  await page.keyboard.press('Shift+S');
  await expect(page.locator('#status')).toContainText('no checksums are shown');
  await page.keyboard.press('m');
  await page.keyboard.press('m');
  await page.keyboard.press('m');
  await expect(page.locator('#files-hash-head')).toHaveText('SHA-256');
  await expect(page.locator('#files-hash-save')).toBeEnabled();
  await page.keyboard.press('Shift+S');
  await expect(page.locator('#status')).toContainText('copied the SHA-256 checksums of 2 files');
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(
    '5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03  docs/hello.txt\n1f825aa2f0020ef7cf91dfa30da4668d791c5d4824fc8e41354b89ec05795ab3  data.bin\n',
  );
});

test('the list says how many files it leaves out: those with no checksum, when it is saved and when it is copied', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']).catch(() => undefined);
  await ready(page);
  // a zip: one stored entry has its MD5, the two deflated ones have none
  await open(page, 'sample.zip', sample('sample.zip'), 'zip_archive');
  await choose(page, 'MD5');
  await row(page, 'docs').locator('.file-twisty').click();
  await expect(hash(page, 'hello.txt')).toHaveText('b1946ac92492d2347c6235b4d2611184');
  await expect(page.locator('#files-hash-save')).toBeEnabled();
  const [download] = await Promise.all([page.waitForEvent('download'), page.locator('#files-hash-save').click()]);
  expect(readFileSync((await download.path()) ?? '', 'utf8')).toBe('b1946ac92492d2347c6235b4d2611184  docs/hello.txt\n');
  await expect(page.locator('#status')).toHaveText('saved checksums.md5 with 1 file; 2 files are not in it (no checksum, or a path that cannot be placed safely)');
  if (test.info().project.name === 'chromium') {
    await page.locator('#files-list').focus();
    await page.keyboard.press('Shift+S');
    await expect(page.locator('#status')).toHaveText('copied the MD5 checksums of 1 file; 2 files are not in it (no checksum, or a path that cannot be placed safely)');
  }
  // a tar that has every checksum says nothing about files left out
  await open(page, 'sample.tar', sample('sample.tar'), 'tar');
  await expect(page.locator('#files-hash-save')).toBeEnabled();
  await Promise.all([page.waitForEvent('download'), page.locator('#files-hash-save').click()]);
  await expect(page.locator('#status')).toHaveText('saved checksums.md5 with 2 files');
});

test('a clipboard that refuses the write says so and points to Save, and does not leave the promise unhandled', async ({ page }) => {
  await ready(page);
  await open(page, 'sample.tar', sample('sample.tar'), 'tar');
  await page.evaluate(() => {
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: () => Promise.reject(new Error('denied')) }, configurable: true });
  });
  await choose(page, 'MD5');
  await expect(page.locator('#files-hash-save')).toBeEnabled();
  await page.locator('#files-list').focus();
  await page.keyboard.press('Shift+S');
  await expect(page.locator('#status')).toContainText('did not let the page write to the clipboard');
});
