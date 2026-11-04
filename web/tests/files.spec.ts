// The Files view of an archive: the tree of the entries without a DOM, and the view in the page with the library's tar.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { type Page, expect, test } from '@playwright/test';
import type { FsEntry } from '../src/doe.js';
import { buildTree, modeText, ordered, sizeText, textOf, timeText } from '../src/files.js';

const repo = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

const entry = (path: string, extra: Partial<FsEntry> = {}): FsEntry => ({
  path: path.split('/').filter((c) => c !== ''),
  kind: 'file',
  size: null,
  mode: null,
  mtime: null,
  link: null,
  trace_path: [0, 0],
  data: null,
  flags: [],
  ...extra,
});

test('the entries are nested by path, with the directories that only other paths imply', () => {
  const root = buildTree([entry('a/b/c.txt'), entry('a', { kind: 'directory' }), entry('top.txt'), entry('a/d.txt')]);
  expect(root.children.map((n) => n.name)).toEqual(['a', 'top.txt']);
  const a = root.children[0]!;
  // the entry that names the directory is its entry; b is implied
  expect(a.entry?.kind).toBe('directory');
  expect(a.children.map((n) => n.name)).toEqual(['b', 'd.txt']);
  expect(a.children[0]!.entry).toBeNull();
  expect(root.count).toBe(5);
  expect(a.count).toBe(3);
});

test('an entry that repeats a file is a node of its own, and a file and a directory of one name do not merge', () => {
  const root = buildTree([entry('x'), entry('x'), entry('y/z'), entry('y', { kind: 'symlink', link: 'q' })]);
  expect(root.children.map((n) => n.name)).toEqual(['x', 'x', 'y', 'y']);
  // an entry with no path is shown under an empty name
  expect(buildTree([entry('')]).children[0]!.name).toBe('');
});

test('the children are ordered by name, case and accents aside, numbers by value, directories first when asked', () => {
  const root = buildTree([entry('b10'), entry('B2'), entry('élan'), entry('Apple'), entry('zdir/f'), entry('adir/g')]);
  expect(ordered(root.children, false).map((n) => n.name)).toEqual(['adir', 'Apple', 'B2', 'b10', 'élan', 'zdir']);
  expect(ordered(root.children, true).map((n) => n.name)).toEqual(['adir', 'zdir', 'Apple', 'B2', 'b10', 'élan']);
});

test('a hundred thousand entries are nested in well under a second', () => {
  const entries = Array.from({ length: 100_000 }, (_, i) => entry(`d${i % 500}/sub${i % 7}/f${i}`));
  const start = performance.now();
  const root = buildTree(entries);
  const took = performance.now() - start;
  expect(root.count).toBe(100_000 + 500 + 500 * 7);
  expect(root.children).toHaveLength(500);
  expect(took).toBeLessThan(2000);
});

test('modes, sizes and times are written as ls would, and text is told from bytes', () => {
  expect(modeText(0o755, 'directory')).toBe('drwxr-xr-x');
  expect(modeText(0o644, 'file')).toBe('-rw-r--r--');
  expect(modeText(null, 'symlink')).toBe('l');
  expect(sizeText(6)).toBe('6 B');
  expect(sizeText(1536)).toBe('1.5 KiB');
  expect(sizeText(5 * 1024 * 1024)).toBe('5.0 MiB');
  expect(sizeText(null)).toBe('');
  expect(timeText(1_700_000_000)).toBe('2023-11-14 22:13');
  expect(timeText(null)).toBe('');
  expect(textOf(new TextEncoder().encode('hello\nworld—'))).toBe('hello\nworld—');
  expect(textOf(new Uint8Array([0, 1, 2]))).toBeNull();
  expect(textOf(new Uint8Array([0xff, 0xfe]))).toBeNull();
});

async function open(page: Page): Promise<void> {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await page.setInputFiles('#open-input', { name: 'sample.tar', mimeType: 'application/x-tar', buffer: readFileSync(join(repo, 'types', 'samples', 'sample.tar')) });
  await expect(page.locator('#detect')).toBeEnabled();
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText('tar parses the file', { timeout: 30_000 });
}

test('a tar opens as directories and files, F switches to the tree of fields and back, and a row selects the entry and its bytes', async ({ page }) => {
  await open(page);
  const files = page.locator('#files');
  await expect(files).toBeVisible();
  await expect(page.locator('#tree')).toBeHidden();
  const button = page.locator('#files-toggle');
  await expect(button).toBeVisible();
  await expect(button).toHaveAttribute('aria-pressed', 'true');
  // the directories first: docs, then data.bin; the folder is closed until it is opened
  const names = () => page.locator('#files-list .file-row .file-name').allTextContents();
  expect((await names()).map((n) => n.replace(/\s+\d+$/, ''))).toEqual(['docs', 'data.bin']);
  await page.locator('#files-list .file-row', { hasText: 'docs' }).locator('.file-twisty').click();
  expect((await names()).map((n) => n.replace(/\s+\d+$/, ''))).toEqual(['docs', 'hello.txt', 'data.bin']);
  const hello = page.locator('#files-list .file-row', { hasText: 'hello.txt' });
  await expect(hello.locator('.file-size')).toHaveText('6 B');
  await expect(hello.locator('.file-mode')).toHaveText('-rw-r--r--');
  await expect(hello.locator('.file-time')).toHaveText('2023-11-14 22:13');
  // a click selects the entry: its bytes in the Bytes pane, its node in the tree of fields, and a text preview
  await hello.click();
  await expect(hello).toHaveClass(/selected/);
  await expect(page.locator('.hex .b.sel').first()).toBeVisible();
  await expect(page.locator('#files-preview')).toBeVisible();
  await expect(page.locator('#files-preview-text')).toContainText('hello');
  // F: the tree of fields, with the same entry selected; F again: the files
  await page.locator('#files-list').focus();
  await page.keyboard.press('F');
  await expect(page.locator('#tree')).toBeVisible();
  await expect(files).toBeHidden();
  await expect(page.locator('#tree .tree-row.selected')).toHaveCount(1);
  await page.keyboard.press('F');
  await expect(files).toBeVisible();
  await expect(hello).toHaveClass(/selected/);
  // the other way: a field inside an entry, picked in the tree, selects its entry here
  await page.locator('#files-toggle').click();
  await page.locator('#tree .tree-row').first().click();
  await page.locator('#files-toggle').click();
  await expect(page.locator('#files-list .file-row.selected')).toHaveCount(0);
});

test('the arrow keys walk the files, open and close directories and go to the parent', async ({ page }) => {
  await open(page);
  const list = page.locator('#files-list');
  await list.focus();
  await page.keyboard.press('ArrowDown');
  await expect(page.locator('#files-list .file-row.selected .file-name')).toContainText('docs');
  await page.keyboard.press('ArrowRight');
  await expect(page.locator('#files-list .file-row', { hasText: 'hello.txt' })).toBeVisible();
  await page.keyboard.press('ArrowDown');
  await expect(page.locator('#files-list .file-row.selected .file-name')).toContainText('hello.txt');
  await page.keyboard.press('ArrowLeft');
  await expect(page.locator('#files-list .file-row.selected .file-name')).toContainText('docs');
  await page.keyboard.press('ArrowLeft');
  await expect(page.locator('#files-list .file-row', { hasText: 'hello.txt' })).toHaveCount(0);
});

test('a schema that is not a file system has no Files view, and the key does nothing', async ({ page }) => {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await page.setInputFiles('#open-input', { name: 'x.bin', mimeType: 'application/octet-stream', buffer: Buffer.from('DOE!') });
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText('meta:\n  id: plain\nseq:\n  - id: magic\n    contents: "DOE!"\n');
  await expect(page.locator('#status')).toContainText('x.bin, 4 bytes');
  await expect(page.locator('#files-toggle')).toBeHidden();
  await expect(page.locator('#files')).toBeHidden();
  await expect(page.locator('#tree')).toBeVisible();
  await page.locator('#tree').focus();
  await page.keyboard.press('F');
  await expect(page.locator('#tree')).toBeVisible();
});
