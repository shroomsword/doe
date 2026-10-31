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
| `io_streams` | instances read with `io:` from `_root._io`, `_parent._io` and the stream of a value, and `parent: _parent` |
| `ipv4_packet` | bit fields that share and span bytes, `valid`, enums, computed instances |
| `tlv_stream` | `switch-on` on an enum, sized windows, `params`, instances in sub-types |
| `bit_enums` | `enum:` and `valid: in-enum` on bit fields, including a single bit, which is an integer when it has an enum |
| `bmp_header` | `valid`, an instance read at a `pos`, floor division, a ternary |
| `gallery_syntax` | the metadata keys of the Kaitai tools, `contents` of strings and bytes, a field without an id, `0b`/`0o` literals, `_` in numbers, `24000.`, unary `+`, a list ending in a comma, negative and `true` enum members, `valid` that names its own field |
| `gif_like` | packed flags, `repeat-until` on a field of the element, an enum case with no body |
| `riff_wave` | string cases, padding, a type that contains itself |
| `process` | `process:` with `xor`, `rol`, `ror` and `zlib`, and a user type read from the result |
| `parent_instances` | instances of the types around a sub-type (`_parent.limit`, `_parent._parent.is_control`), and `type::enum::member` |
| `qualified_field_enums` | `enum: type::name` on fields and instances, with `valid: in-enum` |
| `png_chunks` | `repeat-until` on a string, `strz`, `size-eos`, `valid` with `min` |
| `encodings` | UTF-16 and the legacy encodings the reference decodes (UTF-32 and CP437 are left out: its runtime decodes through `TextDecoder`) |
| `endian_switch` | `meta.endian` with `switch-on` and a default case, a parameter as the selector, a nested type that inherits the choice, an explicit `u2be` |
| `enum_mappings` | enum members written as mappings |
| `expression_features` | `_index` in the expressions, arguments and `repeat-until` of a repeated field, the elements read so far, `x._sizeof`, `_sizeof`, `sizeof<type>`, `.as<type>` and a list of byte literals |
| `import_cycle_a`, `import_cycle_b` | two definitions that import each other |
| `expressions` | arithmetic, bit operations, strings, arrays, floats, conversions |
| `float_types` | floats of both sizes and byte orders |
| `repeat_until_eof` | `_io.eof` |
| `io_position_instances` | `_io.pos` and `_io.eof` in instances: worked out when first used, or at the end of the parse when nothing uses them (#170) |
| `terminated` | `terminator` with `include`, `consume`, `eos-error`, `size` and `pad-right` |
| `value_instance_enum` | a value instance with an enum |
| `import_main`, `import_dep` | `meta.imports` and a type of an imported definition |

## What the reference runtime cannot do

Found while making samples for the gallery; the samples keep clear of them, and
`doe` is the one that is right in each case:

- It reads a signed 64-bit integer wrongly from about -2^40 down (`readS8be` gives
  -1095216660480 for -1099511627776), and any number past 2^53 is a JavaScript
  number with its precision gone (the `protobuf` varint of 2^64 - 1, a `u8` that
  is a double's bits). `doe` is exact. The samples use values below those.
- It reads bit fields of 32 bits at most (`readBitsIntBe: the maximum supported
  bit length is 32`), so the `decimal128` of `bson`, whose definition reads a
  49-bit field, is left out of the sample.
- It cannot read the padding of `id3v2_4` (`size: _root.tag.header.size.value - _io.pos`: `_root.tag` is not yet assigned while the tag is being read), so that sample has a footer and no padding.
- Its dump of a format whose instances point at each other (`iso9660`: the `.`
  entry of a directory is the directory) overflows the stack, so that format has no
  reference output (see `gallery_samples.tsv`).

## A bug in the reference runtime

The JavaScript runtime of Kaitai Struct 0.11.0 skips one byte too many after a
terminated read with `include: true` and `consume: true` (it moves past the
terminator when it takes the bytes and again when it consumes). doe follows the
documented behavior, so `terminated.ksy` does not combine the two on a field
without a `size`.

It is reported upstream as
[kaitai_struct#783](https://github.com/kaitai-io/kaitai_struct/issues/783), and
the fix is in the (still open) pull request
[kaitai_struct_javascript_runtime#26](https://github.com/kaitai-io/kaitai_struct_javascript_runtime/pull/26).
When a runtime release has it, the combination can be added to `terminated.ksy`.

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

## Gallery definitions

`kaitai_struct_formats/` is the [Kaitai Struct format gallery](https://github.com/kaitai-io/kaitai_struct_formats)
as a git submodule, at the commit the submodule points to, so that the suite
needs no network to run once it is checked out (`git clone --recurse-submodules`,
or `git submodule update --init` in a clone without it; the tests do that
themselves when the directory is empty and git can reach the remote). Each definition says in
its `meta.license` how it is licensed; the ones used here are `CC0-1.0`. Nothing
of the gallery is copied into this repository.

The definitions that have samples in `samples/gallery/<dir>/<name>/` are tested
like the other formats, named `gallery/<dir>/<name>`: `gzip`, `zip` (with
`dos_datetime`), `dbf`, `elf`, `java_class`, `mach_o`, `microsoft_pe`, `bmp`, `gif`, `jpeg`, `png` (with `icc_4` and `exif`), `wav`, and the network
formats `ethernet_frame` (with `ipv4_packet`, `tcp_segment`, `udp_datagram` and the
other formats it imports) and `pcap`.
`make_gallery_samples.py` writes the samples, small files that it makes up (the
ELF ones with the C compiler of the machine); the expected output comes from the
reference compiler with `npm run generate`, or `npm run generate -- gallery/archive/zip`
for one of them. `generate.js` takes the underscores out of the keys of an enum (`0x0253_f2f4`), which the YAML reader
of the reference compiler leaves as text it cannot read as a number.

When the tests run locally (`CI` is not set), `cargo test` also checks that the
submodule is at the latest commit of the `master` branch of the gallery, by
asking its remote (`git ls-remote`), and fails with the commands to update it if
it is not. It is skipped in CI, if the remote cannot be reached, and with
`DOE_SKIP_GALLERY_CHECK=1`.

To use a newer gallery, move the submodule (`git -C conformance/kaitai_struct_formats
checkout <commit>`), run `npm run generate`, and review the diff of `expected/`
with the one of the gallery. The gallery has no release tags, so the submodule
points at a commit.

### A plan for files that a tool wrote

`real_files_plan.tsv` has a row for every definition that is tested only with a
file built by hand (or has no sample): the tool that normally writes such a
file, the command that makes one, where to get the tool, and how hard it is
(`easy`: a package of the usual distributions and one command; `medium`: a
tool that has to be built or fetched, or a root or macOS runner; `hard`: the
software is proprietary or from a game, or the file has to come from a device;
`blocked`: something else has to be fixed first, said in the note). A test
keeps the plan in step with `gallery_samples.tsv`: when a format gets a file
that its own tool wrote, its row leaves the plan. Files that are large, or
whose licence does not allow them here, are fetched by their hash and not
committed, as the first criterion of issue #133 says.

## What it does not cover yet

Kaitai Struct features that doe does not have are left out until they exist; the
list is in the "Kaitai Struct compatibility" section of the top-level README.
