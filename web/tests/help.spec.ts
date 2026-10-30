// The help: the web IDE and schema sections of the README, in a dialog.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, test } from '@playwright/test';
import { SECTIONS, buildHelp } from '../help.mjs';

const readme = readFileSync(join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'README.md'), 'utf8');

test('the build makes the help from every listed section of the README, and fails when one is missing', () => {
  const { headings } = buildHelp(readme);
  for (const title of SECTIONS) {
    expect(headings.some((heading) => heading.level === 2 && heading.text === title), title).toBe(true);
  }
  // only those sections: no other `##`
  expect(headings.filter((heading) => heading.level === 2).map((heading) => heading.text)).toEqual(SECTIONS);
  expect(headings.some((heading) => heading.text === 'CLI reference')).toBe(false);
  expect(() => buildHelp(readme, [...SECTIONS, 'No such section'])).toThrow(/No such section/);
  expect(() => buildHelp(readme.replace('## Web IDE', '## The IDE'))).toThrow(/Web IDE/);
});

test('links to sections that are left out are text, and links elsewhere open in a new tab', () => {
  const { html } = buildHelp('## A\n\nSee [the CLI](#cli-reference), [here](#a), [site](https://example.org/x) and [file](types/README.md).\n', ['A']);
  expect(html).not.toContain('href="#cli-reference"');
  expect(html).toContain('<span class="help-gone">the CLI</span>');
  expect(html).toContain('href="#help-a"');
  expect(html).toMatch(/href="https:\/\/example.org\/x"[^>]*target="_blank" rel="noopener noreferrer"/);
  expect(html).toContain('github.com/shroomsword/doe/blob/main/types/README.md');
});

test.describe('in the page', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => (window as unknown as { doe?: { ready: boolean } }).doe?.ready === true);
  });

  test('? opens the help, and Close and Esc close it', async ({ page }) => {
    const help = page.getByRole('button', { name: 'Help' });
    await expect(page.locator('header #help')).toHaveCount(1);
    await help.click();
    await expect(page.locator('#help-dialog')).toBeVisible();
    await expect(page.locator('.help-text h2').first()).toHaveText('Web IDE');
    await page.locator('#help-close').click();
    await expect(page.locator('#help-dialog')).toBeHidden();
    await help.click();
    await expect(page.locator('#help-dialog')).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.locator('#help-dialog')).toBeHidden();
  });

  test('the contents list the headings of the included sections and only those; a click goes to the section', async ({ page }) => {
    await page.getByRole('button', { name: 'Help' }).click();
    const toc = page.locator('.help-toc a');
    await expect(toc.first()).toHaveText('Web IDE');
    const names = await toc.allTextContents();
    expect(names).toContain('Schema format');
    expect(names).toContain('Kaitai Struct compatibility');
    expect(names).not.toContain('CLI reference');
    expect(names).not.toContain('Installation');
    // the headings of the dialog all have an anchor, and the contents lead to them
    await page.locator('.help-toc a', { hasText: 'Kaitai Struct compatibility' }).first().click();
    await expect(page.locator('#help-kaitai-struct-compatibility')).toBeInViewport();
    // the page's address does not change
    expect(new URL(page.url()).hash).toBe('');
  });

  test('an internal link scrolls inside the dialog, a link to a left-out section is not a link, and external links open in a new tab', async ({ page }) => {
    await page.getByRole('button', { name: 'Help' }).click();
    await expect(page.locator('.help-text')).toBeVisible();
    const internal = page.locator('.help-text a[data-help]');
    expect(await internal.count()).toBeGreaterThan(0);
    const anchors = await page.locator('.help-text a[href^="#"]').evaluateAll((links) => links.map((link) => link.getAttribute('href') ?? ''));
    for (const href of anchors) expect(await page.locator(`[id="${href.slice(1)}"]`).count(), href).toBe(1);
    // no link to a section of the README that is not in the help
    expect(await page.locator('.help-text a[href="#cli-reference"], .help-text a[href="#type-library"]').count()).toBe(0);
    const external = page.locator('.help-text a[href^="http"]');
    expect(await external.count()).toBeGreaterThan(0);
    for (const link of await external.all()) {
      await expect(link).toHaveAttribute('target', '_blank');
      await expect(link).toHaveAttribute('rel', /noopener/);
    }
  });

  test('code blocks scroll and tables scroll sideways, and the page does not on a phone', async ({ page }) => {
    await page.setViewportSize({ width: 360, height: 700 });
    await page.getByRole('button', { name: 'Help' }).click();
    await expect(page.locator('.help-text pre').first()).toBeAttached();
    await expect(page.locator('.help-table').first()).toBeAttached();
    expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBeLessThanOrEqual(0);
    const overflow = await page.locator('#help-body').evaluate((body) => body.scrollWidth - body.clientWidth);
    expect(overflow).toBeLessThanOrEqual(0);
  });
});
