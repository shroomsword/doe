// The preview of a text file of an archive in the Files view: the language from the name or a #! line, the colours of the
// themes, the badge that overrides the guess, and the limits.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { type Page, expect, test } from '@playwright/test';
import { LINE_LIMIT, PREVIEW_BYTES, cutLines, textOfCut } from '../src/files.js';
import { EXTENSIONS, LANGUAGES, NAMES, detectLanguage, fromShebang } from '../src/languages.js';
import { tarOf } from './fixture.js';

async function open(page: Page, files: Record<string, string | Uint8Array>, afterLoad?: () => Promise<void>): Promise<void> {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await afterLoad?.();
  await page.setInputFiles('#open-input', { name: 'sample.tar', mimeType: 'application/x-tar', buffer: tarOf(files) });
  await expect(page.locator('#detect')).toBeEnabled();
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText('tar parses the file', { timeout: 30_000 });
}

const row = (page: Page, name: string) => page.locator('#files-list .file-row', { hasText: name });
const text = (page: Page) => page.locator('#files-preview-text');

/** The colours of the tokens (the spans) of the preview. */
async function colours(page: Page): Promise<string[]> {
  return text(page).locator('span').evaluateAll((spans) => spans.map((s) => getComputedStyle(s).color));
}

const RUST = 'fn main() {\n    // say hello\n    let n = 42;\n    println!("hello {n}");\n}\n';

test('the language comes from the name, from the file name, and from a #! line; an unknown file stays plain', () => {
  expect(detectLanguage('main.rs', '')).toBe('rust');
  expect(detectLanguage('Main.PY', '')).toBe('python');
  expect(detectLanguage('a.tar.gz.json', '')).toBe('json');
  expect(detectLanguage('README.md', '')).toBe('markdown');
  expect(detectLanguage('Makefile', '')).toBe('shell');
  expect(detectLanguage('Dockerfile', '')).toBe('dockerfile');
  expect(detectLanguage('Dockerfile.dev', '')).toBe('dockerfile');
  expect(detectLanguage('.bashrc', '')).toBe('shell');
  expect(detectLanguage('run', '#!/bin/sh\necho hi\n')).toBe('shell');
  expect(detectLanguage('run', '#!/usr/bin/env python3\nprint(1)\n')).toBe('python');
  expect(detectLanguage('run', '#!/usr/bin/env -S node --flag\n')).toBe('javascript');
  // the name wins over the line
  expect(detectLanguage('x.rs', '#!/bin/sh\n')).toBe('rust');
  expect(detectLanguage('notes.xyz', 'just words\n')).toBeNull();
  expect(detectLanguage('LICENSE', 'Permission is hereby granted\n')).toBeNull();
  expect(fromShebang('#!/usr/bin/perl\n')).toBeNull();
  expect(fromShebang('# not a shebang\n')).toBeNull();
});

test('the table of the terminal viewer has the same languages, extensions and file names', () => {
  const rust = readFileSync(join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'crates', 'doe-tui', 'src', 'languages.rs'), 'utf8');
  /** The pairs `("a", "b")` of a `pub const NAME: ... = &[ ... ];` of the Rust file. */
  const pairs = (name: string): [string, string][] => {
    const body = new RegExp(`pub const ${name}:[^=]*=\\s*&\\[([\\s\\S]*?)\\n\\];`).exec(rust)?.[1] ?? '';
    return [...body.matchAll(/\(\s*"([^"]+)"\s*,\s*"([^"]+)"/g)].map((m) => [m[1]!, m[2]!]);
  };
  expect(Object.fromEntries(pairs('EXTENSIONS'))).toEqual(EXTENSIONS);
  expect(Object.fromEntries(pairs('NAMES'))).toEqual(NAMES);
  expect(pairs('LANGUAGES')).toEqual(LANGUAGES.map((l) => [l.id, l.name]));
  expect(Object.keys(EXTENSIONS).length).toBeGreaterThan(30);
});

test('a long line is cut, and so is a text cut inside a character', () => {
  const long = `${'a'.repeat(LINE_LIMIT + 50)}\nshort\n`;
  const cut = cutLines(long);
  expect(cut.split('\n')[0]).toBe(`${'a'.repeat(LINE_LIMIT)}…`);
  expect(cut.split('\n')[1]).toBe('short');
  expect(cutLines('short\n')).toBe('short\n');
  // 'é' is two bytes: cut after the first, the character is dropped and the rest is text
  const bytes = new TextEncoder().encode('abcé');
  expect(textOfCut(bytes.subarray(0, 4))).toBe('abc');
  expect(textOfCut(new Uint8Array([0xff, 0xfe, 0xfd, 0xfc, 0xfb]))).toBeNull();
});

test('a Rust file is highlighted, the badge shows the guess, and the page offers the languages', async ({ page }) => {
  await open(page, { 'main.rs': RUST });
  await row(page, 'main.rs').click();
  await expect(text(page)).toContainText('println!');
  await expect(text(page)).toHaveAttribute('data-language', 'rust');
  // the text of the element is the text, unchanged
  expect(await text(page).textContent()).toBe(RUST);
  await expect(text(page).locator('span').first()).toBeVisible();
  const spans = await colours(page);
  const plain = await text(page).evaluate((el) => getComputedStyle(el).color);
  expect(spans.length).toBeGreaterThan(3);
  expect(spans.some((c) => c !== plain)).toBe(true);
  const badge = page.locator('#files-language');
  await expect(badge).toBeVisible();
  await expect(badge.locator('option').first()).toHaveText('Auto: Rust');
  const options = await badge.locator('option').allTextContents();
  for (const name of ['Plain', 'C', 'C++', 'CSS', 'HTML', 'JSON', 'JavaScript', 'Markdown', 'Python', 'Shell', 'TOML', 'XML', 'YAML']) expect(options).toContain(name);
});

test('json, python, markdown, shell, a Makefile and a file with a #! line and no extension are highlighted', async ({ page }) => {
  await open(page, {
    'a.json': '{"name": "doe", "n": 42, "ok": true}\n',
    'b.py': 'def f(x):\n    # comment\n    return "s" + str(x)\n',
    'c.md': '# Title\n\nSome *words* and `code`.\n',
    'd.sh': 'for f in *.c; do echo "$f"; done # done\n',
    Makefile: 'all: main.o\n\tcc -o main main.o # build\n',
    run: '#!/usr/bin/env python3\nprint("hi")  # hello\n',
  });
  for (const [name, language] of [
    ['a.json', 'json'],
    ['b.py', 'python'],
    ['c.md', 'markdown'],
    ['d.sh', 'shell'],
    ['Makefile', 'shell'],
    ['run', 'python'],
  ] as const) {
    await row(page, name).click();
    await expect(text(page)).toHaveAttribute('data-language', language);
    await expect(text(page).locator('span').first()).toBeVisible();
    const plain = await text(page).evaluate((el) => getComputedStyle(el).color);
    expect((await colours(page)).some((c) => c !== plain), `${name} has a coloured token`).toBe(true);
  }
});

test('the colours follow the theme: light, dark and high contrast', async ({ page }) => {
  await open(page, { 'main.rs': RUST });
  await row(page, 'main.rs').click();
  await expect(text(page).locator('span').first()).toBeVisible();
  const seen: Record<string, string> = {};
  for (const id of ['doe-light', 'doe-dark', 'high-contrast']) {
    // through the dialog of the theme button, which covers the page until it is closed
    await page.locator('#theme-button').click();
    await page.selectOption('#theme', id);
    await page.locator('#theme-close').click();
    await expect(page.locator('#theme-dialog')).toBeHidden();
    await expect(text(page).locator('span').first()).toBeVisible();
    const plain = await text(page).evaluate((el) => getComputedStyle(el).color);
    const spans = await colours(page);
    expect(spans.some((c) => c !== plain), `${id} colours the tokens`).toBe(true);
    seen[id] = spans.join(' ');
  }
  expect(new Set(Object.values(seen)).size).toBe(3);
});

test('a file of an unknown type is plain, and the badge overrides the guess and goes back to it', async ({ page }) => {
  await open(page, { 'notes.xyz': 'fn main() { let x = 1; }\n', 'main.rs': RUST });
  await row(page, 'notes.xyz').click();
  await expect(text(page)).toContainText('fn main');
  await expect(text(page)).toHaveAttribute('data-language', 'plain');
  await expect(text(page).locator('span')).toHaveCount(0);
  const badge = page.locator('#files-language');
  await expect(badge.locator('option').first()).toHaveText('Auto: plain');
  await badge.selectOption('rust');
  await expect(text(page)).toHaveAttribute('data-language', 'rust');
  await expect(text(page).locator('span').first()).toBeVisible();
  expect(await text(page).textContent()).toBe('fn main() { let x = 1; }\n');
  // the choice is for this entry: another one is guessed again, and so is this one when it is chosen again
  await row(page, 'main.rs').click();
  await expect(text(page)).toHaveAttribute('data-language', 'rust');
  await expect(badge).toHaveValue('');
  await row(page, 'notes.xyz').click();
  await expect(badge).toHaveValue('rust');
  // and back to the guess, and to plain
  await badge.selectOption('');
  await expect(text(page)).toHaveAttribute('data-language', 'plain');
  await row(page, 'main.rs').click();
  await badge.selectOption('plain');
  await expect(text(page)).toHaveAttribute('data-language', 'plain');
  await expect(text(page).locator('span')).toHaveCount(0);
});

test('an entry over the limit is cut with a mark, a line that is too long is cut, and binary stays binary', async ({ page }) => {
  const big = `${'let x = 1; // line\n'.repeat(Math.ceil(PREVIEW_BYTES / 19) + 100)}`;
  const minified = `${'x'.repeat(LINE_LIMIT * 3)}\nend\n`;
  await open(page, { 'big.rs': big, 'min.js': minified, 'blob.rs': new Uint8Array([0, 1, 2, 3, 0xff]) });
  await row(page, 'big.rs').click();
  await expect(text(page)).toHaveAttribute('data-language', 'rust');
  await expect(text(page)).toContainText('…');
  const shown = (await text(page).textContent()) ?? '';
  expect(shown.endsWith('\n…')).toBe(true);
  expect(shown.length).toBeLessThan(big.length);
  await row(page, 'min.js').click();
  const line = ((await text(page).textContent()) ?? '').split('\n')[0] ?? '';
  expect(line).toBe(`${'x'.repeat(LINE_LIMIT)}…`);
  await row(page, 'blob.rs').click();
  await expect(text(page)).toHaveText('5 bytes (not text)');
  await expect(page.locator('#files-language')).toBeHidden();
});

test('when the parser cannot be loaded the text stays plain', async ({ page }) => {
  // the page is up (its own chunks are loaded); the chunk of the language is not, and is asked for later
  await open(page, { 'main.rs': RUST }, async () => {
    await page.route('**/chunks/**', (route) => route.abort());
  });
  await row(page, 'main.rs').click();
  await expect(text(page)).toContainText('println!');
  await page.waitForTimeout(300);
  await expect(text(page).locator('span')).toHaveCount(0);
  expect(await text(page).textContent()).toBe(RUST);
});
