// Files inside files in the page (#406): a .tar.gz, .tar.xz, .tar.bz2, .tar.zst or .cpio.xz that is opened and detected is shown
// as the file system it holds, with a line above the panes that names the layers and switches between them.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { type Page, expect, test } from '@playwright/test';
import { useType } from './fixture.js';
import { payloadName } from '../src/layers.js';

const repo = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const archive = (path: string): Buffer => readFileSync(join(repo, 'conformance', 'samples_without_reference', 'archive', path));
const sample = (name: string): Buffer => readFileSync(join(repo, 'types', 'samples', name));

test('the name of a payload is the name without the extension the outer layer added', () => {
  expect(payloadName('a.tar.gz')).toBe('a.tar');
  expect(payloadName('a.tgz')).toBe('a.tar');
  expect(payloadName('a.txz')).toBe('a.tar');
  expect(payloadName('a.cpio.zst')).toBe('a.cpio');
  expect(payloadName('a')).toBe('a');
  expect(payloadName('.gz')).toBe('.gz');
});

async function open(page: Page, name: string, bytes: Buffer): Promise<void> {
  // the types of doe's own library are served with the page; nothing else is needed
  await page.route('https://raw.githubusercontent.com/**', (route) => route.fulfill({ status: 404, body: 'not found' }));
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await page.setInputFiles('#open-input', { name, mimeType: 'application/octet-stream', buffer: bytes });
  await page.click('#detect');
}

const buttons = (page: Page) => page.locator('#layers button.layer');

for (const [name, bytes, layers, inner, size] of [
  ['sample.tar.gz', sample('sample.tar.gz'), 'gzip_file > tar', 'tar', 10240],
  ['a.tar.xz', archive('xz/xz_tar.bin'), 'xz > tar', 'tar', 10240],
  ['a.tar.bz2', archive('bzip2/python_bz2_tar.bin'), 'bzip2 > tar', 'tar', 10240],
  ['a.tar.zst', archive('zstd/zstd_tar.bin'), 'zstd > tar', 'tar', 10240],
  ['a.cpio.xz', archive('xz/xz_cpio.bin'), 'xz > cpio', 'cpio', 1024],
  ['a.cpio.zst', archive('zstd/zstd_cpio.bin'), 'zstd > cpio', 'cpio', 1024],
  ['a.cpio.bz2', archive('bzip2/python_bz2_cpio.bin'), 'bzip2 > cpio', 'cpio', 1024],
] as const) {
  test(`${name} is shown as the ${inner} it holds, with its layers named`, async ({ page }) => {
    await open(page, name, bytes);
    await expect(buttons(page)).toHaveCount(2, { timeout: 30_000 });
    await expect(page.locator('#parsed-as')).toHaveText(inner);
    await expect(page.locator('#status')).toContainText(`(layer 2 of 2: ${layers})`);
    await expect(page.locator('#layers')).toContainText(name);
    // the innermost layer is the one shown, with its own bytes
    await expect(buttons(page).nth(1)).toHaveAttribute('aria-current', 'true');
    // the file system of the layer is the Files view
    await expect(page.locator('.file-row').first()).toBeVisible();
    await expect(page.locator('#status')).toContainText(`${size} bytes`);
  });
}

test('the layer switcher shows the file itself and back, with the bytes and the schema of each', async ({ page }) => {
  const outer = sample('sample.tar.gz');
  await open(page, 'sample.tar.gz', outer);
  await expect(buttons(page)).toHaveCount(2, { timeout: 30_000 });
  await expect(page.locator('#parsed-as')).toHaveText('tar');
  await buttons(page).nth(0).click();
  await expect(page.locator('#parsed-as')).toHaveText('gzip_file');
  await expect(buttons(page).nth(0)).toHaveAttribute('aria-current', 'true');
  await expect(page.locator('#status')).toContainText(`sample.tar.gz, ${outer.length} bytes (layer 1 of 2`);
  await expect(page.locator('.cm-content')).toContainText('id: gzip_file');
  await buttons(page).nth(1).click();
  await expect(page.locator('#parsed-as')).toHaveText('tar');
  await expect(page.locator('#status')).toContainText('(layer 2 of 2');
  await expect(page.locator('.cm-content')).toContainText('id: tar');
  // a new file forgets the layers
  await page.setInputFiles('#open-input', { name: 'plain.bin', mimeType: 'application/octet-stream', buffer: Buffer.from('hello') });
  await expect(page.locator('#layers')).toBeHidden();
});

test('a payload that cannot be decoded or fits nothing leaves the layer that was found, with a note', async ({ page }) => {
  // a gzip file of text: the text fits no schema
  await open(page, 'sample.txt.gz', sample('sample.txt.gz'));
  await expect(page.locator('#parsed-as')).toHaveText('gzip_file', { timeout: 30_000 });
  await expect(page.locator('#status')).toContainText('fits no schema');
  await expect(page.locator('#layers')).toBeHidden();
  // a gzip file whose CRC is wrong
  const bad = Buffer.from(sample('sample.tar.gz'));
  bad[bad.length - 8] = (bad[bad.length - 8] ?? 0) ^ 0xff;
  await page.setInputFiles('#open-input', { name: 'bad.tar.gz', mimeType: 'application/octet-stream', buffer: bad });
  await page.click('#detect');
  await expect(page.locator('#status')).toContainText('could not unwrap gzip_file (gzip)', { timeout: 30_000 });
  await expect(page.locator('#parsed-as')).toHaveText('gzip_file');
  await expect(page.locator('#layers')).toBeHidden();
});

test('a type chosen in the dialog for a file that holds a file shows what it holds', async ({ page }) => {
  await page.route('https://raw.githubusercontent.com/**', (route) => route.fulfill({ status: 404, body: 'not found' }));
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await page.setInputFiles('#open-input', { name: 'sample.tar.gz', mimeType: 'application/octet-stream', buffer: sample('sample.tar.gz') });
  await useType(page, 'gzip_file', 'library/gzip_file.yaml');
  await expect(buttons(page)).toHaveCount(2, { timeout: 30_000 });
  await expect(page.locator('#parsed-as')).toHaveText('tar');
});
