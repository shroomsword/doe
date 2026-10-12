# doe

[![CI](https://github.com/shroomsword/doe/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/shroomsword/doe/actions/workflows/ci.yml?query=branch%3Amain)

decoder of everything

`doe` parses binary files according to a declarative YAML schema and prints
the result as human-readable indented text or JSON. It is useful for
inspecting proprietary formats, reverse-engineering firmware images, and
validating binary protocol implementations without writing a one-off parser
each time.

## Installation

```sh
cargo install --path crates/doe-cli
```

### Shell completions

`doe completions <SHELL>` prints a completion script to stdout for `bash`,
`zsh`, `fish`, `powershell` or `elvish`. Install it for your shell:

```sh
# bash
doe completions bash > ~/.local/share/bash-completion/completions/doe

# zsh (any directory on $fpath, then restart the shell or run compinit)
doe completions zsh > ~/.zfunc/_doe

# fish
doe completions fish > ~/.config/fish/completions/doe.fish

# PowerShell: add this line to your $PROFILE
doe completions powershell | Out-String | Invoke-Expression

# elvish: add this line to ~/.config/elvish/rc.elv
eval (doe completions elvish | slurp)
```

### Man page

The man page is generated from the same definition:

```sh
doe man > ~/.local/share/man/man1/doe.1
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
| `repeat: until` + `repeat-until:` | Repeat until an expression is true; `_` is the last parsed value |
| `if:` | Skip the field when the expression is false |
| `enum:` | Map the parsed integer to a named variant |
| `contents:` | Assert fixed bytes (magic numbers, file signatures) |
| `doc:` | Human-readable description (shown in verbose output) |

### Expressions

`size:`, `repeat-expr:`, `repeat-until:`, and `if:` accept a small expression language. Earlier
fields in the same sequence are referenced by name; fields in the parent type
via `_parent.field_name`. Within a `repeat-until:` expression, `_` refers to
the most recently parsed value.

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

# repeat until a sentinel value (terminator is included in the result)
- id: items
  type: u8
  repeat: until
  repeat-until: _ == 0

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

```text
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

```text
doe — binary file parser

Usage: doe [OPTIONS] [FILE] [TYPE_OR_SCHEMA]
       doe <COMMAND>

Commands:
  completions  Print a shell completion script to stdout
  help         Print this message or the help of the given subcommand(s)

Arguments:
  [FILE]            Binary file to parse
  [TYPE_OR_SCHEMA]  Type name (e.g. `png`) or path to a YAML schema file

Options:
  -I, --include-path <DIR>  Add a directory to the schema search path (repeatable)
  -c, --config <FILE>       Config file path [default: ~/.config/doe/config.toml]
      --json                Emit compact JSON output
      --pretty              Emit pretty-printed JSON output
      --indent <STR>        Indentation string for text output [default: "  "]
  -h, --help                Print help, including available types from the include paths
      --color <WHEN>        When to use color on stderr [default: auto] [possible values: auto, always, never]
      --log-level <LEVEL>   Verbosity of the log file [default: debug] [possible values: off, error, warn, info, debug, trace]
      --log-file <FILE>     Write the log file here [default: the XDG state directory]
  -v, --verbose...          Increase logging verbosity
  -q, --quiet...            Decrease logging verbosity
  -V, --version             Print version
```

`FILE` is the binary to parse. `TYPE_OR_SCHEMA` is treated as a schema path if
it starts with `.`, `/` or `~`, or ends with `.yaml`/`.yml`. Otherwise it is a
bare type name resolved through the configured include paths.

Results go to stdout; everything else (errors, warnings, progress) goes to
stderr. Global flags (`--color`, `-v`, `-q`, `--log-level`, `--log-file`) go
after a subcommand, as in `doe completions bash --color never`.

### Color

Color is used on stderr only: errors in red, warnings in yellow. `--color`
takes `auto` (the default: only when stderr is a terminal and `NO_COLOR` is
not set), `always` or `never`.

### Verbosity

`-v` and `-q` are repeatable and control what appears on stderr:

| Flags | stderr shows |
|-------|--------------|
| `-qq` | Nothing; the exit code is the only signal |
| `-q` | Errors only |
| (default) | Errors and warnings |
| `-v` | Also informational messages |
| `-vv` | Also debug messages |
| `-vvv` | Also trace messages |

### Log file

Independently of stderr, `doe` appends to a log file at
`$XDG_STATE_HOME/doe/doe.log` (`~/.local/state/doe/doe.log` when the variable
is unset; the local data directory on Windows). Each line is a timestamp, the
level, the module and the message. The level defaults to `debug`; set it with
`--log-level` or `log.level` in the config file, with the flag winning.
`--log-file` chooses another file. If the file cannot be opened, `doe` prints a
warning and carries on.

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success (a closed stdout, as in `doe ... \| head`, is also success) |
| 1 | Failure: unreadable or invalid input, schema or config |
| 2 | Usage error: invalid arguments |

## Configuration

`doe` reads a TOML file on startup, from `$XDG_CONFIG_HOME/doe/config.toml`
(`~/.config/doe/config.toml` when the variable is unset, on macOS as well) or
the known-folder location on Windows. `-c`/`--config` selects another file. A
missing file is not an error; a file that does not parse, or has an unknown
key, is, and the message names the file and line.

```toml
include_paths = ["~/.doe/types", "/usr/share/doe/types"]

[log]
level = "debug"
```

| Key | Type | Meaning | Overriding flag |
|-----|------|---------|-----------------|
| `include_paths` | list of paths | Directories searched for bare type names, in order. `~/` is expanded. | `-I`/`--include-path` (these come first) |
| `log.level` | `off`, `error`, `warn`, `info`, `debug`, `trace` | Verbosity of the log file. Default `debug`. | `--log-level` |

> **Upgrading:** earlier versions read `~/.doe/config/config.yaml`. That file
> is no longer read; move its settings into the TOML file above.

## Fuzzing

Everything that parses external input has a [`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz)
target in `fuzz/`, which is not part of the workspace:

| Target | Input | What it exercises |
|--------|-------|-------------------|
| `cursor` | bytes plus a list of operations | every `Cursor` read, seek, peek and sub-window |
| `expr` | text | expression parsing and evaluation |
| `engine` | bytes | the parse engine over two fixed schemas that use every field kind |
| `schema` | text | YAML deserialization, schema compilation, then parsing with each compiled type |

Fuzzing needs a nightly toolchain, `cargo-fuzz` and a C++ compiler (for
libFuzzer). The stable toolchain is not affected.

```sh
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz
mkdir -p fuzz/corpus/expr
cargo +nightly fuzz run expr fuzz/corpus/expr fuzz/seeds/expr -- -max_total_time=300
```

The first corpus directory receives the inputs the fuzzer discovers
(`fuzz/corpus/<target>`, git-ignored and created beforehand); the committed
seeds in `fuzz/seeds/<target>` are only read. A crash is written to
`fuzz/artifacts/<target>`: commit it as a regression test under
`crates/*/tests/` before fixing it.

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
# Ok::<(), Box<dyn std::error::Error>>(())
```

For lower-level control, use the modules directly:

```rust,no_run
use doe::schema::compiler::Compiler;
use doe::parser::{cursor::Cursor, engine::Engine};
use doe::render::json;
# let include_dir = std::path::PathBuf::from("types");

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
# Ok::<(), Box<dyn std::error::Error>>(())
```

## License

MIT
