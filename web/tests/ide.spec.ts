// The panes of the page, in a real browser: the tree, the bytes, the editor and
// the problems, and how they stay in step.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, test, type Locator, type Page } from '@playwright/test';

const repo = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

// The example: "DOE", count 2, entry 1 = kind 1 len 5 "hello" (bytes 4–10),
// entry 2 = kind 2 len 3 "bye" (bytes 11–15).

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await page.waitForFunction(() => window.doe?.ready === true);
});

const row = (page: Page, name: string): Locator => page.locator('.tree-row', { has: page.locator('.name', { hasText: new RegExp(`^${name.replace(/[[\]]/g, '\\$&')}$`) }) });
const selectedBytes = (page: Page): Locator => page.locator('.hex .b.sel');

/** Open a node of the tree if it is closed. */
async function expand(page: Page, name: string): Promise<void> {
  const node = row(page, name);
  if ((await node.getAttribute('aria-expanded')) !== 'true') await node.locator('.toggle').click();
}

/** Replace the text of the schema, as typing would. */
async function typeSchema(page: Page, text: string): Promise<void> {
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText(text);
}

test('the example is parsed: a tree, the bytes and no problems', async ({ page }) => {
  await expect(page.locator('#status')).toContainText('example.bin, 16 bytes');
  await expect(row(page, 'example')).toBeVisible();
  await expect(row(page, 'count')).toContainText('2');
  await expect(page.locator('.hex-row').first()).toContainText('44 4f 45'.replace(/ /g, ''));
  await expect(page.locator('.problems')).toContainText('No problems.');
});

test('a click on a field marks its bytes; arrays and nested types work', async ({ page }) => {
  await row(page, 'count').click();
  await expect(selectedBytes(page)).toHaveCount(1);
  await expect(selectedBytes(page)).toHaveAttribute('data-i', '3');

  await row(page, 'entries').locator('.toggle').click();
  await row(page, '[1]').click();
  const bytes = await selectedBytes(page).evaluateAll((cells) => cells.map((cell) => Number(cell.getAttribute('data-i'))));
  expect(bytes).toEqual([11, 12, 13, 14, 15]);
  await expect(page.locator('#selection')).toContainText('example/entries/[1]');

  await row(page, '[1]').locator('.toggle').click();
  await page.locator('.tree-row', { has: page.locator('.name', { hasText: /^text$/ }) }).last().click();
  const text = await selectedBytes(page).evaluateAll((cells) => cells.map((cell) => Number(cell.getAttribute('data-i'))));
  expect(text).toEqual([13, 14, 15]);
});

test('a click on a byte selects the innermost field and opens the way to it', async ({ page }) => {
  // byte 12 is the length of the second entry
  await page.locator('.hex .b[data-i="12"]').click();
  const selected = page.locator('.tree-row.selected');
  await expect(selected).toHaveCount(1);
  await expect(selected.locator('.name')).toHaveText('len');
  await expect(selected.locator('.summary')).toHaveText('3');
  await expect(selectedBytes(page)).toHaveAttribute('data-i', '12');
  // the entries are open, to show it
  await expect(row(page, '[1]')).toBeVisible();
});

test('an instance is shown and has no bytes of its own', async ({ page }) => {
  const instance = row(page, 'last_text');
  await expect(instance).toHaveClass(/instance/);
  await expect(instance.locator('.summary')).toHaveText('"bye"');
  await instance.click();
  await expect(selectedBytes(page)).toHaveCount(0);
  await expect(page.locator('#selection')).toContainText('last_text');
});

test('editing the schema checks it again, underlines the mistake and lists it', async ({ page }) => {
  await typeSchema(page, 'meta:\n  id: t\nseq:\n  - id: a\n    type: u1\n  - id: b\n    type: nope\n');
  await expect(page.locator('.problem')).toHaveCount(1);
  await expect(page.locator('.problem .kind')).toHaveText('schema');
  await expect(page.locator('.problem .where')).toHaveText('7:11');
  await expect(page.locator('.cm-lintRange-error')).toHaveCount(1);
  // no value without a schema that compiles, but the page works
  await expect(page.locator('#status')).toContainText('the schema has problems');
  await expect(page.locator('.tree-row')).toHaveCount(0);

  // a click on the problem puts the cursor there
  await page.locator('.problem').click();
  await expect(page.locator('.cm-content')).toBeFocused();
  await expect(page.locator('.cm-activeLine')).toContainText('type: nope');

  // fixed, it is parsed again
  await typeSchema(page, 'meta:\n  id: t\nseq:\n  - id: a\n    type: u1\n');
  await expect(page.locator('.problems')).toContainText('No problems.');
  await expect(row(page, 'a')).toBeVisible();
});

test('a schema that is not YAML is a problem with a place too', async ({ page }) => {
  await typeSchema(page, 'id: t\nseq: [unclosed\n');
  await expect(page.locator('.problem .kind')).toHaveText('yaml');
  await expect(page.locator('.problem .where')).toBeVisible();
});

test('a parse that fails shows what was read and where it stopped', async ({ page }) => {
  // "DOE", 3 entries promised, one given
  await page.setInputFiles('#open-input', {
    name: 'short.bin',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from([68, 79, 69, 3, 1, 2, 104, 105]),
  });
  await expect(page.locator('.problem .kind')).toHaveText('parse');
  await expect(page.locator('.tree-row.failed')).toHaveCount(1);
  await expect(page.locator('.tree-row.failed .flag')).toHaveText('failed here');
  await expect(row(page, 'count')).toContainText('3');
  await expect(page.locator('#status')).toContainText('the parse failed');
  // a click on the error selects the node where it failed
  await page.locator('.problem').click();
  await expect(page.locator('.tree-row.selected.failed')).toHaveCount(1);
});

test('imports are files the user adds, next to the schema', async ({ page }) => {
  await typeSchema(
    page,
    'meta:\n  id: main\n  imports:\n    - extra\nseq:\n  - id: e\n    type: extra\n',
  );
  await expect(page.locator('.problem .kind')).toHaveText('schema');
  await expect(page.locator('.problem .where')).toHaveText('4:7');

  await page.setInputFiles('#add-files', {
    name: 'extra.ksy',
    mimeType: 'text/plain',
    buffer: Buffer.from('meta:\n  id: extra\nseq:\n  - id: x\n    type: u1\n'),
  });
  await expect(page.locator('#schemas')).toContainText('extra.ksy');
  await expect(page.locator('.problems')).toContainText('No problems.');
  await expect(row(page, 'e')).toBeVisible();

  await page.locator('#schemas [data-remove="extra.ksy"]').click();
  await expect(page.locator('.problem')).toHaveCount(1);
});

test('a file over 256 MiB is refused, and says why', async ({ page }) => {
  await page.evaluate(() => window.doeApp!.openInput({ name: 'big.bin', size: 300 * 1024 * 1024 } as File));
  await expect(page.locator('.problem .kind')).toHaveText('input');
  await expect(page.locator('.problem .message')).toContainText('256 MiB');
  await expect(page.locator('.problem .message')).toContainText('WebAssembly');
  // what was shown is still there
  await expect(row(page, 'count')).toBeVisible();
});

test('a gallery format and its sample are parsed', async ({ page }) => {
  const schema = readFileSync(join(repo, 'conformance/kaitai_struct_formats/image/gif.ksy'));
  const sample = readFileSync(join(repo, 'conformance/samples/gallery/image/gif/small.bin'));
  await page.setInputFiles('#open-schema', { name: 'gif.ksy', mimeType: 'text/plain', buffer: schema });
  await page.setInputFiles('#open-input', { name: 'small.gif', mimeType: 'image/gif', buffer: sample });
  await expect(page.locator('#status')).toContainText('parsed as gif');
  await expect(page.locator('.problems')).toContainText('No problems.');
  await expect(row(page, 'hdr')).toBeVisible();
  await expand(page, 'hdr');
  await expect(row(page, 'magic')).toContainText('3 bytes');
  await expect(row(page, 'version')).toContainText('"89a"');
  // the bytes of the header are the first six
  await row(page, 'hdr').click();
  await expect(page.locator('.hex .b.sel')).toHaveCount(6);
});

test('the panes sit side by side when there is room and stack when there is not', async ({ page }) => {
  const box = async (selector: string): Promise<{ x: number; y: number; width: number }> => {
    const found = await page.locator(selector).boundingBox();
    if (!found) throw new Error(`${selector} is not shown`);
    return found;
  };
  await page.setViewportSize({ width: 1400, height: 900 });
  const [editor, tree, hex] = [await box('#pane-editor'), await box('#pane-tree'), await box('#pane-hex')];
  expect(editor.x).toBeLessThan(tree.x);
  expect(tree.x).toBeLessThan(hex.x);

  await page.setViewportSize({ width: 420, height: 900 });
  const [editorN, treeN, hexN, problemsN] = [await box('#pane-editor'), await box('#pane-tree'), await box('#pane-hex'), await box('#pane-problems')];
  expect(editorN.y).toBeLessThan(treeN.y);
  expect(treeN.y).toBeLessThan(hexN.y);
  expect(hexN.y).toBeLessThan(problemsN.y);
  // no sideways scroll of the page
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  expect(editorN.width).toBeGreaterThan(300);
});

test('a file of tens of megabytes scrolls, with only the rows in view in the page', async ({ page }) => {
  const size = 64 * 1024 * 1024;
  await page.evaluate(async (bytes) => {
    const data = new Uint8Array(bytes);
    // something recognisable at both ends
    data.set([0x44, 0x4f, 0x45, 0], 0);
    data.set([0xde, 0xad, 0xbe, 0xef], bytes - 4);
    await window.doeApp!.openInput(new File([data], 'big.bin'));
  }, size);
  await expect(page.locator('#status')).toContainText('big.bin, 67108864 bytes');
  // a few dozen rows for 4 million
  const rows = await page.locator('.hex-row').count();
  expect(rows).toBeGreaterThan(5);
  expect(rows).toBeLessThan(100);
  await expect(page.locator('.hex-row').first().locator('.hex-off')).toHaveText('00000000');

  // the scroll bar reaches the end of the file
  await page.locator('.hex').evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  const last = page.locator('.hex-row').last();
  await expect(last.locator('.hex-off')).toHaveText('03fffff0');
  await expect(last.locator('.hex-bytes')).toContainText('deadbeef');
  // and a byte there can be clicked
  await page.locator('.hex .b[data-i="67108863"]').click();
  await expect(page.locator('.hex .b.sel')).toHaveCount(0);
});

// ── a parse that takes long ─────────────────────────────────────────────────

// One element for each byte: with 1 MiB of input, far more than the limit
// that these tests set.
const SLOW_SCHEMA = 'meta:\n  id: slow\nseq:\n  - id: all\n    type: u1\n    repeat: eos\n';

/** Open the page with a short limit, a slow schema and an input to match. */
async function slowPage(page: Page, limit: number): Promise<void> {
  await page.goto(`/?limit=${limit}`);
  await page.waitForFunction(() => window.doe?.ready === true);
  await page.evaluate(async () => {
    await window.doeApp!.openInput(new File([new Uint8Array(1024 * 1024)], 'slow.bin'));
  });
}

test('a parse that takes too long is stopped, said so, and the next edit works', async ({ page }) => {
  await slowPage(page, 40);
  await typeSchema(page, SLOW_SCHEMA);
  await expect(page.locator('.problems')).toContainText('took longer than', { timeout: 20_000 });
  await expect(page.locator('#status')).toHaveText('stopped');
  // The worker was replaced: a schema with nothing to read is parsed.
  await typeSchema(page, 'meta:\n  id: small\nseq:\n  - id: a\n    type: u1\n');
  await expect(page.locator('#status')).toContainText('parsed as small', { timeout: 20_000 });
});

test('the page stays responsive while a parse runs', async ({ page }) => {
  await slowPage(page, 3000);
  await typeSchema(page, SLOW_SCHEMA);
  await expect(page.locator('#cancel')).toBeVisible();
  // Timers on the page's thread run during the parse.
  const ticks = await page.evaluate(
    () =>
      new Promise<number>((resolve) => {
        let count = 0;
        const timer = setInterval(() => count++, 10);
        setTimeout(() => {
          clearInterval(timer);
          resolve(count);
        }, 500);
      }),
  );
  expect(ticks).toBeGreaterThan(20);
});

test('a parse can be cancelled', async ({ page }) => {
  await slowPage(page, 60_000);
  await typeSchema(page, SLOW_SCHEMA);
  await expect(page.locator('#cancel')).toBeVisible();
  await page.locator('#cancel').click();
  await expect(page.locator('#status')).toHaveText('cancelled');
  await expect(page.locator('#cancel')).toBeHidden();
  await typeSchema(page, 'meta:\n  id: small\nseq:\n  - id: a\n    type: u1\n');
  await expect(page.locator('#status')).toContainText('parsed as small', { timeout: 20_000 });
});

test('only the newest parse is shown', async ({ page }) => {
  await page.evaluate(async () => {
    const app = window.doeApp!;
    // Two runs started one after the other: the first is ended by the second.
    const client = app.client;
    const first = client.parse({ schema: 'meta:\n  id: first\nseq:\n  - id: a\n    type: u1\n' }).then(
      () => 'answered',
      (e: { reason: string }) => e.reason,
    );
    const second = await client.parse({ schema: 'meta:\n  id: second\nseq:\n  - id: a\n    type: u1\n' });
    (window as unknown as { out: unknown }).out = { first: await first, second: second.id };
  });
  expect(await page.evaluate(() => (window as unknown as { out: unknown }).out)).toEqual({ first: 'cancelled', second: 'second' });
});
