// `process: bzip2 | xz | lzma | zstd` in the page: a field that holds a stream written by the real tool is decoded by the
// WebAssembly module (#406), and a stream that is not of the format is a problem that names the field and the process.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { type Page, expect, test } from '@playwright/test';

const repo = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const sample = (path: string): Buffer => readFileSync(join(repo, 'conformance', 'samples_without_reference', 'archive', path));

const schema = (process: string): string => `meta:
  id: packed
seq:
  - id: body
    size-eos: true
    process: ${process}
    type: str
    encoding: ASCII
`;

async function load(page: Page, text: string, bytes: Buffer, name = 'x.bin'): Promise<void> {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText(text);
  await page.setInputFiles('#open-input', { name, mimeType: 'application/octet-stream', buffer: bytes });
  await expect(page.locator('#status')).toContainText(name);
}

const row = (page: Page, name: string) => page.locator('.tree-row', { has: page.locator(`.name:text-is("${name}")`) }).first();

for (const [process, file] of [
  ['xz', 'xz/xz_text.bin'],
  ['xz', 'xz/xz_two_streams_text.bin'],
  ['bzip2', 'bzip2/python_bz2_text.bin'],
  ['lzma', 'lzma_alone/xz_alone_text.bin'],
  ['zstd', 'zstd/zstd_text.bin'],
  ['zstd', 'zstd/zstd_two_frames_text.bin'],
] as const) {
  test(`a ${process} stream (${file}) written by the real tool is decoded in the page`, async ({ page }) => {
    await load(page, schema(process), sample(file));
    await expect(row(page, 'body').locator('.summary')).toContainText('decoder of everything reads binary files');
    await expect(page.locator('#status')).toContainText('parsed');
  });
}

test('a stream that is not of the format is an error that names the field and the process', async ({ page }) => {
  await load(page, schema('zstd'), Buffer.from('this is not a zstd stream, it is text'));
  await expect(page.locator('#status')).toContainText('the parse failed');
  await expect(page.locator('.problems')).toContainText('body');
  await expect(page.locator('.problems')).toContainText('zstd');
});

test('a bomb stops at the limit of the page and says so', async ({ page }) => {
  await load(page, schema('xz'), sample('xz/xz_bomb.bin'));
  await expect(page.locator('#status')).toContainText('the parse failed', { timeout: 60_000 });
  await expect(page.locator('.problems')).toContainText('limit');
});
