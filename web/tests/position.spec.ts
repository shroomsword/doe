// What a change of the layout must not lose: the scroll offsets of the panes, the focus with its caret, the text and
// caret of a search box.

import { expect, test, type Page } from '@playwright/test';
import { dragPane } from './pane-drag.js';

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

const offsets = (page: Page) =>
  page.evaluate(() => ({
    editor: (document.querySelector('.cm-scroller') as HTMLElement).scrollTop,
    tree: (document.querySelector('#tree-box') as HTMLElement).scrollTop,
    hex: (document.querySelector('#hex .hex') as HTMLElement).scrollTop,
  }));

/** Every offset is where it was (within a pixel), and not the top. */
async function expectKept(page: Page, before: Awaited<ReturnType<typeof offsets>>, what: string): Promise<void> {
  // an editor that has the focus measures itself again a moment after its size changed
  await page.waitForTimeout(150);
  const after = await offsets(page);
  for (const key of ['editor', 'tree', 'hex'] as const) {
    expect(before[key], `${what}: ${key} was scrolled`).toBeGreaterThan(50);
    expect(Math.abs(after[key] - before[key]), `${what}: the ${key} offset`).toBeLessThanOrEqual(1);
  }
}

/** The focus is in the editor, and typing goes on at the end of the schema. */
async function expectCaretAtEnd(page: Page, what: string): Promise<void> {
  expect(await page.evaluate(() => document.activeElement?.className ?? ''), `${what}: the focus`).toContain('cm-content');
  await page.keyboard.type('ZZ');
  await expect(page.locator('.cm-content')).toContainText('size-eos: trueZZ');
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
  const before = await scrollAll(page);
  // from the editor: Alt+Shift+l swaps the Schema pane with its neighbour
  await page.keyboard.press('Alt+Shift+KeyL');
  await expectKept(page, before, 'Alt+Shift+l');
  await expectCaretAtEnd(page, 'Alt+Shift+l');
  // (typing scrolls the editor to its caret: scroll again)
  const again = await scrollAll(page);
  await page.keyboard.press('Alt+Shift+KeyH');
  await expectKept(page, again, 'Alt+Shift+h');
  await expectCaretAtEnd(page, 'Alt+Shift+h');
  // by the mouse
  const third = await scrollAll(page);
  await dragPane(page, 'pane-hex', 'pane-editor', 'before');
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
