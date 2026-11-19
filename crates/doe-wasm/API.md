# The calls of doe-wasm

Ten functions and one class, exported to JavaScript by `wasm-bindgen`. Requests and answers
are **JSON strings**, so the page, a worker and the tests all call them the same
way. The Rust side is `src/api.rs`; the tests in `tests/api.rs` check every
shape below.

```ts
check(request: string): string          // CheckResponse as JSON
inspect(request: string): string        // InspectResponse as JSON: the schemas, the missing imports, the types
parse(request: string, bytes: Uint8Array): string   // ParseResponse as JSON
language(): string                       // LanguageResponse as JSON: the keys, the types and the values that a schema can have, for an editor to offer
draft(request: string, bytes: Uint8Array): string   // DraftResponse as JSON: the schema builder, one change to a schema text against the bytes of a file
compare(request: string, a: Uint8Array, b: Uint8Array): string   // CompareResponse as JSON: two files, parsed with the schema of the request and compared by field
render(request: string, bytes: Uint8Array, format: string): string   // ExportResponse as JSON (`export` is a reserved word in JavaScript)
signature(request: string): string      // SignatureResponse as JSON: what the schema's files start with, and its names
detect_order(request: string, head: Uint8Array): string   // OrderResponse as JSON: which schemas to try on a file, in what order
decode(process: string, bytes: Uint8Array, from: number): Uint8Array   // the payload of a file (`gzip`, `xz`, `zstd`, ...) from byte `from`; throws the reason as a string
version(): string
```

The class `Hasher` is the one thing here that is not JSON: a checksum that is fed as bytes come, so that a file of any
size is hashed without a copy of it in WebAssembly memory. `new Hasher("md5")` (an error for a name that is not an
algorithm), `update(bytes: Uint8Array)` for each piece, `finish(): string` for the lower case hex digits (empty if called
again), and `free()` as for any class of `wasm-bindgen`. The Files view's checksum column is made with it in the worker.

## `draft`

The builder of the web IDE (the model is the crate `doe-draft`, which the terminal viewer's builder uses too). There is no
state: the text is the schema.

```jsonc
{
  "text": "",              // what the builder wrote, as it is in the editor; blank to begin
  "id": "blip",            // the id of the schema when the text is blank
  "files": { "pair.ksy": "..." },   // the schemas of the types it imports, by the name it imports them with
  "op": { "op": "add", "kind": "u2", "name": "values", "repeat": "count" }   // none: only read the text
}
```

`op` is one of: `add` (`at` a type id or none, `index` or the end, `name` or one is proposed, and the kind as a dialog
collects it: `kind` (`u1`…`s8`, `f4`, `f8`, `bytes`, `str`, `strz`, `magic`, `b1`…`b64`, `type`), `length`, `encoding`,
`magic` (bytes), `terminator`, `size`, `type_id`, `repeat` (a number, the name of an earlier integer field, or `eos`)),
`remove` (`at`, `index`), `rename` (`at`, `index`, `to`), `retype` (`at`, `index`, and the kind), `endian` (`to`: `le` or
`be`), `group` (`at`, `start`, `count`, `type_id`, `field_id`), `ungroup` (`at`, `start`), and `propose` (`selection`: how
many bytes are selected, `kind`, `type_id`: what a dialog begins with).

The answer: `ok` (the change was made, or none was asked and the file fits), `buildable` (the text is one that the builder
wrote and can go on with; when it is not, `error` says which line is not, and the text is never rewritten), `text` (the new
schema, or the one that was given when the change was refused), `error`, `layout` (`id`, `endian`, `imports`, and for the
schema and each type its fields as `{id, kind, start_bit, end_bit}`) and `proposal` (`{kind, name}`, for `propose`). A change
that would make the file not fit the schema is refused, as in the terminal; when the file did not fit already, a change is
allowed (it is how to mend it).

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
  "error": null,        // Diagnostic of kind "parse" (or "input"); null when the parse worked
  "docs": { "png::chunk": { "len": "The length of the data." } }   // the doc: of the fields, by type and field
}
```

`docs` has the descriptions (`doc:`) of the fields and instances of the types the trace has, by the fully-qualified name
of the type (the `type` of a node) and the name of the field. The description of a node is the one of its name in the table
of the type of its **parent**; an element (`[0]`) has none, the field has it once, and only fields that have a description
are in it (the table is small, so the trace does not carry a string for every node). It is `{}` for a schema without any.

When the parse fails, `value` and `trace` are what was read until then, the
node where it failed has `failed: true`, and `error` says why. When the schema
does not compile there is a list of `diagnostics` and nothing else. Input
larger than 256 MiB (`MAX_INPUT_BYTES`) is refused with an `input` error.

## LanguageResponse

What an editor offers while a schema is written. It does not depend on a schema, so it is asked once. It comes from the
compiler: the keys at each place are **the ones the parser accepts there** (found by asking it), so they cannot drift from
what the library reads.

```jsonc
{
  "places": {                // schema, meta, field, instance, type, switch, payload, filesystem, param, valid
    "field": [
      { "key": "id", "doc": "The name of the field.", "hidden": false },
      { "key": "-orig-id", "doc": "", "hidden": true }
                             // accepted, but not for people to write: an editor does not offer it
    ]
  },
  "types": [ { "name": "u2le", "doc": "an unsigned integer of 2 bytes, little-endian" } ],   // the built-in types of a field
  "values": { "endian": [ { "name": "le", "doc": "…" } ] }
                             // the names that endian, bit-endian, repeat, encoding and process take
}
```

## CompareResponse

Both files are parsed with the schema of the request (each in memory, so each is as big as a `parse` takes) and compared
by **field path**: a field is the same field in both files wherever it is, a list is compared element by element
(`[i]` with `[i]`), and the length of a list is a difference. A list can be matched **by a key** instead: a field of its
elements, given by the schema (`compare-by: id` on the repeated field) or by the request (`"compare_keys": {"entries":
"name"}`, the name of the list from the root with dots, indexes left out, and the field; it beats the schema's). The
elements that have the same key are the same element wherever they are: one that only a file has is a difference
(`only`), one that moved is not, and the length of such a list is not a difference of its own. A key that cannot be used
(repeated in a file, an element without the field, not a number, a string or bytes, a list of more than 5 million
elements) falls back to the index for that list, and `notes` says why.

```jsonc
{
  "ok": true,                // the schema compiled and both files could be read
  "id": "chunks",
  "diagnostics": [ Diagnostic, ... ],   // what is wrong with the schema or the request
  "error": null,             // Diagnostic: a file that could not be read at all ("the first file: …")
  "a": {                     // the first file; "b" is the second, in the same shape
    "error": null,           // Diagnostic: why its parse stopped (what was read is compared all the same)
    "trace": TraceNode,      // as for ParseResponse, with "diff" and "diff_below" on the nodes (below)
    "bytes": [[4, 5]]        // the bytes of this file that differ, [first, end), sorted and joined; by matched field, not by offset
  },
  "b": { ... },
  "ghosts": [                // what only B has, shown under the parent in A's tree that would hold it
    { "parent": [1], "name": "[1]", "b_path": [1, 1], "summary": null }
                             // "parent" and "b_path" are indexes of children from the root of A and of B
  ],
  "summary": { "fields_a": 9, "fields_b": 9, "changed": 1, "only_a": 0, "only_b": 0, "identical": false },
  "notes": [ "entries: matched by index, not by name: in A, \"a.txt\" is the name of more than one element" ],
  "lists": [                 // the lists of the first file whose elements are structs: for a viewer that offers a key
    { "path": [0], "name": "entries", "len": 3, "candidates": ["id", "name"], "schema_key": null }
                             // "candidates": the fields (numbers, strings or bytes) that are all different, none for a
                             // list of more than 200 000 elements; "schema_key": the schema's compare-by:
  ]
}
```

A node of a list that was matched by a key has `matched_by`: the field. `diff` of a node is `"changed"` (its value differs; for a container: its type, or the length of a list),
`"inside"` (it is equal in itself and something below it differs; `diff_below` is how many) or `"only"` (the other file
has nothing like it); it is `null` for a node that is the same. A field that moved because an earlier one grew is the
same field and is not marked.

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

## SignatureResponse

`signature(request)` takes a [Request](#request) and compiles the schema.

```jsonc
{
  "ok": true,
  "magic": [ { "offset": 0, "bytes": [137, 80, 78, 71] } ],  // every file of the schema starts with these; [] if the schema does not say
  "names": ["png", "png"],      // the format's id, the name of its file and its meta: file-extension, in lower case
  "payload": null,              // or {"process": "gzip", "from": 0} when a file of the schema holds another file (`payload:`): `decode` gives it
  "diagnostics": []             // what is wrong with the schema, when ok is false
}
```

The signature is the `contents:` of the first fields, and the exact `valid:` of
integers and strings, with the fixed-size fields before them (and looking into
a field that is a type); see `doe::detect::magic_of`.

## OrderResponse

`detect_order(request, head)` ranks schemas for a file that starts with `head`
(the first 64 KiB are used). The request has no schema, only what `signature`
gave for each one:

```jsonc
{
  "sources": [ { "name": "png", "magic": [ { "offset": 0, "bytes": [137, 80, 78, 71] } ], "names": ["png"] } ],
  "name": "photo.png",   // the name of the file, for its extension (optional)
  "all": false           // also the schemas with no signature and no name that fits (optional)
}
```

```jsonc
{
  "ok": true,
  "order": [ { "index": 0, "why": "signatureandname" } ],  // places in `sources`, in the order to try them
  "excluded": 40,        // the schemas whose signature the file does not start with: never tried
  "diagnostics": []
}
```

`why` is `signatureandname`, `signature`, `name` or `remaining`. The page tries
them in this order (fetching the definition of each only when it is its turn)
and takes the first that parses.

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
  "diff": null,           // only in a CompareResponse: "changed", "inside" or "only"; null when it is the same
  "diff_below": null,     // only in a CompareResponse: how many differences there are below it
  "children": [ TraceNode, ... ]
}
```

Offsets are in **bits**, so a bit field can be told from its neighbours: byte
`n` is bits `8n` to `8n + 8`. The fields of a type come first, then its
instances.

The width of an integer, for showing it in hexadecimal or binary with the digits
of its type, is `end - start`; an instance computed by an expression has none.
