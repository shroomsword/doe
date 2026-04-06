# doe
decoder of everything

`doe` parses binary files according to a declarative YAML schema and prints
the result as human-readable indented text or JSON. It is useful for
inspecting proprietary formats, reverse-engineering firmware images, and
validating binary protocol implementations without writing a one-off parser
each time.

## Installation

```sh
cargo install --path .
```

## Quick start

Write a schema, point `doe` at a binary file:

```sh
doe firmware.bin ./my_format.yaml
doe dump.bin --json > dump.json
doe packet.bin --pretty -I ~/.doe/types
```

## Schema format

A schema is a YAML file that describes the layout of a binary file.

```yaml
id: png_chunk
doc: "One chunk from a PNG file"
meta:
  endian: be
seq:
  - id: length
    type: u32
  - id: tag
    type: str
    size: 4
    encoding: ascii
  - id: body
    type: bytes
    size: length        # reference to an earlier field
  - id: crc
    type: u32
```

### Primitive types

| Type | Description |
|------|-------------|
| `u8`, `u16`, `u32`, `u64` | Unsigned integers (schema-default endian) |
| `u16le`…`u64le` | Unsigned integers, explicit little-endian |
| `u16be`…`u64be` | Unsigned integers, explicit big-endian |
| `i8`, `i16`, `i32`, `i64` | Signed integers (same endian variants) |
| `f32`, `f64` | IEEE 754 floating-point |
| `bytes` | Raw byte sequence; requires `size:` |
| `str` | String; requires `size:` or `terminator:` |
| `strz` | Null-terminated string |
| `bits` | Bit field; requires `bit_size:` |

### Field attributes

| Attribute | Purpose |
|-----------|---------|
| `size` | Byte count for `bytes`/`str`; may be a literal or an expression |
| `terminator` | Stop byte for `str` (e.g. `terminator: 0x0a` for newline) |
| `encoding` | `utf8` (default), `ascii`, or `latin1` |
| `repeat: eos` | Repeat the field until end of stream |
| `repeat: expr` + `repeat-expr:` | Repeat a computed number of times |
| `if:` | Skip the field when the expression is false |
| `enum:` | Map the parsed integer to a named variant |
| `contents:` | Assert fixed bytes (magic numbers, file signatures) |
| `doc:` | Human-readable description (shown in verbose output) |

### Expressions

`size:`, `repeat-expr:`, and `if:` accept a small expression language. Earlier
fields in the same sequence are referenced by name; fields in the parent type
via `_parent.field_name`.

```yaml
# size from an earlier field
- id: length
  type: u16le
- id: payload
  type: bytes
  size: length

# conditional field
- id: flags
  type: u8
- id: extra_data
  type: u32le
  if: flags & 0x01

# computed repeat count
- id: count
  type: u8
- id: records
  type: record_t
  repeat: expr
  repeat-expr: count

# parent-scope reference (inside a sub-type)
- id: value
  type: bytes
  size: _parent.header_length - 4
```

Supported operators: `+` `-` `*` `/` `%` `&` `|` `^` `<<` `>>` `==` `!=`
`<` `>` `<=` `>=` `&&` `||` `!`. Integer literals may be decimal or `0x`-prefixed hex.

### Sub-types

Complex layouts can define reusable sub-types inline:

```yaml
id: elf
seq:
  - id: header
    type: elf_header
  - id: sections
    type: section_header
    repeat: expr
    repeat-expr: header.section_count
types:
  elf_header:
    seq:
      - id: magic
        contents: [0x7f, 0x45, 0x4c, 0x46]
      - id: bitness
        type: u8
        enum: bits_t
      - id: section_count
        type: u16le
  section_header:
    seq:
      - id: name_offset
        type: u32le
      - id: section_size
        type: u32le
enums:
  bits_t:
    "1": b32
    "2": b64
```

### Imports

A schema may import other schemas by bare name (resolved through include paths)
or by relative path:

```yaml
id: my_format
imports:
  - zlib_block          # resolved from include paths as zlib_block.yaml
  - ./shared_header     # relative to this schema file
seq:
  - id: hdr
    type: shared_header
  - id: body
    type: zlib_block
```

## Output

### Text (default)

Each additional level of nesting is indented by two spaces. Absent fields
(from a false `if:` guard) are omitted entirely.

```
elf
  header  elf::elf_header
    magic  <4 bytes>
    bitness  b64 (2)
    section_count  64
  sections  [64 items]
    [0]  elf::section_header
      name_offset  0
      section_size  0
    [1]  elf::section_header
      name_offset  27
      section_size  700
    ...
```

Use `--indent` to change the indentation unit:

```sh
doe file.bin schema.yaml --indent "    "   # four spaces
doe file.bin schema.yaml --indent $'\t'    # tab
```

### JSON

`--json` emits compact JSON; `--pretty` emits formatted JSON.

Structs become objects with a `"$type"` key. Byte sequences become
`{"$type":"bytes","hex":"deadbeef","len":4}`. Named enum values become
`{"$type":"enum","value":2,"name":"exec"}`. Absent fields appear as `null`.

```sh
doe header.bin elf.yaml --pretty
```

```json
{
  "$type": "elf",
  "header": {
    "$type": "elf::elf_header",
    "magic": { "$type": "bytes", "hex": "7f454c46", "len": 4 },
    "bitness": { "$type": "enum", "value": 2, "name": "b64" },
    "section_count": 64
  },
  "sections": [...]
}
```

## CLI reference

```
Usage: doe [OPTIONS] <FILE> <TYPE_OR_SCHEMA>

Arguments:
  <FILE>            Binary file to parse
  <TYPE_OR_SCHEMA>  Type name (e.g. png) or path to a YAML schema file

Options:
  -I, --include-path <DIR>  Add a directory to the schema search path (repeatable)
  -c, --config <FILE>       Config file path [default: ~/.doe/config/config.yaml]
      --json                Emit compact JSON
      --pretty              Emit pretty-printed JSON
      --indent <STR>        Indentation string for text output [default: "  "]
  -h, --help                Print help
  -V, --version             Print version
```

`TYPE_OR_SCHEMA` is treated as a file path when it begins with `.`, `/`, or
`~`, or ends with `.yaml`/`.yml`. Otherwise it is a bare type name resolved
through the configured include paths.

## Configuration

`doe` reads `~/.doe/config/config.yaml` on startup. The only current key is
`include_paths`, a list of directories to search for bare type names:

```yaml
include_paths:
  - ~/.doe/types
  - /usr/share/doe/types
```

Command-line `-I` paths take priority over config-file paths. A missing config
file is not an error.

## Using as a library

Add `doe` to `Cargo.toml`:

```toml
[dependencies]
doe = { path = "../doe" }
```

The high-level entry point is `parse_file`:

```rust,no_run
use std::path::Path;
use doe::{parse_file, OutputFormat};

let output = parse_file(
    Path::new("firmware.bin"),
    "./my_format.yaml",
    &[],   // extra include paths
    None,  // use default config location
    OutputFormat::Text { indent: "  ".into() },
)?;

print!("{}", output);
```

For lower-level control, use the modules directly:

```rust,no_run
use doe::schema::compiler::Compiler;
use doe::parser::{cursor::Cursor, engine::Engine};
use doe::render::json;

// Compile a schema
let mut compiler = Compiler::new(vec![include_dir]);
compiler.load("my_format", None)?;

// Parse a buffer
let data = std::fs::read("file.bin")?;
let engine = Engine::new(&compiler.registry);
let mut cursor = Cursor::new(&data);
let value = engine.parse_type("my_format", &mut cursor)?;

// Render
println!("{}", json::render_pretty(&value));
```

## License

MIT
