// Share links, the work that the browser remembers, the gallery picker and export.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, test } from '@playwright/test';
import { EXAMPLE_BYTES, EXAMPLE_SCHEMA } from '../src/example.js';

const repo = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const doe = join(repo, 'target', 'debug', 'doe');

const SCHEMA = 'meta:\n  id: shared_thing\nseq:\n  - id: first_field\n    type: u1\n';

async function ready(page: import('@playwright/test').Page, url = '/'): Promise<void> {
  await page.goto(url);
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
}

async function typeSchema(page: import('@playwright/test').Page, text: string): Promise<void> {
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText(text);
}

test('a share link opens the same schema, with its imports, in a fresh browser', async ({ page, browser }) => {
  await ready(page);
  await page.setInputFiles('#add-files', { name: 'extra.ksy', mimeType: 'text/plain', buffer: Buffer.from('meta:\n  id: extra\nseq:\n  - id: x\n    type: u1\n') });
  await typeSchema(page, SCHEMA);
  await expect(page.locator('#status')).toContainText('parsed as shared_thing', { timeout: 10_000 });
  await page.locator('#share').click();
  await expect(page.locator('#status')).toContainText(/link (copied|is in the address bar)/);
  const link = page.url();
  expect(link).toContain('#s=');
  // the link holds the schema and imports, and no bytes
  expect(link.length).toBeLessThan(2000);

  const other = await (await browser.newContext()).newPage();
  await ready(other, link);
  await expect(other.locator('.cm-content')).toContainText('shared_thing');
  await expect(other.locator('#files code')).toHaveText('extra.ksy');
  // no file comes with it
  await expect(other.locator('#status')).toContainText('no file opened');
  await other.context().close();
});

test('a link that holds no schema is said to be wrong, and the page still works', async ({ page }) => {
  await ready(page, '/#s=AAAA');
  await expect(page.locator('.problems')).toContainText('does not hold a schema');
  await expect(page.locator('.cm-content')).toContainText('example');
});

test('reloading the page brings back the schema, and says which file it was used with', async ({ page }) => {
  await ready(page);
  await page.setInputFiles('#open-input', { name: 'last.bin', mimeType: 'application/octet-stream', buffer: Buffer.from([7]) });
  await typeSchema(page, SCHEMA);
  await expect(page.locator('#status')).toContainText('parsed as shared_thing', { timeout: 10_000 });
  await page.reload();
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await expect(page.locator('.cm-content')).toContainText('shared_thing');
  await expect(page.locator('#status')).toContainText('open last.bin again');
  // forgetting means the next reload shows the example
  await page.locator('#forget').click();
  await page.reload();
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await expect(page.locator('.cm-content')).toContainText('A small example');
});

test('a format of the gallery is fetched at the pinned commit, and parses a file that is added', async ({ page }) => {
  const wanted: string[] = [];
  await page.route('https://raw.githubusercontent.com/**', async (route) => {
    const url = new URL(route.request().url());
    wanted.push(url.pathname);
    const [, owner, name, commit, ...path] = url.pathname.split('/');
    expect(`${owner}/${name}`).toBe('kaitai-io/kaitai_struct_formats');
    expect(commit).toMatch(/^[0-9a-f]{40}$/);
    await route.fulfill({ body: readFileSync(join(repo, 'conformance/kaitai_struct_formats', ...path)), contentType: 'text/plain' });
  });
  await ready(page);
  const gallery = (await (await page.request.get('/gallery.json')).json()) as { commit: string; formats: { path: string; id: string }[] };
  expect(gallery.formats.length).toBeGreaterThan(100);
  await page.locator('#gallery').selectOption(String(gallery.formats.findIndex((format) => format.path === 'image/jpeg.ksy')));
  await expect(page.locator('.cm-content')).toContainText('jpeg');
  // the definition came from the commit of the submodule, with what it imports
  await expect(page.locator('#files code')).toHaveText('image/exif.ksy');
  expect(wanted).toEqual(expect.arrayContaining([`/kaitai-io/kaitai_struct_formats/${gallery.commit}/image/jpeg.ksy`, `/kaitai-io/kaitai_struct_formats/${gallery.commit}/image/exif.ksy`]));
  const sample = readFileSync(join(repo, 'conformance/samples/gallery/image/jpeg/baseline.bin'));
  await page.setInputFiles('#open-input', { name: 'baseline.jpg', mimeType: 'image/jpeg', buffer: sample });
  await expect(page.locator('#status')).toContainText('parsed as jpeg', { timeout: 20_000 });
});

test('export gives what the command line prints', async ({ page }) => {
  test.skip(!existsSync(doe), 'target/debug/doe is not built');
  const dir = mkdtempSync(join(tmpdir(), 'doe-export-'));
  writeFileSync(join(dir, 'example.yaml'), EXAMPLE_SCHEMA);
  writeFileSync(join(dir, 'example.bin'), EXAMPLE_BYTES);
  await ready(page);
  await expect(page.locator('#export')).toBeEnabled();
  for (const [format, flags] of [['text', []], ['pretty', ['--pretty']], ['json', ['--json']]] as const) {
    await page.locator('#export-format').selectOption(format);
    const [download] = await Promise.all([page.waitForEvent('download'), page.locator('#export').click()]);
    expect(download.suggestedFilename()).toBe(format === 'text' ? 'example.bin.txt' : 'example.bin.json');
    const saved = readFileSync((await download.path()) ?? '', 'utf8');
    const cli = execFileSync(doe, [join(dir, 'example.bin'), join(dir, 'example.yaml'), ...flags]).toString();
    expect(saved, format).toBe(cli);
  }
});

test('export is not offered when there is nothing parsed', async ({ page }) => {
  await ready(page);
  await typeSchema(page, 'meta:\n  id: broken\nseq:\n  - id: a\n    type: nope\n');
  await expect(page.locator('.problems')).toContainText('nope', { timeout: 10_000 });
  await expect(page.locator('#export')).toBeDisabled();
});
