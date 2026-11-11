// What a change of the layout must not lose: the scroll offsets of the panes, the focus with its caret, the text and
// caret of a search box.

import { expect, test, type Page } from '@playwright/test';
import { dragToZone } from './pane-drag.js';

test.use({ viewport: { width: 1400, height: 800 } });

const FIELDS = Array.from({ length: 90 }, (_, i) => `  - id: f${i}\n    type: u1`).join('\n');
const SCHEMA = `${Array.from({ length: 120 }, (_, i) => `# line ${i + 1} of a long schema`).join('\n')}\nmeta:\n  id: many\nseq:\n${FIELDS}\n  - id: rest\n    size-eos: true\n`;

async function setUp(page: Page): Promise<void> {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText(SCHEMA);
  await page.setInputFiles('#open-input', { name: 'many.bin', mimeType: 'application/octet-stream', buffer: Buffer.alloc(3000, 7) });
  await expect(page.locator('#status')).toContainText('many.bin, 3000 bytes');
  await expect(page.locator('#tree .tree-row').nth(30)).toBeAttached();
  // leave the caret at the end of the schema
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+End');
}

/** Scroll the editor, the tree and the bytes, and say where they are. */
async function scrollAll(page: Page): Promise<Awaited<ReturnType<typeof offsets>>> {
  await page.evaluate(() => {
    (document.querySelector('.cm-scroller') as HTMLElement).scrollTop = 777;
    (document.querySelector('#tree-box') as HTMLElement).scrollTop = 321;
    (document.querySelector('#hex .hex') as HTMLElement).scrollTop = 150;
  });
  await page.waitForTimeout(100);
  return offsets(page);
}

/** Where the editor, the tree and the bytes are scrolled to; and the first line of the editor that is in view (CodeMirror
 *  re-estimates the heights of the lines it has not drawn, so the same place in it is found by its line, not by the pixel). */
const offsets = (page: Page) =>
  page.evaluate(() => ({
    line: (() => {
      const top = (document.querySelector('.cm-scroller') as HTMLElement).getBoundingClientRect().top;
      return [...document.querySelectorAll('.cm-line')].find((l) => l.getBoundingClientRect().bottom > top + 1)?.textContent ?? '';
    })(),
    editor: (document.querySelector('.cm-scroller') as HTMLElement).scrollTop,
    tree: (document.querySelector('#tree-box') as HTMLElement).scrollTop,
    hex: (document.querySelector('#hex .hex') as HTMLElement).scrollTop,
  }));

/** Every offset is where it was (within a pixel), and not the top. */
async function expectKept(page: Page, before: Awaited<ReturnType<typeof offsets>>, what: string): Promise<void> {
  // an editor that has the focus measures itself again a moment after its size changed
  await page.waitForTimeout(150);
  const after = await offsets(page);
  expect(before.editor, `${what}: the editor was scrolled`).toBeGreaterThan(50);
  // the same place in the editor: its first visible line, within a few lines (its heights are estimates until drawn)
  const number = (line: string): number => Number(/(\d+)\D*$/.exec(line)?.[1] ?? -1000);
  expect(Math.abs(number(after.line) - number(before.line)), `${what}: the first line in view was "${before.line}", now "${after.line}"`).toBeLessThanOrEqual(5);
  expect(after.editor, `${what}: the editor is not at the top`).toBeGreaterThan(50);
  for (const key of ['tree', 'hex'] as const) {
    expect(before[key], `${what}: ${key} was scrolled`).toBeGreaterThan(50);
    expect(Math.abs(after[key] - before[key]), `${what}: the ${key} offset`).toBeLessThanOrEqual(1);
  }
}

/** The focus is in the editor, and typing goes on at the end of the schema. */
async function expectCaretAtEnd(page: Page, what: string): Promise<void> {
  expect(await page.evaluate(() => document.activeElement?.className ?? ''), `${what}: the focus`).toContain('cm-content');
  await page.keyboard.type('ZZ');
  const where = await page.evaluate(() => {
    const text = document.querySelector('.cm-content')?.textContent ?? '';
    return `ZZ at ${text.indexOf('ZZ')} of ${text.length}: ...${text.slice(Math.max(0, text.indexOf('ZZ') - 20), text.indexOf('ZZ') + 8)}...`;
  });
  expect(where, `${what}: where the typed text went`).toContain('size-eos: trueZZ');
  await page.keyboard.press('Backspace');
  await page.keyboard.press('Backspace');
}

test('Reset layout keeps the scroll of the editor, the tree and the bytes, and the caret of the editor', async ({ page }) => {
  await setUp(page);
  // move a pane first, so that the reset has something to put back
  await page.locator('#pane-tree').focus();
  await page.keyboard.press('Alt+Shift+KeyL');
  let before = await scrollAll(page);
  await page.getByRole('button', { name: 'Reset the layout' }).click();
  await expectKept(page, before, 'the reset button');
  // the key, from the editor (its pane has the focus after Esc), with the caret at the end
  await page.keyboard.press('Alt+Shift+KeyL');
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+End');
  before = await scrollAll(page);
  await page.keyboard.press('Escape');
  await page.keyboard.press('Shift+R');
  await expectKept(page, before, 'the R key');
});

test('a move by the keyboard and by dragging keeps the offsets, and the focus stays in the editor at its caret', async ({ page }) => {
  await setUp(page);
  // the editor is scrolled away from its caret, as it is when someone has scrolled with the wheel
  const before = await scrollAll(page);
  await page.keyboard.press('Alt+Shift+KeyL');
  await expectKept(page, before, 'Alt+Shift+l');
  // with the caret in view, typing goes on at the caret after a move (the caret that cannot be drawn is not a thing to test)
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+End');
  await page.keyboard.press('Alt+Shift+KeyH');
  await expectCaretAtEnd(page, 'Alt+Shift+h');
  // by the mouse
  const third = await scrollAll(page);
  await dragToZone(page, 'pane-hex', 'l');
  await expectKept(page, third, 'a drag');
});

test('closing and showing a pane, and maximizing and restoring one, keep the offsets of the panes', async ({ page }) => {
  await setUp(page);
  const before = await scrollAll(page);
  // close the Bytes pane and show it again
  await page.getByRole('button', { name: 'Close the Bytes pane' }).click();
  await page.getByRole('button', { name: 'Panes' }).click();
  await page.locator('#panes-list li').nth(2).locator('input').check();
  await page.keyboard.press('Escape');
  await expectKept(page, before, 'closing and showing Bytes');
  // maximize the Parsed pane and restore it: the others were hidden meanwhile
  await page.getByRole('button', { name: 'Maximize the Parsed pane' }).click();
  await page.getByRole('button', { name: 'Restore the Parsed pane' }).click();
  await expectKept(page, before, 'maximize and restore');
  // close the Schema pane too (it holds the editor) and show it
  await page.getByRole('button', { name: 'Close the Schema pane' }).click();
  await page.getByRole('button', { name: 'Panes' }).click();
  await page.locator('#panes-list li').nth(0).locator('input').check();
  await page.keyboard.press('Escape');
  await expectKept(page, before, 'closing and showing Schema');
});

test('the text and the caret of a search box survive a move', async ({ page }) => {
  await setUp(page);
  await page.locator('#pane-hex').focus();
  await page.keyboard.press('/');
  const box = page.locator('#pane-hex .pane-search input');
  await expect(box).toBeFocused();
  await page.keyboard.type('07 07 07');
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.press('ArrowLeft');
  await box.evaluate((input: HTMLInputElement) => input.setSelectionRange(2, 5));
  // swap the Bytes pane with the one beside it (the focus is in its search box, which is a field: Alt+Shift works there)
  await page.keyboard.press('Alt+Shift+KeyH');
  await expect(box).toBeFocused();
  expect(await box.inputValue()).toBe('07 07 07');
  expect(await box.evaluate((input: HTMLInputElement) => [input.selectionStart, input.selectionEnd])).toEqual([2, 5]);
  // and typing replaces the selection, where it was
  await page.keyboard.type('X');
  expect(await box.inputValue()).toBe('07X 07');
});
