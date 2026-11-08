// The bytes of the Bytes pane, drawn in the colour of the field they belong to (the colour its value has in the tree), with
// the selection unchanged over them.

import { expect, test } from '@playwright/test';
import type { TraceNode } from '../src/doe.js';
import { buildByteKinds, kindOfNode } from '../src/kinds.js';
import { SELECTED_CONTRAST, THEMES, contrast, readableOn } from '../src/theme.js';
import { loadExample } from './fixture.js';

const leaf = (name: string, startByte: number, endByte: number, summary: string | null, extra: Partial<TraceNode> = {}): TraceNode => ({
  name,
  type: null,
  start: startByte * 8,
  end: endByte * 8,
  instance: false,
  failed: false,
  problem: null,
  processed: false,
  summary,
  integer: null,
  children: [],
  ...extra,
});

const parent = (name: string, children: TraceNode[], extra: Partial<TraceNode> = {}): TraceNode => ({
  ...leaf(name, 0, 0, null, extra),
  start: children[0]?.start ?? null,
  end: children[children.length - 1]?.end ?? null,
  children,
});

test('the kind of a node is the one the tree colours its value with', () => {
  expect(kindOfNode(leaf('a', 0, 1, '5'))).toBe('number');
  expect(kindOfNode(leaf('a', 0, 1, '"hi"'))).toBe('string');
  expect(kindOfNode(leaf('a', 0, 4, '<4 bytes: 01 02>'))).toBe('bytes');
  expect(kindOfNode(leaf('a', 0, 1, 'true'))).toBe('boolean');
  expect(kindOfNode(leaf('a', 0, 1, 'x', { integer: { value: '1', name: 'kind' } }))).toBe('enum');
  expect(kindOfNode(leaf('a', 0, 1, '7', { integer: { value: '7', name: null } }))).toBe('number');
  expect(kindOfNode(leaf('a', 0, 1, null))).toBeNull();
});

test('every byte is owned by the leaf field that holds it, a struct by its children, a gap by nothing', () => {
  const root = parent('root', [
    leaf('n', 0, 2, '5'),
    parent('entry', [leaf('k', 2, 3, 'greeting (1)'), leaf('t', 4, 7, '"abc"')]),
    leaf('blob', 8, 10, '<2 bytes: 00 01>'),
    leaf('flag', 10, 11, 'false'),
    leaf('plain', 11, 12, null),
  ]);
  const kinds = buildByteKinds(root);
  const at = (i: number) => kinds.kindAt(i);
  expect([0, 1].map(at)).toEqual(['number', 'number']);
  expect(at(2)).toBe('enum');
  // byte 3 is in the struct but in none of its fields
  expect(at(3)).toBeNull();
  expect([4, 5, 6].map(at)).toEqual(['string', 'string', 'string']);
  expect(at(7)).toBeNull();
  expect([8, 9].map(at)).toEqual(['bytes', 'bytes']);
  expect(at(10)).toBe('boolean');
  // a field with no kind of its own, and bytes past the last field
  expect(at(11)).toBeNull();
  expect(at(12)).toBeNull();
  expect(at(1_000_000)).toBeNull();
});

test('where fields overlap the first of the sequence wins, then the instances; what a transform made is not looked at', () => {
  const root = parent('root', [
    leaf('inst', 0, 6, '"text"', { instance: true }),
    leaf('a', 2, 4, '1'),
    leaf('b', 3, 5, '"s"'),
    leaf('dec', 0, 10, '9', { processed: true }),
  ]);
  const kinds = buildByteKinds(root);
  // bytes 2-3 are a's, byte 3 is also b's (a came first), byte 4 is b's; the instance has the rest of 0-5
  expect([0, 1, 2, 3, 4, 5, 6].map((i) => kinds.kindAt(i))).toEqual(['string', 'string', 'number', 'number', 'string', 'string', null]);
  // bit fields share a byte: the first has it
  const bits = parent('bits', [leaf('a', 0, 0, '1', { start: 0, end: 3 }), leaf('b', 0, 0, 'true', { start: 3, end: 8 })]);
  expect(buildByteKinds(bits).kindAt(0)).toBe('number');
});

test('two hundred thousand leaf fields are indexed in well under a few seconds and looked up quickly (a million are indexed in the terminal viewer\'s test; this machine has little memory for objects)', () => {
  const children = Array.from({ length: 200_000 }, (_, i) => leaf(`f${i}`, i * 2, i * 2 + 2, i % 3 === 0 ? '7' : i % 3 === 1 ? '"s"' : '<2 bytes: 00 00>'));
  const root = parent('root', children);
  const start = performance.now();
  const kinds = buildByteKinds(root);
  const built = performance.now() - start;
  const lookups = performance.now();
  let counted = 0;
  for (let i = 0; i < 100_000; i += 7) if (kinds.kindAt(i * 4) !== null) counted += 1;
  expect(counted).toBeGreaterThan(0);
  expect(kinds.kindAt(0)).toBe('number');
  expect(kinds.kindAt(2)).toBe('string');
  expect(kinds.kindAt(4)).toBe('bytes');
  expect(kinds.size).toBeLessThanOrEqual(200_000);
  expect(built).toBeLessThan(5000);
  expect(performance.now() - lookups).toBeLessThan(1000);
});

test('a value colour is readable on the selection in every built-in theme, by the colour itself or the text colour of the theme', () => {
  expect(THEMES.length).toBeGreaterThan(20);
  const moved: string[] = [];
  for (const theme of THEMES) {
    for (const kind of ['number', 'string', 'bytes', 'enum', 'boolean'] as const) {
      const colour = theme.colors[kind];
      const onSelection = readableOn(colour, theme.colors.selection, theme.colors.foreground);
      const onPart = readableOn(onSelection, theme.colors.selection_part, theme.colors.foreground);
      expect(contrast(onSelection, theme.colors.selection), `${theme.id} ${kind} on the selection`).toBeGreaterThanOrEqual(SELECTED_CONTRAST);
      expect(contrast(onPart, theme.colors.selection_part), `${theme.id} ${kind} on the lighter selection`).toBeGreaterThanOrEqual(SELECTED_CONTRAST);
      if (onPart !== colour) moved.push(`${theme.id}:${kind}`);
    }
  }
  // most pairs are fine as they are: the colour is only replaced where it has to be
  expect(moved.length).toBeLessThan(THEMES.length * 5 * 0.4);
  // a colour that is readable is not touched
  expect(readableOn('#000000', '#ffffff', '#111111')).toBe('#000000');
});

/** Open a node of the tree, the first one of that name. */
const open = async (page: import('@playwright/test').Page, name: string): Promise<void> => {
  await page.locator(`.tree-row:has(> .name:text-is("${name}"))`).first().locator('.toggle').click();
};

/** The colour of the text of an element, as the page computes it. */
const colourOf = (page: import('@playwright/test').Page, selector: string) => page.locator(selector).first().evaluate((el) => getComputedStyle(el).color);

test('the bytes of a field are drawn in the colour the field has in the tree, in light, dark and high-contrast themes', async ({ page }) => {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await loadExample(page);
  const summary = (name: string) => `.tree-row:has(> .name:text-is("${name}")) .summary`;
  await open(page, 'entries');
  await open(page, '[0]');
  for (const id of ['doe-light', 'doe-dark', 'high-contrast']) {
    await page.locator('#theme-button').click();
    await page.selectOption('#theme', id);
    await page.locator('#theme-close').click();
    await expect(page.locator('#theme-dialog')).toBeHidden();
    await expect(page.locator('.hex .b.k-number').first()).toBeVisible();
    // count (byte 3) is a number, kind (byte 4) an enum, len (byte 5) a number, text (bytes 6-10) a string
    expect(await colourOf(page, '.hex .b[data-i="3"]'), `${id} count`).toBe(await colourOf(page, summary('count')));
    expect(await colourOf(page, '.hex .b[data-i="4"]'), `${id} kind`).toBe(await colourOf(page, `.tree-row:has(> .name:text-is("kind")) .summary`));
    expect(await colourOf(page, '.hex .b[data-i="6"]'), `${id} text`).toBe(await colourOf(page, `.tree-row:has(> .name:text-is("text")) .summary`));
    // the character column has the same colour as the hex of its byte
    expect(await colourOf(page, '.hex .a[data-i="6"]')).toBe(await colourOf(page, '.hex .b[data-i="6"]'));
    // and the three kinds are three colours
    const colours = new Set([await colourOf(page, '.hex .b[data-i="3"]'), await colourOf(page, '.hex .b[data-i="4"]'), await colourOf(page, '.hex .b[data-i="6"]')]);
    expect(colours.size).toBe(3);
  }
});

test('the selection is the same as before over the colours: its background, the partly covered bytes, and the kind stays', async ({ page }) => {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await loadExample(page);
  const background = (selector: string) => page.locator(selector).first().evaluate((el) => getComputedStyle(el).backgroundColor);
  const unselected = await background('.hex .b[data-i="3"]');
  expect(unselected).toBe('rgba(0, 0, 0, 0)');
  await page.locator('.tree-row:has(> .name:text-is("count"))').click();
  await expect(page.locator('.hex .b.sel[data-i="3"]')).toHaveClass(/k-number/);
  const selected = await background('.hex .b[data-i="3"]');
  const expected = await page.evaluate(() => {
    const probe = document.createElement('div');
    probe.style.background = 'var(--select)';
    document.body.append(probe);
    const colour = getComputedStyle(probe).backgroundColor;
    probe.remove();
    return colour;
  });
  expect(selected).toBe(expected);
  // the text on the selection is the one that stays readable there
  const text = await colourOf(page, '.hex .b[data-i="3"]');
  const wanted = await page.evaluate(() => {
    const probe = document.createElement('div');
    probe.style.color = 'var(--sel-number)';
    document.body.append(probe);
    const colour = getComputedStyle(probe).color;
    probe.remove();
    return colour;
  });
  expect(text).toBe(wanted);
  // a struct selected: its bytes keep their own kinds, all with the selection background
  await open(page, 'entries');
  await page.locator('.tree-row:has(> .name:text-is("[0]"))').first().click();
  await expect(page.locator('.hex .b.sel[data-i="4"]')).toHaveClass(/k-enum/);
  await expect(page.locator('.hex .b.sel[data-i="6"]')).toHaveClass(/k-string/);
  expect(await background('.hex .b[data-i="6"]')).toBe(expected);
  // a byte outside it has the colour of its kind and no background
  expect(await background('.hex .b[data-i="3"]')).toBe(unselected);
});

test('the button and b turn the colours off and on, and the choice is kept', async ({ page }) => {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await loadExample(page);
  const button = page.locator('#byte-colours');
  await expect(button).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('.hex .b.k-number').first()).toBeVisible();
  await button.click();
  await expect(button).toHaveAttribute('aria-pressed', 'false');
  await expect(page.locator('.hex [class*="k-"]')).toHaveCount(0);
  // the key, with the focus in the bytes
  await page.locator('.hex').focus();
  await page.keyboard.press('b');
  await expect(button).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('.hex .b.k-number').first()).toBeVisible();
  await page.keyboard.press('b');
  await expect(page.locator('.hex [class*="k-"]')).toHaveCount(0);
  // off is remembered
  await page.reload();
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await loadExample(page);
  await expect(page.locator('#byte-colours')).toHaveAttribute('aria-pressed', 'false');
  await expect(page.locator('.hex .b').first()).toBeVisible();
  await expect(page.locator('.hex [class*="k-"]')).toHaveCount(0);
  await page.locator('#byte-colours').click();
  await expect(page.locator('.hex .b.k-number').first()).toBeVisible();
});

test('a file with no schema looks as it did: nothing is coloured', async ({ page }) => {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await page.setInputFiles('#open-input', { name: 'x.bin', mimeType: 'application/octet-stream', buffer: Buffer.from([1, 2, 3, 4]) });
  await expect(page.locator('.hex .b').first()).toBeVisible();
  await expect(page.locator('.hex [class*="k-"]')).toHaveCount(0);
});
