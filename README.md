# doe

[![CI](https://github.com/shroomsword/doe/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/shroomsword/doe/actions/workflows/ci.yml?query=branch%3Amain)
[![Release](https://github.com/shroomsword/doe/actions/workflows/release.yml/badge.svg)](https://github.com/shroomsword/doe/actions/workflows/release.yml)

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

## Type library

Ready-made type definitions live in [`types/`](https://github.com/shroomsword/doe/tree/main/types). Point `doe` at the
directory with `-I` (or list it under `include_paths` in the config file) and
use a type by name:

```sh
doe -I types /bin/ls elf
doe -I types --json libc.so.6 elf | jq '.program_headers[].p_type.name'
```

| Type | Describes |
|------|-----------|
| `elf` | ELF64 little-endian files: the header, the program header table and the section header table, each found through the header. Contents are not decoded. See the comments in [`types/elf.yaml`](https://github.com/shroomsword/doe/blob/main/types/elf.yaml) for its limits. |

Running `doe --help` with `-I types` lists the available types.

## Schema format

A schema is a YAML file that describes the layout of a binary file.

```yaml
id: png_chunk
doc: "One chunk from a PNG file"
meta:
  endian: be
seq:
  - id: length
    type: u4
  - id: tag
    type: str
    size: 4
    encoding: ascii
  - id: body
    type: bytes
    size: length        # reference to an earlier field
  - id: crc
    type: u4
```

### Primitive types

| Type | Description |
|------|-------------|
| `u1`, `u2`, `u4`, `u8` | Unsigned integers of 1, 2, 4 and 8 bytes (schema-default endian) |
| `u2le`, `u4le`, `u8le` | Unsigned integers, explicit little-endian |
| `u2be`, `u4be`, `u8be` | Unsigned integers, explicit big-endian |
| `s1`, `s2`, `s4`, `s8` | Signed integers (same endian variants) |
| `f4`, `f8` | IEEE 754 floating-point of 4 and 8 bytes (also with `le` and `be`) |
| `bytes` | Raw byte sequence; requires `size:` |
| `str` | String; requires `size:` or `terminator:` |
| `strz` | Null-terminated string |
| `bits` | Bit field; requires `bit_size:` |

The numeric types are named by their size in bytes, as in Kaitai Struct.

> **Upgrading:** earlier versions named them by their size in bits (`u16`, `i32`,
> `f64`). Those names are now an error that names the replacement (`u2`, `s4`,
> `f8`). `u8`, `u8le` and `u8be` are the exception: they used to be one byte and
> are now eight, and they are not reported, so check them by hand.

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
| `pos:` | Read the field at this byte offset instead of the current position; see [Seeking](#seeking) |
| `enum:` | Map the parsed integer to a named variant |
| `contents:` | Assert fixed bytes (magic numbers, file signatures) |
| `doc:` | Human-readable description (shown in verbose output) |

### Expressions

`size:`, `repeat-expr:`, `repeat-until:`, `if:` and `pos:` accept an expression
language. Earlier fields in the same sequence are referenced by name; fields in
the parent type via `_parent.field_name`. Within a `repeat-until:` expression,
`_` refers to the most recently parsed value (a field of a sub-type is
`_.name`). A field that holds a sub-type exposes its fields as `field.name`, to
any depth.

Expressions are typed. A field's value is an integer, a float, a string, a byte
sequence or an array of those (an array of sub-types, and a `bytes` field longer
than 64 KiB, cannot be referenced). `size:`, `repeat-expr:` and `pos:` need an
integer; `if:` needs a condition.

```yaml
# size from an earlier field
- id: length
  type: u2le
- id: payload
  type: bytes
  size: length

# conditional field
- id: flags
  type: u1
- id: extra_data
  type: u4le
  if: flags & 0x01

# computed repeat count
- id: count
  type: u1
- id: records
  type: record_t
  repeat: expr
  repeat-expr: count

# repeat until a sentinel value (terminator is included in the result)
- id: items
  type: u1
  repeat: until
  repeat-until: _ == 0

# parent-scope reference (inside a sub-type)
- id: value
  type: bytes
  size: _parent.header_length - 4

# field of an earlier sub-type field (any depth: outer.inner.count)
- id: header
  type: header_t
- id: entries
  type: record_t
  repeat: expr
  repeat-expr: header.record_count
```

```yaml
# a string compared with a literal
- id: tag
  type: str
  size: 4
  encoding: ascii
- id: version
  type: u1
  if: tag == 'DATA'

# a conditional value (quote it: YAML reads `: ` in a plain value as a mapping)
- id: wide
  type: u1
- id: body
  type: bytes
  size: 'wide != 0 ? 8 : 4'

# the length of an earlier field
- id: name
  type: str
  size: 8
  encoding: ascii
- id: name_copy
  type: bytes
  size: name.length * 2
```

**Operators**, from lowest to highest precedence: `?:`, `||`, `&&`, the
comparisons `==` `!=` `<` `>` `<=` `>=`, `|`, `^`, `&`, `<<` `>>`, `+` `-`,
`*` `/` `%`, and the prefix operators `!` `-` `~`.

**Literals:** integers (decimal or `0x` hex), floats (`1.5`), strings, `true`,
`false` and arrays (`[1, 2, 3]`). A string is `'raw'`, with no escapes, or
`"with escapes"`, which understands `\\ \" \' \n \r \t \0 \xNN \uNNNN`.

**Integers** are exact over the whole `u64` and `i64` ranges (they are 128-bit
internally); a literal that does not fit in 128 bits is a schema error, and
arithmetic that overflows is an error rather than wrapping. `/` and `%` round
towards negative infinity, as in Kaitai Struct, so `-7 / 2` is `-4` and `-7 % 3`
is `2`. An integer and a float can be mixed (the result is a float). A shift
amount must be from 0 to 126. `&&`, `||`, `!` and `?:` also take an integer as a
condition, where non-zero is true.

**Strings** can be compared (`==`, `<`, ...) and joined with `+`; a string or
array built by an expression is limited to 1 MiB.

**Members and methods:**

| On | Member |
|----|--------|
| string | `.length`, `.size`, `.reverse`, `.to_i`, `.to_i(radix)`, `.substring(from, to)`, `.to_s` |
| bytes | `.length`, `.size`, `[i]`, `.first`, `.last`, `.min`, `.max` |
| array | `.length`, `.size`, `[i]`, `.first`, `.last`, `.min`, `.max` |
| integer | `.to_s` |
| float | `.to_i`, `.to_s` |
| boolean | `.to_i`, `.to_s` |

### Seeking

`pos:` reads a field somewhere other than where the previous field ended. It
takes a literal or an expression, like `size:`, and is an offset in bytes from
the start of the file. Once the field has been read (all of its repeats, if it
has `repeat:`), parsing carries on from where it was before, so `pos:` fields
do not move the position of the fields around them.

```yaml
seq:
  - id: header
    type: header_t        # holds table_offset and table_count
  - id: table
    type: entry_t
    pos: header.table_offset
    repeat: expr
    repeat-expr: header.table_count
  - id: trailer           # read right after the header, not after the table
    type: u4
```

Fields appear in the output in schema order, not file order. A `pos:` that is
negative or past the end of the file is an error, and so is a repeated element
made up only of `pos:` fields, since it consumes no input. To stop a crafted
file from making every element re-read a large region, `pos:` fields may read
16 times the size of the file in total, plus 1 MiB.

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
        type: u1
        enum: bits_t
      - id: section_count
        type: u2le
  section_header:
    seq:
      - id: name_offset
        type: u4le
      - id: section_size
        type: u4le
enums:
  bits_t:
    "1": b32
    "2": b64
```

Enums are lexically scoped: an `enums:` block is visible in the type that
declares it and in every type nested inside it. A nested type can declare an
enum with the same name to replace the outer one for its own fields. An enum
declared in a sub-type is not visible in its parent or its siblings.

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

### Large byte sequences

A `bytes` field longer than 64 KiB is not held in memory or printed in full.
The text output shows its length and where it starts, and the JSON output adds
the first 256 bytes:

```text
body  <1048576 bytes at 0x40>
```

```json
{ "$type": "bytes", "hex": "89504e47…", "len": 1048576, "offset": 64, "truncated": true }
```

Shorter fields are unchanged. A `str` with a `size:` or a terminator longer
than 16 MiB is an error.

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

## Memory use

`doe` reads a regular file as the schema asks for it, through a 64 KiB cache,
instead of loading it, so parsing a few structures in a file of many gigabytes
takes a few megabytes. What the result holds is what grows: every parsed field
and every `bytes` field up to 64 KiB (see above). Input that cannot seek, such
as a pipe or `/dev/stdin`, is read into memory in full.

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
