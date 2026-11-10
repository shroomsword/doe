// The page served by `doe --ide`: the types of the machine, and a file too
// large for the browser that stays where it is. (local-server.mjs starts it.)

import { saveAs, tarOf, useType, withSavePicker } from './fixture.js';
import { readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { type Page, expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
});

test('the types of the machine are offered and open with their imports', async ({ page }) => {
  await expect(page.locator('#types-box')).toBeVisible();
  const options = await page.locator('#types option').allTextContents();
  expect(options).toEqual(['choose…', 'common — two bytes', 'pair']);
  await page.locator('#types').selectOption({ label: 'pair' });
  await expect(page.locator('.cm-content')).toContainText('imports');
  // the import comes from the machine too: no problems, though "common" is not in the page
  await expect(page.locator('.problems .problem')).toHaveCount(0);
  await expect(page.locator('#schemas li:not(.main) code')).toHaveCount(1);
});

test('a file larger than the browser limit is read and parsed by the server', async ({ page }) => {
  const use = page.locator('#use-input');
  await expect(use).toHaveText('Use big.bin');
  await page.locator('#types').selectOption({ label: 'pair' });
  await use.click();
  await expect(page.locator('#status')).toContainText('big.bin, 314572800 bytes (on this machine)', { timeout: 20_000 });
  await expect(page.locator('#status')).toContainText('parsed as pair');
  // the fields, from the first bytes of the file
  const a = page.locator('.tree-row', { has: page.locator('.name', { hasText: /^a$/ }) });
  await expect(a).toContainText('1');
  // the bytes come in as they are shown
  await expect(page.locator('.hex-row').first().locator('.hex-bytes')).toContainText('0001');
  await page.locator('.hex').evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await expect(page.locator('.hex-row').last().locator('.hex-bytes')).toContainText('deadbeef');
});

test('the gallery is allowed by the page policy, and saving works on the file that stays on the machine', async ({ page }) => {
  await withSavePicker(page, null);
  // The request is answered here; what is tested is that the policy of the server lets the page make it.
  await page.route('https://raw.githubusercontent.com/**', (route) =>
    route.fulfill({ body: 'meta:\n  id: from_gallery\nseq:\n  - id: a\n    type: u2be\n', contentType: 'text/plain' }),
  );
  const gallery = (await (await page.request.get('/gallery.json')).json()) as { formats: { path: string; bundled?: boolean }[] };
  // a format of the Kaitai Struct gallery (the types of doe's own library come before it and are served by the page)
  const first = gallery.formats.find((format) => !format.bundled);
  if (first) {
    await useType(page, first.path, first.path);
    await expect(page.locator('.cm-content')).toContainText('from_gallery');
  }
  // and the page may fetch a type of the library from the server that serves it
  const bundled = gallery.formats.find((format) => format.bundled);
  expect(bundled).toBeDefined();
  await useType(page, 'library/ar', 'library/ar.yaml');
  await expect(page.locator('.cm-content')).toContainText('id: ar');

  await page.locator('#types').selectOption({ label: 'pair' });
  await page.locator('#use-input').click();
  await expect(page.locator('#status')).toContainText('parsed as pair', { timeout: 20_000 });
  await expect(page.locator('#save')).toBeEnabled();
  const download = await saveAs(page, 'json');
  const text = readFileSync((await download.path()) ?? '', 'utf8');
  expect(JSON.parse(text)).toMatchObject({ a: 1 });
});

test('the parsers of the preview of an archive come from the chunks the binary carries', async ({ page }) => {
  const chunks: string[] = [];
  page.on('response', (response) => {
    if (response.url().includes('/chunks/')) chunks.push(`${response.status()} ${response.headers()['content-type'] ?? ''}`);
  });
  await page.setInputFiles('#open-input', { name: 'sample.tar', mimeType: 'application/x-tar', buffer: tarOf({ 'main.rs': 'fn main() { let n = 42; }\n' }) });
  await expect(page.locator('#detect')).toBeEnabled();
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText('tar parses the file', { timeout: 30_000 });
  await page.locator('#files-list .file-row', { hasText: 'main.rs' }).click();
  await expect(page.locator('#files-preview-text')).toHaveAttribute('data-language', 'rust');
  await expect(page.locator('#files-preview-text span').first()).toBeVisible();
  // every chunk was served, as JavaScript
  expect(chunks.length).toBeGreaterThan(0);
  expect(chunks.every((c) => c.startsWith('200 text/javascript'))).toBe(true);
});

// ── the libraries follow the files (the server watches them; nothing polls) ──────────────────

const machine = process.env.DOE_IDE_TEST_DIR ?? join(tmpdir(), 'doe-ide-test');
const typesDir = join(machine, 'types');
const configFile = join(machine, 'config.toml');
const typeText = (id: string, doc = ''): string => `meta:\n  id: ${id}\n${doc ? `doc: ${doc}\n` : ''}seq:\n  - id: a\n    type: u1\n`;
const options = (page: Page) => page.locator('#types option');

test('a type that is added, changed and removed on the machine shows in the list with no reload', async ({ page }) => {
  const file = join(typesDir, 'fresh.yaml');
  try {
    await page.locator('#types').selectOption({ label: 'pair' });
    writeFileSync(file, typeText('fresh', 'a new one'));
    await expect(options(page)).toHaveText(['choose…', 'common — two bytes', 'fresh — a new one', 'pair']);
    // the choice the person made stays
    await expect(page.locator('#types')).toHaveValue(/pair\.ksy$/);
    writeFileSync(file, typeText('fresh', 'changed'));
    await expect(options(page)).toContainText(['choose…', 'common — two bytes', 'fresh — changed', 'pair']);
    // and Browse has it too
    await page.keyboard.press('t');
    await expect(page.locator('#browse-list')).toContainText('fresh');
    await page.keyboard.press('Escape');
    rmSync(file);
    await expect(options(page)).toHaveText(['choose…', 'common — two bytes', 'pair']);
  } finally {
    rmSync(file, { force: true });
  }
});

test('a save that renames a new file over the type is noticed, and the temporary file is not', async ({ page }) => {
  const file = join(typesDir, 'saved.yaml');
  try {
    writeFileSync(join(typesDir, '.saved.yaml.swp'), 'x');
    writeFileSync(join(typesDir, 'saved.yaml.tmp'), typeText('saved', 'by rename'));
    renameSync(join(typesDir, 'saved.yaml.tmp'), file);
    await expect(options(page)).toContainText(['choose…', 'common — two bytes', 'pair', 'saved — by rename']);
  } finally {
    rmSync(file, { force: true });
    rmSync(join(typesDir, '.saved.yaml.swp'), { force: true });
  }
  await expect(options(page)).toHaveText(['choose…', 'common — two bytes', 'pair']);
});

test('the config is read again: a path that is added gives its types, and a config that does not parse keeps the list', async ({ page }) => {
  const extra = join(machine, 'extra');
  writeFileSync(join(extra, 'extra.yaml'), typeText('extra', 'from the config'));
  try {
    writeFileSync(configFile, `include_paths = [${JSON.stringify(extra)}]\ndefault_types = false\n`);
    await expect(options(page)).toContainText(['extra — from the config']);
    // a mistake in the config: said, and the list stays
    writeFileSync(configFile, 'include_paths = [\n');
    await expect(page.locator('#types-note')).toContainText('could not be read');
    await expect(options(page)).toContainText(['extra — from the config']);
    // a new type in the folder that is still watched is seen
    writeFileSync(join(extra, 'second.yaml'), typeText('second'));
    writeFileSync(configFile, `include_paths = [${JSON.stringify(extra)}]\ndefault_types = false\n`);
    await expect(page.locator('#types-note')).toHaveText('');
    await expect(options(page)).toContainText(['second']);
    // the path is taken out again
    writeFileSync(configFile, 'default_types = false\n');
    await expect(options(page)).toHaveText(['choose…', 'common — two bytes', 'pair']);
  } finally {
    writeFileSync(configFile, 'default_types = false\n');
    rmSync(join(extra, 'extra.yaml'), { force: true });
    rmSync(join(extra, 'second.yaml'), { force: true });
  }
});

test('a type that changed on disk is announced, and the text in the editor is not replaced until it is asked', async ({ page }) => {
  const file = join(typesDir, 'pair.ksy');
  const original = readFileSync(file, 'utf8');
  try {
    await page.locator('#types').selectOption({ label: 'pair' });
    await expect(page.locator('.cm-content')).toContainText('imports');
    await expect(page.locator('#types-changed')).toBeHidden();
    // the person edits; the file changes under them
    await page.locator('.cm-content').click();
    await page.keyboard.press('ControlOrMeta+End');
    await page.keyboard.insertText('\n# my own edit\n');
    writeFileSync(file, `${original}# changed on disk\n`);
    await expect(page.locator('#types-changed')).toContainText('pair.ksy changed on this machine');
    await expect(page.locator('.cm-content')).toContainText('my own edit');
    await expect(page.locator('.cm-content')).not.toContainText('changed on disk');
    await page.locator('#types-reload-open').click();
    await expect(page.locator('.cm-content')).toContainText('changed on disk');
    await expect(page.locator('.cm-content')).not.toContainText('my own edit');
    await expect(page.locator('#types-changed')).toBeHidden();
  } finally {
    writeFileSync(file, original);
  }
});

test('a page that is not changed makes no requests for the types, and the key L reads the libraries now', async ({ page }) => {
  const requests: string[] = [];
  page.on('request', (request) => {
    if (request.url().includes('/api/types')) requests.push(request.url());
  });
  // nothing polls: a quiet few seconds asks for nothing
  await page.waitForTimeout(3000);
  expect(requests.filter((url) => !url.endsWith('/api/types/events'))).toEqual([]);
  await page.locator('body').click();
  await page.keyboard.press('Shift+L');
  await expect(page.locator('#status')).toContainText('read the type libraries of this machine again');
  expect(requests.some((url) => url.endsWith('/api/types/reload'))).toBe(true);
});

test('a lost connection to the server is said', async ({ page }) => {
  await page.route('**/api/types/events', (route) => route.abort());
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await expect(page.locator('#types-note')).toHaveText('lost the connection to doe --ide');
  // the list that was read stays
  await expect(options(page)).toHaveText(['choose…', 'common — two bytes', 'pair']);
});
