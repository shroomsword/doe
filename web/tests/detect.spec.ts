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
  problems: [],
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
  await expect(page.locator('#parsed-as')).toHaveText('png');
  await expect(page.locator('.tree-row').first()).toBeVisible();
  // a few definitions were fetched, not the gallery
  expect(asked.length).toBeGreaterThan(0);
  expect(asked.length).toBeLessThan(10);
  expect(asked).toContain('image/png.ksy');
});

test('a libpcap capture is found by the values of its magic number, whatever its name says', async ({ page }) => {
  test.skip(!hasGallery || !existsSync(join(samples, 'network', 'pcap', 'scapy_seven_packets.bin')), 'the gallery is not checked out');
  await serveGallery(page);
  await ready(page);
  const capture = readFileSync(join(samples, 'network', 'pcap', 'scapy_seven_packets.bin'));
  // the four byte orders and precisions of the magic number: the sample is little-endian, microseconds
  const others = [Buffer.from([0xa1, 0xb2, 0xc3, 0xd4]), Buffer.from([0x4d, 0x3c, 0xb2, 0xa1]), Buffer.from([0xa1, 0xb2, 0x3c, 0x4d])];
  expect([...capture.subarray(0, 4)]).toEqual([0xd4, 0xc3, 0xb2, 0xa1]);
  // a file named like the pcapng format is still found as pcap
  await page.setInputFiles('#open-input', { name: 'dump.pcapng', mimeType: 'application/octet-stream', buffer: capture });
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText('pcap parses the file', { timeout: 30_000 });
  await expect(page.locator('#detect-report li.fit')).toContainText('the file starts with its signature');
  await expect(page.locator('#parsed-as')).toHaveText('pcap');
  // the other values of the enum are signatures too: the file is tried as pcap (its parse may fail on the rest)
  for (const magic of others) {
    await page.setInputFiles('#open-input', { name: 'x.bin', mimeType: 'application/octet-stream', buffer: Buffer.concat([magic, capture.subarray(4)]) });
    await page.click('#detect');
    await expect(page.locator('#detect-report')).toContainText('pcap', { timeout: 30_000 });
    await expect(page.locator('#detect-report')).not.toContainText('no gallery format');
  }
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
  // the editor is empty, so the schema is looked for at once
  await expect(page.locator('#detect-report')).toContainText('gif parses the file', { timeout: 30_000 });
});

test("the type of a file that starts with a signature of doe's own library is found without the network, and opens read-only", async ({ page }) => {
  // nothing may be asked of GitHub: the library is served with the page
  const asked = await serveGallery(page);
  await ready(page);
  await page.setInputFiles('#open-input', { name: 'module', mimeType: 'application/octet-stream', buffer: readFileSync(join(repo, 'types', 'samples', 'sample.wasm')) });
  await expect(page.locator('#detect')).toBeEnabled();
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText('wasm parses the file', { timeout: 30_000 });
  await expect(page.locator('#editing')).toContainText('library/wasm.yaml');
  await expect(page.locator('#schemas li.main')).toContainText('doe type library');
  await expect(page.locator('.tree-row').first()).toBeVisible();
  expect(asked).toEqual([]);
});

test('a cpio archive is found by either of its magic numbers, without the network, and opens as files', async ({ page }) => {
  const asked = await serveGallery(page);
  await ready(page);
  for (const [file, name] of [
    ['sample.cpio', 'initramfs'],
    ['sample_bin_be.cpio', 'old.bin'],
  ] as const) {
    await page.setInputFiles('#open-input', { name, mimeType: 'application/octet-stream', buffer: readFileSync(join(repo, 'types', 'samples', file)) });
    await page.click('#detect');
    await expect(page.locator('#detect-report')).toContainText('cpio parses the file', { timeout: 30_000 });
    await expect(page.locator('#editing')).toContainText('library/cpio.yaml');
    await expect(page.locator('#files-list .file-row').first()).toBeVisible();
  }
  expect(asked).toEqual([]);
});

test('a SquashFS image is found by hsqs, without the network, and its root directory is in the tree', async ({ page }) => {
  const asked = await serveGallery(page);
  await ready(page);
  await page.setInputFiles('#open-input', { name: 'rootfs', mimeType: 'application/octet-stream', buffer: readFileSync(join(repo, 'types', 'samples', 'sample_gzip.squashfs')) });
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText('squashfs parses the file', { timeout: 30_000 });
  await expect(page.locator('#editing')).toContainText('library/squashfs.yaml');
  await expect(page.locator('.tree-row').first()).toBeVisible();
  expect(asked).toEqual([]);
});

test('a UBI image is found by UBI#, without the network, and its eraseblocks are in the tree', async ({ page }) => {
  const asked = await serveGallery(page);
  await ready(page);
  await page.setInputFiles('#open-input', { name: 'nand.img', mimeType: 'application/octet-stream', buffer: readFileSync(join(repo, 'types', 'samples', 'sample.ubi')) });
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText('ubi parses the file', { timeout: 30_000 });
  await expect(page.locator('#editing')).toContainText('library/ubi.yaml');
  await expect(page.locator('.tree-row').first()).toBeVisible();
  expect(asked).toEqual([]);
});

test('a zlib stream is found by its first two bytes, without the network', async ({ page }) => {
  const asked = await serveGallery(page);
  await ready(page);
  await page.setInputFiles('#open-input', { name: 'blob', mimeType: 'application/octet-stream', buffer: readFileSync(join(repo, 'types', 'samples', 'sample.zlib')) });
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText('zlib_stream parses the file', { timeout: 30_000 });
  await expect(page.locator('#editing')).toContainText('library/zlib_stream.yaml');
  await expect(page.locator('.tree-row').first()).toBeVisible();
  expect(asked).toEqual([]);
});

test("a gzip file is found as doe's gzip_file (which says it holds a file), not the gallery's gzip with the same signature", async ({ page }) => {
  test.skip(!hasGallery, 'the gallery is not checked out');
  await serveGallery(page);
  await ready(page);
  await page.setInputFiles('#open-input', { name: 'sample.tar.gz', mimeType: 'application/octet-stream', buffer: readFileSync(join(repo, 'types', 'samples', 'sample.tar.gz')) });
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText('gzip_file parses the file', { timeout: 30_000 });
  await expect(page.locator('#detect-report li')).toHaveCount(1);
});

test('a Snappy framed stream is found by its identifier, without the network', async ({ page }) => {
  const asked = await serveGallery(page);
  await ready(page);
  await page.setInputFiles('#open-input', { name: 'data', mimeType: 'application/octet-stream', buffer: readFileSync(join(repo, 'types', 'samples', 'sample.sz')) });
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText('snappy parses the file', { timeout: 30_000 });
  await expect(page.locator('#editing')).toContainText('library/snappy.yaml');
  await expect(page.locator('.tree-row').first()).toBeVisible();
  expect(asked).toEqual([]);
});

test('an lzop file is found by its magic, without the network, and its blocks are in the tree', async ({ page }) => {
  const asked = await serveGallery(page);
  await ready(page);
  await page.setInputFiles('#open-input', { name: 'data', mimeType: 'application/octet-stream', buffer: readFileSync(join(repo, 'types', 'samples', 'sample.lzo')) });
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText('lzop parses the file', { timeout: 30_000 });
  await expect(page.locator('#editing')).toContainText('library/lzop.yaml');
  await expect(page.locator('.tree-row').first()).toBeVisible();
  expect(asked).toEqual([]);
});

test('a Brotli stream is opened with the type chosen from the dialog, and its meta-blocks are in the tree', async ({ page }) => {
  await ready(page);
  await page.setInputFiles('#open-input', { name: 'data.br', mimeType: 'application/octet-stream', buffer: readFileSync(join(repo, 'types', 'samples', 'sample.br')) });
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText('brotli parses the file', { timeout: 30_000 });
  await expect(page.locator('#editing')).toContainText('library/brotli.yaml');
  await expect(page.locator('.tree-row').first()).toBeVisible();
});

test('an lzip file is found by LZIP, without the network, and its trailer is in the tree', async ({ page }) => {
  const asked = await serveGallery(page);
  await ready(page);
  await page.setInputFiles('#open-input', { name: 'data', mimeType: 'application/octet-stream', buffer: readFileSync(join(repo, 'types', 'samples', 'sample_two.lz')) });
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText('lzip parses the file', { timeout: 30_000 });
  await expect(page.locator('#editing')).toContainText('library/lzip.yaml');
  await expect(page.locator('.tree-row').first()).toBeVisible();
  expect(asked).toEqual([]);
});

test('a PDF file is found by its header, without the network, and its table and objects are in the tree', async ({ page }) => {
  const asked = await serveGallery(page);
  await ready(page);
  await page.setInputFiles('#open-input', { name: 'doc', mimeType: 'application/octet-stream', buffer: readFileSync(join(repo, 'types', 'samples', 'sample.pdf')) });
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText('pdf parses the file', { timeout: 30_000 });
  await expect(page.locator('#editing')).toContainText('library/pdf.yaml');
  await expect(page.locator('.tree-row').first()).toBeVisible();
  expect(asked).toEqual([]);
});

test('an MP4 file is found by ftyp at offset 4, without the network', async ({ page }) => {
  const asked = await serveGallery(page);
  await ready(page);
  await page.setInputFiles('#open-input', { name: 'video', mimeType: 'application/octet-stream', buffer: readFileSync(join(repo, 'types', 'samples', 'sample.mp4')) });
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText('mp4 parses the file', { timeout: 30_000 });
  await expect(page.locator('#editing')).toContainText('library/mp4.yaml');
  await expect(page.locator('.tree-row').first()).toBeVisible();
  expect(asked).toEqual([]);
});

test("a type of doe's own library is marked as tried on a file built by hand, and is used from the dialog", async ({ page }) => {
  await ready(page);
  await page.click('#browse');
  await page.fill('#browse-filter', 'library/tar.yaml');
  const tar = page.locator('#browse-dialog li', { hasText: 'library/tar.yaml' });
  await expect(tar.locator('.browse-tag[title]')).toHaveText('○');
  await expect(tar.locator('.browse-tag[title]')).toHaveAttribute('title', /built by hand/);
  await tar.getByRole('button', { name: 'Use' }).click();
  await expect(page.locator('.cm-content')).toContainText('id: tar');
  await expect(page.locator('#schemas li.main')).toContainText('main schema · doe type library');
});

test('the dialog marks the formats that have been tried on a file, and says how', async ({ page }) => {
  test.skip(!hasGallery, 'the gallery is not checked out');
  await ready(page);
  await page.click('#browse');
  const entry = async (filter: string, path: string) => {
    await page.fill('#browse-filter', filter);
    return page.locator('#browse-dialog li', { has: page.locator('.browse-path', { hasText: new RegExp(`^${path.replace('.', '\\.')}$`) }) });
  };
  const elf = await entry('elf', 'executable/elf.ksy');
  await expect(elf.locator('.browse-tag', { hasText: '✓' })).toHaveAttribute('title', /written by the tool/);
  // a format that has only a file built by hand (one of the formats that no tool of the plan has written a file for yet)
  const tim = await entry('psx_tim', 'image/psx_tim.ksy');
  await expect(tim.locator('.browse-tag', { hasText: '○' })).toHaveAttribute('title', /built by hand/);
  // a format with no file is not marked
  const lvm2 = await entry('lvm2', 'filesystem/lvm2.ksy');
  await expect(lvm2).toBeVisible();
  await expect(lvm2.locator('.browse-tag', { hasText: /[✓○]/ })).toHaveCount(0);
});
