// Finding the schema of the opened file among the gallery formats: the library
// ranks them (by signature, then by name), and the first that parses the file
// wins. A definition is fetched only when it is its turn, so the page does not
// download the gallery to find one format.

import { CallEnded } from './client.js';
import type { DetectSource, DetectWhy, OrderResponse, ParseResponse, Request } from './doe.js';

/** A format of the gallery as `gallery.json` lists it, with its signature when it has one. */
export interface GalleryFormat {
  path: string;
  id: string;
  title: string | null;
  imports: string[];
  magic?: { offset: number; bytes: number[] }[];
  names?: string[];
  /** Parsed in the tests with a file of the format: written by its own tool, or built by hand. */
  verified?: 'real' | 'crafted';
}

export interface Attempt {
  name: string;
  why: DetectWhy;
  /** `fit` is the one that parses the file. */
  result: 'fit' | 'failed' | 'stopped' | 'not fetched';
  detail: string;
}

export interface Found {
  /** The format that parses the file, with its definition and what it imports. */
  chosen: { format: GalleryFormat; text: string; files: Record<string, string> } | null;
  attempts: Attempt[];
  /** How many formats there were, and how many were left out because the file does not start with their signature. */
  known: number;
  excluded: number;
}

export interface Deps {
  formats: GalleryFormat[];
  order(sources: DetectSource[], name: string | null, all: boolean): Promise<OrderResponse>;
  /** The text of a file of the gallery, by path. */
  fetchText(path: string): Promise<string>;
  /** Parse the input with a schema; throws `CallEnded` when the parse is stopped. */
  parse(request: Request): Promise<ParseResponse>;
  progress(text: string): void;
  cancelled(): boolean;
}

export const WHY: Record<DetectWhy, string> = {
  signatureandname: 'the signature and the name fit',
  signature: 'the file starts with its signature',
  name: 'the name of the file fits',
  remaining: 'no signature to check; tried because all were asked for',
};

/** The words for a short line of the report. */
export function describe(attempt: Attempt): string {
  return `${attempt.name} (${WHY[attempt.why]}): ${attempt.result === 'fit' ? 'parsed' : attempt.detail}`;
}

/** Try the formats in the library's order; the first that parses the input is the answer. */
export async function detectGallery(deps: Deps, name: string | null, all: boolean): Promise<Found> {
  const sources: DetectSource[] = deps.formats.map((format) => ({
    name: format.id,
    magic: format.magic ?? [],
    names: format.names ?? [format.id.toLowerCase()],
  }));
  const ranked = await deps.order(sources, name, all);
  const found: Found = { chosen: null, attempts: [], known: deps.formats.length, excluded: ranked.excluded };
  if (!ranked.ok) return found;
  for (const [position, { index, why }] of ranked.order.entries()) {
    if (deps.cancelled()) break;
    const format = deps.formats[index];
    if (!format) continue;
    deps.progress(`trying ${format.id} (${position + 1} of ${ranked.order.length})…`);
    const attempt: Attempt = { name: format.id, why, result: 'failed', detail: '' };
    found.attempts.push(attempt);
    let text: string;
    let files: Record<string, string> = {};
    try {
      const fetched = await Promise.all([format.path, ...format.imports].map((path) => deps.fetchText(path)));
      text = fetched[0] ?? '';
      files = Object.fromEntries(format.imports.map((path, i) => [path, fetched[i + 1] ?? '']));
    } catch (error) {
      attempt.result = 'not fetched';
      attempt.detail = `could not fetch the definition: ${String(error)}`;
      continue;
    }
    try {
      const answer = await deps.parse({ schema: text, files, path: format.path, include_paths: [''] });
      if (answer.ok && answer.error === null && answer.diagnostics.length === 0) {
        attempt.result = 'fit';
        found.chosen = { format, text, files };
        break;
      }
      const problem = answer.error ?? answer.diagnostics[0];
      attempt.detail = `did not fit: ${problem?.message ?? 'the parse failed'}`;
    } catch (error) {
      if (error instanceof CallEnded && error.reason === 'cancelled') {
        found.attempts.pop();
        break;
      }
      attempt.result = 'stopped';
      attempt.detail = error instanceof CallEnded ? `stopped: ${error.message}` : `stopped: ${String(error)}`;
    }
  }
  return found;
}
