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

`doe --completions <SHELL>` prints a completion script to stdout for `bash`,
`zsh`, `fish`, `powershell` or `elvish`. Install it for your shell:

```sh
# bash
doe --completions bash > ~/.local/share/bash-completion/completions/doe

# zsh (any directory on $fpath, then restart the shell or run compinit)
doe --completions zsh > ~/.zfunc/_doe

# fish
doe --completions fish > ~/.config/fish/completions/doe.fish

# PowerShell: add this line to your $PROFILE
doe --completions powershell | Out-String | Invoke-Expression

# elvish: add this line to ~/.config/elvish/rc.elv
eval (doe --completions elvish | slurp)
```

### Man pages

Two manual pages are generated from this README and the command-line
definition: `doe(1)`, the command (options, output formats, configuration), and
`doe-schema(5)`, the schema format and the Kaitai Struct compatibility notes.
Install them where `man` looks:

```sh
doe --man > ~/.local/share/man/man1/doe.1
doe --man=schema > ~/.local/share/man/man5/doe-schema.5
```

The release archives contain both in `doc/`.

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
| `b1`…`b64` | Bit field of that many bits (also with `le` and `be`); see [Bit fields](#bit-fields) |
| `bits` | The same as `bN`, with the size in `bit_size:` |

The numeric types are named by their size in bytes, as in Kaitai Struct.

> **Upgrading:** earlier versions named them by their size in bits (`u16`, `i32`,
> `f64`). Those names are now an error that names the replacement (`u2`, `s4`,
> `f8`). `u8`, `u8le` and `u8be` are the exception: they used to be one byte and
> are now eight, and they are not reported, so check them by hand.

### Bit fields

`b1`…`b64` read that many bits. Consecutive bit fields share bytes, and a field
may span bytes:

```yaml
seq:
  - id: version
    type: b4        # the high nibble of the first byte
  - id: header_len
    type: b4        # the low nibble
  - id: flags
    type: b3
  - id: fragment_offset
    type: b13       # spans into the next byte
  - id: ttl
    type: u1        # not a bit field: starts on the next byte boundary
```

By default the first field takes the most significant bits of a byte. Set
`meta.bit-endian: le` to take the least significant bits first, or write the
order on one field (`b4le`, `b4be`); it is independent of `meta.endian`. A
nested type inherits the setting. Any field that is not a bit field starts on a
byte boundary and drops the unread bits of the byte before it, as does a bit
field of the other order, `seek` and `pos:`. A `b1` is a boolean (`true` or
`false` in the output, and in expressions) unless it has an `enum:`, which makes
it the integer 0 or 1 and names it; the wider ones are unsigned integers. As in Kaitai Struct, entering a sub-type does not start a new byte.

> **Upgrading:** `bits` with `bit_size:` used to read whole bytes (two `bits: 4`
> fields took two bytes). It is now the same as `bN`, so they share one byte.
> `bit_size` must be from 1 to 64.

### Field attributes

| Attribute | Purpose |
|-----------|---------|
| `size` | Byte count for `bytes`/`str`, or the size of the window a user type is parsed in (see [Sized sub-types](#sized-sub-types)); may be a literal or an expression |
| `terminator` | Stop byte for `bytes` and `str` (e.g. `terminator: 0x0a` for newline); without a type the field is `bytes`. With `size` the whole size is read and the value is cut at the first terminator. No terminator before the end of the input is an error |
| `include` | With `terminator`: keep the terminator in the value (default `false`) |
| `consume` | With `terminator`: continue after the terminator (default `true`); `false` leaves it for the next field |
| `eos-error` | With `terminator`: `false` takes the rest of the input when there is no terminator, instead of an error |
| `pad-right` | With `size`: strip trailing bytes of this value from the result |
| `encoding` | `utf8` (default), `ascii`, `latin1`, or any of the encodings in [Encodings](#encodings) |
| `repeat: eos` | Repeat the field until end of stream |
| `repeat: expr` + `repeat-expr:` | Repeat a computed number of times |
| `repeat: until` + `repeat-until:` | Repeat until an expression is true; `_` is the last parsed value |
| `if:` | Skip the field when the expression is false |
| `size-eos` | `true` to take the rest of the enclosing window or the input as the size; for `bytes`, `str` and user types |
| `parent` | `_parent` (or `_parent._parent`): the type the field reads gets the parent of the type the field is in, as its `_parent` (see [Instances](#instances)) |
| `process` | Transform the bytes of the field's `size` window before they are parsed: `xor(key)` (an integer or a byte sequence, repeated), `rol(n)`, `ror(n)` (rotate each byte) or `zlib`. For `bytes`, `str` and user types that have a `size` or `size-eos`. `zlib` decompresses into memory up to 256 MiB; a larger result, or data that is not zlib data, is an error that names the field |
| `pos:` | Read the field at this byte offset instead of the current position; see [Seeking](#seeking) |
| `valid:` | Fail the parse unless the value read satisfies the rules; see [Validating values](#validating-values) |
| `enum:` | Map the parsed integer to a named variant. `name` is an enum visible in the type; `type::name` is one declared in another type (a nested path is allowed) |
| `contents:` | Assert fixed bytes (magic numbers, file signatures) |
| `doc:` | Human-readable description (shown in verbose output) |

### Encodings

`encoding:` (on a field, or `meta.encoding` as the default) takes these names,
in any case and with `-` and `_` interchangeable:

| Encoding | Names |
|----------|-------|
| UTF-8, ASCII, ISO-8859-1 | `UTF-8`, `ASCII`, `ISO-8859-1` (also `utf8`, `ascii`, `latin1`) |
| UTF-16, UTF-32 | `UTF-16LE`, `UTF-16BE`, `UTF-32LE`, `UTF-32BE` |
| Code pages | `CP437`, `CP1250` to `CP1258` (also `windows-1252`...) |
| ISO 8859 | `ISO-8859-2` to `ISO-8859-16` (not 12) |
| East Asian and others | `Shift_JIS`, `GBK`, `GB18030`, `Big5`, `EUC-JP`, `EUC-KR`, `KOI8-R`, `KOI8-U` |

Bytes that are not valid in the encoding are an error that names the field, the
encoding and the offset in the string; nothing is replaced by `U+FFFD`. A byte
order mark is not removed. The legacy encodings are decoded by
[`encoding_rs`](https://crates.io/crates/encoding_rs), so they follow the WHATWG
tables, which is also what the JavaScript runtime of Kaitai Struct uses (one
consequence: `ISO-8859-9`, `ISO-8859-11` and the ISO-8859 names that WHATWG maps
to a Windows code page decode as that code page).

### Expressions

`size:`, `repeat-expr:`, `repeat-until:`, `if:` and `pos:` accept an expression
language. Earlier fields in the same sequence are referenced by name; fields in
the parent type via `_parent.field_name`, and those of the top-level type via
`_root.field_name`. Within a `repeat-until:` expression, `_` refers to the most
recently parsed value (a field of a sub-type is `_.name`). A field that holds a
sub-type exposes its fields as `field.name`, to any depth, and an array of
sub-types is indexed as `entries[0].offset` or `entries.last.size`.

Expressions are typed. A field's value is an integer, a float, a boolean, a
string, a byte sequence, an array or a sub-type. `size:`, `repeat-expr:` and
`pos:` need an integer; `if:` needs a condition. A `bytes` field longer than
64 KiB, and an array or sub-type of more than about a million values, are too
large to copy into an expression: only their `.size` can be used.

`_io` is the stream the expression is in, which is a window for a sized
sub-type: `_io.pos` (bytes from its start), `_io.size` and `_io.eof`. An enum
member is written `enum_name::member` and stands for its value, using the enums
visible in the type (see [Sub-types](#sub-types)), or `type::enum::member` for an
enum declared in another type (`type` is looked up as a type name is: in the
type, then in the types around it).

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

An element of a repeated field has to consume input under `repeat: eos` and
`repeat: until`, or the field would repeat forever. Under `repeat: expr` it may
take no bytes (the entry after a `long` in the constant pool of a Java class
does not), as long as such elements are not more numerous than the bytes the
repeat has read.

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

# an enum member
- id: kind
  type: u1
  enum: kind_t
- id: payload
  type: u2
  if: kind == kind_t::data

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

**Literals:** integers (decimal, `0x` hex, `0b` binary or `0o` octal, with `_` between digits: `0xffff_ffff`), floats (`1.5`, or `24000.`), strings, `true`,
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

**Position and size:** in a repeated field, `_index` is the position of the
element (0 for the first) in its `size`, `type` arguments, `switch-on` and
`repeat-until`, and the elements read so far are the field's name
(`groups[_index - 1].total`). `field._sizeof` is the number of bytes a field
took (Kaitai Struct allows it for fields of a fixed size only), `_sizeof` the
size of the type the expression is in, and `sizeof<type>` and `bitsizeof<type>`
the size of a type or of a primitive such as `s4` or `b12`, in bytes or bits;
for a type, it is an error unless every field of the type has a fixed size.
`x.as<type>` is `x`: it names which type a value of a `switch-on` is, and is not
checked. A list of integer literals that all fit in a byte (`[0x49, 0x49]`) is a
byte sequence, so `magic == [0x49, 0x49]` works for a `bytes` field.

**Members and methods:**

| On | Member |
|----|--------|
| string | `.length`, `.size`, `.reverse`, `.to_i`, `.to_i(radix)`, `.substring(from, to)`, `.to_s` |
| bytes | `.length`, `.size`, `[i]`, `.first`, `.last`, `.min`, `.max`, `.to_s(encoding)` (see [Encodings](#encodings)) |
| array | `.length`, `.size`, `[i]`, `.first`, `.last`, `.min`, `.max` |
| integer | `.to_s` |
| float | `.to_i`, `.to_s` |
| boolean | `.to_i`, `.to_s` |

### Seeking

`pos:` reads a field somewhere other than where the previous field ended. It
takes a literal or an expression, like `size:`, and is an offset in bytes from
the start of the file (or, inside a [sized sub-type](#sized-sub-types), from the
start of its window). Once the field has been read (all of its repeats, if it
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

#### Sized sub-types

`size:` on a field whose type is a sub-type parses it in a window of exactly
that many bytes, starting at the current position. The type cannot read past
the end of the window, `repeat: eos` inside it stops there, and `pos:` inside
it is relative to its start. The enclosing type then continues after the whole
window, whatever the sub-type used of it. `size-eos: true` makes the window
run to the end of the enclosing window or of the input. With `repeat:`, each
element gets its own window.

```yaml
seq:
  - id: records
    type: record
    repeat: eos
types:
  record:
    seq:
      - id: kind
        type: u1
      - id: length
        type: u2
      - id: body
        type: body_t
        size: length          # body_t may read at most `length` bytes
```

`size` and `size-eos` apply to `bytes`, `str` and sub-types; on a numeric or
bit type, or both on one field, they are a schema error.

Enums are lexically scoped: an `enums:` block is visible in the type that
declares it and in every type nested inside it. A nested type can declare an
enum with the same name to replace the outer one for its own fields. An enum
declared in a sub-type is not visible in its parent or its siblings.

An enum member is written `1: name`, or as a mapping with the name as `id`
(`1: {id: name, doc: ...}`); `doc`, `doc-ref` and `-orig-id` are ignored.

The parameters of a type are fields of its values to expressions:
`color_mask_given.has_alpha_mask` is the `has_alpha_mask` that `color_mask`
was given. They are not in the output.

#### Choosing the byte order by a value

`meta.endian` is `le`, `be`, or a rule that chooses between them by a value
read earlier, as formats with a byte-order mark need. It can be given in a
schema's `meta` and in the `meta` of a type (which also takes `bit-endian` and
`encoding`):

```yaml
seq:
  - id: mark
    type: str
    size: 2
    encoding: ASCII
  - id: body
    type: body(mark)
types:
  body:
    params:
      - id: mark
        type: str
    meta:
      endian:
        switch-on: mark
        cases:
          '"II"': le
          '"MM"': be     # `_` is the default case
    seq:
      - id: count
        type: u4         # little or big endian, as `mark` says
      - id: fixed
        type: u2be       # an explicit byte order always wins
```

The value is evaluated when the type starts, so it can only use parameters,
`_root` and `_parent`, not the fields of the type itself. Types nested in it
use the same byte order, and so does a type that sets none and is used by it.
A value that matches no case, with no `_` case, is an error. As in Kaitai
Struct, a type with no `endian` anywhere takes the byte order of the type that
uses it.

#### Choosing a type by a value

When the layout of a field depends on a tag read earlier, `type:` can be a
mapping:

```yaml
seq:
  - id: kind
    type: u1
    enum: kind_t
  - id: length
    type: u2
  - id: body
    type:
      switch-on: kind
      cases:
        kind_t::text: text_t       # a sub-type
        2: u4le                    # a numeric type
        '"RIFF"': riff_t           # a string key
        _: bytes                   # the default
    size: length
```

`switch-on` is an expression and each key in `cases` is one too (integers,
enum members, strings in quotes, booleans); the first key equal to the
selector wins and `_` is the default. A case's value is any type name. A
`size` or `size-eos` on the field is a window around whichever type is chosen,
as for a sized sub-type, and a `bytes` or `str` case fills it; without one, a
`bytes` or `str` case needs its own `size`. If nothing matches and there is no
default, a field with a size is read as bytes and one without is absent, as in
Kaitai Struct. Quote keys and expressions that YAML would read as something
else (`'"RIFF"'`, `'kind > 0 ? 1 : 2'`).

### Validating values

`valid:` states what a field's value may be, and the parse fails if the data
breaks it, naming the field, the value and the rule. A bare value means "equal
to"; a mapping gives rules that must all hold:

```yaml
seq:
  - id: magic
    type: u4
    valid: 0x7f454c46        # eq
  - id: version
    type: u1
    valid:
      min: 1                 # inclusive
      max: 3
  - id: kind
    type: u1
    enum: kind_t
    valid:
      in-enum: true          # a member of the field's enum
  - id: tag
    type: u1
    valid:
      any-of: [1, 2, 4]
  - id: length
    type: u2
    valid:
      expr: _ % 2 == 0 && _ < limit   # `_` is the value
```

`eq`, `min`, `max` and the elements of `any-of` are expressions, so they can use
earlier fields, instances and enum members (`kind_t::data`); `expr` sees the
value as `_`. The value checked is the one read, before it is named after an
enum member, and with `repeat` every element is checked. A field with `if:`
that is not read is not checked. `in-enum` needs `enum:` on the field; an empty
`valid`, an empty `any-of`, and `valid` on a `value:` instance are schema
errors. `valid` also works on an instance that reads.

### Parameterized types

A type can take values from the field that uses it. `params:` lists them, and
the field passes arguments after the type name:

```yaml
seq:
  - id: count
    type: u1
  - id: first
    type: block(count * 2, 'a')       # arguments are expressions
  - id: second
    type: block(4, 'b')
types:
  block:
    params:
      - id: len
        type: u4          # documents the parameter; not enforced
      - id: label
        type: str
    seq:
      - id: data
        type: bytes
        size: len         # a parameter is a name like any field
```

The arguments are expressions in the scope of the field's own type, evaluated
when the field is read (for each element of a `repeat`, and in `switch-on`
cases too), so they can use earlier fields, instances and enum members. In the
new type a parameter can be used in any expression, and by its own children as
`_parent.name`. The number of arguments must match the number of parameters;
that is checked when the schema is compiled, and so is an argument on a type
that takes none. A type with parameters cannot be parsed on its own, and a
parameter cannot share a name with a field or an instance of its type.

### Instances

`instances:` names values that are computed or read on demand, on a type or on
the schema itself:

```yaml
seq:
  - id: header
    type: header_t
  - id: body
    type: bytes
    size: payload_len            # uses an instance, which is computed first
instances:
  payload_len:                   # a computed value
    value: header.words * 4
  table:                         # read elsewhere, like a field with `pos:`
    pos: header.table_offset
    type: entry
    repeat: expr
    repeat-expr: header.table_count
  first_entry:
    value: table[0]
```

An instance either reads, with the attributes of a `seq` field (`type`,
`size`, `repeat`, `if`, `enum`, `contents`, ...) and `pos:` for where (without
it, where the sequence ended), or computes, with `value:` (an expression or a
literal such as `4` or `true`, and optionally `if:` and `enum:`). Reading one
does not move the position of the sequence.

A `seq` field or another instance that uses an instance in a `size`, `if`,
`pos`, `repeat`, `switch-on` or `value` expression gets it computed first, in
dependency order; two that depend on each other are an error. Every instance
is also computed after the sequence, in schema order, and appears in the output
after the fields of `seq`: in text as `name @0x40` for one that was read (no
offset for a computed value), and in JSON inline, with the offsets of the ones
that were read in a `$offsets` object next to them. Any error aborts the parse.
`--no-instances` leaves them out of the output (those that fields use are still
computed), and so does `ParseOptions { instances: false }` with
`parse_file_with`. An instance that cannot be computed where its type ends,
because it uses something that is read later (`_parent.method` of a field after
it, or `_root.header.names` while `header` is still being parsed), is computed
once the whole input is read, as Kaitai Struct computes instances on first use,
and still appears in its place in the output; one that still cannot be computed
then, or two that need each other, abort the parse. A value that was copied from
a struct (`linked: value: _root.sections[3]`) has its instances too. A name
cannot be both a field and an instance. A sub-type
can use an instance of an enclosing type as `_parent.name`: the field of the
enclosing type that reads the sub-type computes the instance first, against
the stream of its own type, so it may only depend on what has been read by
then. An instance that reads a type that uses it is an error.

`io:` makes an instance read from another stream than the one of its type,
which `pos:` and `size-eos:` are then relative to. It is an expression whose
value is the stream of the whole input, `_root._io`, of the type around,
`_parent._io`, or of a value that was parsed earlier, `slot._io` (the window of
a sub-type with a `size`, or the stream it shared with its parent):

```yaml
seq:
  - id: slot
    type: slot
    size: len              # `slot` is an empty type that keeps its window
instances:
  inside:
    io: slot._io           # read inside that window again, later
    pos: 4
    type: u2
types:
  slot: {}
```

`parent: _parent` (or `_parent._parent`) on a field makes the `_parent` of the
type it reads that of the type the field is in instead of that type itself,
as in Kaitai Struct. Other values of `parent:` are not supported. The stream
of a value is not in the output.

A field with a `size` and no `type` is `bytes`, as in Kaitai Struct.

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

Two schemas may import each other (the types of each refer to those of the
other); a schema that is already being loaded is not loaded again.

## Kaitai Struct compatibility

doe reads definitions written in the [Kaitai Struct](https://kaitai.io/) language:
a `.ksy` file works as a schema (`doe file.bin format.ksy`), and an import
resolves as Kaitai Struct resolves it (a name is looked for next to the
importing file and then in the include paths, a name with a leading slash in the
include paths only). The `meta` keys that do not affect parsing (`title`,
`license`, `file-extension`, ...) and `doc-ref` are accepted and ignored, and so
are the `ks-*` options, `to-string` and the keys of the Kaitai Struct tools that
start with `-` (`-orig-id`, `-webide-representation`, ...). A field without an
`id` is read, and named `_unnamed` and its position in `seq`. [`conformance/`](https://github.com/shroomsword/doe/tree/main/conformance) checks that doe and the
reference compiler's parser produce the same result for a set of formats and
samples, among them definitions from the Kaitai Struct gallery, which it keeps
as a git submodule (`git clone --recurse-submodules`, or `git submodule update --init`
to run the tests in a clone without it).

### The Kaitai Struct gallery

All 190 definitions of the [format gallery](https://github.com/kaitai-io/kaitai_struct_formats)
compile. The conformance suite parses real files with these definitions and
checks the result against the reference compiler's:

| Directory | Formats |
| --- | --- |
| `archive` | `gzip`, `zip` |
| `database` | `dbf` |
| `executable` | `elf`, `java_class`, `mach_o`, `microsoft_pe` |
| `image` | `bmp`, `gif`, `jpeg`, `png` |
| `media` | `wav` |
| `network` | `ethernet_frame` (with `ipv4_packet`, `tcp_segment`, `udp_datagram` and the other formats it imports), `pcap` |

`sqlite3` is left out on purpose: its definition reads every column as a string
(the `if` that should limit `as_str` to text columns is commented out), which
only works in the JavaScript runtime of the reference, where a missing size reads
as an empty string; doe reports the missing size as an error.

The others compile, but no file has been compared to the reference yet, so one
of them may still fail on a file, or read it differently. Give the gallery as an
include path, so that the imports between its definitions resolve:

```sh
doe -I kaitai_struct_formats image.png kaitai_struct_formats/image/png.ksy
```

What is supported is what this README describes, with Kaitai's names and syntax:
`seq`, `types`, `enums`, `instances`, `params`, `imports`, `valid`,
`switch-on`/`cases`, `repeat`, `if`, `contents`, `size`, `size-eos` and `pos`;
the numeric, bit and string types; and Kaitai's expression language, including
the word operators `and`, `or` and `not`.

What is different:

- The names of the numeric types are Kaitai's, and `u8` is 64 bits (see the
  upgrade note under [Primitive types](#primitive-types)).
- Output lists the instances of a type after its fields, and a failing instance
  aborts the parse.
- `%` and `/` accept a negative divisor and round towards negative infinity;
  the JavaScript runtime of Kaitai Struct refuses a divisor that is not positive.
- `substring` out of range is an error, where the runtimes differ.
- An enum member can be called `true` or `false`, and its number can be
  negative; a signed field that reads a negative number has a negative enum
  value.

What is not supported yet, and fails when a definition uses it: a `terminator`
of more than one byte; and encodings other than those in
[Encodings](#encodings).

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

Arguments:
  [FILE]            Binary file to parse
  [TYPE_OR_SCHEMA]  Type name (e.g. `png`) or path to a YAML schema file

Options:
  -I, --include-path <DIR>   Add a directory to the schema search path (repeatable)
  -c, --config <FILE>        Config file path [default: ~/.config/doe/config.toml]
      --json                 Emit compact JSON output
      --pretty               Emit pretty-printed JSON output
      --no-instances         Leave out the values a schema's `instances` compute or read elsewhere
      --indent <STR>         Indentation string for text output [default: "  "]
  -h, --help                 Print help, including available types from the include paths
      --color <WHEN>         When to use color on stderr [default: auto] [possible values: auto, always, never]
      --log-level <LEVEL>    Verbosity of the log file [default: debug] [possible values: off, error, warn, info, debug, trace]
      --log-file <FILE>      Write the log file here [default: the XDG state directory]
  -v, --verbose...           Increase logging verbosity
  -q, --quiet...             Decrease logging verbosity
      --completions <SHELL>  Print a shell completion script to stdout and exit [possible values: bash, elvish, fish, powershell, zsh]
      --man[=<PAGE>]         Print a manual page (roff) to stdout and exit: `doe` (the default) or `schema`, the schema format [possible values: doe, schema]
  -V, --version              Print version
```

`FILE` is the binary to parse. `TYPE_OR_SCHEMA` is treated as a schema path if
it starts with `.`, `/` or `~`, or ends with `.yaml`/`.yml`. Otherwise it is a
bare type name resolved through the configured include paths.

Results go to stdout; everything else (errors, warnings, progress) goes to
stderr. `--completions` and `--man` print and exit; they cannot be combined with
a file, a type or the options that only matter when parsing (`-I`, `-c`,
`--json`, `--pretty`, `--no-instances`), but `--color`, `-v`, `-q`, `--log-level`
and `--log-file` can go with them, as in `doe --completions bash --color never`.

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

Schemas do not have to be files. A compiler reads them from a `SchemaSource`:
the filesystem by default, or a `MemorySource` that holds files by path, for a
program with no filesystem (such as WebAssembly in a browser). Imports are found
by the same rules, next to the importing file and then in the include paths:

```rust
use std::path::{Path, PathBuf};
use doe::schema::compiler::Compiler;
use doe::schema::source::MemorySource;

let files = MemorySource::new()
    .with_file("main.ksy", "meta:\n  id: main\n  imports:\n    - common\nseq: []\n")
    .with_file("lib/common.ksy", "meta:\n  id: common\nseq: []\n");
let mut compiler = Compiler::with_source(vec![PathBuf::from("lib")], Box::new(files));
let main = compiler.read_schema(Path::new("main.ksy"))?;
compiler.process_schema(&main)?;
assert!(compiler.registry.contains("common"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

To know where in the input every value came from (to select the bytes of a
field, or the field of a byte), parse with `parse_type_traced`. It returns the
same value and a tree of `TraceNode`s with the bit range of each field, element
of a repeated field and instance:

```rust,no_run
# use doe::schema::compiler::Compiler;
# use doe::parser::{cursor::Cursor, engine::Engine};
# let mut compiler = Compiler::new(vec![]);
# compiler.load("my_format", None)?;
# let data = std::fs::read("file.bin")?;
# let engine = Engine::new(&compiler.registry);
let (value, root) = engine.parse_type_traced("my_format", &mut Cursor::new(&data))?;
for field in &root.children {
    if let Some(span) = field.span {
        println!("{}: bytes {}..{}", field.name, span.start_byte(), span.end_byte());
    }
}
// The path of children to the innermost field that holds byte 12.
let path = root.find_byte(12);
# Ok::<(), Box<dyn std::error::Error>>(())
```

A field placed with `pos:` has the range it was read from. What is read from the
result of a `process:` transform has offsets in the transformed bytes, and its
nodes have `processed` set.

To keep what was read when a parse fails (to show a schema's author how far it
got), use `parse_type_partial`. It returns a `ParseOutcome` with the value read
so far, the trace and the error:

```rust,no_run
# use doe::schema::compiler::Compiler;
# use doe::parser::{cursor::Cursor, engine::Engine};
# let mut compiler = Compiler::new(vec![]);
# compiler.load("my_format", None)?;
# let data = std::fs::read("file.bin")?;
# let engine = Engine::new(&compiler.registry);
let outcome = engine.parse_type_partial("my_format", &mut Cursor::new(&data))?;
if let Some(error) = &outcome.error {
    eprintln!("failed: {error}");
}
// The fields read before the failure, and the node of the one that failed.
let partial = outcome.value;
let failed = outcome.trace.children.iter().find(|node| node.failed);
# Ok::<(), Box<dyn std::error::Error>>(())
```

A type that failed has the fields before the failing one, a repeated field the
elements before it, and a field that fails a `valid:` rule keeps the value it
read. The value being read when the parse failed has `failed` set in the trace,
and the ones around it end where it started to fail. `parse_type_partial` fails
(as an `Err`) only when nothing could be tried: the type is unknown, or takes
parameters.

## License

MIT
