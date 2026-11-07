// Markdown entries of an archive, rendered: the constructs, the switch and its key, the limits, and above all that the text
// of an untrusted archive can neither put HTML in the page nor make the page ask for anything.

import { type Page, expect, test } from '@playwright/test';
import { languageOfFence } from '../src/languages.js';
import { tarOf } from './fixture.js';

async function open(page: Page, files: Record<string, string | Uint8Array>): Promise<void> {
  await page.goto('/');
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await page.setInputFiles('#open-input', { name: 'sample.tar', mimeType: 'application/x-tar', buffer: tarOf(files) });
  await expect(page.locator('#detect')).toBeEnabled();
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText('tar parses the file', { timeout: 30_000 });
}

const row = (page: Page, name: string) => page.locator('#files-list .file-row', { hasText: name });
const rendered = (page: Page) => page.locator('#files-preview-rendered');

const SAMPLE = [
  '# Title',
  '',
  'Some *emphasis*, **strong**, ~~gone~~, `code` and a [link](https://example.test/a \"t\").',
  '',
  '- one',
  '  - nested',
  '- [x] done',
  '- [ ] todo',
  '',
  '3. three',
  '4. four',
  '',
  '> quoted',
  '',
  '| a | b |',
  '|---|---|',
  '| 1 | 2 |',
  '',
  '```rust',
  'fn main() { let n = 42; }',
  '```',
  '',
  '---',
  '',
  'Setext',
  '------',
  '',
  '![alt text](https://example.test/i.png)',
  '',
].join('\n');

test('the language of a fence is told from its first word', () => {
  expect(languageOfFence('rust')).toBe('rust');
  expect(languageOfFence('RS')).toBe('rust');
  expect(languageOfFence('js')).toBe('javascript');
  expect(languageOfFence('sh')).toBe('shell');
  expect(languageOfFence('yml')).toBe('yaml');
  expect(languageOfFence('.py')).toBe('python');
  expect(languageOfFence('')).toBeNull();
  expect(languageOfFence('klingon')).toBeNull();
});

test('a Markdown entry has a Source and Rendered switch, and the rendering has the constructs', async ({ page }) => {
  await open(page, { 'README.md': SAMPLE, 'main.rs': 'fn main() {}\n' });
  await row(page, 'main.rs').click();
  // not Markdown: no switch
  await expect(page.locator('#files-view')).toBeHidden();
  await row(page, 'README.md').click();
  await expect(page.locator('#files-view')).toBeVisible();
  await expect(page.locator('#files-view button[data-view="source"]')).toHaveAttribute('aria-pressed', 'true');
  // the source is the default, and highlighted
  await expect(page.locator('#files-preview-text')).toBeVisible();
  await expect(rendered(page)).toBeHidden();
  await page.locator('#files-view button[data-view="rendered"]').click();
  await expect(rendered(page)).toBeVisible();
  await expect(page.locator('#files-preview-text')).toBeHidden();
  const md = rendered(page);
  await expect(md.locator('h1')).toHaveText('Title');
  await expect(md.locator('h2')).toHaveText('Setext');
  await expect(md.locator('em')).toHaveText('emphasis');
  await expect(md.locator('strong')).toHaveText('strong');
  await expect(md.locator('del')).toHaveText('gone');
  await expect(md.locator('code.md-code')).toHaveText('code');
  // a link is text with its target written out
  await expect(md.locator('.md-link')).toHaveText('link');
  await expect(md.locator('.md-url')).toHaveText(' <https://example.test/a>');
  await expect(md.locator('ul > li')).toHaveCount(4);
  await expect(md.locator('ul ul li')).toHaveText('nested');
  await expect(md.locator('.md-task')).toHaveText(['☑ ', '☐ ']);
  await expect(md.locator('ol')).toHaveAttribute('start', '3');
  await expect(md.locator('ol li')).toHaveText(['three', 'four']);
  await expect(md.locator('blockquote')).toContainText('quoted');
  await expect(md.locator('table th')).toHaveText(['a', 'b']);
  await expect(md.locator('table td')).toHaveText(['1', '2']);
  await expect(md.locator('hr')).toHaveCount(1);
  await expect(md.locator('.md-image')).toHaveText('[image: alt text]');
  await expect(md.locator('.md-image')).toHaveAttribute('title', 'https://example.test/i.png');
  // the code block is highlighted with the language of the fence
  await expect(md.locator('pre.md-pre code span').first()).toBeVisible();
  expect(await md.locator('pre.md-pre code').textContent()).toBe('fn main() { let n = 42; }');
  // and back to the source
  await page.locator('#files-view button[data-view="source"]').click();
  await expect(page.locator('#files-preview-text')).toBeVisible();
  await expect(rendered(page)).toBeHidden();
});

test('the key v switches, the choice is kept for the next Markdown entry and for a reload of the page, and a file that is not Markdown says so', async ({ page }) => {
  await open(page, { 'a.md': '# A\n', 'b.markdown': '# B\n', 'c.txt': 'plain\n' });
  await row(page, 'a.md').click();
  await page.locator('#files-list').focus();
  await page.keyboard.press('v');
  await expect(rendered(page).locator('h1')).toHaveText('A');
  // the next Markdown entry is rendered at once
  await row(page, 'b.markdown').click();
  await expect(rendered(page).locator('h1')).toHaveText('B');
  await row(page, 'c.txt').click();
  await expect(page.locator('#files-view')).toBeHidden();
  await page.locator('#files-list').focus();
  await page.keyboard.press('v');
  await expect(page.locator('#status')).toContainText('not Markdown');
  // remembered for the session: a reload keeps it
  await page.reload();
  await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  await page.setInputFiles('#open-input', { name: 'sample.tar', mimeType: 'application/x-tar', buffer: tarOf({ 'a.md': '# A\n' }) });
  await expect(page.locator('#detect')).toBeEnabled();
  await page.click('#detect');
  await expect(page.locator('#detect-report')).toContainText('tar parses the file', { timeout: 30_000 });
  await row(page, 'a.md').click();
  await expect(rendered(page).locator('h1')).toHaveText('A');
  await page.locator('#files-list').focus();
  await page.keyboard.press('v');
  await expect(page.locator('#files-preview-text')).toBeVisible();
});

test('raw HTML, javascript: links and images with remote URLs are inert text, and nothing is requested', async ({ page }) => {
  const requests: string[] = [];
  const dialogs: string[] = [];
  page.on('request', (request) => requests.push(request.url()));
  page.on('dialog', (dialog) => {
    dialogs.push(dialog.message());
    void dialog.dismiss();
  });
  const evil = [
    '# <img src="https://evil.test/h.png" onerror="alert(1)">',
    '',
    '<script>alert("script")</script>',
    '',
    '<img src="https://evil.test/b.png" onerror="alert(2)"> inline <b onmouseover="alert(3)">bold</b>',
    '',
    '[click](javascript:alert(4)) and [data](data:text/html,<script>alert(5)</script>)',
    '',
    '![pixel](https://evil.test/p.png) <https://evil.test/auto>',
    '',
    '<iframe src="https://evil.test/f"></iframe>',
    '',
    '[ref]: https://evil.test/ref',
    '',
    '![via ref][ref] &lt;script&gt; &#60;b&#62;',
    '',
  ].join('\n');
  await open(page, { 'evil.md': evil });
  const before = requests.length;
  await row(page, 'evil.md').click();
  await page.locator('#files-view button[data-view="rendered"]').click();
  const md = rendered(page);
  await expect(md).toBeVisible();
  // the text is there, as text
  await expect(md).toContainText('<script>alert("script")</script>');
  await expect(md).toContainText('javascript:alert(4)');
  await expect(md).toContainText('https://evil.test/auto');
  await expect(md.locator('.md-image').first()).toHaveAttribute('title', 'https://evil.test/p.png');
  // and no element that can load, run or navigate
  expect(await md.locator('img, script, iframe, a, link, object, embed, form, input, video, audio, svg').count()).toBe(0);
  // no attribute of an event handler or a source anywhere in it
  const attributes = await md.evaluate((root) => {
    const names: string[] = [];
    for (const element of [root, ...root.querySelectorAll('*')]) for (const attribute of element.getAttributeNames()) names.push(attribute);
    return [...new Set(names)].sort();
  });
  expect(attributes.filter((name) => name.startsWith('on') || name === 'src' || name === 'href' || name === 'srcdoc')).toEqual([]);
  // the entities are decoded to text, not to elements
  await expect(md).toContainText('<script> <b>');
  await page.waitForTimeout(500);
  expect(requests.slice(before).filter((url) => url.includes('evil.test'))).toEqual([]);
  expect(dialogs).toEqual([]);
});

test('with the network cut, the rendering still works and asks for nothing outside the page', async ({ page, context }) => {
  await open(page, { 'README.md': SAMPLE });
  const outside: string[] = [];
  await context.route('**/*', (route) => {
    const url = new URL(route.request().url());
    if (url.hostname !== '127.0.0.1') outside.push(url.href);
    return route.continue();
  });
  await row(page, 'README.md').click();
  await page.locator('#files-view button[data-view="rendered"]').click();
  await expect(rendered(page).locator('h1')).toHaveText('Title');
  await expect(rendered(page).locator('pre.md-pre code span').first()).toBeVisible();
  expect(outside).toEqual([]);
});

test('a deeply nested or very large file is capped and the page stays usable', async ({ page }) => {
  const quotes = `${'> '.repeat(900)}deep\n`;
  const lists = Array.from({ length: 200 }, (_, i) => `${'  '.repeat(i)}- item ${i}`).join('\n');
  const table = `| a | b |\n|---|---|\n${'| 1 | 2 |\n'.repeat(8000)}`;
  // lines of ten emphasised words: each line is short (a longer one is cut), and the entry is cut at 100,000 bytes
  const many = Array.from({ length: 4000 }, () => '*a* *b* *c* *d* *e* *f* *g* *h* *i* *j*').join('\n');
  await open(page, { 'quotes.md': quotes, 'lists.md': lists, 'table.md': table, 'many.md': many });
  await row(page, 'quotes.md').click();
  const start = Date.now();
  await page.locator('#files-view button[data-view="rendered"]').click();
  await expect(rendered(page)).toBeVisible();
  // nothing is deeper than the limit, and the text of what is too deep is still there
  expect(await rendered(page).evaluate((root) => Math.max(...[...root.querySelectorAll('blockquote')].map((q) => { let n = 0; for (let e: Element | null = q; e && e !== root; e = e.parentElement) n += 1; return n; })))).toBeLessThan(80);
  await expect(rendered(page)).toContainText('deep');
  await row(page, 'lists.md').click();
  await expect(rendered(page)).toContainText('item 199');
  await row(page, 'table.md').click();
  await expect(rendered(page).locator('.md-note')).toContainText('too large to render');
  await row(page, 'many.md').click();
  await expect(rendered(page).locator('.md-note')).toContainText('too large to render');
  // the page answers: the source is one click away and shows the text
  await page.locator('#files-view button[data-view="source"]').click();
  await expect(page.locator('#files-preview-text')).toBeVisible();
  expect(Date.now() - start).toBeLessThan(25_000);
});
