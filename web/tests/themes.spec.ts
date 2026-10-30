// Themes: the built-in ones, the picker, a theme file, what is remembered, and
// that the tree, the bytes and the editor use the colours of the theme.

import { loadExample } from './fixture.js';
import { type Page, expect, test } from '@playwright/test';
import { valueKind } from '../src/model.js';
import { COLOR_NAMES, THEMES, parseThemeFile, systemTheme } from '../src/theme.js';

async function ready(page: Page): Promise<void> {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
}

const find = (id: string) => {
  const theme = THEMES.find((t) => t.id === id);
  if (!theme) throw new Error(`no theme ${id}`);
  return theme;
};

/** `#rrggbb` as the browser writes a computed colour. */
function rgb(hex: string): string {
  const n = Number.parseInt(hex.slice(1), 16);
  return `rgb(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255})`;
}

const fileOf = (name: string, text: string) => ({ name, mimeType: 'text/plain', buffer: Buffer.from(text) });

function themeFile(name: string, background = '#102030'): string {
  const colors = { ...find('doe-dark').colors, background };
  return `name = "${name}"\nvariant = "dark"\nauthor = "me"\n[colors]\n${COLOR_NAMES.map((key) => `${key} = "${colors[key]}"`).join('\n')}\n`;
}

// ── no browser ───────────────────────────────────────────────────────────────

test('there are at least eight built-in themes, each with its licence, and a pair for the system', () => {
  expect(THEMES.length).toBeGreaterThanOrEqual(8);
  for (const theme of THEMES) {
    expect(theme.license, theme.id).not.toBe('');
    expect(Object.keys(theme.colors).sort(), theme.id).toEqual([...COLOR_NAMES].sort());
  }
  expect(systemTheme(false).variant).toBe('light');
  expect(systemTheme(true).variant).toBe('dark');
});

test('a theme file is read as the terminal viewer reads it', () => {
  const theme = parseThemeFile(themeFile('Mine', '#0a0b0c'));
  expect(theme.name).toBe('Mine');
  expect(theme.variant).toBe('dark');
  expect(theme.author).toBe('me');
  expect(theme.colors.background).toBe('#0a0b0c');
  // comments, single quotes, and a variant that is left out
  const text = `# a theme\nname = 'Plain' # its name\n[colors] # the colours\n${COLOR_NAMES.map((key) => `${key} = "#aabbcc"`).join('\n')}\n`;
  const plain = parseThemeFile(text);
  expect(plain.name).toBe('Plain');
  expect(plain.variant).toBe('dark');
});

test('a theme file that is not usable says what is wrong', () => {
  const full = themeFile('T');
  const bad = (text: string): string => {
    try {
      parseThemeFile(text);
    } catch (error) {
      return (error as Error).message;
    }
    throw new Error('it was read');
  };
  expect(bad(full.replace('accent = ', '# accent = '))).toContain('accent');
  expect(bad(full.replace(/background = "[^"]*"/, 'background = "white"'))).toContain('white');
  expect(bad(`${full}sparkle = "#ffffff"\n`)).toContain('sparkle');
  expect(bad(full.replace('variant = "dark"', 'variant = "grey"'))).toContain('grey');
  expect(bad(full.replace('name = "T"\n', ''))).toContain('name');
  expect(bad('name = "x\n')).toContain('not closed');
  expect(bad('what')).toContain('line 1');
  expect(bad(`${full}[colors]\n`)).toContain('twice');
});

test('the kind of a value is told from the text that is shown', () => {
  expect(valueKind('300 (0x12c)')).toBe('number');
  expect(valueKind('-5')).toBe('number');
  expect(valueKind('1.5')).toBe('number');
  expect(valueKind('"abc"')).toBe('string');
  expect(valueKind('<4 bytes>')).toBe('bytes');
  expect(valueKind('<12 bytes at 0x10>')).toBe('bytes');
  expect(valueKind('true')).toBe('boolean');
  expect(valueKind('png (1)')).toBe('enum');
  expect(valueKind('<header>')).toBeNull();
  expect(valueKind(null)).toBeNull();
});

// ── in the page ──────────────────────────────────────────────────────────────

test('the system setting is followed until a theme is chosen', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'dark' });
  await ready(page);
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'doe-dark');
  await expect(page.locator('#theme')).toHaveValue('system');
  expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe(rgb(find('doe-dark').colors.background));
  await page.emulateMedia({ colorScheme: 'light' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'doe-light');
  expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe(rgb(find('doe-light').colors.background));
});

test('the picker lists the built-in themes and a choice shows in the page, the editor and the bytes, and is remembered', async ({ page }) => {
  await ready(page);
  const options = await page.locator('#theme option').evaluateAll((items) => items.map((item) => (item as HTMLOptionElement).value));
  expect(options.length).toBeGreaterThanOrEqual(9); // system and at least eight themes
  for (const theme of THEMES) expect(options).toContain(theme.id);

  await page.selectOption('#theme', 'nord');
  const nord = find('nord').colors;
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'nord');
  expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe(rgb(nord.background));
  expect(await page.evaluate(() => getComputedStyle(document.body).color)).toBe(rgb(nord.foreground));
  // the editor has the same background and text colour
  expect(await page.locator('.cm-editor').evaluate((e) => getComputedStyle(e).backgroundColor)).toBe(rgb(nord.background));
  expect(await page.locator('.cm-content').evaluate((e) => getComputedStyle(e).color)).toBe(rgb(nord.foreground));

  await page.reload();
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await expect(page.locator('#theme')).toHaveValue('nord');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'nord');

  await page.selectOption('#theme', 'system');
  await page.reload();
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await expect(page.locator('#theme')).toHaveValue('system');
});

test('the tree and the bytes use the colours of the theme, and nothing else', async ({ page }) => {
  await ready(page);
  await loadExample(page);
  await expect(page.locator('#status')).toContainText('parsed', { timeout: 10_000 });
  for (const id of ['solarized-light', 'dracula', 'high-contrast']) {
    await page.selectOption('#theme', id);
    const colors = find(id).colors;
    const allowed = new Set([...Object.values(colors).map(rgb), 'rgba(0, 0, 0, 0)']);
    // select a row, so that the selection is drawn in both panes
    await page.locator('.tree-row').nth(1).click();
    const used = await page.evaluate(() => {
      const seen: { where: string; property: string; value: string }[] = [];
      const parts = document.querySelectorAll('#tree-box *, #hex *, #pane-tree .pane-title, #pane-hex .pane-title, #pane-editor .pane-title, #pane-editor .pane-actions *, #pane-tree h2, #pane-hex h2, #pane-editor h2, .cm-line span');
      for (const part of parts) {
        const style = getComputedStyle(part);
        for (const property of ['color', 'backgroundColor', 'borderTopColor'] as const) {
          const value = style[property];
          // a border that is not there has the colour of the text: it is not drawn
          if (property === 'borderTopColor' && style.borderTopWidth === '0px') continue;
          seen.push({ where: `${part.tagName}.${part.className}`, property, value });
        }
      }
      return seen;
    });
    for (const item of used) expect(allowed.has(item.value), `${id}: ${item.where} ${item.property} ${item.value}`).toBe(true);
    // the kinds of value have their own colours
    const color = (selector: string) => page.locator(selector).first().evaluate((e) => getComputedStyle(e).color);
    expect(await color('.summary.number')).toBe(rgb(colors.number));
    expect(await color('.summary.string')).toBe(rgb(colors.string));
    expect(await color('.type')).toBe(rgb(colors.type));
    expect(await page.locator('.hex .sel').first().evaluate((e) => getComputedStyle(e).backgroundColor)).toBe(rgb(colors.selection));
  }
});

test('a theme file is loaded, kept in the browser, and offered in the picker', async ({ page }) => {
  await ready(page);
  await page.setInputFiles('#open-theme', fileOf('mine.toml', themeFile('Mine', '#102030')));
  await expect(page.locator('#theme')).toHaveValue('custom');
  await expect(page.locator('#theme option[value="custom"]')).toContainText('Mine');
  expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe(rgb('#102030'));
  expect(await page.locator('.cm-editor').evaluate((e) => getComputedStyle(e).backgroundColor)).toBe(rgb('#102030'));

  await page.reload();
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await expect(page.locator('#theme')).toHaveValue('custom');
  expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe(rgb('#102030'));
  // the built-in themes are still there
  await page.selectOption('#theme', 'monokai');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'monokai');
  await page.selectOption('#theme', 'custom');
  expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe(rgb('#102030'));
});

test('a theme file that is not usable changes nothing and says why', async ({ page }) => {
  await ready(page);
  await page.selectOption('#theme', 'dracula');
  await page.setInputFiles('#open-theme', fileOf('broken.toml', 'name = "Broken"\n[colors]\nbackground = "red"\n'));
  await expect(page.locator('#status')).toContainText('broken.toml is not a theme');
  await expect(page.locator('#status')).toContainText('background');
  await expect(page.locator('#theme')).toHaveValue('dracula');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dracula');
});
