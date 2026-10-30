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
