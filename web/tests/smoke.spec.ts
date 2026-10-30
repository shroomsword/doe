// The page loads the WebAssembly module, and the module answers a schema and a
// binary with the tree and the diagnostics, in a real browser.

import { expect, test } from '@playwright/test';
import type { DoeClient } from '../src/client.js';

const SCHEMA = `meta:
  id: pairs
seq:
  - id: count
    type: u1
  - id: values
    type: u2le
    repeat: expr
    repeat-expr: count
`;

declare global {
  interface Window {
    doe?: DoeClient & { ready: true };
    doeApp?: import('../src/app.js').App;
  }
}

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await page.waitForFunction(() => window.doe?.ready === true);
});

/** Whether box `a` comes before box `b` in reading order: to its left, or on a row above (a title bar wraps). */
function before(a: { x: number; y: number; width: number; height: number } | null, b: { x: number; y: number; width: number; height: number } | null): boolean {
  if (!a || !b) return false;
  return a.y + a.height <= b.y + 1 || a.x + a.width <= b.x + 1;
}

test('the module is loaded, and a first visit has nothing in the editor and no error', async ({ page }) => {
  await expect(page.locator('#version')).not.toBeEmpty();
  await expect(page.locator('.cm-editor')).toBeVisible();
  await expect(page.locator('.cm-content')).toHaveText('');
  await expect(page.locator('#status')).toContainText('no file opened');
  await expect(page.locator('#status')).toContainText('type a schema');
  await expect(page.locator('#parsed-as')).toHaveText('');
  await expect(page.locator('.problems .problem')).toHaveCount(0);
  await expect(page.locator('#save')).toBeDisabled();
  await expect(page.locator('#example')).toHaveCount(0);
});

test('a file opened with no schema is shown as bytes, with nothing to say against it', async ({ page }) => {
  await page.setInputFiles('#open-input', { name: 'x.bin', mimeType: 'application/octet-stream', buffer: Buffer.from('DOE-bytes') });
  await expect(page.locator('#status')).toContainText('x.bin, 9 bytes');
  await expect(page.locator('.hex-row').first()).toContainText('444f45');
  await expect(page.locator('.problems .problem')).toHaveCount(0);
  // and a schema typed afterwards is run on it
  await page.locator('.cm-content').click();
  await page.keyboard.insertText('meta:\n  id: shown\nseq:\n  - id: magic\n    contents: "DOE"\n');
  await expect(page.locator('#status')).toContainText('parsed as shown', { timeout: 10_000 });
});

test('a hosted page has no types of a machine and no file to use', async ({ page }) => {
  await expect(page.locator('#types-box')).toBeHidden();
  await expect(page.locator('#use-input')).toBeHidden();
});

test('parse gives the value and the ranges of its parts', async ({ page }) => {
  const answer = await page.evaluate(
    ([schema]) => {
      window.doe!.setInput(new Uint8Array([2, 1, 0, 2, 0]));
      return window.doe!.parse({ schema: schema! });
    },
    [SCHEMA],
  );
  expect(answer.ok).toBe(true);
  expect(answer.value).toMatchObject({ count: 2, values: [1, 2] });
  expect(answer.trace?.name).toBe('pairs');
  expect(answer.trace?.start).toBe(0);
  expect(answer.trace?.end).toBe(40);
  const values = answer.trace?.children.find((child) => child.name === 'values');
  expect(values?.children.map((child) => [child.start, child.end])).toEqual([
    [8, 24],
    [24, 40],
  ]);
});

test('a schema with a mistake has a diagnostic with a position', async ({ page }) => {
  const answer = await page.evaluate(() =>
    window.doe!.check({ schema: 'id: t\nseq:\n  - id: a\n    type: nope\n' }),
  );
  expect(answer.ok).toBe(false);
  expect(answer.diagnostics[0]?.position).toMatchObject({ line: 4, column: 11 });
});

test('a parse that fails keeps what was read and marks where', async ({ page }) => {
  const answer = await page.evaluate(
    ([schema]) => {
      window.doe!.setInput(new Uint8Array([3, 1, 0]));
      return window.doe!.parse({ schema: schema! });
    },
    [SCHEMA],
  );
  expect(answer.ok).toBe(false);
  expect(answer.error?.kind).toBe('parse');
  expect(answer.value).toMatchObject({ count: 3 });
});

test('the buttons of the schema are in the title bar of the Schema pane, and say what they do', async ({ page }) => {
  const button = (id: string) => page.locator(`label.button:has(#${id})`);
  await expect(button('open-schema')).toContainText('Open');
  await expect(button('open-schema')).toHaveAttribute('title', /replacing its text.*Imports that were added stay/);
  await expect(button('add-files')).toContainText('Add imports');
  await expect(button('add-files')).toHaveAttribute('title', /main schema is not changed/);
  await expect(button('open-input')).toContainText('Open');
  await expect(button('open-input')).toHaveAttribute('title', /never uploaded/);
});

test('Open, Add imports and Browse... are at the right end of the Schema pane\'s title bar, not in the header', async ({ page }) => {
  const bar = page.locator('#pane-editor .pane-title');
  const heading = bar.locator('h2');
  const actions = bar.locator('.pane-actions');
  await expect(actions.locator('#open-schema')).toHaveCount(1);
  await expect(actions.locator('#add-files')).toHaveCount(1);
  await expect(actions.locator('#browse')).toHaveText('Browse...');
  await expect(page.locator('header #open-schema, header #add-files, header #browse')).toHaveCount(0);
  // in that order, to the right of the heading, ending at the right edge of the bar
  const boxes = await Promise.all(['label.button:has(#open-schema)', 'label.button:has(#add-files)', '#browse'].map((s) => actions.locator(s).boundingBox()));
  expect(before(boxes[0] ?? null, boxes[1] ?? null)).toBe(true);
  expect(before(boxes[1] ?? null, boxes[2] ?? null)).toBe(true);
  expect(before(await heading.boundingBox(), boxes[0] ?? null)).toBe(true);
  // the last one ends at the right end of the bar
  const barBox = await bar.boundingBox();
  const last = boxes[2];
  expect((last?.x ?? 0) + (last?.width ?? 0)).toBeGreaterThan((barBox?.x ?? 0) + (barBox?.width ?? 0) - 16);
});

test('the title bars wrap on a narrow screen and the page does not scroll sideways', async ({ page }) => {
  await page.setViewportSize({ width: 340, height: 700 });
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(0);
  await expect(page.locator('#browse')).toBeVisible();
});

test('Open, Detect and auto are in the title bar of the Parsed pane, and the detection report is under it', async ({ page }) => {
  const bar = page.locator('#pane-tree .pane-title');
  const actions = bar.locator('.pane-actions');
  await expect(actions.locator('#open-input')).toHaveCount(1);
  await expect(actions.locator('#detect')).toHaveText('Detect');
  await expect(actions.locator('#detect-auto')).toHaveCount(1);
  await expect(page.locator('header #open-input, header #detect, header #detect-auto, header #detect-report')).toHaveCount(0);
  // in that order, to the right of the heading
  const [open, detect, auto] = await Promise.all(['label.button:has(#open-input)', '#detect', '#detect-auto-box'].map((s) => actions.locator(s).boundingBox()));
  expect(before(open ?? null, detect ?? null)).toBe(true);
  expect(before(detect ?? null, auto ?? null)).toBe(true);
  expect(before(await bar.locator('h2').boundingBox(), open ?? null)).toBe(true);
  // the report is in the pane, under the bar (it is shown by the tests of detection)
  await expect(page.locator('#pane-tree #detect-report')).toHaveCount(1);
});

test('Integers is the last tool at the right end of the Parsed pane\'s title bar, with This field just before it', async ({ page }) => {
  const bar = page.locator('#pane-tree .pane-title');
  await expect(bar.locator('#radix-box')).toHaveCount(1);
  await expect(bar.locator('#field-radix-box')).toHaveCount(1);
  await expect(page.locator('#pane-tree .pane-tools')).toHaveCount(0);
  // This field is for an integer that is selected: before that it is not shown
  await expect(bar.locator('#field-radix-box')).toBeHidden();
  // Integers ends the bar, at its right end
  const radix = await bar.locator('#radix-box').boundingBox();
  const barBox = await bar.boundingBox();
  expect((radix?.x ?? 0) + (radix?.width ?? 0)).toBeGreaterThan((barBox?.x ?? 0) + (barBox?.width ?? 0) - 16);
  expect(before(await bar.locator('#detect-auto-box').boundingBox(), radix)).toBe(true);
});

test('the Status pane has the status line and the problems, and says nothing about there being none', async ({ page }) => {
  const pane = page.locator('#pane-status');
  await expect(pane.locator('h2')).toHaveText('Status');
  await expect(page.locator('#pane-problems, #h-problems')).toHaveCount(0);
  // the status line is in the pane, and not in the header
  await expect(pane.locator('#status')).toHaveCount(1);
  await expect(page.locator('header #status, header #cancel')).toHaveCount(0);
  await expect(pane.locator('#cancel')).toHaveCount(1);
  // with nothing wrong the pane has the status only
  await expect(page.locator('body')).not.toContainText('No problems');
  await expect(page.locator('#problems .problem')).toHaveCount(0);
  await expect(page.locator('#problems')).toBeHidden();
  // a mistake is listed under the status
  await page.locator('.cm-content').click();
  await page.keyboard.insertText('meta:\n  id: broken\nseq:\n  - id: a\n    type: nope\n');
  await expect(pane.locator('.problem')).toContainText('nope', { timeout: 10_000 });
  await expect(pane.locator('#status')).not.toBeEmpty();
  // and fixing it takes the list away again
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText('meta:\n  id: ok\nseq:\n  - id: a\n    type: u1\n');
  await expect(page.locator('#problems .problem')).toHaveCount(0, { timeout: 10_000 });
  await expect(page.locator('body')).not.toContainText('No problems');
});
