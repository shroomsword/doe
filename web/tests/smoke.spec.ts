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

test('the module is loaded, and a first visit has nothing in the editor and no error', async ({ page }) => {
  await expect(page.locator('#version')).not.toBeEmpty();
  await expect(page.locator('.cm-editor')).toBeVisible();
  await expect(page.locator('.cm-content')).toHaveText('');
  await expect(page.locator('#status')).toContainText('no file opened');
  await expect(page.locator('#status')).toContainText('type a schema');
  await expect(page.locator('.problems')).toContainText('No problems.');
  await expect(page.locator('#export')).toBeDisabled();
  await expect(page.locator('#example')).toHaveCount(0);
});

test('a file opened with no schema is shown as bytes, with nothing to say against it', async ({ page }) => {
  await page.setInputFiles('#open-input', { name: 'x.bin', mimeType: 'application/octet-stream', buffer: Buffer.from('DOE-bytes') });
  await expect(page.locator('#status')).toContainText('x.bin, 9 bytes');
  await expect(page.locator('.hex-row').first()).toContainText('444f45');
  await expect(page.locator('.problems')).toContainText('No problems.');
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

test('the three file buttons say what they open, and each explains what it does to the rest', async ({ page }) => {
  const button = (id: string) => page.locator(`label.button:has(#${id})`);
  await expect(button('open-schema')).toContainText('Open main schema');
  await expect(button('open-schema')).toHaveAttribute('title', /replacing its text.*Imports that were added stay/);
  await expect(button('add-files')).toContainText('Add imported schemas');
  await expect(button('add-files')).toHaveAttribute('title', /main schema is not changed/);
  await expect(button('open-input')).toContainText('Open file to parse');
  await expect(button('open-input')).toHaveAttribute('title', /never uploaded/);
});
