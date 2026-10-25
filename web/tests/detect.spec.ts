// Finding the schema of a file: the order and the trying (no browser), and the
// button of the page with the gallery served from the checkout.

import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { type Page, expect, test } from '@playwright/test';
import { CallEnded } from '../src/client.js';
import { type Deps, type GalleryFormat, describe, detectGallery } from '../src/detect.js';
import type { DetectSource, OrderResponse, ParseResponse } from '../src/doe.js';

const repo = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const gallery = join(repo, 'conformance', 'kaitai_struct_formats');
const samples = join(repo, 'conformance', 'samples', 'gallery');

const answer = (ok: boolean, message = 'nope'): ParseResponse => ({
  ok,
  id: 'x',
  diagnostics: [],
  value: null,
  trace: null,
  error: ok ? null : { severity: 'error', kind: 'parse', message, position: null },
});

function format(id: string, magic: number[] | null = null): GalleryFormat {
  return { path: `${id}.ksy`, id, title: null, imports: [], names: [id], ...(magic ? { magic: [{ offset: 0, bytes: magic }] } : {}) };
}

/** Deps whose order is given, and whose parse is decided by the schema text (`fits:<id>` parses). */
function deps(formats: GalleryFormat[], order: OrderResponse['order'], calls: string[], extra: Partial<Deps> = {}): Deps {
  return {
    formats,
    order: (_sources: DetectSource[]) => Promise.resolve({ ok: true, order, excluded: 3, diagnostics: [] }),
    fetchText: (path) => {
      calls.push(`fetch ${path}`);
      return Promise.resolve(path.replace('.ksy', ''));
    },
    parse: (request) => {
      calls.push(`parse ${request.schema}`);
      return Promise.resolve(request.schema.startsWith('good') ? answer(true) : answer(false, `${request.schema} does not fit`));
    },
    progress: () => undefined,
    cancelled: () => false,
    ...extra,
  };
}

// ── no browser ───────────────────────────────────────────────────────────────

test('the first format that parses is the answer, and only the ones that were tried are fetched', async () => {
  const formats = [format('bad1'), format('good1'), format('good2'), format('bad2')];
  const calls: string[] = [];
  const found = await detectGallery(
    deps(formats, [{ index: 0, why: 'signature' }, { index: 1, why: 'signature' }, { index: 2, why: 'name' }], calls),
    'x.bin',
    false,
  );
  expect(found.chosen?.format.id).toBe('good1');
  expect(found.attempts.map((a) => [a.name, a.result])).toEqual([
    ['bad1', 'failed'],
    ['good1', 'fit'],
  ]);
  expect(found.excluded).toBe(3);
  // good2 and bad2 were never fetched
  expect(calls.filter((c) => c.startsWith('fetch'))).toEqual(['fetch bad1.ksy', 'fetch good1.ksy']);
  expect(describe(found.attempts[0]!)).toContain('bad1 (the file starts with its signature): did not fit: bad1 does not fit');
});

test('a format that takes too long is reported and the next is tried; a definition that cannot be fetched too', async () => {
  const formats = [format('slow'), format('gone'), format('good')];
  const calls: string[] = [];
  const found = await detectGallery(
    deps(formats, [{ index: 0, why: 'signature' }, { index: 1, why: 'signature' }, { index: 2, why: 'signature' }], calls, {
      fetchText: (path) => (path === 'gone.ksy' ? Promise.reject(new Error('404')) : Promise.resolve(path.replace('.ksy', ''))),
      parse: (request) =>
        request.schema === 'slow' ? Promise.reject(new CallEnded('timeout', 'it took longer than 5 s')) : Promise.resolve(answer(request.schema === 'good')),
    }),
    null,
    false,
  );
  expect(found.attempts.map((a) => a.result)).toEqual(['stopped', 'not fetched', 'fit']);
  expect(found.attempts[0]?.detail).toContain('5 s');
  expect(found.chosen?.format.id).toBe('good');
});

test('when nothing parses there is no answer, and cancelling stops the loop', async () => {
  const formats = [format('bad1'), format('bad2')];
  const order: OrderResponse['order'] = [{ index: 0, why: 'name' }, { index: 1, why: 'remaining' }];
  const none = await detectGallery(deps(formats, order, []), null, true);
  expect(none.chosen).toBeNull();
  expect(none.attempts).toHaveLength(2);

  let stop = false;
  const cancelled = await detectGallery(
    deps(formats, order, [], {
      parse: () => {
        stop = true;
        return Promise.reject(new CallEnded('cancelled', 'the parse was cancelled'));
      },
      cancelled: () => stop,
    }),
    null,
    false,
  );
  expect(cancelled.chosen).toBeNull();
  expect(cancelled.attempts).toHaveLength(0);
});

// ── in the page ──────────────────────────────────────────────────────────────

async function ready(page: Page): Promise<void> {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
}

/** Serve the gallery's files from the checkout, and count what was asked for. */
async function serveGallery(page: Page): Promise<string[]> {
  const asked: string[] = [];
  await page.route('https://raw.githubusercontent.com/**', async (route) => {
    const path = new URL(route.request().url()).pathname.split('/').slice(4).join('/');
    asked.push(path);
    const file = join(gallery, path);
    if (existsSync(file)) await route.fulfill({ status: 200, contentType: 'text/plain', body: readFileSync(file) });
    else await route.fulfill({ status: 404, body: 'not found' });
  });
  return asked;
}

const hasGallery = existsSync(join(gallery, 'README.md')) && existsSync(join(samples, 'image', 'png', 'rgba.bin'));

test('the button finds the format of a file by its signature and fetches only the ones it tries', async ({ page }) => {
  test.skip(!hasGallery, 'the gallery is not checked out');
  const asked = await serveGallery(page);
  await ready(page);
  await page.setInputFiles('#open-input', { name: 'picture', mimeType: 'application/octet-stream', buffer: readFileSync(join(samples, 'image', 'png', 'rgba.bin')) });
  await expect(page.locator('#detect')).toBeEnabled();
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('#detect-report')).toContainText('png parses the file', { timeout: 30_000 });
  await expect(page.locator('#detect-report li.fit')).toContainText('the file starts with its signature');
  // the format is open and parsed
  await expect(page.locator('#editing')).toContainText('png.ksy');
  await expect(page.locator('.tree-row').first()).toBeVisible();
  // a few definitions were fetched, not the gallery
  expect(asked.length).toBeGreaterThan(0);
  expect(asked.length).toBeLessThan(10);
  expect(asked).toContain('image/png.ksy');
});

test('a file that nothing fits has a report, and "Try them all" is offered', async ({ page }) => {
  test.skip(!hasGallery, 'the gallery is not checked out');
  const asked = await serveGallery(page);
  await ready(page);
  await page.setInputFiles('#open-input', { name: 'noise.xyz', mimeType: 'application/octet-stream', buffer: Buffer.from('this is not any format at all') });
  await page.click('#detect');
  await expect(page.locator('#status')).toContainText('no gallery format parses the file', { timeout: 30_000 });
  await expect(page.locator('#detect-report')).toContainText('left out because the file does not start with their signature');
  await expect(page.locator('#detect-all')).toBeVisible();
  expect(asked.length).toBe(0);
});

test('with the setting on, opening a file looks for the schema, and the setting is remembered', async ({ page }) => {
  test.skip(!hasGallery, 'the gallery is not checked out');
  await serveGallery(page);
  await ready(page);
  await page.check('#detect-auto');
  await page.reload();
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await expect(page.locator('#detect-auto')).toBeChecked();
  await page.setInputFiles('#open-input', { name: 'x', mimeType: 'application/octet-stream', buffer: readFileSync(join(samples, 'image', 'gif', 'small.bin')) });
  // the editor still has the example, so the schema is looked for at once
  await expect(page.locator('#detect-report')).toContainText('gif parses the file', { timeout: 30_000 });
});
