# The calls of doe-wasm

Five functions, exported to JavaScript by `wasm-bindgen`. Requests and answers
are **JSON strings**, so the page, a worker and the tests all call them the same
way. The Rust side is `src/api.rs`; the tests in `tests/api.rs` check every
shape below.

```ts
check(request: string): string          // CheckResponse as JSON
inspect(request: string): string        // InspectResponse as JSON: the schemas, the missing imports, the types
parse(request: string, bytes: Uint8Array): string   // ParseResponse as JSON
render(request: string, bytes: Uint8Array, format: string): string   // ExportResponse as JSON (`export` is a reserved word in JavaScript)
version(): string
```

## Request

```jsonc
{
  "schema": "meta:\n  id: t\nseq: ...",   // the text of the schema (required)
  "files": { "common.ksy": "..." },        // other files, by path (imports)
  "path": "main.ksy",                      // where the schema is; default "main.ksy"
  "include_paths": ["lib"],                // where imports are looked for besides next to the schema
  "type": "t::header"                      // the type to parse with; the schema's own by default
}
```

Imports are found by the rules of Kaitai Struct: next to the importing file,
then in the include paths; a name with a leading slash in the include paths
only; `./` and `../` relative to the importing file. A path with a directory
(`"lib/common.ksy"`) is a path in this set of files, not on a disk.

## CheckResponse

```jsonc
{ "ok": true, "id": "t", "diagnostics": [] }
{ "ok": false, "id": null, "diagnostics": [ Diagnostic, ... ] }
```

## ParseResponse

```jsonc
{
  "ok": true,           // the schema compiled and the parse worked
  "id": "t",            // null when the schema did not compile
  "diagnostics": [],    // what is wrong with the schema (then no value, trace or error)
  "value": { ... },     // the value as `doe --json` prints it; what was read before the failure if it failed
  "trace": TraceNode,   // where each part of the value came from
  "error": null         // Diagnostic of kind "parse" (or "input"); null when the parse worked
}
```

When the parse fails, `value` and `trace` are what was read until then, the
node where it failed has `failed: true`, and `error` says why. When the schema
does not compile there is a list of `diagnostics` and nothing else. Input
larger than 256 MiB (`MAX_INPUT_BYTES`) is refused with an `input` error.

## ExportResponse

The whole value, rendered as `doe` prints it on the command line (the same
text, byte for byte). `format` is `"text"` (two-space indent), `"json"`
(compact) or `"pretty"` (indented JSON). A parse that fails gives no output.

```jsonc
{ "ok": true, "diagnostics": [], "output": "...", "error": null }
{ "ok": false, "diagnostics": [ Diagnostic, ... ], "output": null, "error": null }  // the schema does not compile
{ "ok": false, "diagnostics": [], "output": null, "error": Diagnostic }  // the parse failed, or the format is unknown
```

## InspectResponse

`inspect(request)` lists the schemas a request is made of. It reads the files
and does not compile them, so it answers also for a schema that does not
compile (and for one with an import that is not there).

```jsonc
{
  "ok": true,
  "schemas": [                       // the main schema first, then each file it imports (and they import), once
    {
      "path": "main.ksy",            // as imports found it
      "id": "chunks",                // null when the file could not be read
      "imports": ["common"],         // as the file writes them
      "types": ["chunks", "chunks::item"],   // what `type` of a request takes: the id, and id::name for the types in `types:`
      "error": null                  // Diagnostic (with the place in that file's text) when it could not be read
    }
  ],
  "missing": [                       // imports that were not found
    { "name": "common", "from": "main.ksy", "message": "schema 'common' not found; searched: ..." }
  ],
  "diagnostics": []                  // only when the request is not valid (then ok is false)
}
```

## Diagnostic

```jsonc
{
  "severity": "error",
  "kind": "yaml",        // request | yaml | schema | input | parse
  "message": "...",
  "position": { "line": 6, "column": 11, "offset": 62, "len": 3 }   // or null
}
```

`position` is in the text of `schema` (lines and columns from 1, `offset` and
`len` in bytes) and is there for errors that are about that text and whose place
is known; an error in an imported file, or in the input, has none.

## TraceNode

```jsonc
{
  "name": "length",       // field or instance name; "[3]" for an element; the type for the root
  "type": "t::chunk",     // the user type read here, or null
  "start": 64,            // first bit, from the start of the input (null for a "value:" instance)
  "end": 96,              // the bit after the last one
  "instance": false,
  "failed": false,        // the parse failed here (only the innermost value)
  "processed": false,     // offsets are in the bytes a `process:` produced, not in the input
  "summary": "13",        // the value as text; null for a type or a list; cut at 200 characters
  "integer": { "value": "13", "name": null },
                          // for an integer or an enum: the number in decimal, with all its
                          // digits (a string: a JavaScript number loses some of a 64-bit one),
                          // and the name of the member; null for anything else
  "children": [ TraceNode, ... ]
}
```

Offsets are in **bits**, so a bit field can be told from its neighbours: byte
`n` is bits `8n` to `8n + 8`. The fields of a type come first, then its
instances.

The width of an integer, for showing it in hexadecimal or binary with the digits
of its type, is `end - start`; an instance computed by an expression has none.
