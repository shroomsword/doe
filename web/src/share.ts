// The schema in a link, and in the browser's storage. Only text that the
// person wrote or added as imports goes here: never the binary.

/** Where a schema came from: typed in the editor, opened from a file, added as an import, or taken from the gallery or the library. */
export type Origin = 'typed' | 'file' | 'added' | 'gallery' | 'library' | 'bundled';

import { type Radix, isRadix } from './radix.js';

const ORIGINS: readonly string[] = ['typed', 'file', 'added', 'gallery', 'library', 'bundled'];

/** What is shared and remembered. */
export interface Work {
  schema: string;
  files: Record<string, string>;
  path: string;
  include_paths?: string[];
  /** The type to parse with; the schema's own when it is left out. */
  type?: string;
  /** Where the main schema came from. */
  origin?: Origin;
  /** Where each imported file came from; `added` for those that are not named. */
  origins?: Record<string, Origin>;
  /** The radix of fields that have one of their own, by the name of the field (type and name). */
  radixes?: Record<string, Radix>;
}

/** The longest link that is made, in characters. */
export const MAX_LINK = 100_000;
/** The most bytes that a link or the storage may expand to. */
const MAX_WORK_BYTES = 4 * 1024 * 1024;

const toBase64Url = (bytes: Uint8Array): string => {
  let binary = '';
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
};

const fromBase64Url = (text: string): Uint8Array => {
  const binary = atob(text.replace(/-/g, '+').replace(/_/g, '/'));
  return Uint8Array.from(binary, (c) => c.charCodeAt(0));
};

async function pipe(bytes: Uint8Array, stream: CompressionStream | DecompressionStream, limit: number): Promise<Uint8Array> {
  const reader = new Blob([bytes as BlobPart]).stream().pipeThrough(stream as ReadableWritablePair<Uint8Array, Uint8Array>).getReader();
  const parts: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.length;
    if (total > limit) {
      await reader.cancel();
      throw new Error('too large');
    }
    parts.push(value);
  }
  const out = new Uint8Array(total);
  let at = 0;
  for (const part of parts) {
    out.set(part, at);
    at += part.length;
  }
  return out;
}

/** The text of a link's hash for this work: compressed, in the characters of a URL. */
export async function encodeWork(work: Work): Promise<string> {
  const json = new TextEncoder().encode(JSON.stringify(work));
  return toBase64Url(await pipe(json, new CompressionStream('deflate-raw'), MAX_WORK_BYTES));
}

function isWork(value: unknown): value is Work {
  if (typeof value !== 'object' || value === null) return false;
  const work = value as Record<string, unknown>;
  const files = work.files;
  return (
    typeof work.schema === 'string' &&
    typeof work.path === 'string' &&
    typeof files === 'object' &&
    files !== null &&
    Object.values(files).every((text) => typeof text === 'string') &&
    (work.include_paths === undefined || (Array.isArray(work.include_paths) && work.include_paths.every((p) => typeof p === 'string'))) &&
    (work.type === undefined || typeof work.type === 'string') &&
    (work.origin === undefined || (typeof work.origin === 'string' && ORIGINS.includes(work.origin))) &&
    (work.origins === undefined ||
      (typeof work.origins === 'object' &&
        work.origins !== null &&
        Object.values(work.origins).every((origin) => typeof origin === 'string' && ORIGINS.includes(origin)))) &&
    (work.radixes === undefined ||
      (typeof work.radixes === 'object' && work.radixes !== null && Object.values(work.radixes).every((radix) => isRadix(radix))))
  );
}

/** The work in a link's code, or null when it is not one. */
export async function decodeWork(code: string): Promise<Work | null> {
  try {
    const text = new TextDecoder().decode(await pipe(fromBase64Url(code), new DecompressionStream('deflate-raw'), MAX_WORK_BYTES));
    const value: unknown = JSON.parse(text);
    return isWork(value) ? value : null;
  } catch {
    return null;
  }
}

/** The code in a hash (`#s=…`), or null. */
export function codeOfHash(hash: string): string | null {
  const match = /^#s=([A-Za-z0-9_-]+)$/.exec(hash);
  return match?.[1] ?? null;
}

const KEY = 'doe-ide:work';

export interface Remembered {
  work: Work;
  /** The name of the last file that was opened, to say which one to open again. */
  inputName: string | null;
}

/** The work that this browser remembers, if any. */
export function recall(): Remembered | null {
  try {
    const text = localStorage.getItem(KEY);
    if (!text) return null;
    const value = JSON.parse(text) as { work?: unknown; inputName?: unknown };
    if (!isWork(value.work)) return null;
    return { work: value.work, inputName: typeof value.inputName === 'string' ? value.inputName : null };
  } catch {
    return null;
  }
}

/** Remember the work in this browser (storage can be missing or full: then it is not remembered). */
export function remember(work: Work, inputName: string | null): void {
  try {
    const text = JSON.stringify({ work, inputName });
    if (text.length < MAX_WORK_BYTES) localStorage.setItem(KEY, text);
  } catch {
    // no storage: the page works without it
  }
}

export function forget(): void {
  try {
    localStorage.removeItem(KEY);
  } catch {
    // no storage
  }
}
