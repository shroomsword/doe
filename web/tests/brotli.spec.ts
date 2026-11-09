// `process: brotli` in the page: a field that holds a Brotli stream is decoded by the WebAssembly module, and the library's
// `brotli` and `woff2` types read their payload and tables through it.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { type Page, expect, test } from '@playwright/test';

const repo = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const sample = (path: string): Buffer => readFileSync(join(repo, path));

const SCHEMA = `meta:
  id: packed
seq:
  - id: body
    size-eos: true
    process: brotli
    type: str
    encoding: ASCII
`;

async function load(page: Page, schema: string, bytes: Buffer, name = 'x.bin'): Promise<void> {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText(schema);
  await page.setInputFiles('#open-input', { name, mimeType: 'application/octet-stream', buffer: bytes });
  await expect(page.locator('#status')).toContainText(name);
}

const row = (page: Page, name: string) => page.locator('.tree-row', { has: page.locator(`.name:text-is("${name}")`) }).first();

test('a field that holds a real Brotli stream is decoded in the page', async ({ page }) => {
  await load(page, SCHEMA, sample('conformance/samples_without_reference/archive/brotli/cramjam_text_q11.bin'));
  await expect(row(page, 'body').locator('.summary')).toContainText('decoder of everything reads binary files');
  await expect(page.locator('#status')).toContainText('parsed');
});

test('a stream that is not Brotli is an error that names the field and the process', async ({ page }) => {
  await load(page, SCHEMA, Buffer.from('this is not a brotli stream, it is text'));
  await expect(page.locator('#status')).toContainText('the parse failed');
  await expect(page.locator('.problems')).toContainText('body');
  await expect(page.locator('.problems')).toContainText('brotli');
});

test('a woff2 font has its table directory and tables, read through the stream', async ({ page }) => {
  await load(page, readFileSync(join(repo, 'types', 'woff2.yaml'), 'utf8'), sample('conformance/samples_without_reference/font/woff2/fonttools_square.bin'), 'font.woff2');
  await expect(page.locator('#status')).toContainText('font.woff2');
  await row(page, 'font_data').locator('.toggle').click();
  await row(page, 'tables').locator('.toggle').click();
  await expect(page.locator('.tree-row .name:text-is("[9]")').last()).toBeVisible();
  await expect(row(page, 'table_directory').locator('.summary, .type, .range').first()).toBeVisible();
});
