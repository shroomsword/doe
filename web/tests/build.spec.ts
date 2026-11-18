// Building a schema from the bytes of the open file, in the page: the worked example of the README is built with the
// buttons and the dialog, with no YAML typed, and the text in the editor is the schema that the README shows.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { type Page, expect, test } from '@playwright/test';

const here = dirname(fileURLToPath(import.meta.url));
const readme = readFileSync(join(here, '..', '..', 'README.md'), 'utf8');
const BLIP = readFileSync(join(here, '..', '..', 'crates', 'doe-tui', 'tests', 'example', 'blip.bin'));

/** The fenced block that follows the line `<!-- example: NAME -->` in the README. */
function block(name: string): string {
  const after = readme.split(`<!-- example: ${name} -->`)[1] ?? '';
  const lines = after.split('\n');
  const start = lines.findIndex((l) => l.startsWith('```')) + 1;
  const end = lines.findIndex((l, i) => i >= start && l.startsWith('```'));
  return `${lines.slice(start, end).join('\n')}\n`;
}

test.use({ viewport: { width: 1500, height: 900 } });

/** The page with an empty editor and the file open, in build mode. */
async function building(page: Page, file = BLIP): Promise<void> {
  await page.goto('/');
  await expect(page.locator('body')).toHaveAttribute('data-help', 'ready');
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.press('Delete');
  await page.setInputFiles('#open-input', { name: 'blip.bin', mimeType: 'application/octet-stream', buffer: file });
  await expect(page.locator('#status')).toContainText('blip.bin');
  await page.locator('#build-toggle').click();
  await expect(page.locator('#build-panel')).toBeVisible();
  await expect(page.locator('#build-state')).toContainText('0 fields');
}

const editorText = (page: Page): Promise<string> => page.locator('.cm-content').evaluate((el) => (el as HTMLElement).innerText.replace(/\n\n/g, '\n'));

/** Add a field with the dialog: the kind, and the answers by the id of their box. */
async function add(page: Page, kind: string, name: string, answers: Record<string, string> = {}): Promise<void> {
  await page.locator('#build-add').click();
  await expect(page.locator('#build-dialog')).toBeVisible();
  await page.locator('#build-kind').selectOption(kind);
  for (const [id, value] of Object.entries(answers)) await page.locator(`#${id}`).fill(value);
  await page.locator('#build-name').fill(name);
  await page.locator('#build-apply').click();
  await expect(page.locator('#build-dialog')).toBeHidden();
}

test('the example of the README is built with the dialog, with no YAML typed, and the text is the README\'s schema', async ({ page }) => {
  await building(page);
  await add(page, 'magic', 'signature', { 'build-magic': '42 4c 49 50' });
  await add(page, 'u1', 'version');
  await add(page, 'u1', 'count');
  await add(page, 'u2', 'values', { 'build-repeat': 'count' });
  await add(page, 'strz', 'title');
  await add(page, 'bits', 'kind', { 'build-bits': '3' });
  await add(page, 'bits', 'level', { 'build-bits': '5' });
  await expect(page.locator('#build-fields li[role="option"]')).toHaveCount(7);
  await expect(page.locator('#build-fields li').nth(3)).toContainText('0x6..0xc');
  await expect(page.locator('#build-fields li').nth(3)).toContainText('u2[]');
  expect((await editorText(page)).replace(/\s+$/, '')).toBe(block('blip.yaml').replace(/\s+$/, ''));
  // and it parses the file
  await expect(page.locator('#status')).not.toContainText('error', { timeout: 10_000 });
  await expect(page.locator('.tree-row .name', { hasText: /^title$/ })).toBeVisible();
});

test('a click on a byte after the last field says how many bytes the next one has, and the dialog proposes the kind and a name', async ({ page }) => {
  await building(page);
  await page.locator('#hex .b[data-i="3"]').click();
  await expect(page.locator('#build-state')).toContainText('4 bytes chosen');
  await page.keyboard.press('Shift+A');
  await expect(page.locator('#build-dialog')).toBeVisible();
  await expect(page.locator('#build-kind')).toHaveValue('u4');
  await expect(page.locator('#build-name')).toHaveValue('u4_1');
  await page.locator('#build-kind').selectOption('magic');
  await expect(page.locator('#build-magic')).toHaveValue('42 4c 49 50');
  await page.locator('#build-name').fill('signature');
  await page.locator('#build-apply').click();
  await expect(page.locator('#build-fields li[role="option"]')).toHaveCount(1);
  await expect(page.locator('#build-fields li').first()).toContainText('0x0..0x4');
});

test('a refused change says why in the dialog and leaves the text alone', async ({ page }) => {
  await building(page);
  await add(page, 'u1', 'a');
  const before = await editorText(page);
  await page.locator('#build-add').click();
  await page.locator('#build-kind').selectOption('u1');
  await page.locator('#build-name').fill('a');
  await page.locator('#build-apply').click();
  await expect(page.locator('#build-error')).toContainText('there is a field called a already');
  await expect(page.locator('#build-dialog')).toBeVisible();
  // a length that is missing is said in words, before the library is asked
  await page.locator('#build-kind').selectOption('bytes');
  await page.locator('#build-apply').click();
  await expect(page.locator('#build-error')).toContainText('length');
  await page.locator('#build-cancel').click();
  expect(await editorText(page)).toBe(before);
  // a field the file is too short for
  await page.locator('#build-add').click();
  await page.locator('#build-kind').selectOption('bytes');
  await page.locator('#build-length').fill('500');
  await page.locator('#build-name').fill('big');
  await page.locator('#build-apply').click();
  await expect(page.locator('#build-error')).toContainText('does not fit');
  await page.locator('#build-cancel').click();
  expect(await editorText(page)).toBe(before);
});

test('rename, change the kind, group, take apart, remove and the byte order work on the selected field', async ({ page }) => {
  await building(page);
  await add(page, 'u1', 'a');
  await add(page, 'u1', 'b');
  await add(page, 'u1', 'c');
  await page.locator('#build-fields li[role="option"]').first().click();
  await page.locator('#build-rename').click();
  await page.locator('#build-ask-1').fill('first');
  await page.locator('#build-ask-apply').click();
  await expect(page.locator('#build-ask-dialog')).toBeHidden();
  await expect(page.locator('#build-fields li').first()).toContainText('first');
  // a name that is taken stays in the dialog with the reason
  await page.locator('#build-rename').click();
  await page.locator('#build-ask-1').fill('b');
  await page.locator('#build-ask-apply').click();
  await expect(page.locator('#build-ask-error')).toContainText('there is a field called b already');
  await page.locator('#build-ask-cancel').click();
  // change the kind of the second
  await page.locator('#build-fields li[role="option"]').nth(1).click();
  await page.locator('#build-retype').click();
  await page.locator('#build-kind').selectOption('u2');
  await page.locator('#build-apply').click();
  await expect(page.locator('#build-fields li').nth(1)).toContainText('u2');
  // group the first two into a type, then take it apart
  await page.locator('#build-fields li[role="option"]').first().click();
  await page.locator('#build-group').click();
  await page.locator('#build-ask-1').fill('head');
  await page.locator('#build-ask-2').fill('2');
  await page.locator('#build-ask-3').fill('head');
  await page.locator('#build-ask-apply').click();
  await expect(page.locator('#build-ask-dialog')).toBeHidden();
  await expect(page.locator('#build-fields')).toContainText('type head');
  await page.locator('#build-fields li[role="option"]').first().click();
  await page.locator('#build-ungroup').click();
  await expect(page.locator('#build-fields')).not.toContainText('type head');
  // remove one with the key, and flip the byte order
  await page.locator('#build-fields').focus();
  await page.keyboard.press('Delete');
  await expect(page.locator('#build-fields li[role="option"]')).toHaveCount(2);
  await page.locator('#build-endian').click();
  await expect(page.locator('#build-state')).toContainText('big-endian');
});

test('Ctrl-Z in the editor undoes a change of the builder, and a hand edit between two changes is kept', async ({ page }) => {
  await building(page);
  await add(page, 'u1', 'a');
  const one = await editorText(page);
  await add(page, 'u1', 'b');
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+z');
  await expect(page.locator('#build-fields li[role="option"]')).toHaveCount(1);
  expect(await editorText(page)).toBe(one);
  // rename by hand in the editor, then add: the hand edit is kept
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText(one.replace('id: a', 'id: renamed'));
  await expect(page.locator('#build-fields')).toContainText('renamed');
  await add(page, 'u1', 'b');
  await expect(page.locator('#build-fields')).toContainText('renamed');
  await expect(page.locator('#build-fields li[role="option"]')).toHaveCount(2);
});

test('a text that the builder did not write is said not to be, with its line, and is not changed', async ({ page }) => {
  await building(page);
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  const text = 'meta:\n  id: x\nseq:\n  - id: a\n    type: u1\n    if: 1 == 1\n';
  await page.keyboard.insertText(text);
  await expect(page.locator('#build-state')).toContainText('line');
  await expect(page.locator('#build-add')).toBeDisabled();
  expect(await editorText(page)).toBe(text);
});

test('the keys B and A, the dialog and the list are reachable from the keyboard, and what is done is announced', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('body')).toHaveAttribute('data-help', 'ready');
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.press('Delete');
  await page.setInputFiles('#open-input', { name: 'blip.bin', mimeType: 'application/octet-stream', buffer: BLIP });
  await expect(page.locator('#status')).toContainText('blip.bin');
  await page.locator('.hex').focus();
  await page.keyboard.press('Shift+B');
  await expect(page.locator('#build-panel')).toBeVisible();
  await expect(page.locator('#build-fields')).toBeFocused();
  await page.keyboard.press('Shift+A');
  await expect(page.locator('#build-dialog')).toBeVisible();
  await expect(page.locator('#build-kind')).toBeFocused();
  // Enter in the box of the name applies (a list does not submit a form)
  await page.locator('#build-name').press('Enter');
  await expect(page.locator('#build-dialog')).toBeHidden();
  await expect(page.locator('#layout-live')).toContainText('added');
  await expect(page.locator('#build-fields')).toBeFocused();
  await expect(page.locator('#build-fields')).toHaveAttribute('aria-activedescendant', /build-row-top-0/);
  // B leaves build mode
  await page.keyboard.press('Shift+B');
  await expect(page.locator('#build-panel')).toBeHidden();
});

test('a library type of an imported schema can be a kind of a field', async ({ page }) => {
  await building(page);
  await page.setInputFiles('#add-files', {
    name: 'pair.ksy',
    mimeType: 'text/plain',
    buffer: Buffer.from('meta:\n  id: pair\n  endian: le\nseq:\n  - id: x\n    type: u1\n  - id: y\n    type: u1\n'),
  });
  await expect(page.locator('#schemas')).toContainText('pair.ksy');
  await page.locator('#build-add').click();
  await page.locator('#build-kind').selectOption('type');
  await page.locator('#build-type').fill('pair');
  await page.locator('#build-name').fill('first');
  await page.locator('#build-apply').click();
  await expect(page.locator('#build-dialog')).toBeHidden();
  await expect(page.locator('#build-fields li').first()).toContainText('pair');
  expect(await editorText(page)).toContain('imports:');
});
