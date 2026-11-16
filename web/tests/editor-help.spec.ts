// Help while a schema is written: the keys and names that fit at the cursor, what is under the pointer, and go to
// definition. It all has to work while the schema has a mistake.

import { type Page, expect, test } from '@playwright/test';

/** The page with this text in the editor, and the cursor at its end. */
async function write(page: Page, text: string): Promise<void> {
  await page.goto('/');
  await expect(page.locator('body')).toHaveAttribute('data-help', 'ready');
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText(text);
}

/** Move the pointer over a word of a line of the editor (the line is found by a text it has). */
async function pointAt(page: Page, line: string, word: string): Promise<void> {
  const spot = await page.evaluate(
    ([lineText, wordText]) => {
      for (const el of Array.from(document.querySelectorAll('.cm-line'))) {
        const text = el.textContent ?? '';
        if (!text.includes(lineText as string)) continue;
        const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
        let offset = text.indexOf(lineText as string) + (lineText as string).indexOf(wordText as string);
        for (let node = walker.nextNode(); node; node = walker.nextNode()) {
          const length = node.textContent?.length ?? 0;
          if (offset < length) {
            const range = document.createRange();
            range.setStart(node, offset + 1);
            range.setEnd(node, offset + 2);
            const rect = range.getBoundingClientRect();
            return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
          }
          offset -= length;
        }
      }
      return null;
    },
    [line, word],
  );
  expect(spot).not.toBeNull();
  await page.mouse.move((spot?.x ?? 0) - 30, (spot?.y ?? 0) + 40);
  await page.mouse.move(spot?.x ?? 0, spot?.y ?? 0, { steps: 4 });
}

/** The labels that the completion list offers. */
async function offered(page: Page): Promise<string[]> {
  await page.keyboard.press('Control+Space');
  const list = page.locator('.cm-tooltip-autocomplete');
  await expect(list).toBeVisible();
  return (await list.locator('.cm-completionLabel').allTextContents()).map((s) => s.trim());
}

test('a key in a field is offered with what it is for, and what the field has is not offered again', async ({ page }) => {
  await write(page, 'seq:\n  - id: a\n    type: u1\n    ');
  const labels = await offered(page);
  expect(labels).toContain('size');
  expect(labels).toContain('repeat');
  expect(labels).not.toContain('id');
  expect(labels).not.toContain('type');
  expect(labels).not.toContain('-orig-id');
  await expect(page.locator('.cm-tooltip-autocomplete .cm-completionDetail').first()).not.toHaveText('');
});

test('the keys of the top, of meta and of an instance are the ones that go there', async ({ page }) => {
  await write(page, 'meta:\n  id: s\nseq: []\n');
  const top = await offered(page);
  expect(top).toContain('types');
  expect(top).toContain('instances');
  expect(top).not.toContain('meta');
  expect(top).not.toContain('size');
  await page.keyboard.press('Escape');
  await write(page, 'meta:\n  ');
  const meta = await offered(page);
  expect(meta).toContain('endian');
  expect(meta).not.toContain('seq');
  await page.keyboard.press('Escape');
  await write(page, 'instances:\n  x:\n    ');
  const instance = await offered(page);
  expect(instance).toContain('value');
  expect(instance).toContain('pos');
});

test('after type: the built-in types, the types of the schema, and the types it imports', async ({ page }) => {
  await write(page, 'seq:\n  - id: a\n    type: ');
  let labels = await offered(page);
  expect(labels).toContain('u2le');
  expect(labels).toContain('str');
  await page.keyboard.press('Escape');
  await write(page, 'seq:\n  - id: a\n    type: u1\ntypes:\n  header:\n    seq: []\n  other:\n    seq:\n      - id: x\n        type: ');
  labels = await offered(page);
  expect(labels).toContain('header');
  expect(labels).toContain('other');
});

test('an expression offers the fields before it, and not the ones after it', async ({ page }) => {
  await write(page, 'seq:\n  - id: count\n    type: u1\n  - id: items\n    type: u1\n    repeat: expr\n    repeat-expr: \n  - id: later\n    type: u1\n');
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+Home');
  for (let i = 0; i < 6; i += 1) await page.keyboard.press('ArrowDown');
  await page.keyboard.press('End');
  const labels = await offered(page);
  expect(labels).toContain('count');
  expect(labels).toContain('_index');
  expect(labels).not.toContain('later');
});

test('nothing is offered inside the text of a doc or a comment', async ({ page }) => {
  await write(page, 'doc: some words ');
  await page.keyboard.press('Control+Space');
  await expect(page.locator('.cm-tooltip-autocomplete')).toHaveCount(0);
  await write(page, 'meta:\n  id: s # a comm');
  await page.keyboard.press('Control+Space');
  await expect(page.locator('.cm-tooltip-autocomplete')).toHaveCount(0);
});

test('hovering a key, a built-in type and a local type says what they are', async ({ page }) => {
  await write(page, 'seq:\n  - id: a\n    type: u2le\n  - id: b\n    type: head\ntypes:\n  head:\n    doc: The head of it\n    seq:\n      - id: x\n        type: u1\n');
  const tip = page.locator('.cm-tooltip .doe-help');
  await pointAt(page, 'type: u2le', 'u2le');
  await expect(tip).toContainText('u2le');
  await expect(tip).toContainText('little-endian');
  await pointAt(page, 'type: head', 'head');
  await expect(tip).toContainText('The head of it');
  await pointAt(page, '- id: a', 'id');
  await expect(tip).toContainText('id:');
});

test('F12 goes to the definition of a type, even when the schema has a mistake', async ({ page }) => {
  await write(page, 'seq:\n  - id: b\n    type: head\n  - id: c\n    size: [\ntypes:\n  head:\n    seq: []\n');
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+Home');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('End');
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.press('F12');
  await expect(page.locator('.cm-selectionBackground').first()).toBeVisible();
  const selected = await page.evaluate(() => window.getSelection()?.toString() ?? '');
  expect(selected).toBe('head');
});

test('F12 on a built-in type says that it has no definition', async ({ page }) => {
  await write(page, 'seq:\n  - id: b\n    type: u1\n');
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+Home');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('End');
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.press('F12');
  await expect(page.locator('#status')).toContainText('built in');
});

test('an imported type is offered, and F12 on it opens the file of the import at its definition', async ({ page }) => {
  await write(page, 'meta:\n  id: s\n  imports:\n    - other\nseq:\n  - id: a\n    type: other::part\n');
  await page.setInputFiles('#add-files', {
    name: 'other.ksy',
    mimeType: 'text/plain',
    buffer: Buffer.from('meta:\n  id: other\nseq: []\ntypes:\n  part:\n    seq:\n      - id: x\n        type: u1\n'),
  });
  await expect(page.locator('#schemas')).toContainText('other');
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+End');
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('End');
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.press('F12');
  await expect(page.locator('.cm-content')).toContainText('id: other');
  expect(await page.evaluate(() => window.getSelection()?.toString() ?? '')).toBe('part');
});
