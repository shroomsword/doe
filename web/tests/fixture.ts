// A small schema and a file that fits it, for the tests to work on: the page
// starts with nothing in the editor.

import { deflateRawSync } from 'node:zlib';
import { type Page, expect } from '@playwright/test';

export const EXAMPLE_SCHEMA = `meta:
  id: example
  endian: le
doc: A small example. Edit this schema, or open your own file and schema.
seq:
  - id: magic
    contents: "DOE"
  - id: count
    type: u1
  - id: entries
    type: entry
    repeat: expr
    repeat-expr: count
instances:
  last_text:
    value: entries[count - 1].text
types:
  entry:
    seq:
      - id: kind
        type: u1
        enum: kinds
      - id: len
        type: u1
      - id: text
        type: str
        size: len
        encoding: ASCII
enums:
  kinds:
    1: greeting
    2: farewell
`;

const text = (value: string): number[] => [...value].map((char) => char.charCodeAt(0));

/** "DOE", two entries: a greeting "hello" and a farewell "bye". */
export const EXAMPLE_BYTES = new Uint8Array([...text('DOE'), 2, 1, 5, ...text('hello'), 2, 3, ...text('bye')]);

/** Put the schema in the editor, as typing would, and open the file: what the tests start from. */
export async function loadExample(page: Page): Promise<void> {
  await page.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.insertText(EXAMPLE_SCHEMA);
  await page.setInputFiles('#open-input', { name: 'example.bin', mimeType: 'application/octet-stream', buffer: Buffer.from(EXAMPLE_BYTES) });
  await expect(page.locator('#status')).toContainText('example.bin, 16 bytes');
}

/** Choose a type in the dialog of *Browse types*: filter by `filter` and press *Use* on the entry whose file is `path`. */
export async function useType(page: Page, filter: string, path: string): Promise<void> {
  await page.click('#browse');
  await page.fill('#browse-filter', filter);
  const escaped = path.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const item = page.locator('#browse-dialog li', { has: page.locator('.browse-path', { hasText: new RegExp(`^${escaped}$`) }) });
  await item.getByRole('button', { name: 'Use' }).click();
}

/** The address of a link that an earlier version made for this work: `#s=` and the work, compressed, in base64url. */
export function linkOf(work: Record<string, unknown>): string {
  const code = deflateRawSync(Buffer.from(JSON.stringify(work))).toString('base64url');
  return `/#s=${code}`;
}

/**
 * Decide, before the page loads, whether the browser has `showSaveFilePicker`:
 * with `stub`, a fake that records what is written in `window.__saved` (by the
 * name it was given); without it, the function is not there, as in Firefox.
 */
export async function withSavePicker(page: Page, stub: { name: string } | null): Promise<void> {
  await page.addInitScript((given) => {
    const w = window as unknown as Record<string, unknown>;
    w.__saved = {};
    if (given === null) {
      delete w.showSaveFilePicker;
      return;
    }
    w.showSaveFilePicker = async () => ({
      name: given.name,
      async createWritable() {
        let text = '';
        return {
          async write(data: string) {
            text += data;
          },
          async close() {
            (w.__saved as Record<string, string>)[given.name] = text;
          },
        };
      },
    });
  }, stub);
}

/** Open the save dialog of the page, choose the format and press *Save*; the download that comes, if any. */
export async function saveAs(page: Page, format: 'text' | 'json', name?: string) {
  await page.locator('#save').click();
  await page.locator(`input[name="save-format"][value="${format}"]`).check();
  if (name !== undefined) await page.locator('#save-name').fill(name);
  const [download] = await Promise.all([page.waitForEvent('download'), page.locator('#save-go').click()]);
  return download;
}

/** A tar (ustar) of regular files, made here so that each test says which files are in the archive. */
export function tarOf(files: Record<string, string | Uint8Array>): Buffer {
  const encoder = new TextEncoder();
  const blocks: Uint8Array[] = [];
  for (const [name, content] of Object.entries(files)) {
    const data = typeof content === 'string' ? encoder.encode(content) : content;
    const header = new Uint8Array(512);
    const put = (at: number, value: string): void => header.set(encoder.encode(value), at);
    put(0, name);
    put(100, '0000644\0');
    put(108, '0000000\0');
    put(116, '0000000\0');
    put(124, `${data.length.toString(8).padStart(11, '0')}\0`);
    put(136, '00000000000\0');
    put(148, '        ');
    put(156, '0');
    put(257, 'ustar\0');
    put(263, '00');
    const sum = header.reduce((a, b) => a + b, 0);
    put(148, `${sum.toString(8).padStart(6, '0')}\0 `);
    blocks.push(header, data, new Uint8Array((512 - (data.length % 512)) % 512));
  }
  blocks.push(new Uint8Array(1024));
  return Buffer.concat(blocks);
}
