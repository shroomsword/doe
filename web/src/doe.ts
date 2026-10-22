// A typed wrapper over the module that crates/doe-wasm builds. The shapes are
// the ones of crates/doe-wasm/API.md.

import init, * as wasm from './wasm/doe_wasm.js';

export interface Request {
  schema: string;
  files?: Record<string, string>;
  path?: string;
  include_paths?: string[];
  type?: string;
}

export interface Position {
  line: number;
  column: number;
  offset: number | null;
  len: number | null;
}

export interface Diagnostic {
  severity: 'error';
  kind: 'request' | 'yaml' | 'schema' | 'input' | 'parse';
  message: string;
  position: Position | null;
}

export interface CheckResponse {
  ok: boolean;
  id: string | null;
  diagnostics: Diagnostic[];
}

export interface TraceNode {
  name: string;
  type: string | null;
  /** First bit, from the start of the input. */
  start: number | null;
  /** The bit after the last one. */
  end: number | null;
  instance: boolean;
  failed: boolean;
  processed: boolean;
  summary: string | null;
  children: TraceNode[];
}

export interface ParseResponse {
  ok: boolean;
  id: string | null;
  diagnostics: Diagnostic[];
  value: unknown;
  trace: TraceNode | null;
  error: Diagnostic | null;
}

export interface Doe {
  version(): string;
  check(request: Request): CheckResponse;
  parse(request: Request, bytes: Uint8Array): ParseResponse;
}

/** Load the WebAssembly module and return the calls. */
export async function loadDoe(wasmUrl: URL | string): Promise<Doe> {
  await init({ module_or_path: wasmUrl });
  return {
    version: () => wasm.version(),
    check: (request) => JSON.parse(wasm.check(JSON.stringify(request))) as CheckResponse,
    parse: (request, bytes) =>
      JSON.parse(wasm.parse(JSON.stringify(request), bytes)) as ParseResponse,
  };
}
