// A click outside a dialog closes it (#403): all the dialogs of the page, what a press and a release in different places do, what is
// kept when a dialog that has a value is closed that way, and that Esc and the buttons still work.

import { expect, test, type Page } from '@playwright/test';
import { loadExample } from './fixture.js';
import { outside } from '../src/dialogs.js';

test('a point is outside a dialog when it is beyond its box, and its padding is inside', () => {
  const dialog = { getBoundingClientRect: () => ({ left: 100, right: 300, top: 50, bottom: 250 }) } as unknown as HTMLElement;
  expect(outside(dialog, { clientX: 99, clientY: 100 })).toBe(true);
  expect(outside(dialog, { clientX: 301, clientY: 100 })).toBe(true);
  expect(outside(dialog, { clientX: 150, clientY: 49 })).toBe(true);
  expect(outside(dialog, { clientX: 150, clientY: 251 })).toBe(true);
  expect(outside(dialog, { clientX: 100, clientY: 50 })).toBe(false);
  expect(outside(dialog, { clientX: 300, clientY: 250 })).toBe(false);
  expect(outside(dialog, { clientX: 200, clientY: 150 })).toBe(false);
});

const ready = async (page: Page): Promise<void> => {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
};

/** The dialogs: how each is opened and what has the focus when it is (the opener, which gets the focus back). */
const DIALOGS: { id: string; opener: string; open: (page: Page) => Promise<void>; setup?: (page: Page) => Promise<void> }[] = [
  { id: 'browse-dialog', opener: '#browse', open: (p) => p.locator('#browse').click() },
  { id: 'help-dialog', opener: '#help', open: (p) => p.locator('#help').click() },
  { id: 'theme-dialog', opener: '#theme-button', open: (p) => p.locator('#theme-button').click() },
  { id: 'panes-dialog', opener: '#panes-button', open: (p) => p.locator('#panes-button').click() },
  { id: 'columns-dialog', opener: '#columns-button', open: (p) => p.locator('#columns-button').click() },
  {
    id: 'keys-dialog',
    opener: '#pane-tree',
    open: async (p) => {
      await p.locator('#pane-tree').focus();
      await p.keyboard.press('Shift+K');
    },
  },
  {
    id: 'save-dialog',
    opener: '#save',
    setup: loadExample,
    open: (p) => p.locator('#save').click(),
  },
  {
    id: 'dock-dialog',
    opener: '#pane-tree',
    open: async (p) => {
      await p.locator('#pane-tree').focus();
      await p.keyboard.press('Shift+P');
    },
  },
];

test.use({ viewport: { width: 1500, height: 900 } });

/** A point on the backdrop: the corner of the window, away from every dialog. */
const BACKDROP = { x: 3, y: 3 };

for (const dialog of DIALOGS) {
  test.describe(dialog.id, () => {
    test.beforeEach(async ({ page }) => {
      await ready(page);
      await dialog.setup?.(page);
      await dialog.open(page);
      await expect(page.locator(`#${dialog.id}`)).toBeVisible();
    });

    test('a click outside closes it and the focus goes back to what opened it', async ({ page }) => {
      await page.mouse.click(BACKDROP.x, BACKDROP.y);
      await expect(page.locator(`#${dialog.id}`)).toBeHidden();
      expect(await page.evaluate(() => (document.activeElement as HTMLElement | null)?.id ?? document.activeElement?.closest('section')?.id)).toBe(dialog.opener.slice(1));
    });

    test('a click inside does not close it', async ({ page }) => {
      const heading = (await page.locator(`#${dialog.id} h3`).first().boundingBox())!;
      await page.mouse.click(heading.x + 4, heading.y + 4);
      await expect(page.locator(`#${dialog.id}`)).toBeVisible();
    });

    test('a press inside released outside, and a press outside released inside, close nothing', async ({ page }) => {
      const heading = (await page.locator(`#${dialog.id} h3`).first().boundingBox())!;
      // a drag that starts in the dialog (as selecting text does) and ends over the backdrop
      await page.mouse.move(heading.x + 4, heading.y + 4);
      await page.mouse.down();
      await page.mouse.move(BACKDROP.x, BACKDROP.y, { steps: 5 });
      await page.mouse.up();
      await expect(page.locator(`#${dialog.id}`)).toBeVisible();
      // and one that starts on the backdrop and ends in the dialog
      await page.mouse.move(BACKDROP.x, BACKDROP.y);
      await page.mouse.down();
      await page.mouse.move(heading.x + 4, heading.y + 4, { steps: 5 });
      await page.mouse.up();
      await expect(page.locator(`#${dialog.id}`)).toBeVisible();
    });

    test('Esc still closes it', async ({ page }) => {
      await page.keyboard.press('Escape');
      await expect(page.locator(`#${dialog.id}`)).toBeHidden();
    });
  });
}

test('a value that was typed in the columns dialog is not applied by a click outside, as with Esc', async ({ page }) => {
  await ready(page);
  await page.locator('#columns-button').click();
  await page.locator('#columns-input').fill('4');
  await page.mouse.click(BACKDROP.x, BACKDROP.y);
  await expect(page.locator('#columns-dialog')).toBeHidden();
  // reopened it shows the number that is in use, not what was typed
  await page.locator('#columns-button').click();
  await expect(page.locator('#columns-input')).not.toHaveValue('4');
});

test('a name that was typed in the save dialog is not used by a click outside: nothing is saved and the dialog is closed', async ({ page }) => {
  await ready(page);
  await loadExample(page);
  const downloads: string[] = [];
  page.on('download', (d) => downloads.push(d.suggestedFilename()));
  await page.locator('#save').click();
  await page.locator('#save-name').fill('mine.txt');
  await page.mouse.click(BACKDROP.x, BACKDROP.y);
  await expect(page.locator('#save-dialog')).toBeHidden();
  await page.waitForTimeout(300);
  expect(downloads).toEqual([]);
  // the dialog starts again from the name of the file
  await page.locator('#save').click();
  await expect(page.locator('#save-name')).not.toHaveValue('mine.txt');
});

test('a dialog with something to go back from goes back first, as Esc does: the reader of a type in the browser', async ({ page }) => {
  await ready(page);
  await page.locator('#browse').click();
  await page.locator('#browse-list li').first().getByRole('button', { name: 'View' }).click();
  await expect(page.locator('#browse-reader')).toBeVisible();
  await page.mouse.click(BACKDROP.x, BACKDROP.y);
  await expect(page.locator('#browse-reader')).toBeHidden();
  await expect(page.locator('#browse-dialog')).toBeVisible();
  await page.mouse.click(BACKDROP.x, BACKDROP.y);
  await expect(page.locator('#browse-dialog')).toBeHidden();
});

test('the buttons of the dialogs still close them', async ({ page }) => {
  await ready(page);
  for (const [open, close, id] of [['#help', '#help-close', 'help-dialog'], ['#panes-button', '#panes-close', 'panes-dialog'], ['#theme-button', '#theme-close', 'theme-dialog']] as const) {
    await page.locator(open).click();
    await page.locator(close).click();
    await expect(page.locator(`#${id}`)).toBeHidden();
  }
});
