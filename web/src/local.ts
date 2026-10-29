// What `doe --ide` offers besides the page: the types of this machine, and an
// input file that stays where it is. A hosted page has none of it: asking for
// /api/local gives no JSON there, and `detectLocal` says so.

import type { ExportFormat, ExportResponse, Magic, ParseResponse, Request } from './doe.js';

export interface LocalInfo {
  version: string;
  include_paths: string[];
  input: { name: string; size: number } | null;
}

export interface LocalType {
  id: string;
  doc: string | null;
  path: string;
}

/** What a type of this machine starts with and what it is called. */
export interface LocalSignature {
  id: string;
  path: string;
  magic: Magic[];
  names: string[];
  /** Why the schema could not be compiled. */
  error: string | null;
}

/** What `doe --ide` found out when it looked for the type of its input file. */
export interface LocalDetection {
  chosen: { name: string; path: string } | null;
  attempts: { name: string; why: string; fits: boolean; outcome: string }[];
  known: number;
  excluded: number;
}

export interface LocalSchema {
  path: string;
  text: string;
  files: Record<string, string>;
  include_paths: string[];
}

export class LocalApi {
  constructor(readonly info: LocalInfo) {}

  async types(): Promise<LocalType[]> {
    return (await this.json('api/types')) as LocalType[];
  }

  /** The signatures of the types of this machine (the server compiles them, which takes a while). */
  async signatures(): Promise<LocalSignature[]> {
    return (await this.json('api/signatures')) as LocalSignature[];
  }

  /** Find the type of the input file among the types of this machine. */
  async detect(all: boolean): Promise<LocalDetection> {
    return (await this.json(`api/detect${all ? '?all=1' : ''}`)) as LocalDetection;
  }

  async schema(path: string): Promise<LocalSchema> {
    return (await this.json(`api/schema?path=${encodeURIComponent(path)}`)) as LocalSchema;
  }

  async range(offset: number, len: number): Promise<Uint8Array> {
    const response = await fetch(`api/input?offset=${offset}&len=${len}`);
    if (!response.ok) throw new Error(`could not read the file: ${response.status}`);
    return new Uint8Array(await response.arrayBuffer());
  }

  /** Parse the input file named on the command line, on this machine. */
  async parse(request: Request, signal: AbortSignal): Promise<ParseResponse> {
    const response = await fetch('api/parse', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(request),
      signal,
    });
    if (!response.ok) throw new Error((await response.text()).trim() || `the server answered ${response.status}`);
    return (await response.json()) as ParseResponse;
  }

  /** The whole value of the input file as `doe` prints it. */
  async export(request: Request, format: ExportFormat, signal: AbortSignal): Promise<ExportResponse> {
    const response = await fetch(`api/export?format=${format}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(request),
      signal,
    });
    if (!response.ok) throw new Error((await response.text()).trim() || `the server answered ${response.status}`);
    return (await response.json()) as ExportResponse;
  }

  private async json(url: string): Promise<unknown> {
    const response = await fetch(url);
    if (!response.ok) throw new Error((await response.text()).trim() || `the server answered ${response.status}`);
    return response.json();
  }
}

/** The calls of the local server, or null when the page is not served by one. */
export async function detectLocal(): Promise<LocalApi | null> {
  try {
    const response = await fetch('api/local');
    if (!response.ok || !(response.headers.get('content-type') ?? '').includes('application/json')) return null;
    const info = (await response.json()) as Partial<LocalInfo>;
    return typeof info.version === 'string' ? new LocalApi(info as LocalInfo) : null;
  } catch {
    return null;
  }
}
