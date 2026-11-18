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
  kind: 'request' | 'yaml' | 'schema' | 'input' | 'parse' | 'text' | 'fix';
  message: string;
  position: Position | null;
  /** For a `fix`: the text of the schema with the change, which was tried and helps. */
  fix?: string;
}

/** What the library says about where a parse broke (`doe::explain::Failure`). */
export interface Failure {
  kind: 'past_end' | 'partial_element' | 'contents';
  /** The byte of the input where it broke. */
  offset: number;
  /** The failed value, as indexes of children from the root of the trace. */
  path: number[];
  /** The failed value, written as the schema does: `entries[2].len`. */
  field: string;
  /** What happened, with the byte and the field. */
  message: string;
  suggestions: Suggestion[];
}

/** A change to the schema that was tried, and helped. */
export interface Suggestion {
  id: string;
  text: string;
  /** The text of the schema with the change. */
  schema: string;
  result: 'succeeds' | 'goes further';
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
  /** In a comparison of two files: how this node differs from the one of the other file; absent or null if it is the same. */
  diff?: 'changed' | 'inside' | 'only' | null;
  /** In a comparison: how many differences there are below this node. */
  diff_below?: number | null;
  /** What is wrong with a value that was read: a string with bytes that are not valid in its encoding. */
  problem: string | null;
  processed: boolean;
  summary: string | null;
  /** For an integer or an enum: the number in decimal with all its digits, and the member's name. */
  integer: { value: string; name: string | null } | null;
  /** For a field that places or sizes another value: the node, where it starts (bits) and how many others the field decides. */
  points_to?: { path: number[]; start: number; more: number } | null;
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
  /** The descriptions (`doc:`) of fields and instances, by the type of the node that holds them and then the field's name. */
  docs?: Docs;
  /** Where the parse broke and what might fix it, when it failed in a way that has a place. */
  failure?: Failure | null;
}

/** One of the two files of a comparison. */
export interface CompareSide {
  /** Why the parse of this file stopped, if it did. */
  error: Diagnostic | null;
  trace: TraceNode;
  /** The bytes of this file that differ, as `[first, end)`, sorted; by matched field, not by offset. */
  bytes: [number, number][];
}

/** A field or element that only the second file has, shown under the parent in the first file's tree. */
export interface Ghost {
  /** The path in the first file's tree of the node that would hold it. */
  parent: number[];
  name: string;
  /** Its path in the second file's tree. */
  b_path: number[];
  summary: string | null;
}

export interface CompareSummary {
  fields_a: number;
  fields_b: number;
  changed: number;
  only_a: number;
  only_b: number;
  identical: boolean;
}

export interface CompareResponse {
  ok: boolean;
  id: string | null;
  diagnostics: Diagnostic[];
  /** A file that could not be read at all (too big, say). */
  error: Diagnostic | null;
  a: CompareSide | null;
  b: CompareSide | null;
  ghosts: Ghost[];
  summary: CompareSummary | null;
}

/** The descriptions of fields: `docs[type][name]`. */
export type Docs = Record<string, Record<string, string>>;

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
  /** Whether `data` is the file's own bytes (false for a deflated zip entry: a checksum of it would mislead). */
  data_is_file?: boolean;
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
  /** The schema says what a file of it holds: between equal fits it is tried first. */
  informative?: boolean;
}

/** What `signature` answers: what a type starts with and what the format is called. */
export interface SignatureResponse {
  ok: boolean;
  /** What a file of the schema holds, when it holds another file: `process` (`gzip`, `xz`, ...) decodes the bytes from `from`. */
  payload?: { process: string; from: number } | null;
  magic: Magic[];
  alternatives: Magic[][];
  checks: number;
  informative: boolean;
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

/** A key of the schema language, with what it is for. */
export interface LanguageKey {
  key: string;
  doc: string;
  hidden: boolean;
}

/** A name that the language gives meaning to (a built-in type, an encoding). */
export interface LanguageName {
  name: string;
  doc: string;
}

/** What `language` answers: the keys of each place, the built-in types, and the names some keys take. */
export interface LanguageResponse {
  places: Record<string, LanguageKey[]>;
  types: LanguageName[];
  values: Record<string, LanguageName[]>;
}

/** Where a field of the schema that is built is: bits from the start of the schema (or of its type). */
export interface DraftSlot {
  id: string;
  kind: string;
  start_bit: number;
  end_bit: number;
}

/** The schema that the builder has: its fields and the types it defines. */
export interface DraftLayout {
  id: string;
  endian: 'le' | 'be';
  imports: string[];
  fields: DraftSlot[];
  types: { id: string; fields: DraftSlot[] }[];
}

/** What a dialog is asked for when a field is added or its kind changed. */
export interface DraftKindSpec {
  kind: string;
  length?: number;
  encoding?: string;
  magic?: number[];
  terminator?: number;
  size?: number;
  type_id?: string;
  repeat?: string;
}

/** One change to the schema that is built (see `crates/doe-wasm/API.md`). */
export type DraftOp =
  | ({ op: 'add'; at?: string | null; index?: number; name?: string } & DraftKindSpec)
  | { op: 'remove'; at?: string | null; index: number }
  | { op: 'rename'; at?: string | null; index: number; to: string }
  | ({ op: 'retype'; at?: string | null; index: number } & DraftKindSpec)
  | { op: 'endian'; to: 'le' | 'be' }
  | { op: 'group'; at?: string | null; start: number; count: number; type_id: string; field_id: string }
  | { op: 'ungroup'; at?: string | null; start: number }
  | { op: 'propose'; selection?: number | null; kind?: string; type_id?: string };

export interface DraftRequest {
  text: string;
  id?: string;
  files?: Record<string, string>;
  op?: DraftOp;
}

export interface DraftResponse {
  ok: boolean;
  buildable: boolean;
  text: string;
  error: string | null;
  layout: DraftLayout | null;
  proposal: { kind: string; name: string } | null;
}

export interface Doe {
  /** The schema builder: one change to the text of a schema, against the bytes of a file. */
  draft(request: DraftRequest, bytes: Uint8Array): DraftResponse;
  version(): string;
  /** The language of a schema, as the compiler defines it. */
  language(): LanguageResponse;
  check(request: Request): CheckResponse;
  inspect(request: Request): InspectResponse;
  /** The signature of the type of the request (`type`, or the schema's own). */
  signature(request: Request): SignatureResponse;
  parse(request: Request, bytes: Uint8Array): ParseResponse;
  /** Parse two files with the schema of the request and compare them by field. */
  compare(request: Request, a: Uint8Array, b: Uint8Array): CompareResponse;
  export(request: Request, bytes: Uint8Array, format: ExportFormat): ExportResponse;
  /** The order in which to try schemas on a file that starts with `head`. */
  detectOrder(sources: DetectSource[], head: Uint8Array, name: string | null, all: boolean): OrderResponse;
  /** The checksums (lower case hex) of ranges of `bytes`, each fed to the algorithm in pieces; null for a range outside. */
  digest(algorithm: string, bytes: Uint8Array, ranges: [number, number][]): (string | null)[];
  /** The payload of a file: `bytes` from `from`, decoded as `process` says. Throws the reason when it cannot be decoded. */
  decode(process: string, bytes: Uint8Array, from: number): Uint8Array;
}

/** How much of a range is given to the checksum at once. */
const DIGEST_CHUNK = 4 * 1024 * 1024;

/** Load the WebAssembly module and return the calls. */
export async function loadDoe(wasmUrl: URL | string): Promise<Doe> {
  await init({ module_or_path: wasmUrl });
  return {
    version: () => wasm.version(),
    language: () => JSON.parse(wasm.language()) as LanguageResponse,
    draft: (request, bytes) => JSON.parse(wasm.draft(JSON.stringify(request), bytes)) as DraftResponse,
    check: (request) => JSON.parse(wasm.check(JSON.stringify(request))) as CheckResponse,
    inspect: (request) => JSON.parse(wasm.inspect(JSON.stringify(request))) as InspectResponse,
    signature: (request) => JSON.parse(wasm.signature(JSON.stringify(request))) as SignatureResponse,
    parse: (request, bytes) =>
      JSON.parse(wasm.parse(JSON.stringify(request), bytes)) as ParseResponse,
    compare: (request, a, b) => JSON.parse(wasm.compare(JSON.stringify(request), a, b)) as CompareResponse,
    export: (request, bytes, format) =>
      JSON.parse(wasm.render(JSON.stringify(request), bytes, format)) as ExportResponse,
    detectOrder: (sources, head, name, all) =>
      JSON.parse(wasm.detect_order(JSON.stringify({ sources, name, all }), head)) as OrderResponse,
    decode: (process, bytes, from) => wasm.decode(process, bytes, from),
    digest: (algorithm, bytes, ranges) =>
      ranges.map(([from, to]) => {
        if (from < 0 || from > to || to > bytes.length) return null;
        const hasher = new wasm.Hasher(algorithm);
        try {
          for (let at = from; at < to; at += DIGEST_CHUNK) hasher.update(bytes.subarray(at, Math.min(to, at + DIGEST_CHUNK)));
          return hasher.finish();
        } finally {
          hasher.free();
        }
      }),
  };
}
