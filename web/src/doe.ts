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
  kind: 'request' | 'yaml' | 'schema' | 'input' | 'parse' | 'text';
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
  /** What is wrong with a value that was read: a string with bytes that are not valid in its encoding. */
  problem: string | null;
  processed: boolean;
  summary: string | null;
  /** For an integer or an enum: the number in decimal with all its digits, and the member's name. */
  integer: { value: string; name: string | null } | null;
  children: TraceNode[];
}

export interface ParseResponse {
  ok: boolean;
  id: string | null;
  diagnostics: Diagnostic[];
  value: unknown;
  trace: TraceNode | null;
  error: Diagnostic | null;
  /** Strings with bytes that are not valid in their encoding (kind `text`); the parse went on. */
  problems: Diagnostic[];
  /** The entries of the value, when the schema says that it is a file system (`filesystem:`). */
  filesystem?: Listing | null;
}

export type FsKind = 'file' | 'directory' | 'symlink' | 'hardlink' | 'device' | 'fifo' | 'socket';

/** One entry of an archive, as `doe::filesystem` lists it. */
export interface FsEntry {
  /** The components of the path. */
  path: string[];
  kind: FsKind;
  size: number | null;
  mode: number | null;
  /** Seconds since 1970. */
  mtime: number | null;
  link: string | null;
  /** The children to follow from the root of the trace to the entry's node. */
  trace_path: number[];
  /** The bytes of the contents in the input: the first and the byte after the last. */
  data: [number, number] | null;
  /** `empty`, `absolute`, `parent` or `repeated`: why the entry cannot be placed as it is. */
  flags: string[];
}

export interface Listing {
  entries: FsEntry[];
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

/** What a file of a schema starts with: these bytes at this offset. */
export interface Magic {
  offset: number;
  bytes: number[];
}

/** What is known of a schema before it is tried (see `signature` and `detect_order`). */
export interface DetectSource {
  name: string;
  magic: Magic[];
  /** Other starts of a file of the schema, when it has more than one (the values of a magic number that is an enum). */
  alternatives?: Magic[][];
  names: string[];
  /** How many things the schema checks; a schema that checks nothing is tried last when all are tried. */
  checks?: number;
}

/** What `signature` answers: what a type starts with and what the format is called. */
export interface SignatureResponse {
  ok: boolean;
  magic: Magic[];
  alternatives: Magic[][];
  checks: number;
  names: string[];
  diagnostics: Diagnostic[];
}

export type DetectWhy = 'signatureandname' | 'signature' | 'name' | 'remaining';

export interface OrderResponse {
  ok: boolean;
  /** Places in the `sources` of the request, in the order to try them. */
  order: { index: number; why: DetectWhy }[];
  /** How many were left out because the file does not start with their signature. */
  excluded: number;
  diagnostics: Diagnostic[];
}

/** How the whole value is printed: as `doe` prints text, compact JSON (`--json`) or indented JSON (`--pretty`). */
export type ExportFormat = 'text' | 'json' | 'pretty';

export interface Doe {
  version(): string;
  check(request: Request): CheckResponse;
  inspect(request: Request): InspectResponse;
  /** The signature of the type of the request (`type`, or the schema's own). */
  signature(request: Request): SignatureResponse;
  parse(request: Request, bytes: Uint8Array): ParseResponse;
  export(request: Request, bytes: Uint8Array, format: ExportFormat): ExportResponse;
  /** The order in which to try schemas on a file that starts with `head`. */
  detectOrder(sources: DetectSource[], head: Uint8Array, name: string | null, all: boolean): OrderResponse;
}

/** Load the WebAssembly module and return the calls. */
export async function loadDoe(wasmUrl: URL | string): Promise<Doe> {
  await init({ module_or_path: wasmUrl });
  return {
    version: () => wasm.version(),
    check: (request) => JSON.parse(wasm.check(JSON.stringify(request))) as CheckResponse,
    inspect: (request) => JSON.parse(wasm.inspect(JSON.stringify(request))) as InspectResponse,
    signature: (request) => JSON.parse(wasm.signature(JSON.stringify(request))) as SignatureResponse,
    parse: (request, bytes) =>
      JSON.parse(wasm.parse(JSON.stringify(request), bytes)) as ParseResponse,
    export: (request, bytes, format) =>
      JSON.parse(wasm.render(JSON.stringify(request), bytes, format)) as ExportResponse,
    detectOrder: (sources, head, name, all) =>
      JSON.parse(wasm.detect_order(JSON.stringify({ sources, name, all }), head)) as OrderResponse,
  };
}
