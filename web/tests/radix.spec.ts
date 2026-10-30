// The radix in which integers are shown: the formatting, which is checked with
// the table that the library checks too, and the controls of the page.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { type Page, expect, test } from '@playwright/test';
import type { TraceNode } from '../src/doe.js';
import { RADIXES, fieldKey, formatInteger, integerText, widthOf } from '../src/radix.js';

const repo = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

interface Row {
  value: string;
  bits: number | null;
  dec: string;
  hex: string;
  bin: string;
  oct: string;
}

const table = JSON.parse(readFileSync(join(repo, 'crates', 'doe', 'tests', 'radix.json'), 'utf8')) as Row[];

function node(name: string, extra: Partial<TraceNode> = {}): TraceNode {
  return { name, type: null, start: null, end: null, instance: false, failed: false, processed: false, summary: null, integer: null, children: [], ...extra };
}

// ── no browser ───────────────────────────────────────────────────────────────

test('integers are written as the library writes them, with the table the library is tested with', () => {
  expect(table.length).toBeGreaterThan(10);
  for (const row of table) {
    for (const radix of RADIXES) {
      expect(formatInteger(row.value, row.bits, radix.id), `${row.value} in ${row.bits} bits as ${radix.id}`).toBe(row[radix.id]);
    }
  }
});

test('a 64-bit value is exact: it does not go through a JavaScript number', () => {
  // 2^53 + 1 is not a double
  expect(Number('9007199254740993').toString()).not.toBe('9007199254740993');
  expect(formatInteger('9007199254740993', 64, 'dec')).toBe('9007199254740993');
  expect(formatInteger('9007199254740993', 64, 'hex')).toBe('0x0020000000000001');
  expect(formatInteger('18446744073709551615', 64, 'hex')).toBe('0xffffffffffffffff');
  expect(formatInteger('-9223372036854775808', 64, 'hex')).toBe('-0x8000000000000000');
});

test('an enum member keeps its name and has its number in the radix, and nothing else changes', () => {
  const member = node('kind', { start: 0, end: 8, integer: { value: '1', name: 'text' } });
  expect(integerText(member, 'hex')).toBe('text (0x01)');
  expect(integerText(member, 'dec')).toBe('text (1)');
  const unnamed = node('kind', { start: 0, end: 8, integer: { value: '9', name: null } });
  expect(integerText(unnamed, 'bin')).toBe('0b00001001');
  // not an integer: no text of its own, the tree shows the summary as it was
  expect(integerText(node('name', { summary: '"hi"' }), 'hex')).toBeNull();
  // a value that was not read from the input has no width: no padding
  expect(integerText(node('total', { instance: true, integer: { value: '5', name: null } }), 'hex')).toBe('0x5');
  expect(widthOf(node('x', { start: 8, end: 11 }))).toBe(3);
  expect(widthOf(node('x'))).toBeNull();
});

test('a field is kept by its type and name, and every element of a list is the field of the list', () => {
  const root = node('top', { type: 'top' });
  const list = node('items', { type: null });
  const item = node('[3]');
  const inner = node('len');
  const itemType = node('[0]', { type: 'top::item' });
  expect(fieldKey('', root, list)).toBe('top.items');
  expect(fieldKey('top.items', list, item)).toBe('top.items[]');
  expect(fieldKey('top.items', list, node('[0]'))).toBe(fieldKey('top.items', list, node('[7]')));
  expect(fieldKey('top.items[]', itemType, inner)).toBe('top::item.len');
});

// ── in the page ──────────────────────────────────────────────────────────────

const SCHEMA = [
  'meta:',
  '  id: numbers',
  '  endian: le',
  'seq:',
  '  - id: a',
  '    type: u2',
  '  - id: b',
  '    type: s1',
  '  - id: big',
  '    type: u8',
  '  - id: max',
  '    type: u8',
  '  - id: list',
  '    type: u1',
  '    repeat: expr',
  '    repeat-expr: 3',
  '  - id: kind',
  '    type: u1',
  '    enum: kinds',
  '  - id: ratio',
  '    type: f4',
  '  - id: name',
  '    type: str',
  '    size: 2',
  '    encoding: ascii',
  'enums:',
  '  kinds:',
  '    1: text',
  '',
].join('\n');

// a = 31, b = -1, big = 2^53 + 1, max = 2^64 - 1, list = 1 2 3, kind = 1, ratio = 1.5, name = "hi"
const INPUT = Buffer.from([0x1f, 0x00, 0xff, 0x01, 0, 0, 0, 0, 0, 0x20, 0, ...Array<number>(8).fill(0xff), 1, 2, 3, 1, 0x00, 0x00, 0xc0, 0x3f, 0x68, 0x69]);

async function ready(page: Page): Promise<void> {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
}

async function load(page: Page): Promise<void> {
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText(SCHEMA);
  await page.setInputFiles('#open-input', { name: 'in.bin', mimeType: 'application/octet-stream', buffer: INPUT });
  await expect(page.locator('#status')).toContainText('parsed as numbers', { timeout: 10_000 });
  await page.locator('.tree-row', { has: page.locator('.name', { hasText: /^list$/ }) }).locator('.toggle').click();
}

const summary = (page: Page, name: string) =>
  page.locator('.tree-row', { has: page.locator('.name', { hasText: new RegExp(`^${name}$`) }) }).locator('.summary');

test('integers are decimal at first, and every radix of the header shows them with the width of their type', async ({ page }) => {
  await ready(page);
  await load(page);
  await expect(page.locator('#radix')).toHaveValue('dec');
  await expect(summary(page, 'a')).toHaveText('31');
  await expect(summary(page, 'big')).toHaveText('9007199254740993');
  await expect(summary(page, 'max')).toHaveText('18446744073709551615');
  await expect(summary(page, 'b')).toHaveText('-1');

  await page.selectOption('#radix', 'hex');
  await expect(summary(page, 'a')).toHaveText('0x001f');
  await expect(summary(page, 'b')).toHaveText('-0x01');
  await expect(summary(page, 'big')).toHaveText('0x0020000000000001');
  await expect(summary(page, 'max')).toHaveText('0xffffffffffffffff');
  await expect(summary(page, 'kind')).toHaveText('text (0x01)');
  await expect(summary(page, '\\[2\\]')).toHaveText('0x03');
  // floats and strings are not changed
  await expect(summary(page, 'ratio')).toHaveText('1.5');
  await expect(summary(page, 'name')).toHaveText('"hi"');

  await page.selectOption('#radix', 'bin');
  await expect(summary(page, 'a')).toHaveText('0b0000000000011111');
  await page.selectOption('#radix', 'oct');
  await expect(summary(page, 'a')).toHaveText('0o000037');
  await page.selectOption('#radix', 'dec');
  await expect(summary(page, 'a')).toHaveText('31');
});

test('the choice of Integers is remembered', async ({ page }) => {
  await ready(page);
  await page.selectOption('#radix', 'hex');
  await page.reload();
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await expect(page.locator('#radix')).toHaveValue('hex');
});

test('a field can have a radix of its own, for every element, and it is remembered with the schema', async ({ page }) => {
  await ready(page);
  await load(page);
  // the control is for integers only
  await expect(page.locator('#field-radix-box')).toBeHidden();
  await summary(page, 'ratio').click();
  await expect(page.locator('#field-radix-box')).toBeHidden();

  await summary(page, '\\[0\\]').click();
  await expect(page.locator('#field-radix-box')).toBeVisible();
  await expect(page.locator('#field-radix')).toHaveValue('');
  // it is in the title bar, just left of Integers
  const [field, global] = await Promise.all([page.locator('#field-radix-box').boundingBox(), page.locator('#radix-box').boundingBox()]);
  const above = (field?.y ?? 0) + (field?.height ?? 0) <= (global?.y ?? 0) + 1;
  const left = (field?.x ?? 0) + (field?.width ?? 0) <= (global?.x ?? 0) + 1;
  expect(above || left).toBe(true);
  await page.selectOption('#field-radix', 'bin');
  for (const [index, text] of ['0b00000001', '0b00000010', '0b00000011'].entries()) {
    await expect(summary(page, `\\[${index}\\]`)).toHaveText(text);
  }
  // the other fields stay as they were, and the global choice does not take the field's own away
  await expect(summary(page, 'a')).toHaveText('31');
  await page.selectOption('#radix', 'hex');
  await expect(summary(page, 'a')).toHaveText('0x001f');
  await expect(summary(page, '\\[1\\]')).toHaveText('0b00000010');

  // taking the field's own choice back puts it in the global radix again
  await page.selectOption('#field-radix', '');
  await expect(summary(page, '\\[1\\]')).toHaveText('0x02');
  await page.selectOption('#field-radix', 'oct');

  // after a reload, with the same schema, the field is still octal
  await page.reload();
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await page.setInputFiles('#open-input', { name: 'in.bin', mimeType: 'application/octet-stream', buffer: INPUT });
  await expect(page.locator('#status')).toContainText('parsed as numbers', { timeout: 10_000 });
  await page.locator('.tree-row', { has: page.locator('.name', { hasText: /^list$/ }) }).locator('.toggle').click();
  await expect(summary(page, '\\[2\\]')).toHaveText('0o003');
  await expect(summary(page, 'a')).toHaveText('0x001f');
});
