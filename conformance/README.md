# Conformance suite

doe reads format definitions written in the [Kaitai Struct](https://kaitai.io/)
language. This directory checks that it reads them the way Kaitai Struct's own
compiler does: each format here is parsed against sample files with doe and
with the reference compiler's parser, and the results must be the same.

```
formats/<name>.ksy         the format definition
samples/<name>/<s>.bin     a sample file (small, made up)
expected/<name>/<s>.json   what the reference parser produced for it
```

`cargo test` runs the comparison (`crates/doe/tests/integration/conformance.rs`);
it reads the committed `expected/` files, so it needs no Node or network.

## What is compared

Both outputs are brought to a common form: keys without case and underscores
(the JavaScript runtime renames `file_size` to `fileSize`), enums as their
numbers, byte sequences as hex strings, integral floats as integers, no
`null`s (a field that was not read is absent in doe and `undefined` in the
reference), no doe metadata (`$type`, `$offsets`), and no type parameters
(the reference exposes them as properties). Instances are included.

## The formats

The definitions are written for this suite, not copied from elsewhere. They are
small models of well-known formats, chosen to cover the constructs real
definitions use:

| Format | Exercises |
|--------|-----------|
| `ipv4_packet` | bit fields that share and span bytes, `valid`, enums, computed instances |
| `tlv_stream` | `switch-on` on an enum, sized windows, `params`, instances in sub-types |
| `bit_enums` | `enum:` and `valid: in-enum` on bit fields, including a single bit, which is an integer when it has an enum |
| `bmp_header` | `valid`, an instance read at a `pos`, floor division, a ternary |
| `gif_like` | packed flags, `repeat-until` on a field of the element, an enum case with no body |
| `riff_wave` | string cases, padding, a type that contains itself |
| `process` | `process:` with `xor`, `rol`, `ror` and `zlib`, and a user type read from the result |
| `parent_instances` | instances of the types around a sub-type (`_parent.limit`, `_parent._parent.is_control`), and `type::enum::member` |
| `qualified_field_enums` | `enum: type::name` on fields and instances, with `valid: in-enum` |
| `png_chunks` | `repeat-until` on a string, `strz`, `size-eos`, `valid` with `min` |
| `encodings` | UTF-16 and the legacy encodings the reference decodes (UTF-32 and CP437 are left out: its runtime decodes through `TextDecoder`) |
| `endian_switch` | `meta.endian` with `switch-on` and a default case, a parameter as the selector, a nested type that inherits the choice, an explicit `u2be` |
| `enum_mappings` | enum members written as mappings |
| `expressions` | arithmetic, bit operations, strings, arrays, floats, conversions |
| `float_types` | floats of both sizes and byte orders |
| `repeat_until_eof` | `_io.eof` |
| `terminated` | `terminator` with `include`, `consume`, `eos-error`, `size` and `pad-right` |
| `value_instance_enum` | a value instance with an enum |
| `import_main`, `import_dep` | `meta.imports` and a type of an imported definition |

## A bug in the reference runtime

The JavaScript runtime of Kaitai Struct 0.11.0 skips one byte too many after a
terminated read with `include: true` and `consume: true` (it moves past the
terminator when it takes the bytes and again when it consumes). doe follows the
documented behavior, so `terminated.ksy` does not combine the two on a field
without a `size`.

## Changing the suite

To add a format, put its `.ksy` in `formats/`, its samples in `samples/<name>/`
(`make_samples.py` writes the existing ones), list it in the `formats!` macro in
`conformance.rs`, and regenerate the expected output:

```sh
cd conformance
npm ci
npm run generate            # or: node generate.js <name>
```

The generator compiles each definition with the reference compiler
(`kaitai-struct-compiler` 0.11.0, which is GPL-3.0-or-later and used only as a
tool here: nothing of it is part of doe or of this repository) to JavaScript
and parses the samples with the result. If the reference rejects a definition or
a sample, so does the generator. Review the diff of `expected/` before
committing it.

## What it does not cover yet

Kaitai Struct features that doe does not have are left out until they exist; the
list is in the "Kaitai Struct compatibility" section of the top-level README.
