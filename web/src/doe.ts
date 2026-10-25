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

export interface ExportResponse {
  ok: boolean;
  diagnostics: Diagnostic[];
  output: string | null;
  error: Diagnostic | null;
}

/** One schema file of a request: the main one or an import. */
export interface SchemaInfo {
  /** As imports found it. */
  path: string;
  id: string | null;
  /** The imports it names, as it writes them. */
  imports: string[];
  /** What `type` of a request takes: the id, and `id::name` for the types in it. */
  types: string[];
  /** Why the file could not be read, with the place in its text. */
  error: Diagnostic | null;
}

export interface MissingImport {
  name: string;
  from: string;
  message: string;
}

export interface InspectResponse {
  ok: boolean;
  schemas: SchemaInfo[];
  missing: MissingImport[];
  diagnostics: Diagnostic[];
}

/** How the whole value is printed: as `doe` prints text, compact JSON (`--json`) or indented JSON (`--pretty`). */
export type ExportFormat = 'text' | 'json' | 'pretty';

export interface Doe {
  version(): string;
  check(request: Request): CheckResponse;
  inspect(request: Request): InspectResponse;
  parse(request: Request, bytes: Uint8Array): ParseResponse;
  export(request: Request, bytes: Uint8Array, format: ExportFormat): ExportResponse;
}

/** Load the WebAssembly module and return the calls. */
export async function loadDoe(wasmUrl: URL | string): Promise<Doe> {
  await init({ module_or_path: wasmUrl });
  return {
    version: () => wasm.version(),
    check: (request) => JSON.parse(wasm.check(JSON.stringify(request))) as CheckResponse,
    inspect: (request) => JSON.parse(wasm.inspect(JSON.stringify(request))) as InspectResponse,
    parse: (request, bytes) =>
      JSON.parse(wasm.parse(JSON.stringify(request), bytes)) as ParseResponse,
    export: (request, bytes, format) =>
      JSON.parse(wasm.render(JSON.stringify(request), bytes, format)) as ExportResponse,
  };
}
