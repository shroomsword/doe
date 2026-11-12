# doe

<p align="center"><img src="web/logo.svg" width="144" height="144" alt="doe: a doe in cyberpunk goggles"></p>

[![CI](https://github.com/shroomsword/doe/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/shroomsword/doe/actions/workflows/ci.yml?query=branch%3Amain)
[![Release](https://github.com/shroomsword/doe/actions/workflows/release.yml/badge.svg)](https://github.com/shroomsword/doe/actions/workflows/release.yml)
[![Coverage](https://img.shields.io/endpoint?url=https%3A%2F%2Fshroomsword.github.io%2Fdoe%2Fcoverage.json)](#test-coverage)

decoder of everything

`doe` parses binary files according to a declarative YAML schema and prints
the result as human-readable indented text or JSON. It is useful for
inspecting proprietary formats, reverse-engineering firmware images, and
validating binary protocol implementations without writing a one-off parser
each time.

**Try it in the browser:** the web IDE is hosted at
<https://shroomsword.github.io/doe/>. Open a schema and a file, or pick a format
of the Kaitai Struct gallery, and see the parse tree next to the bytes. It runs in
your browser; nothing you open is uploaded (see [Web IDE](#web-ide)).

## Installation

```sh
cargo install --path crates/doe-cli
doe --install-types
```

`cargo install` puts the `doe` binary in `~/.cargo/bin` and nothing else, so
the types are in the binary and `doe --install-types` writes them: see
[Installing the types](#installing-the-types).

### The web IDE

The web IDE (`doe --ide`, see [Web IDE](#web-ide)) is a page that is built into
the binary. It is not in git: it is made from `web/` with Node and the
WebAssembly toolchain (`npm run build` in `web/`; see `web/README.md`).

- The **release archives** have it in every binary: the release workflow builds
  the page once and fails if it is not in a binary.
- A plain `cargo build` or `cargo install --path crates/doe-cli` needs no Node:
  if `web/dist` is not there, `doe --ide` still starts and warns that the page is
  missing, and serves a page that says so. To have it, run `npm run build` in
  `web/` first and then `cargo build`; `DOE_IDE_DIST` names another directory,
  and `DOE_IDE_REQUIRE_BUNDLE=1` makes the build fail if there is none.
- Every push to `main` publishes the page to GitHub Pages (workflow `pages`), as
  the hosted IDE, which is the same page without the types and files of
  `doe --ide`.
- `cargo build --no-default-features` leaves out `--ide` and the page.

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
doe firmware.bin ./custom_format.yaml
doe dump.bin --json > dump.json
doe packet.bin --pretty -I ~/.doe/types
doe mystery.bin -I ~/.doe/types   # which schema? see Detecting the schema
```

## Type library

Ready-made type definitions live in [`types/`](https://github.com/shroomsword/doe/tree/main/types): the common
file types that the [Kaitai Struct gallery](#the-kaitai-struct-gallery) does not have. They are in the binary
(`doe --install-types`, see [Installing the types](#installing-the-types)); from a
checkout, point `doe` at the directory with `-I` (or list it under `include_paths`
in the config file) and use a type by name:

```sh
doe -I types backup.tar tar
doe -I types --json capture.pcapng pcapng | jq '.blocks[].body.link_type.name'
```

| Type | Describes |
|------|-----------|
| `tar` | tar archives (ustar and V7): every entry's header and contents, up to the zeroed blocks that end it |
| `ar` | Unix `ar` archives (static libraries `.a`, and the layer of a Debian `.deb` package): the members, with their headers and bytes |
| `pcapng` | pcapng packet captures: the section header, interfaces, and enhanced and simple packets (little-endian files) |
| `wasm` | WebAssembly modules: the sections, with their ids and sizes (LEB128) and bytes |
| `npy` | numpy `.npy` array files: the header text and the data |
| `woff` | WOFF web fonts: the header and the table directory |
| `woff2` | WOFF 2 web fonts: the header, the table directory (`UIntBase128` lengths, known tags, transforms) and the tables, from the Brotli stream decoded with `process: brotli` (the contents of a table are bytes) |
| `flac` | FLAC audio: the metadata blocks (stream info, tags, padding...) and the frames as bytes |
| `gzip_file` | gzip files: the header of the first member and the trailer; its payload is the gunzipped file (a `.tar.gz` is shown as the tar) |
| `cpio` | cpio archives: newc and crc (`070701`, `070702`: initramfs, RPM payloads), odc, and the old binary format in both byte orders, with the entries as files and directories |
| `ubi` | UBI images (the volume layer under UBIFS on NAND flash): the headers of every eraseblock and the layout volume's table of volumes |
| `zlib_stream` | zlib streams (RFC 1950): the window size, the level, the dictionary id and the Adler-32, with the inflated data; its payload is the inflated file |
| `snappy` | Snappy framed streams (`.sz`): the chunks with their CRCs, and each compressed block as its literals and copies; `snappy::snappy_block` reads a raw block |
| `compress_z` | Unix `compress` files (`.Z`): the flags (maximum code size, block mode); the LZW codes are bytes |
| `lzop` | lzop files (`.lzo`): the header and its flags, and every block's sizes and checksums; the LZO1X data is bytes |
| `brotli` | Brotli streams (`.br`): the window size and the meta-blocks up to the first compressed one, with the bytes of uncompressed and metadata blocks; the compressed data is bytes; the payload is the decoded stream (`doe FILE.tar.br` shows the tar) |
| `lzma_alone` | LZMA-alone files (`.lzma`): the properties split into `lc`, `lp` and `pb`, the dictionary size and the size; the compressed data is bytes |
| `lzip` | lzip files (`.lz`): the header with the dictionary size decoded, the trailer and the last member |
| `squashfs` | `SquashFS` 4.0 images: the superblock, the id, fragment and export tables, and the root inode and directory (metadata that is stored or a zlib stream) |
| `wav_audio` | WAVE audio (RIFF): the format, the sample bytes and the tags, with the text of the tags read as UTF-8 |
| `pdf` | PDF: the header, the classic cross-reference table and trailer, and the objects as tokens (no stream data or typed views) |
| `mp4` | MP4 / ISO base media: the movie and tracks, the sample tables and entries, tags and fragments (the samples are not decoded) |
| `flv` | Flash Video: the header and the tags with their sizes and times |
| `tzfile` | time zone files (TZif): the transitions and local time types of the first data block |
| `psd` | Photoshop documents (PSD and PSB): the header, the image resource blocks, the lengths of the other sections |
| `bzip2` | bzip2 streams: the header and the first block's magic, CRC, origin pointer and symbol-map ranges (a file of several streams shows the first) |
| `xz` | xz files: every stream (found from the end), its index and its blocks with their headers and filters; the compressed data is bytes |
| `zstd` | Zstandard files: data and skippable frames, the window, the blocks, the literals header and sequence count of compressed blocks, and the seekable format's seek table |
| `lz4` | LZ4 files: standard, legacy and skippable frames, the blocks with their sizes and checksums, and the sequences of a compressed block |
| `coff` | COFF object files: the header, the sections, and the symbols and string table found from the header |
| `mpegts` | MPEG transport streams: the 188-byte packets with their headers, adaptation fields and payloads |
| `mp3` | MPEG audio layer III: the frames with their headers, and bit rate, sample rate and length worked out |
| `cab` | Microsoft Cabinet files: the folders, the file entries and the data blocks |
| `sevenzip` | 7-Zip archives: the signature header, and the packed streams and the next header as bytes |
| `tga_image` | TGA images: the header, the colour map, the image data as bytes and, in a file of version 2, the footer, the extension area (only when there is one) and the developer directory |
| `mar_archive` | Mozilla update archives (`.mar`), with or without the size and signatures that the gallery's `mozilla_mar` always reads (the files of the Python package `mar` have none): the index and the files in it |
| `efi_signature_list` | UEFI signature lists (`.esl`, as `cert-to-efi-sig-list` writes them) without the 4 bytes of attributes that the file of an EFI variable (the gallery's `efivar_signature_list`) starts with |
| `zip_archive` | ZIP archives (and `.jar`, `.apk`, `.docx`...): the entries read from the central directory, with their local headers and data; an archive with a comment is not read |

Each type begins with a comment that says what it covers, its limits and the specification it follows, and
has a sample file in `types/samples/` that `types/make_samples.py` makes (nothing is copied from another
project) and a test of its values. [`types/README.md`](https://github.com/shroomsword/doe/blob/main/types/README.md)
lists the formats that the gallery lacks, which of them the library has, and why the others are not in it.

**ELF files** are read by the gallery's `elf.ksy`, which handles both byte orders and word sizes, notes
and dynamic sections (install the types with `doe --install-types`, or check the gallery out as the submodule
`conformance/kaitai_struct_formats`, or clone it, and point `-I` at it):

```sh
doe -I conformance/kaitai_struct_formats /bin/ls elf
doe -I conformance/kaitai_struct_formats --json libc.so.6 elf | jq '.header.program_headers[].type.name'
```

Running `doe --help` with `-I types` lists the available types, and
`doe -I types --list-types` prints them for a script: one per line with the id,
the file (relative to the type path) and the first line of its `doc`, separated
by tabs when the output is not a terminal (aligned when it is). `--json` makes it
an array of `{id, path, type_path, doc, signature, error}`, where `signature` says
whether the type starts with a signature (so `--detect` tries it first) and `error`
why a type cannot be compiled. A schema file that cannot be read as YAML is listed
with no id and its error; the command still exits 0, but a type path that is not a
directory is an error. `doe --list-types SCHEMA` lists the types that a schema
defines (`SCHEMA::TYPE`) and the ones that it imports.

A type path is searched through its subdirectories, and a type is found by the
`id` in the file (`meta.id` in a `.ksy` file) and not only by the name of the
file. With the Kaitai Struct formats checked out, this reads `ls` with
`executable/elf.ksy`:

```sh
doe -I conformance/kaitai_struct_formats /bin/ls elf
```

A file of the name is tried first (`elf.ksy` or `elf.yaml` in the type path).
Two files below one type path that declare the same id are an error that
names them; the same id in two type paths is not, and the earlier path wins.
The search goes eight directories deep and looks at 20,000 files at most, and
does not follow a link to a directory.

### Installing the types

The [type library](#type-library) and the [Kaitai Struct gallery](#the-kaitai-struct-gallery)
(about 190 formats, 2 MB) are built into the binary, so a `doe` from
`cargo install` or a release has them. `doe --install-types` writes them to
`~/.doe/types` (the directory named by the environment variable `DOE_TYPES_DIR`
when it is set), in two folders: `doe/` (this library) and `kaitai/` (the
gallery, in its own folders). `doe` searches that directory **last**, after the
`-I` paths and the config's `include_paths`, so types of your own win; a type is
found by its name wherever it is below the directory:

```sh
doe --install-types
doe backup.tar tar
doe firmware.bin           # the schema is looked for among them too (Detecting the schema)
doe --list-types           # shows them, the directory is a type path
```

`--install-types=DIR` writes to another directory (add it with `-I` or
`include_paths`; a package can install to `/usr/share/doe/types` this way).
A manifest, `.doe-types`, beside the files records the version of `doe` and a
hash of each file as it was written. Run the command again after an upgrade:
files that are as `doe` wrote them are replaced by the new version's, files that
were removed from the library are removed, and **files that you changed are kept**
and listed (`--install-types --force` replaces them too). Put your own types
elsewhere or under other names, and they are never touched.

Without the directory, `doe FILE` says to run `doe --install-types` instead of
"no schema fits". `default_types = false` in the [config file](#configuration)
leaves the directory out of the search whatever is installed. A build without
the `bundled-types` feature (`cargo install doe-cli --no-default-features
--features tui,ide`) has no types in it and `--install-types` says so.

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
| `process` | Transform the bytes of the field's `size` window before they are parsed: `xor(key)` (an integer or a byte sequence, repeated), `rol(n)`, `ror(n)` (rotate each byte), `zlib` (a zlib stream, RFC 1950), `deflate` (a raw deflate stream, RFC 1951) or `gzip` (gzip data, RFC 1952: every member, with its header read and its CRC-32 and size checked) or `brotli` (a Brotli stream, RFC 7932: the tables of a WOFF 2 font, the body of a `Content-Encoding: br` response; bytes after the end of the stream are not looked at), `bzip2` (every stream, the CRCs checked), `xz` (every stream and its padding; the LZMA2 data, the delta and branch filters, and the CRC32, CRC64 or SHA-256 checks verified), `lzma` (a `.lzma` file, LZMA alone), `lzip` (every member, the CRC-32 and the sizes checked) or `zstd` (every frame, skippable frames skipped, the content checksum verified). For `bytes`, `str` and user types that have a `size` or `size-eos`. They all decompress into memory up to 256 MiB, enforced while decoding; a larger result, or data that is not what the process says, is an error that names the field |
| `pos:` | Read the field at this byte offset instead of the current position; see [Seeking](#seeking) |
| `valid:` | Fail the parse unless the value read satisfies the rules; see [Validating values](#validating-values) |
| `enum:` | Map the parsed integer to a named variant. `name` is an enum visible in the type; `type::name` is one declared in another type (a nested path is allowed) |
| `contents:` | Assert fixed bytes (magic numbers, file signatures) |
| `doc:` | Human-readable description of the field or instance; the web IDE and the terminal viewer show its first line after the value in the tree (`D` hides it) and the whole text under the tree or in the inspector |

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

Bytes that are not valid in the encoding do not stop the parse: the string has
`U+FFFD` where UTF-8, UTF-16, UTF-32 or a legacy encoding could not decode them
(and, for ASCII, the character of Latin-1 for a byte above `0x7f`), as the
reference runtime reads them, and the field after it is read as usual. The
problem is reported where the value is shown:

- on the command line, the whole output is written first, and then the error
  (exit status 1) lists each string: the type and field, the encoding, the byte
  of the string and the byte of the input;
- in the terminal viewer, the row has a `!` in its first cell and the bar says
  that the text is not valid in its encoding;
- in the web IDE, the row of the tree has the flag *not valid text* (the tooltip
  has the message) and the Status pane lists each string as a problem of kind
  `text`; a click selects the field;
- in the library, `Engine::parse_type_reported` and `parse_file_report` return the
  strings with the value, a traced parse has the message on the node
  (`TraceNode::problem`), and `ParseOutcome::problems` has the list.

A byte order mark is not removed. The legacy encodings are decoded by
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
A field that is not there (its `if:` was false) is equal to nothing: `vr == 'SQ'` is false and
`vr != 'SQ'` true when `vr` was not read, and as a `size:` it is 0 (as in the reference, where a missing
size reads nothing); any other use of it is an error. The same goes for an instance whose `if:` is false.

Expressions are typed. A field's value is an integer, a float, a boolean, a
string, a byte sequence, an array or a sub-type. `size:`, `repeat-expr:` and
`pos:` need an integer; `if:` needs a condition. A `bytes` field longer than
64 KiB, and an array or sub-type of more than about a million values, are too
large to copy into an expression: only their `.size` can be used.

`_io` is the stream the expression is in, which is a window for a sized
sub-type: `_io.pos` (bytes from its start), `_io.size` and `_io.eof`. The stream of a value that was parsed
earlier is `value._io` (`_root.header._io.size` is the size of the window of `header`; its `.pos` and `.eof` are what they were when `header` was done). An enum
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

A path from `_root` can go into a field whose type is still being read, as it
can in the runtimes of Kaitai Struct that create an object before they read it
(Java, C#, Python): in `size: _root.tag.header.size.value - _io.pos`, inside
`tag`, the fields of `tag` that were read before are there. A field that has
not been read yet is not (the JavaScript runtime has none of this).

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
amount must be from 0 to 126. `~` of a number from 2^31 to 2^32-1 (a `u4` with its top bit set) inverts it in 32 bits, as the reference does, so `~0xFFFFFC17` is `1000`. `&&`, `||`, `!` and `?:` also take an integer as a
condition, where non-zero is true.

**Strings** can be compared (`==`, `<`, ...) and joined with `+`; a string or
array built by an expression is limited to 1 MiB.

**Position and size:** in a repeated field, `_index` is the position of the
element (0 for the first) in its `size`, `type` arguments, `switch-on` and
`repeat-until`, and the elements read so far are the field's name
(`groups[_index - 1].total`). `field._sizeof` is the number of bytes a field
took (Kaitai Struct allows it for fields of a fixed size only; for those of the root type it is known before the field is read, as `_root.header._sizeof`, and so is that of a field of a fixed size of an instance or of a field of the root, such as `footer.op._sizeof`, which does not read `footer`), `_sizeof` the
size of the type the expression is in, and `sizeof<type>` and `bitsizeof<type>`
the size of a type or of a primitive such as `s4` or `b12`, in bytes or bits;
for a type, it is an error unless every field of the type has a fixed size (a
field repeated a number of times written in the schema, `repeat-expr: 4`, and a
string with a terminator cut out of a window of a given `size`, count).
`x.as<type>` is `x`: it names which type a value of a `switch-on` is, and is not
checked. A list of integer literals that all fit in a byte (`[0x49, 0x49]`) is a
byte sequence, so `magic == [0x49, 0x49]` works for a `bytes` field.
`.as<bytes>` on a list of numbers makes it a byte sequence (an item that is not
0 to 255 is an error), so `[a, b].as<bytes>.to_s("ASCII")` is a string. A repeated
element that takes no bytes is allowed, up to a few thousand more than the bytes
read.

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
value as `_`, and may use the instances of the type, also those that are
computed from the value being checked (`is_set`, `value & 1 != 0`). The value
checked is the one read, before it is named after an enum member, and with `repeat` every element is checked. A field with `if:`
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

A `value:` instance of the root type whose expression is a constant (`value: 3`) is known from the start, so a
nested type can read it as `_root.name` before the end of the root.

A `seq` field or another instance that uses an instance in a `size`, `if`,
`pos`, `repeat`, `switch-on` or `value` expression gets it computed first, in
dependency order; two that depend on each other are an error. Every instance
is also computed after the sequence, in schema order, and appears in the output
after the fields of `seq`: in text as `name @0x40` for one that was read (no
offset for a computed value), and in JSON inline, with the offsets of the ones
that were read in a `$offsets` object next to them. Any error aborts the parse.

A `value:` instance that asks where a stream is (`_io.pos`, `_io.eof`, also
through the instances that use it) is worked out, as in the reference
compiler's parsers, when it is first used, with the stream where it is then:
by the `if:` of a field only if the condition gets as far as using it (`a and
instance` does not when `a` is false), and, when nothing uses it, once the
whole input has been read, with the stream where the parse left it. So an
instance `value: _io.pos` of a type that is read in the middle of the input is
the size of what was read, not the place where its type ended.
`--no-instances` leaves them out of the output (those that fields use are still
computed), and so does `ParseOptions { instances: false }` with
`parse_file_with`. An instance that cannot be computed where its type ends,
because it uses something that is read later (`_parent.method` of a field after
it, or `_root.header.names` while `header` is still being parsed), is computed
once the whole input is read, as Kaitai Struct computes instances on first use,
and still appears in its place in the output; one that still cannot be computed
then, or two that need each other, abort the parse. An instance that reads a type at
the place and in the stream where an instance around it is reading the same type (the `.` entry of a
directory, which leads back to the directory) is not read again: it is shown as absent, with the offset
it would be read from. A value that was copied from
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
id: custom_format
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

As in Kaitai Struct, a type of an imported schema is its own `_root` where it is
used from another schema, and has no `_parent`: `_root.magic` in `mach_o` is the
magic of the Mach-O file, also when `mach_o_fat` reads it as the object of one of
its architectures, and `_root._io` is the stream of that object.

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
compile. How much more is known is in a table with a row for every definition,
[`conformance/gallery_samples.tsv`](conformance/gallery_samples.tsv): the files it
is tested with, where they come from and what allows them to be here. A test fails
if a definition has no row, and `python3 conformance/gallery_table.py` adds the rows
of new ones. The numbers (a test checks that this line is current, and the CI log
prints it):

> 190 definitions: 76 with a file written by its own tool, 99 with a file built by hand, 0 failing on a file, 9 part of another, 6 without a sample

The first kind are written by the tool that normally makes the format
(`conformance/make_real_samples.py` and `make_gallery_samples.py` run them: the
`gzip`, `zip` and `pickle` modules of Python, the C compiler for ELF, `clang` for
Mach-O, `cpio`, `ssh-keygen`, `openssl`, `grub-mkfont`, and glibc for `utmp`);
the second are valid files that `conformance/make_gallery_samples.py` and
`conformance/make_samples_by_hand.py` build byte by byte, from the specification
of the format (the RFC, the format's documentation) and not from the definition, so they show that `doe` reads the format as the reference compiler does,
not that it reads files from the world. Both are parsed by the conformance suite
and compared with the reference compiler's output:

<!-- gallery-formats:begin -->
| Directory | Formats |
| --- | --- |
| `3d` | `gltf_binary` |
| `archive` | `android_bootldr_asus`, `android_bootldr_huawei`, `android_bootldr_qcom`, `android_dto`, `android_img`, `android_sparse`, `cpio_old_le`, `gzip`, `lzh`, `mozilla_mar`, `phar_without_stub`, `rar`, `respack`, `rpm`, `xar`, `zchunk`, `zip`, `zisofs` |
| `cad` | `monomakh_sapr_chg` |
| `common` | `utf16_with_bom`, `utf8_string` |
| `database` | `dbf`, `gettext_mo`, `sqlite3`, `tsm` |
| `executable` | `android_nanoapp_header`, `dex`, `dos_mz`, `elf`, `java_class`, `mach_o`, `mach_o_fat`, `microsoft_pe`, `python_pyc_27`, `swf`, `uefi_te` |
| `filesystem` | `amlogic_emmc_partitions`, `android_super`, `apm_partition_table`, `apple_single_double`, `btrfs_stream`, `cramfs`, `gpt_partition_table`, `iso9660`, `luks`, `mbr_partition_table`, `tr_dos_image`, `vdi`, `vfat`, `vmware_vmdk`, `zx_spectrum_tap` |
| `firmware` | `andes_firmware`, `broadcom_trx`, `ines`, `uf2`, `uimage` |
| `font` | `grub2_font`, `pcf_font`, `ttf` |
| `game` | `allegro_dat`, `doom_wad`, `dune_2_pak`, `fallout2_dat`, `fallout_dat`, `ftl_dat`, `gran_turismo_vol`, `heaps_pak`, `heroes_of_might_and_magic_agg`, `heroes_of_might_and_magic_bmp`, `minecraft_nbt`, `quake2_md2`, `quake_mdl`, `quake_pak`, `renderware_binary_stream`, `saints_row_2_vpp_pc`, `warcraft_2_pud` |
| `geospatial` | `shapefile_index`, `shapefile_main` |
| `hardware` | `dtb`, `edid`, `mifare/mifare_classic` |
| `image` | `bmp`, `dicom`, `exif`, `gif`, `gimp_brush`, `icc_4`, `ico`, `jpeg`, `nitf`, `pcx`, `pcx_dcx`, `pif`, `png`, `psx_tim`, `tga`, `webp`, `wmf`, `xwd` |
| `log` | `aix_utmp`, `glibc_utmp`, `hashcat_restore`, `mcap`, `sudoers_ts`, `systemd_journal`, `windows_evt_log` |
| `machine_code` | `code_6502` |
| `macos` | `compressed_resource`, `ds_store`, `mac_os_resource_snd`, `resource_compression/dcmp_0`, `resource_compression/dcmp_1`, `resource_compression/dcmp_variable_length_integer`, `resource_fork` |
| `media` | `android_opengl_shaders_cache`, `au`, `avi`, `blender_blend`, `creative_voice_file`, `genmidi_op2`, `id3v1_1`, `id3v2_3`, `id3v2_4`, `magicavoxel_vox`, `ogg`, `quicktime_mov`, `standard_midi_file`, `stl`, `tracker_modules/fasttracker_xm_module`, `tracker_modules/s3m`, `vp8_duck_ivf`, `wav` |
| `network` | `bitcoin_transaction`, `dime_message`, `dns_packet`, `ethernet_frame`, `hccap`, `hccapx`, `icmp_packet`, `ipv4_packet`, `ipv6_packet`, `microsoft_network_monitor_v2`, `packet_ppi`, `pcap`, `rtcp_payload`, `rtp_packet`, `rtpdump`, `some_ip/some_ip`, `some_ip/some_ip_container`, `some_ip/some_ip_sd`, `tcp_segment`, `tls_client_hello`, `udp_datagram`, `websocket` |
| `scientific` | `nt_mdt/nt_mdt`, `nt_mdt/nt_mdt_pal`, `spectroscopy/avantes_roh60`, `spectroscopy/specpr` |
| `security` | `efivar_signature_list`, `openpgp_message`, `ssh_public_key` |
| `serialization` | `asn1/asn1_der`, `bson`, `chrome_pak`, `google_protobuf`, `microsoft_cfb`, `msgpack`, `php_serialized_value`, `python_pickle`, `ruby_marshal` |
| `windows` | `windows_lnk_file`, `windows_minidump`, `windows_resource_file`, `windows_shell_items` |
<!-- gallery-formats:end -->

Every format that has a file is read to match the reference compiler (or, for `iso9660`, checked
against fields written in the tests). A format that fails on a file gets a row that says
`failing`, with its issue, and its file stays out of the suite until it is fixed.
A file is never made up to fit `doe`: the files built by hand follow the format's
specification and are checked against the reference compiler, and a format
without any says so in the table. A real file of such a format, from a tool or
from the world, is always welcome in place of a built one.

The others compile, but no file has been compared to the reference yet, so one
of them may still fail on a file, or read it differently (the web IDE's *Browse...* marks
the formats that have a file: ✓ for one written by its own tool, ○ for one built by hand). Give the gallery as an
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

## Web IDE

`doe --ide` serves the web IDE on this machine and prints its address:

```sh
doe --ide                 # http://127.0.0.1:8765/
doe --ide --port 0 --open # any free port, and open the browser
```

The page is built into the binary, so nothing else is needed, and it works
offline. It shows a schema, the tree of what it parses, the bytes and the
problems, in step with each other; parsing runs in the browser (WebAssembly) and
nothing is sent anywhere. Files are opened in the page, up to 256 MiB (but see below).

Under the editor is the list of the **schemas** that are open: the main one
first, then each file it imports, with its id, where it came from (typed in the
editor, opened from a file, added, fetched from the gallery, from the doe type
library, or from the type library of this machine) and a cross with the number of its problems; imports
that are not there are listed too, with the file that asked for them. Picking an
entry shows its text in the editor, and what was added or typed can be edited;
what was fetched from the gallery or a library is read-only until *Edit a copy*
makes it the person's own. The title of the Parsed pane says which type the file is parsed as (the one chosen in *Parse as*, else the id of the main schema, also while the parse fails). *Parse as* chooses the type to parse with from the types of all the
open schemas (the main schema's own, by default); the choice is part of a share
link and of the saved work, and the status line says which type was parsed.

*Browse...* in the title bar of the Schema pane opens one list of every type that can be used: the
types of the open schemas, the type library of this machine (with `doe --ide`),
the [doe type library](#type-library) (`tar`, `pcapng`, `wasm`...) and the formats
of the Kaitai Struct gallery, in one list sorted by name and then path (case and
accents do not matter, and a number counts by its value). The path says where a
type is from: `open: …` for a schema in the editor, `local: …` for the library of
this machine, `library/…` for doe's own and the directory and file of the gallery
(`archive/zip.ksy`), so typing `library/` or `open:` in the filter narrows the
list to a source. Each has its id, its file and its doc or title, and for the gallery and the doe
library whether it has a signature (`doe` tries it first on a file that starts
with it) and a ✓ or ○ when it was tried on a file (the doe library's types all
have ○: a sample built by hand). The filter above the list keeps the entries that
have every word you type in the id, the path or the doc. *View* reads the text of
one (read-only; `Esc` goes back to the list), and *Use* parses with it: an open
schema's type is set as *Parse as*, and a format of the library or the gallery is
opened in the editor, as picking it from the *Types* list does (the header's list
of the types of this machine, with `doe --ide`). An
open schema that does not read has its error and cannot be used. The dialog is a
native one: it is reached with the keyboard, `Esc` closes it, and it fits a phone.
A type that starts with a **signature** says so: the page asks `doe`
for the signature of each open type, and of each type of the library of this
machine, while the dialog is open (the title of the list says how far it has
got), and a type whose schema does not compile has its error. *Rank by the opened
file* puts the types in the order in which *Detect* would try them on the
file: first those whose signature the file starts with (and whose name fits),
then those with no signature whose name fits the file's extension; the others
are left out, and each line says why it is there. The ranking waits until every
signature is known. It is the order of trying, not the result of it: *Detect
schema* does the parsing that tells which one fits.

**Integers** are decimal at first. *Integers*, at the right end of the Parsed pane's title bar, writes all of them in
hexadecimal (`0x001f`), binary (`0b00011111`) or octal (`0o37`), with the digits
padded to the width of the type (a bit field has as many binary digits as bits),
negative numbers as a sign and magnitude (`-0x1f`), and enum members with their
number in the radix next to the name (`text (0x01)`). Selecting an integer field
shows *This field*, a radix of its own that holds for every element of a list and
every parse of the same schema (it is kept by the type and the name of the field,
saved with the work). A 64-bit number is exact: it is
never turned into a JavaScript number. The choice of *Integers* is remembered in
the browser; a saved file and the command line print decimal, as they did.

Four things make the page pleasant to come back to:

- **Links:** the page no longer makes share links, but a link that an earlier
  version made (a schema and its imports in the part after `#s=`, compressed; it
  never held the file) still opens the same schema.
- **Saved work:** the schema, its imports and the name of the last file are kept
  in this browser (`localStorage`) and come back on reload; they are never sent
  anywhere, and clearing the site's data in the browser (or emptying the editor)
  starts the page empty. The file itself is not kept: open it again.
- **Gallery:** the list of the [Kaitai Struct gallery](https://github.com/kaitai-io/kaitai_struct_formats)
  formats is in the page, in *Browse...*; using one downloads its definition (and what it
  imports) from GitHub at the commit that the page was built with. The
  definitions are not part of the page, since not all of them are CC0.
  The list has the **doe type library** (`types/`, see
  [Type library](#type-library)): those definitions are small and part of this repository, so they
  are served with the page (`library/NAME.yaml`) and nothing is fetched from
  GitHub for them. They are in the same list as the gallery's, so *Detect*
  ranks and tries them together by signature and name, and the list is made from
  `types/` when the page is built: a new type is there without a change to the
  page. A shared link holds the schema's text, so it opens the same type.
- **Save:** the save button (a disk icon) in the Parsed pane's title bar opens a
  dialog to save the whole parsed value as `doe` prints it on the command line:
  as text, or as indented JSON (`--pretty`), under the name in the dialog. In the
  browsers that let a page open the system's save dialog (Chromium-based ones),
  *Browse...* chooses the folder and the file; in the others (Firefox, Safari) the
  browser decides where a download goes, and the dialog says so. The file is made
  and written in the browser.

`doe --ide` also offers what a page in a browser cannot see on this machine:

- the **types** found through the include paths (`-I`) and the config, in a
  list in the header; choosing one opens its schema with the files it imports.
  The list **follows the files**: the server watches the include paths and the
  config file with the file system's change notifications (inotify on Linux,
  `FSEvents` on macOS, `ReadDirectoryChangesW` on Windows), and a second or so after a
  `.yaml` or `.ksy` file is added, changed or removed, or the config changes
  `include_paths`, the list in the header and *Browse...* are made again, with no restart and
  no reload of the page (your filter and the type you chose stay). Nothing polls: neither
  the server nor the page. The page holds one connection to the server for the news
  (`GET /api/types/events`) and says so in the header when it is lost. If the type
  that is open was **changed on disk**, the page says so and offers *Reload it*; the text in
  the editor is never replaced without it. A library that cannot be read (a duplicate type id, a
  mistake in the config) is said in the header and the last good list stays. Where
  notifications cannot be had (a watch limit, a folder on a network drive, a build
  without them) the header says that the libraries are not watched, and the reload
  button (the arrow next to the list, or the key `L`) reads them again at once;
- a **file named on the command line** (`doe --ide big.bin`), as the input of
  the page (the button *Use big.bin*), however large it is. It is read and
  parsed here, as `doe` reads files, and the page gets the bytes it shows in
  pieces, so the limit of 256 MiB does not apply. A parse of it cannot be
  stopped by a time limit, only by **Cancel parse**, which drops its answer.

A browser does not tell a page the path of a file it opens, so a file can only
be named on the command line, not chosen in the page.

The server answers on `127.0.0.1` only, serves only the files of the page, and
refuses a request whose `Host` or `Origin` is not itself, so another web page
cannot use it. It reads only the schemas that the include paths list, and the
one file named on the command line; no request can name another path. `--port` and `--open` are only valid with `--ide`. The server is
behind the `ide` cargo feature, which is on by default; build with
`--no-default-features` for a `doe` without it, where `--ide` says so. A binary
built without the files of the page (`web/dist`, see `web/README.md`) serves a
page that says that instead.

### Keys

The page can be driven from the keyboard; `K` lists the keys in the page. The
letters work when the focus is not in a text field (the schema editor, a box),
never with Ctrl, Alt or Meta held, and not while a dialog is open. `Esc` leaves
the editor for its pane, closes a dialog and closes a search. A click outside a dialog closes it too, as `Esc`
does (what was typed in it is not applied, as with `Esc`; a press inside that is let go outside, as when text is
selected, does not close it).

| Key | Does |
|---|---|
| `?` | Help |
| `K` | Show these keys |
| `T` | Choose a theme |
| `o` | Open a file to parse |
| `O` | Open a schema file |
| `t` | Browse the types and schemas, and load one |
| `L` | Read the type libraries of this machine again (with doe --ide) |
| `i` | Add imports |
| `d` | Detect the schema of the open file |
| `s` | Save the parsed value |
| `F` | Switch the Parsed pane between the files of an archive and the tree of fields |
| `m` | Show the MD5 of each file in the Files view, then the SHA-1, then the SHA-256, then hide them |
| `M` | The previous checksum of the Files view (SHA-256, SHA-1, MD5, none) |
| `S` | Copy the list of checksums of the Files view, in the format of `md5sum -c` |
| `v` | Show a Markdown file of the Files view rendered, or as its source |
| `Tab` | Next pane (from a pane, not from a button or a field) |
| `Shift+Tab` | Previous pane |
| `h` | Focus the pane on the left |
| `j` | Focus the pane below |
| `k` | Focus the pane above |
| `l` | Focus the pane on the right |
| `Alt+Shift+h` | Move the pane to the left (it swaps places with that one) |
| `Alt+Shift+j` | Move the pane down |
| `Alt+Shift+k` | Move the pane up |
| `Alt+Shift+l` | Move the pane to the right |
| `R` | Put the panes back in their places, all shown, at their default sizes |
| `x` | Close the focused pane (the Schema pane has its X button) |
| `p` | Choose the panes that are shown |
| `P` | Choose a place for the focused pane: a corner or a side of the panes area |
| `f` | Maximize the focused pane, or restore it (Esc restores too) |
| `/` | Search in the focused pane, or in the help when it is open |
| `g` | Go to an offset in the bytes |
| `a` | Turn automatic detection on or off |
| `r` | Change how integers are shown (decimal, hexadecimal, binary, octal) |
| `c` | Set the number of columns of the Bytes pane |
| `b` | Colour the bytes of the Bytes pane by the field they belong to, or not |
| `D` | Show the description (`doc:`) of each field after its value in the Parsed pane, or not |
| `C` | Cancel the parse that is running |
| `1` | Focus the Schema pane |
| `2` | Focus the Parsed pane (shows it if it is closed) |
| `3` | Focus the Bytes pane (shows it if it is closed) |
| `4` | Focus the Status pane (shows it if it is closed) |

`/` searches the pane that has the focus: in **Schema**, with the search panel
of the editor; in **Parsed**, the next node whose name, type or value has the
text (`Enter` for the next match, `Shift+Enter` for the previous one); in
**Bytes**, hexadecimal bytes (`de ad`), text, or an offset (`@1f4`, in
hexadecimal; `@#500` in decimal), which is also what `g` asks for; in
**Status**, the lines of the problems and of the detection report that have the
text. In the help dialog `/` opens a search bar over its text: the matches are marked and counted, `Enter` and `Shift+Enter` go to the next and previous one, and `Esc` closes the bar (a second `Esc` closes the dialog).

**The layout.** The four panes are in four places: three side by side and the fourth under them in a wide
window, two columns with the third and fourth across in a medium one, one column in a narrow one. The **Status pane is
fixed** along the bottom: it cannot be moved, only closed and maximized. **Schema, Parsed and Bytes** can be **docked**:
drag one by the grip at the left of its title bar (with a mouse also by the empty part of the bar or its heading; with a
finger or a pen by the grip) and the area above Status shows **eight positions**, the four corners and the four sides.
The position under the pointer lights up and a dashed outline shows the layout that dropping there makes; `Esc` cancels.
A **side** (left, right, top, bottom) makes the pane a strip of a third along that side, and the other panes keep their
arrangement in what is left; the bottom is directly above Status. A **corner** gives the pane that quarter, and the
other panes fill the rest in their reading order, one column at a time, so that with Schema, Parsed and Bytes side by
side, dragging Parsed to the top right makes Schema the whole left half, Parsed the top right quarter and Bytes the bottom
right quarter, with Status where it was. Dropping on the **middle of another pane** swaps the two. With only two panes
shown the corners are not offered, and with one nothing is. A closed pane leaves the arrangement and comes back to where
it was. The **same moves without the mouse**: `P` (or a click on the grip of a title bar) opens a list of the
eight places, drawn as a map, which the arrows move over and `Enter` chooses; the move is announced to a screen reader
("Parsed docked at the top right: Schema left, Parsed top right, Bytes bottom right"). `Alt+Shift+h`, `j`, `k` or `l`
swap the pane that has the focus with the one next to it in that direction. A window that is too narrow (under 760 px)
puts the panes one under the other, in the order of the arrangement, and does not offer docking; the arrangement comes
back when the window is wide again, and in a medium window it fills the height of the window instead of making the page
longer. The layout is kept in the browser's local storage and applied when the page loads (version 5 of the record, a
tree of splits with a weight for each child; a record of version 4, which had rows of panes, is read as the nearest tree,
and one that is not valid loses the arrangement and not the order or the sizes); if the browser has no storage the page
starts with the default layout. Once the layout is not the default, a button in the header and the key `R` put the panes
back.

**Resizing the panes.** There is a separator in the gutter between each two neighbouring panes (a thin bar that
turns blue under the pointer and when it has the focus). Drag it to make one pane wider (or taller) and its
neighbour narrower (or shorter) by as much; each keeps room for its title bar and buttons (12 rem wide, 6 rem high)
and the drag stops there. `Esc` during a drag puts the sizes back, and a double-click on a separator puts that split
back to the default. A separator can be focused (`Tab` reaches them after the panes) and works as the window
splitter of the ARIA practices does: the arrow keys along the separator (`←` `→` for a vertical one, `↑` `↓` for a
horizontal one) move it by 5% of the room of its two panes (`Shift` by 20%), `Home` and `End` give the room to one
side as far as the minimums allow, and `Enter` puts the split back to the default; the new sizes are announced to a
screen reader ("Schema 40%, Parsed 60%") and the separator has its share in `aria-valuenow`, with `aria-valuemin`
and `aria-valuemax`. A finger or a pen drags a separator too (it is at least 24 px to hit on a touch screen).
In the wide window the separators between the side-by-side panes set their widths, and the one above the low pane
shares the height of the window with it (a docked arrangement has a separator between every two neighbours of each split, and one above Status); in the medium window the one between the columns sets their widths and
the ones between the rows set the height of the row above (the page gets longer or shorter, as it scrolls); in the
narrow window each sets the height of the pane above it. The sizes are proportions, not pixels, and **the three
layouts have their own**: what is set in the wide window is not used in the narrow one. They belong to the places,
not to the panes (the pane in the second place has the second column's width), so a closed pane's room goes to the
panes that are left and comes back when it is shown again. They are kept in the browser's local storage with the
order and the closed panes (version 3 of the layout record; an older record has the default sizes, and sizes that
are not valid are ignored without losing the rest), and Reset (`R`) puts the default sizes back with the order. A
maximized pane hides the separators.

**Closing panes.** Every pane has an X at the right of its title bar, and `x` closes the focused
pane (except Schema, whose X button must be clicked). `p` (or the panes button in the header) opens a list of the four panes with a box
for each that shows or closes it. A closed pane is only hidden, so it keeps what it shows; the panes that are
shown fill the places in their order, side by side in a wide window, with no hole. Which panes are closed is
kept with the order of the panes, and Reset (`R`) shows them all. `2`, `3` and `4` show a closed pane and
focus it, a parse that fails or a detection report shows the Status pane, and `g` shows the Bytes pane, so that
nothing is lost because a pane is closed.

**Files of an archive.** A schema that says its files are file systems (the `filesystem:` key, see
[Archives as files](#archives-as-files)) gets a **Files** view in the Parsed pane: the entries nested by path as
directories and files, with size, mode and time (UTC), in place of the tree of fields. `F` or the Files button (the
file-tree icon) in the title bar of the Parsed pane (it is there only for a file system) switches between the two. Arrows move, `→` and `←`
open and close a directory (and go to the parent), `Enter` or a double-click toggles it, and a click on a name selects the
entry in the tree of fields and its bytes in the Bytes pane (and selecting a field or a byte inside an entry selects the
entry here). A text file has a preview under the list (the first 100,000 bytes, highlighted in the colours of the theme),
and **Save** downloads an entry's contents when the page has all its bytes. The language of the preview comes from the
name of the entry (`main.rs`, `Makefile`, `Dockerfile`, `*.json`…) or, when the name does not say, from a `#!` first line;
the **language list** next to Save shows the guess (`Auto: Rust`) and lets you choose another language or plain text for
that entry. The parsers (JSON, YAML, Markdown, HTML, XML, CSS, JavaScript and TypeScript, Python, Rust, C and C++, shell,
TOML, INI, Dockerfile, SQL, Diff) are loaded the first time a file of the language is shown. A line of more than 2000
characters is cut with `…`, and so is an entry of more than 100,000 bytes.

A **Markdown** entry (`.md`, `.markdown`) has a **Source | Rendered** switch in the preview bar, and `v` does the same from
the keyboard (`r` is the page's key for the radix of integers). Rendered shows headings, paragraphs with emphasis, strong text and
strikethrough, nested lists (also ordered, and task lists with checked and unchecked boxes), block quotes, tables, rules and code blocks, coloured
with the language of the fence when it names one the preview knows. The file is from an archive and is **not trusted**, so
nothing in it acts: raw HTML is shown as the text it is, never put in the page; a link is its text with the target written
out (`text <https://…>`) and is not a link; an image is its alt text (`[image: alt]`, the target in the tooltip) and is never
loaded, so opening an archive cannot make a request. A file nested too deep, or with more than 20,000 elements or 4,000 table
cells, is rendered as far as the limits go and says so. The choice is kept for the rest of the session. Footnotes and math are
shown as text. An entry with an empty name, an absolute path, a `..` or the path of an earlier entry has a warning sign and
is not resolved; a directory shows more than 500 children a page at a time (`N more…`).

**Checksums of the files.** The hash button next to the Files button opens a menu of the checksums (**MD5**, **SHA-1**, **SHA-256** and
None, the one in use marked); `m` goes through them from the keyboard (MD5, SHA-1, SHA-256, none) and `M` goes back. The
checksum is a column of the Files view after the size, 32 digits for MD5, 40 for SHA-1 and 64 for SHA-256, for every file (a directory, a link and the
other kinds have none), and the header of the column names it. It is for comparing a file of an archive with a directory
or another archive without extracting anything. MD5 and SHA-1 are **not security measures**: both have known collisions (the
header's tooltip says so); SHA-256 is the one to compare with a checksum that a project publishes. In a pane too narrow
for the 64 digits the column shows the first 16 and an ellipsis, with the whole digest in the tooltip and in the preview bar. The choice is remembered in the browser and put on the first archive that is opened. The
checksums are made by the worker, a few megabytes at a time, so the page stays usable: `…` shows until a file's digits
arrive, they are kept for the input **for each algorithm** (going back to one is at once), and opening another input drops
them. A file whose entry the schema gives no contents for shows `—`, and so does a file whose stored bytes are not the file:
a **deflated zip entry** has the compressed stream there, and a checksum of that would be wrong (the tooltip says so; a stored
zip entry has its checksum). An empty file has the checksum of nothing. The input must be in the page: a file that stays
on the machine of `doe --ide` shows `—` (its bytes are not loaded here). The preview bar shows the digits of the selected
file with a **Copy** button, and **Save checksums** writes `checksums.md5`, `checksums.sha1` or `checksums.sha256` in the format of `md5sum`,
`sha1sum` and `sha256sum` (`<digits>  <path>`, two spaces, one line for each file in the order of the archive), which `md5sum -c
checksums.md5`, `sha1sum -c checksums.sha1` or `sha256sum -c checksums.sha256` checks in the folder the archive is extracted to; an entry that cannot be
placed safely is left out of it, and so is a file with no checksum; the status line says how many files the list does not have. `S` puts the same list on the clipboard.

**Descriptions of the fields.** A field or an instance that has a `doc:` in the schema shows the **first line** of it after
its value in the Parsed pane, dim and italic, cut to the width of the row with an ellipsis (a row is never wrapped and
the pane does not scroll sideways because of it); the **whole text** is the tooltip of the description and is shown
under the tree when the field is selected. `D`, or the button of the Parsed pane with the speech bubble, shows or hides
the descriptions (they are shown to begin with, and the choice is kept in the browser). The search of the pane (`/`) also
finds a word of a description while the descriptions are shown. It is the description of the **field**: an element of a
repeated field (`[0]`, `[1]`) has none, the field has it once, and a field of a user type shows the field's own `doc:`,
not the one of the type. The text is plain text: `<`, `&` and the markup of Markdown in it are shown as written. The
parameters of a type and the values of an enum have a `doc:` too, and are not shown (they are not rows of the tree).

**Colours of the Bytes pane.** Every byte that belongs to a leaf field is drawn, in the hex and in the character column, in the
colour that field's value has in the Parsed pane: a number, a string, a run of bytes, an enum, a boolean (a struct or an array
has the colours of its fields, and a gap between fields, a field with no kind of its own such as a float, and a byte after the
last field keep the plain colour). The pane reads as a map of the file. The selection is as it was: the selected field's
bytes keep the selection background (the lighter one with the dotted underline for the bytes that a bit field shares),
and over it the text keeps the colour of its kind, or, in a theme where that colour is hard to read on the selection (a
contrast under 3:1), the theme's text colour. A byte that two fields hold takes
the colour of the one of the sequence, then of an instance, like the next-field keys of the terminal viewer. `b`, or the
button at the left of the columns button in the title bar of the Bytes pane, turns the colours off and on; it is kept in the
browser's local storage. The index of which field owns which byte is made after the tree is shown.

**Columns of the Bytes pane.** `c`, or the columns button (an icon of a rectangle in three bands) in the title bar
of the Bytes pane, opens a dialog in which the number of columns is entered: a column is one byte, so the number is the
bytes in a row (1 to 64; the quick choices are 8, 16, 24 and 32, and 16 is what the pane always had). `Enter` or
**Apply** sets it, `Esc` or **Cancel** changes nothing, a number that is not a whole number in range is refused with a
message, and **Reset** goes back to 16. The pane keeps the first byte in view and the selected field, and the number
is kept in the browser's local storage. A row that is wider than the pane scrolls sideways. Cancelling a parse that is
running is `C` (Shift+c).

**Maximizing a pane.** `f` on the focused pane, the maximize button in its title bar (the square; it becomes
two squares, restore, while the pane is maximized) or a double-click on the empty part of the title bar make the
pane fill the page. The other panes are hidden, not closed: they keep what they show and parsing goes on. The same
three, and `Esc` (when no dialog, search or editor is in the way), put the page back. While a pane is maximized
`Tab`, `h j k l` and the moves do nothing, and `1` to `4` for another pane restore the page first. It is a
state of the view and is not kept with the layout.

## CLI reference

```text
doe — binary file parser

Usage: doe [OPTIONS] [FILE] [TYPE_OR_SCHEMA]

Arguments:
  [FILE]            Binary file to parse
  [TYPE_OR_SCHEMA]  Type name (e.g. `png`) or path to a YAML schema file; add `::TYPE` to parse with a type inside it. Without it, the schema is looked for among the types of the include paths (see `--detect`)

Options:
      --detect <WHAT>        When no `TYPE_OR_SCHEMA` is given: `all` also tries the types that have no signature and whose name the file does not carry (slow for a big library). Without it the types whose signature the file starts with, and those named like the file, are tried; `-v` says which [possible values: all]
      --no-unwrap            Show the outermost layer only: a `.tar.gz` is shown as the gzip file it is, not as the tar it holds. Without it, when the schema is looked for and the file holds another file (its schema says so with `payload:`), that is decoded in memory and its schema found, to a depth of 4; `--tui` and `--ide` open the innermost layer from a temporary file that is removed when they are left
      --layer <N>            Show layer N of a file that holds files (1 is the file itself, 2 what it holds...). Without it the innermost layer is shown as text, and every layer as JSON with `--json` or `--pretty`
  -I, --include-path <DIR>   Add a directory to the schema search path (repeatable)
  -c, --config <FILE>        Config file path [default: ~/.config/doe/config.toml]
      --json                 Emit compact JSON output
      --pretty               Emit pretty-printed JSON output
      --no-instances         Leave out the values a schema's `instances` compute or read elsewhere
      --indent <STR>         Indentation string for text output [default: "  "]
      --build <OUT>          With `--tui`: build a schema one field at a time from the hex view of FILE, to be saved to OUT with `w`: a file that is not there yet (the schema is named for it), or a schema that this made, to go on with. Instead of a `TYPE_OR_SCHEMA`
      --list-types           Print the types that can be used and exit: one per line with the file each is in (relative to its type path) and the first line of its doc, aligned on a terminal and separated by tabs otherwise; with `--json` or `--pretty`, an array of objects with `id`, `path`, `type_path`, `doc`, `signature` and `error`. The type paths are searched in their subdirectories too. Give a SCHEMA instead of a FILE to list the types it defines and imports
      --install-types[=<DIR>] Write the type library and the Kaitai Struct gallery that are built into this binary to DIR (default: `~/.doe/types`, or `$DOE_TYPES_DIR`) and exit. `doe` searches that directory after the `-I` and config paths. Run it again after an upgrade: files that were not changed are replaced by the new version's, files that were are kept (see --force)
      --force                With --install-types, replace the files that were changed too
  -h, --help                 Print help, including available types from the include paths
      --color <WHEN>         When to use color on stderr [default: auto] [possible values: auto, always, never]
      --log-level <LEVEL>    Verbosity of the log file [default: debug] [possible values: off, error, warn, info, debug, trace]
      --log-file <FILE>      Write the log file here [default: the XDG state directory]
  -v, --verbose...           Increase logging verbosity
  -q, --quiet...             Decrease logging verbosity
      --completions <SHELL>  Print a shell completion script to stdout and exit [possible values: bash, elvish, fish, powershell, zsh]
      --man[=<PAGE>]         Print a manual page (roff) to stdout and exit: `doe` (the default) or `schema`, the schema format [possible values: doe, schema]
      --tui                  Show FILE decoded with the given type or schema in an interactive viewer in the terminal, instead of printing it. Needs a terminal on stdin and stdout
      --watch                With `--tui`: read the schema and the file again when either changes on disk (the modification times are looked at twice a second)
      --theme <NAME|FILE>    With `--tui`: the colour theme, a built-in one by its id (`nord`, `solarized-dark`, `catppuccin-mocha`...), a file `NAME.toml` in the `themes` directory next to the config file, or the path of a theme file; `terminal` is the colours of the terminal itself, which is the default (the `theme` key of the config file changes that)
      --transparent          With `--tui`: leave the background of the screen to the terminal (a translucent or blurred one shows through), and use the theme's colours for the text; the selection, the status bar and the marks keep theirs (the `transparent` key of the config file, or of a theme file, changes that; `--no-transparent` overrides them)
      --no-transparent       With `--tui`: use the background of the theme, whatever the config file or the theme file says about `transparent`
      --radix <RADIX>        With `--tui`: the radix in which integers are first shown, `dec` (the default), `hex`, `bin` or `oct`; `x` in the viewer goes through them, and `X` chooses one for the selected field (the `radix` key of the config file changes the default)
      --ide                  Serve the web IDE on this machine and print its address, instead of parsing a file. Only the loopback address is used. A FILE is offered to the page as the input, however large it is; the types found through the include paths and the config are offered too
      --port <PORT>          With `--ide`: the port to serve on (0 for any free one) [default: 8765]
      --open                 With `--ide`: open the page in the browser
  -V, --version              Print version
```

`FILE` is the binary to parse. `TYPE_OR_SCHEMA` is treated as a schema path if
it starts with `.`, `/` or `~`, or ends with `.yaml`/`.yml`. Otherwise it is a
bare type name resolved through the configured include paths.

`TYPE_OR_SCHEMA::TYPE` parses with a type inside the schema instead of the
schema's own: `doe file.bin custom_format.yaml::chunk` (or `png::chunk` for a type of
the library), where `chunk` is a type in the `types:` of the schema (`a::b` for
one inside another), or a type of a schema that it imports, named in full
(`custom_format.yaml::common::header`). A type that is not there is an error that
lists the ones that are.

Results go to stdout; everything else (errors, warnings, progress) goes to
stderr. `--completions` and `--man` print and exit; they cannot be combined with
a file, a type or the options that only matter when parsing (`-I`, `-c`,
`--json`, `--pretty`, `--no-instances`), but `--color`, `-v`, `-q`, `--log-level`
and `--log-file` can go with them, as in `doe --completions bash --color never`.

### Files inside files

A schema can say that a file of it holds another file with a `payload:` key:

```yaml
payload:
  process: gzip   # zlib, deflate, gzip, brotli, bzip2, xz, lzma, lzip or zstd
  from: 0         # where the data starts, in bytes (the default is 0)
```

When `doe` looks for the schema of a file (no `TYPE_OR_SCHEMA`), and the schema it finds has a `payload:`, it decodes the
payload **in memory** (up to 256 MiB; nothing is written to disk) and looks for the schema of the result, as it did for
the file: by signature, then by name, which is the name of the file without its last extension (`x.tar.gz` holds `x.tar`,
and `x.tgz` holds `x.tar` too). That goes on to a depth of four layers. The library's `gzip_file`, `brotli`, `bzip2`, `xz`, `lzma_alone`, `lzip` and `zstd` have the key, so
`doe archive.tar.gz`, `.tar.br`, `.tar.bz2`, `.tar.xz`, `.tar.zst` and `.tar.lzma` show the tar (and `.cpio.gz`, `.cpio.xz`, ... the cpio, or any other file inside), and says on stderr what the layers are (`zstd` and `lzma_alone`, which have no signature, are found by the `.zst` and `.lzma` of the name):

```text
$ doe archive.tar.gz
archive.tar.gz is gzip_file > tar (--no-unwrap shows the outer layer only)
tar
  entries  [...]
```

`--no-unwrap` shows the outermost layer alone (a gzip file as a gzip file, as before); `--layer N` shows layer N (1 is the
file itself). With `--json` or `--pretty` every layer is printed, outermost first, as `{"layers": [{"type": "gzip_file",
"value": ...}, {"type": "tar", "value": ...}]}`; with `--layer` it is that layer alone, as the type prints it. A type
given by name is never unwrapped by the command line, but `--tui` shows a type that holds a file (`doe --tui archive.tar.gz gzip_file`, or `gzip_file` chosen in the type browser) as the layer it holds. Where two types fit a file equally well (the signature is the same), the one whose schema says what the file holds (`filesystem:` or `payload:`) is tried first, so doe's `gzip_file` comes before the gallery's `gzip`. If the payload cannot be decoded (a corrupt stream, a bad CRC, more than the limit) or
fits no schema, the outer layer is shown and `-v` says why. The offsets of an inner layer are in its decoded bytes, not in
the file. In the web IDE a file that is opened in the page and detected (the *Detect* button, or the setting that does it on opening) is unwrapped in the page in the same way (the decoders are in the WebAssembly module): the page shows the innermost layer, and a line above the panes, *Layers*, names them (`sample.tar.gz` › `sample.tar`) and switches between them, with the Bytes pane and the schema of each. A type chosen in *Browse types* for a file that holds a file (`gzip_file` on a `.tar.gz`) is shown as what it holds. A payload that cannot be decoded, or fits no schema, leaves the layer that was found, with a note in the status line. In the terminal viewer `L` shows the next layer (and the file itself after the last), and `--tui` and `--ide` open the innermost layer too (or the one `--layer` names; `--no-unwrap` opens the file): the
viewers read a file by its path, so the layer is written to a temporary file named as it would be (`archive.tar`), which is
removed when they are left, and the Bytes pane shows the bytes of that layer.

### Archives as files

A schema can say that it describes a file system with a `filesystem:` key, so that the viewers (the Files view of the web
IDE; the Files view of the terminal viewer, `F`) draw its entries as a directory tree. The key maps fields to columns;
every value is a dotted path of field names from an entry (or from the root, for `entries`), and `size`, `mode`, `mtime`,
`link` and `data` may be a list of paths, of which the first that an entry has is used (for a format whose entries have more
than one layout, like cpio's):

```yaml
filesystem:
  entries: entries          # the list of entries, from the root of the value (required)
  path: [prefix, name]      # one field or several, joined with / when they are not empty (required)
  size: size_value          # bytes
  mode: mode_value          # permission bits, and the file type bits (0o170000) when the format has them
  mtime: mtime_value        # seconds since 1970
  kind: type_flag           # a field that says what the entry is, read with `kinds`
  kinds:                    # for each kind, the values of `kind` that mean it
    directory: ["5"]
    symlink: ["2"]
  link: link_name           # the target of a link
  data: contents            # the bytes of the contents (for the preview and for saving)
  data_is_file: is_stored   # true when `data` are the file's own bytes (a flag, or a number other than 0); no key: always
  directory_slash: false    # a path ending in / is not a directory by itself (ar's names end in /)
  ignore: ["TRAILER!!!"]    # entries that are not files, left out of the list (cpio's trailer)
```

An entry is a directory, symbolic link, hard link, device, fifo, socket or file by `kinds`, else by the type bits of
`mode`, else by a `/` at the end of its path (unless `directory_slash: false`). The paths are normalised (empty and `.`
components dropped) but never resolved: an empty name, an absolute path, a `..` and a path that an earlier entry has are
listed with a flag, so that they show up and cannot hide the others. An entry with no name, no size and no contents (the
block that ends a tar) is not listed. `data_is_file` is for a format whose data range is not always the file: a zip
entry's range is the compressed stream when the entry is deflated, so `zip_archive` sets it from its compression method, and
the viewers show no preview, save or checksum of those bytes as if they were the file (the Bytes pane still shows them). A list of
paths takes the first that an entry has, as `size` and `data` do. The library's `tar`, `ar`, `zip_archive` and `cpio` have the key. The list is in the
`filesystem` field of the answer of the library's `parse`, and `doe::filesystem::list` makes it from a parsed value.

### Detecting the schema

`doe FILE` with no type looks for the schema of the file among the types of the
include paths (`-I` and the config), and parses the file with the first that
fits. The order is:

1. **By signature.** A schema whose first fields are `contents:` (or an integer
   or string with an exact `valid:`, after fields of a fixed size, also inside a
   field that is a type) can only describe a file that starts with those bytes.
   That is read from the compiled schema without parsing anything, and a schema
   whose signature the file does not have is left out. A first field that is a
   magic number written as an integer of two bytes or more with an `enum:` and
   no `valid:` (its id has `magic` or `signature` in it, as `magic_number: u4be`
   of `pcap`) has one signature for each value of the enum, and the file fits if
   it starts with any of them. A signature comes before a name that fits: a
   libpcap capture called `dump.pcapng` is found as `pcap`, and `pcapng` is not
   even tried.
2. **By name.** The extension of the file against the schema's `id`, the name of
   its file and its `meta: file-extension`, for the schemas that have no
   signature; and to put a schema first when both fit.
3. **By trying.** The schemas that are left are tried in that order, each with
   a limit of 2 million fields and 10 seconds, and a panic caught, so that one
   bad schema does not stop the others. A try that stops with an error is a
   schema that did not fit; the first that parses the file without an error
   wins (it may leave bytes unread, so look at the result).

`-v` says what was tried and why, one line each: `tried png (the file starts with
its signature): parsed (1234 bytes read)`. When nothing fits it is an error that
lists them, and the number of schemas that were left out because of the
signature. `doe --tui FILE` does the same before the viewer opens (the viewer has
no command to open another file, so that is the only time it looks).

**The limits.** A format with no signature (a magic number that is an `enum`
whose id does not say it is a magic number, as in `wav`; the signature further in
than a field of a size that is not fixed) is only found by its name, or when
`--detect all` is given, which tries every schema that is left, the ones that
check something (`contents:`, `valid:`) before the ones that check nothing and so
fit any file, and can take a long time for a big library. When a parse with a
type given by name fails on a file that starts with the signature of another
type, the error says which. Only the first 64 KiB of
the file are compared with signatures. Of the 327 samples of the Kaitai gallery
in the tests, 152 are found with their own schema (those of the formats whose
signature is read from the schema), and the rest need their name or
`--detect all`; a test keeps the number that are found from falling.

In the web IDE, *Detect* in the title bar of the Parsed pane does the same for the opened file
among the formats of the gallery: the page has each format's signature and names
(computed when it was built, by the same library), asks the library for the
order, and fetches the definition of a format only when it is its turn, so a
PNG costs a few requests, not the gallery. The formats tried are listed with why
each was tried and what came of it, a format that takes longer than the limit of
the page's worker is reported and the next is tried, and *Try them all* adds the
formats with no signature. *Auto* (remembered in the
browser) does it on opening a file, when the editor is empty or the
schema does not parse the file. With `doe --ide FILE` the types of the include
paths are searched as well, natively, for the file named on the command line
(`Use FILE`); a file opened in the page is searched among the gallery only.

### Terminal viewer

`doe --tui FILE TYPE_OR_SCHEMA` opens the file in an interactive viewer in the
terminal instead of printing it. It needs a terminal on stdin and stdout (with a
pipe it stops with an error), and it parses with the same schema rules as the
rest of the command, also showing what was read when the parse fails. It shows
two panes. The **tree** of the parsed value: each field with its type and value
(integers in decimal with the hex next to them, enum names with their number,
strings quoted, byte sequences with a hex preview), instances dimmed, and the
field where a parse failed marked with a cross in the first column. The
**bytes** of the file next to it (below it on a narrow terminal): offsets, hex
and text, with as many bytes per line as fit. The bytes of the selected field are
highlighted down to the bit: a byte that is only partly the field's (a bit field
that shares it with a neighbour) is underlined instead. The title of the pane
says which bytes and bits they are; a field that is read from the result of a
`process:` is not in the file's bytes, and the title says so. Only the lines in
view are read, so a file of many gigabytes opens and scrolls at once.
Strings from the file are shown with control characters escaped, so nothing in a
file can send commands to the terminal.

This is `doe --tui demo.bin demo.yaml` for a small made-up format, with the
selection on the `size` of the first record (the bytes of the field are
highlighted in the second pane, and the status line says where they are):

```text
┌ demo ────────────────────────────────────────────────────────────────────── parsed ┐
│ ▾ demo                                                                            █│
│     magic = <4 bytes> 44 4f 45 31                                                 █│
│     version = 3                                                                   █│
│     flags = 5                                                                     █│
│     count = 2                                                                     █│
│     name = "demo"                                                                 █│
│   ▾ records                                                                       █│
│     ▾ [0]  demo::record                                                           █│
│         kind = text (1)                                                           █│
│         size = 11                                                                 ║│
└ q quit · Tab switch pane · i inspector ────────────────────────────────────────────┘
┌ size: bytes 0x0000000e..0x0000000f ────────────────────────────────────────────────┐
│00000000  44 4f 45 31 03 00 05 02  64 65 6d 6f 00 01 0b 00  DOE1....demo....        │
│00000010  68 65 6c 6c 6f 20 77 6f  72 6c 64 02 08 00 89 8a  hello world.....        │
│00000020  8b 8c 8d 8e 8f 90                                 ......                  │
└ 38 bytes ──────────────────────────────────────────────────────────────────────────┘
 demo/records/[0]/size · unsigned integer · @0xe (14) · 2 bytes (… demo.bin · 38 bytes
```

The viewer and the [web IDE](#web-ide) show the same parse of the same schemas.
The IDE is for writing a schema (an editor beside the tree, the bytes and the
errors, in a browser, with files and schemas dropped on it); the terminal viewer is
for looking at a file where you already are (over `ssh`, or beside the editor you
use) and for files that are too big to put in a page, and with `--watch` it can
follow a schema you edit elsewhere. Neither needs the other.

In the tree, move with the arrow keys or `j`/`k`, `PgUp`/`PgDn`, `Home`/`End`
(or `G`); `→`/`l` opens a node, `←`/`h` closes it or goes to its parent, `Enter` or
`Space` toggles it; `]` and `[` go to the next and previous field at the same
level; the bytes scroll to the selected field. `Tab` moves the keys to the bytes:
`←`/`→` (or `h`/`l`) move by a byte, `↑`/`↓` (`k`/`j`) by a line, `PgUp`/`PgDn` by a
page, `Home`/`End` to the start and end of the line, `Ctrl-Home` and `Ctrl-End`
(or `G`) to the start and end of the file, and the tree selects the innermost
field that holds the byte, opening its parents and scrolling the tree to it. A
byte in a gap between the fields of a type selects the type; a byte after
everything that was parsed leaves the selection where it was and the status line
says it is not in a field. Moving in the tree puts the cursor on the start of the
selected field.

`b` moves the cursor by **bits**: `←`/`→` (or `h`/`l`) then move one bit, over the
end of a byte, the tree selects the innermost field that holds the bit (so two bit
fields that share a byte are told apart), the byte of the cursor is underlined and
the title of the pane says `bit 3 of byte 0x1` (bits are counted from the start of
the byte, as the title of a bit field counts them). `b` again goes back to bytes. A
byte can be held by several fields: the one of the sequence is selected, and when
others hold it too (an instance at `pos:`, a `value:` over a window) the status
line says how many, and `.` and `,` go to the next and the previous of them. Only
the instances of the types on the way to the field are looked at, so a key press
never looks through the whole tree; an instance inside a type that is somewhere
else in the file is found from the tree, not from its bytes.

The description (`doc:`) of a field is shown after its value in the tree, dim, as its first line cut to the width of the
pane; the whole text is in the inspector (`i`) under `doc` when the field is selected (not for a selection of bytes), and `D`
shows or hides the descriptions (they are shown to begin with, and a reload keeps the choice). The search (`/`) finds a
word of a description while they are shown. An element of a repeated field has none; the field has it once.

`f` makes the pane that has the keys fill the screen (its title says `full screen`), so that a long tree or a wide hex dump is read in all of it, and `f` again puts both panes back; `Esc` does too, before it would leave, and `Tab` while a pane fills the screen moves to the other pane, which then fills it. The selection, the cursor and the scroll positions of both panes are as they were. In a prompt, a search or the help `f` is a letter, and on a screen with room for one pane only (a narrow, short terminal) it changes nothing. The inspector, `i`, takes the place of the tree in a full-screen tree, as it does on a small screen. `f` is the key that maximizes a pane in the web IDE too.

A file that holds another file (a `.tar.gz` is a gzip file that holds a tar; a `.tar.xz`, `.cpio.zst`, ... likewise) is opened on the innermost layer, and `L` shows the next layer, and the file itself after the last, with a message that says which layer is shown (`layer 2 of 2: gzip_file > tar`); a file that holds nothing says so.

Each byte of the bytes pane is drawn in the colour of the field it belongs to, the colour that field's value has in the tree
(yellow numbers, green strings, red runs of bytes, magenta enums, cyan booleans with the terminal's own colours; the theme's
with a theme), so that the pane reads as a map of the file; `B` turns the colours off and on. The selection is as it was:
with a theme (24-bit colour) the selected bytes keep the selection background and a colour of their kind that is readable
on it (the theme's text colour when the colour has a contrast under 3:1 there); with the terminal's own colours the
selected bytes are drawn as they always were. The cursor byte and the range of `v` keep their styles over the colours. A
struct, an array, a gap and a field with no kind (a float) have the plain colour. The index of which field owns each byte is made
once for a parse, with the parse (in its thread, so the first screen does not wait for it); with `NO_COLOR` nothing is coloured.

`v` starts a **selection** of bytes at the cursor, and moving the cursor extends
it in either direction (the bytes are drawn reversed and in italics, and the
title of the pane says `3 bytes selected (0x10..0x12)`); `v` again, or `q` or
`Esc`, ends it. With the inspector (`i`) open, it shows the selected bytes in the
forms it shows a field in: an integer in both byte orders if there are 1, 2, 4 or
8 of them, a float for 4 or 8, text, binary. `y` puts the selected bytes (up to
4096) as hex on the clipboard of the terminal with the OSC 52 escape sequence,
which most terminals support, some only when it is turned on, and also says them
in the status line. Reading the file again drops the selection.

**Files of an archive.** When the schema says that the file is a file system (a `filesystem:` key, see
[Archives as files](#archives-as-files): the library's `tar`, `ar` and `zip_archive` have it), the tree pane opens on the
**files**: the entries nested by path as directories and files, with size, mode and time (UTC), a `▸`/`▾` for a
directory with the number of entries inside, a warning sign for an entry that cannot be placed safely (an empty name, an absolute
path, a `..`, a path an earlier entry has), and "N more…" after 500 children. The tree keys move in them (`↑` `↓` `j` `k`,
`PgUp` `PgDn`, `Home`, `End`, `→` or `l` to open a directory or go into it, `←` or `h` to close it or go to the parent,
`Enter` or `Space` to toggle), the entry that is selected is selected in the tree of fields too and its bytes in the bytes
pane (the contents of a file are there, and in the inspector), and `F` switches to the tree of fields and back, with the
field or entry that was selected.

`m` adds a column of **checksums** to the files, after the size: the MD5 (32 digits), and pressed again the SHA-1 (40 digits),
then the SHA-256 (64 digits), and once more it goes away; `M` goes the other way. In a pane too narrow for the 64 digits of a SHA-256
the column shows the first 16 and an ellipsis (`y` still copies the whole digest). The algorithm is named in the hint at the bottom of the pane and on the
status line. `…` shows while a thread reads the file and makes them (the screen stays alive and shows each as it comes), `—`
for a file with none (the schema gives no contents, or the stored bytes are not the file, as for a deflated zip entry; the
status line says why when you move onto it), nothing for a directory. The digests made for one algorithm are kept, so going
back to it is at once. On a narrow pane the time and then the mode are left out before the name is. `y` copies the digits of
the selected file to the clipboard of the terminal where it allows it. MD5 and SHA-1 are checksums for comparing files, not
security measures; SHA-256 is the one to compare with a published checksum.

`Enter` (or `p`) on a file **previews** it: the first 100,000 bytes of the entry, if they are text, over the viewer with
line numbers, highlighted in the colours of the viewer's theme (also in the 16-colour themes, and plain with no colour). The
language comes from the name (`main.rs`, `Makefile`, `Dockerfile`, `*.json`…) and, when the name does not say, from a `#!`
first line (`#!/usr/bin/env python3`); a file of an unknown type is shown plain, as are TOML, INI and Dockerfiles, which the
grammars the viewer carries do not have (TypeScript is read with the JavaScript grammar). `↑` `↓` `j` `k`, `PgUp` `PgDn`,
`Home` and `End` scroll; a line of more than 2000 characters is cut with `…`, and an entry of more than 100,000 bytes ends with
`…`. `s` asks for a file name (starting with the entry's name) and saves the **whole** entry there, never over a file that
exists; `Esc` or `q` goes back to the files. On a **Markdown** entry `r` (or `v`) switches between the source and the
**rendered** text: headings in bold and the theme's accent, lists with bullets and indent, block quotes with a bar, tables
drawn with box characters, rules, links as `text <url>`, images as `[image: alt]`, code blocks coloured by the language of the
fence; paragraphs are wrapped to the width of the pane and wrapped again when the terminal is resized. Raw HTML is shown as
text. The choice is kept for the next Markdown entry. An entry that is not text says `N bytes (not text)`.

**Building a schema.** `doe --tui FILE --build OUT.yaml` starts the viewer on an
empty schema that grows one field at a time from the hex pane, and saves it with
`w`; see [Building a schema](#building-a-schema).

**Large files.** The viewer is built not to be what makes a big file hard to
look at. The first screen is drawn at once and says that the file is being
parsed (the parse runs in another thread, and a file with millions of fields
takes the engine a few seconds); the file itself is read as the schema asks
for it and as the bytes are shown, a window at a time, so a file of gigabytes
(it may be sparse) opens and scrolls with the memory of a small one. A list is
shown through a window of 200 elements that moves to the element you go to
(search, `g`, the end of the list), so a list of a million elements never has
more than a few hundred rows. The screen is redrawn when something changed, and
while keys come faster than it can be drawn only every 50 ms. The budget is in
`crates/doe-tui/tests/performance.rs`, which tests it on a generated tree of a
million fields and a file of 6 GiB: the first screen in 500 ms, a key and a frame
in 100 ms, a step of a search in 100 ms and no more than 32 MiB of memory added;
what is measured is tens of times smaller.

When the parse fails, the tree is what was read up to the failure, the field
where it failed is marked and selected, and the bytes are at it. A message under
the tree says what happened, in which field (`rec/items/[2]/kind`) and type, and
why (`e` hides it or shows it again). When nothing can be parsed (a schema that
does not compile, a type that does not exist, a file that cannot be read) the
viewer opens all the same and the first pane has the error instead of the tree:
what was wrong, what caused it, and for a schema that is a file, the place in it
(`custom_format.yaml:12:5`, the line, and a caret under the place). The bytes of the
file are shown if it can be read. After `q` the message is printed again on the
normal screen, and the exit status of `doe --tui` is 1 if the last parse failed
(or nothing could be parsed), 0 if it worked.

`r` reads the schema (with the files it imports) and the file again and parses:
writing a schema is a loop of editing it and looking at the result, and quitting
each time loses the place. The new tree has the same nodes open and the
selection on the field with the same name all the way from the root (so a field
added before it does not move it), or on the deepest of those that is still there;
the scroll stays as far as the new tree allows. If the new schema or file cannot
be used, the old tree stays, the error is shown with its place in the schema,
and the exit status after `q` says that the last read failed. With `--watch`,
`doe` looks at the modification times of the schema, its imports and the file
twice a second and reads them again by itself when one of them has changed and
has stayed the same for one more look, so a file that an editor is still writing
is not read half way, and a change is read once.

`/` searches the names, types and values of the fields, as the tree shows them
(`300 (0x12c)`, `"abc"`, `png (1)`): a query without a capital letter ignores
case, one with a capital letter does not. `n` and `N` go to the next and previous
match, round the end of the tree. The search looks at the fields in steps between
redraws, so a tree of millions of fields does not freeze the screen, and any key
stops it. `g` asks for an offset (decimal, or `0x…` for hex), shows it in the
bytes and selects the innermost field that holds it; an offset past the end of the
file, or in the part of the file that was not parsed, says so. `?` lists every key; in that list `/` searches (`n` and `N` go on, `Esc` clears the search) and the arrow keys scroll.

`s` lists the **schemas** that are open: the one named on the command line first,
then the files it imports (and they import), each with its id, when the file was
last changed, and a cross with the error and its place if it could not be read;
under each, the **types** it defines; and the imports that are not there, with the
file that asked for them. `↑`/`↓` move, `Enter` on a schema reads its text in a
pager with line numbers (read-only; `Esc` goes back, and the line of an error is
marked), and `Enter` on a type reads the file again and parses it with that type,
as `doe --tui FILE SCHEMA::TYPE` does; the type that is parsed has a `●`. `r`
reads everything again, and `Esc` closes the list. If the new type cannot be used,
the old tree stays and the error is shown.

The pager that reads a schema (from the list of schemas, and `v` in the types
browser) **colours** it with the colours of the [theme](#themes): keys and the
sections at the left edge in the accent colour, the names of types and of enums
in theirs, strings and the text of `doc:` blocks, numbers (`0x…` and `0b…` too),
`true`, `false` and `null`, and comments dim. The line of an error keeps the
error colour. With the terminal's own colours it uses the ANSI colours, and with
`NO_COLOR` or `--color never` the text is plain. The colouring is a scanner of the
YAML that schemas use, not a parser: what it does not understand is shown as it
is.

`T` browses the colour **themes** and chooses one (kept in the config file): the viewer
shows each as you move through the list (see [Themes](#themes)).

`t` browses the types that are **available**: those of the type paths (`-I`, and
`include_paths` of the config file), found in their subdirectories too, grouped
by the path they are in, each with its id, the file it is in, and the first line
of its `doc`. `/` types a filter (every word of it must be in the id, the path
or the doc; `Enter` stops typing, `Esc` clears it), `v` reads the text of the
selected type in a read-only pager, and `Enter` reads the open file again with
that type, as `doe --tui FILE TYPE` does; the type in use has a `●`, and if the
new type cannot be used the old tree stays and the error is shown. `Esc` goes back
one step at a time and then closes the browser. The schemas are compiled in steps
while the list is open, a few at a time between keys, to see which types start
with a **signature** (`contents:` or an exact `valid:` in their first fields); the
title says how far it has come, and a type that has one says `signature`, and one
that does not compile has its error. `r` **ranks** the list by the open file, in
the order in which `--detect` would try the types on it: first those whose
signature the file starts with (and whose name fits), then those with no
signature whose name fits the file's extension; a type whose signature the file
does not have is left out, and each line says why it is there. The ranking waits
until the schemas are read. `r` again puts the groups back. It is the order of
trying, not the result of it: `doe FILE` without a type (or *Detect*) does
the parsing that tells which one fits.

`x` writes the integers in the next **radix**: decimal (the first one, with the hex
next to the numbers from 16 up), hexadecimal (`0x001f`), binary (`0b00011111`),
octal (`0o37`), and decimal again. The digits are padded to the width of the
type, so a `u2` is four hex digits and a bit field has as many binary digits as
bits; a negative number is its sign and magnitude (`-0x1f`), not two's complement,
and the inspector adds the two's complement. An enum member keeps its name and
has its number in the radix (`text (0x01)`). Floats, strings and byte sequences
do not change, and the search matches values as they are in decimal. `X` gives
the selected integer field a radix of its own (decimal, hex, binary, octal, and
back to following `x`), kept by the field's type and name, so that it holds for
every element of a list and after `r` reads the schema again; one flags field can
be binary while the rest stay hex. `--radix hex` (or `radix = "hex"` in the
config file) starts with another radix than decimal. The text of a number is
made by `doe::radix::format_integer`, and the web IDE writes the same strings.
The line that is typed for `/` and `g` is edited as a shell line is: the arrow keys,
`Home`/`End`, `Backspace`/`Delete`, `Ctrl-W` (a word), `Ctrl-U`, `Ctrl-K`, and a
paste (one line) is inserted at the cursor; `Enter` runs it, `Esc` cancels it.

The **status bar** at the bottom has the path of the selected field
(`header/entries/[3]/name`), its type (or the kind of its value), where its bytes
are (`@0x14 (20) · 4 bytes (32 bits)`, or the bit it starts at for a bit field,
or that it is computed or made by a `process:` and so not in the file), and the
name and size of the file. `i` shows the **inspector**: the selected bytes read
from the file, whatever the parsed value was, as unsigned and signed numbers in
both byte orders (decimal, hex and, up to 4 bytes, binary), as a float when the
field is 4 or 8 bytes, as the value of the bits for a bit field, and as text in
UTF-8, ASCII, Latin-1 and UTF-16 (the first 64 bytes of a longer field). For a
field with no bytes of the file it says why. It is drawn below the bytes when
there is room, else in place of them.
`q`, `Esc` and Ctrl-C leave it, and the terminal is put back also if the
program panics or is stopped by `SIGTERM` or `SIGHUP`. It uses colour unless
`NO_COLOR` is set or `--color never` is given. It is behind the `tui` cargo
feature, which is on by default: `--no-default-features` leaves it out, and
`--tui` then says so. It cannot be combined with `--json`, `--pretty`,
`--no-instances`, `--indent` or `--ide`.

**How it is tested.** The model, the keys and the drawing are tested by feeding
events to the app and drawing it on a `ratatui` `TestBackend`, in the CI test job
on all four targets (Linux x86-64, macOS arm64, Windows x86-64 run them; macOS
x86-64 is built on the arm64 runner, so its tests are compiled but not run). A few tests start the real
`doe --tui` on a pseudo-terminal, send keys, and check the screen and that the
terminal is put back after `q`, after a signal and after a panic inside the
viewer. Those run on Linux and macOS; **on Windows there is no pseudo-terminal
test** (it would need `ConPTY` and a dependency for it), so the setup and
restoring of the console there is not covered by tests, and `SIGTERM` and
`SIGHUP` do not exist there.

### Building a schema

`doe --tui FILE --build OUT.yaml` is for a file that has no schema yet. The viewer
opens on `FILE` with an empty schema, named for `OUT.yaml` (so the name has lower
case letters, digits and `_`; the file must not be there yet), and the schema grows
one field at a time from the hex pane: say how many bytes a field is and what it is,
and the file is parsed again at once with what you have, the tree and the bytes
showing it. `w` saves the schema; `doe FILE OUT.yaml` prints the same tree. The title
of the tree says `building`, and `building · not saved` once the schema has changed.
It takes no `TYPE_OR_SCHEMA` and is not the same as `--watch`. If `OUT.yaml` is
there and the builder made it, the viewer goes on with it: it is read back, and the
lengths that the file decides (text that ends, repeats) are taken from `FILE`. A
schema that the builder did not make, or that has anything it cannot give back
(`instances`, `if`...), is refused and the line is named, rather than
losing a part of it. A schema with `types:` of the shape the builder writes (a flat `seq` for each type) is read back.

| Key | Does |
|-----|------|
| `v`, then the movement keys | select the bytes the next field is (its length is the selection's) |
| `a` | add a field where the schema ends (with the tree focused and a field that has a type of the schema selected, into that type; with a field of a type selected, into that type after it) |
| `Z` | make the fields of the selected bytes (or the selected field) a **new type** of the schema; it asks for the name |
| `O` | take the type of the selected field apart: its fields take the field's place |
| `A` | add a field at the cursor, with a `skip_1` field of the bytes before it |
| `u`, `Ctrl-R` | take the last change back, do it again |
| `d` | remove the selected field (the last at once, another after a second `d`) |
| `r` | rename the selected field |
| `c` | change its kind or length |
| `E` | switch the whole schema between little and big endian |
| `w` | save to `OUT.yaml` |
| `q` | leave (asks once if the schema is not saved) |

`a` asks, at the bottom of the screen, for the *kind*, then what the kind needs, then
the *name*; `Enter` takes what is proposed and `Esc` gives up.

| Kind | Is | Then asks for |
|------|----|---------------|
| `u1 u2 u4 u8`, `s1 s2 s4 s8` | an integer of that many bytes, in the byte order of the schema | whether it is repeated |
| `f4 f8` | a float | whether it is repeated |
| `bytes` | bytes kept as they are | the length, whether it is repeated |
| `str` | text of a length | the length, the encoding, whether it is repeated |
| `strz` | text that ends with a byte | the terminator (`0`), the encoding; the length is found in the file (within 64 KiB) |
| `magic` | a signature: bytes the file must have (`contents:`), which `doe` uses to find the schema | the bytes in hex (the selection is proposed) |
| `new` | a **new empty type** of the schema and a field that has it: it asks for the name of the type, then of the field; select the field and press `a` in the tree to add fields into the type | the name of the type, the name of the field |
| `b1` to `b64` | a bit field; the ones that follow each other share bytes, and the cursor moves by bits to where the next goes on | |
| `type`, or the id of a type | a type of the type paths (`-I`, the config): `type` opens the list of the types (filter with `/`, `Enter` chooses, `Esc` gives none), and the id of a type can be typed instead | the size of the window it is read from (`Enter` for as much as it needs), whether it is repeated |

The kind that is proposed is the integer of the size of the selection (1, 2, 4 or 8
bytes), else `bytes`, else `u1`; the name is proposed from the kind (`u2_1`) and has to
be a lower case name that no other field has. A field can be **repeated**: the
question takes a number, the name of an earlier integer field (a count read from the
file), or `eos` for until the end of the file (which has to be a whole number of
elements); a field that is the count of a repeat cannot be removed, renamed or made
something else while the repeat uses it. **New types.** `Z` takes the fields that the selected bytes are exactly (`v`, then the movement keys; the selection has to start and end at the edges of fields), or the selected field, out of the schema and makes them a type with the name that is asked for; a field that has the new type takes their place, so the file reads as it did and the tree shows the type's fields under it. The new type is written in the schema (`types:`, one file, nothing more on the type paths) and is offered first in the list of types of the kind `type`, or can be typed, to be the type of another field, and repeated (a number, an earlier integer field, `eos`: a list of entries is a header and an entry type). Fields of a type are added, renamed, changed and removed by selecting them in the tree (`a`, `r`, `c`, `d`), and `u` undoes every one of these changes. A name that another type, a type the schema imports or the schema has is refused; a type that another field has cannot be taken apart (the message names the field); a change that would make a type contain itself, or leave a repeat without the count it reads, is refused with the reason; bit fields cannot be grouped yet, and a repeat by the value of a field is made in the fields of the schema itself, not in a type, for now.

A field of a **type of the type paths** is written as `type: png_header` and the schema gets an `imports:` entry for it
(once for each type), so the saved schema compiles with the same type paths; its bytes in the file are what the type reads (or the
window, with a size), and the next field starts after them. A type that takes parameters is not offered (the builder cannot give
them a value yet), the schema being built cannot import itself, and a type that imports the schema (a cycle), or that does not
fit the file, is refused with the reason, like any other field. A field that does not fit in the rest of the
file is refused, and so is any change that makes the schema longer than the file:
the question is asked again, or everything stays as it was.

In a viewer that builds, `r` renames and does not read the files again (they are
read again after every change anyway). `u` can go back over every change since the
viewer was opened (an addition, a removal, a rename, a change of kind or of byte
order); a new change forgets what could have been done again. The status line says
what each key did.

`w` writes the YAML of the schema, with a comment at the top and the place of each
field in a comment before it, to a file beside `OUT.yaml` which then takes its
place, so a half-written schema is never seen; a file that was not there when the
builder began and is now, and a symbolic link, are not written over.

**An example.** `blip.bin` is 18 bytes: `BLIP`, a version `01`, a count `03`,
three little-endian 16-bit values `0a00 1400 1e00`, the text `demo` and its zero
byte, and a byte `a3` (`101 00011`) that is a 3-bit number, 5, and a 5-bit number, 3. These are the keys that
make a schema for it (a space separates groups of keys, and two spaces start the
comment on the right; the test `crates/doe-tui/tests/example.rs` presses exactly
these in a viewer, so they cannot go out of date):

<!-- example: keys -->
```text
v lll                                 select the four bytes of the signature
a <C-u>magic<Enter>                   add a field of the kind magic;
<Enter>                               the selected bytes are proposed as its bytes;
<C-u>signature<Enter>                 and this is its name
a <Enter> <Enter> <C-u>version<Enter>   a byte (u1, proposed), repeated once, called version
a <Enter> <Enter> <C-u>count<Enter>     another byte: the number of values
a <C-u>u2<Enter> count<Enter> <C-u>values<Enter>   a u2, repeated as many times as count says
a <C-u>strz<Enter> <Enter> <Enter> <C-u>title<Enter>   text up to the zero: terminator 0, ASCII
a <C-u>b3<Enter> <C-u>kind<Enter>     three bits
a <C-u>b5<Enter> <C-u>level<Enter>    and the five bits that are left of the byte
w                                     save
```

The schema that was saved:

<!-- example: blip.yaml -->
```yaml
meta:
  id: blip
  endian: le
seq:
  # 0x0..0x4
  - id: signature
    contents: [0x42, 0x4c, 0x49, 0x50]
  # 0x4..0x5
  - id: version
    type: u1
  # 0x5..0x6
  - id: count
    type: u1
  # 0x6..0xc
  - id: values
    type: u2
    repeat: expr
    repeat-expr: count
  # 0xc..0x11
  - id: title
    type: str
    terminator: 0
    encoding: ASCII
  # bits 136..139 (byte 0x11)
  - id: kind
    type: b3
  # bits 139..144 (byte 0x11)
  - id: level
    type: b5
```

and `doe blip.bin blip.yaml` prints:

<!-- example: blip.txt -->
```text
blip
  signature  <4 bytes>
  version  1
  count  3
  values  [3 items]
    [0]  10
    [1]  20
    [2]  30
  title  "demo"
  kind  5
  level  3
```

### Themes

The terminal viewer and the [web IDE](#web-ide) have the same built-in colour
themes, from one table in the repository (`crates/doe-tui/themes/themes.toml`;
the web IDE's file is made from it, and a test fails when that file is out of
date). A theme names its colours by what they are for, not by the widget: the
background and the text, secondary text, borders and panels, the accent (focus
and the names of types), the selection and a partial selection (a byte that is
only partly the field's), errors and warnings, and a colour for each kind of
value in the tree (integers and floats, strings, byte sequences, enum names,
booleans, offsets, instances). Every built-in theme has at least 4.5:1 (WCAG AA)
between its text and its background and between its error colour and its
background, and the text is readable on its selection; the two high-contrast
themes are 7:1. A test checks all of it, and another that the viewer draws
with the colours of the theme and no others.

In the terminal viewer, `--theme NAME|FILE` (or the `theme` key of the
[config file](#configuration)) chooses one. `NAME` is the id from the table
below (case, spaces and `_` do not matter: `Solarized Dark` is
`solarized-dark`), the name of a file `NAME.toml` in the `themes` directory
next to the config file (`~/.config/doe/themes/` on Linux), or `FILE` is the
path of a theme file. Without it the viewer uses the colours of the terminal
itself (`--theme terminal` says so), so it matches whatever palette the person
has chosen for the terminal. A theme is `#rrggbb` colours; they are used as
they are on a terminal with 24-bit colour (`COLORTERM=truecolor`), and the
nearest of the 256 colours of xterm (`TERM` with `256color`) or of the 16
colours of every terminal are used on one with fewer. `NO_COLOR` and
`--color never` give no colour and no theme, as before.

In the viewer, **`T`** opens the list of the themes, at the right edge of the
screen so that the viewer behind it can be seen: the terminal's own colours first,
then the built-in themes (dark, then light; each group in the order of the names, without regard to case or accents, with a number in a name by its value), then your own files (`NAME.toml` of
the `themes` directory; one that cannot be read is listed with its error). Moving
through the list shows each theme on the whole viewer, the list included. `/`
filters by name, `Enter` uses the selected theme and **saves it**: it sets the `theme` key of the config file
that the run reads (`--config`, or the default `~/.config/doe/config.toml`; the file and its directory are made
when they are not there), so that the next run starts with it. This is the one thing the viewer writes. The
rest of the file is left as it is (other keys, comments and order); a file that is not valid TOML is not
touched, and the message says why the theme was not saved, which is still used for the session. The
terminal's own colours take the key out. If `--theme` was given on the command line the message says that it
wins at the next run until it is removed. `Esc` puts back the theme that was in use and writes nothing. With
`NO_COLOR` or `--color never` the list says there is no colour in this run and applies nothing (nor saves).

**A transparent background.** A theme paints the whole screen with its `background`. With `transparent = true` in
the config file, `--transparent` on the command line, or `transparent = true` in a theme file, the viewer leaves the
screen's background to the terminal (its default background, not black), so that a translucent or blurred terminal
shows through, and uses the theme's colours for the text. The setting of the command line wins over the config file,
and the config file over the theme file (`--no-transparent` turns it off for a run); the built-in themes are opaque.
Only the background of the screen goes: the selected row and bytes, the status bar, the prompts and the marks keep
their colours, and the help and the lists clear their area with the terminal's default background. The contrast
that the tests check is the theme's text on the theme's background, so with `transparent` the text is only as readable
as the terminal's own background lets it be (a dark theme on a light terminal will not be). Nothing changes with
`NO_COLOR` or `--color never`, which apply no theme, or without a theme.

A theme file is TOML, with the colours of the table:

```toml
name = "Example theme"
variant = "dark"          # or "light"; "dark" if left out
author = "A. Person"      # author, license, source and note are optional
transparent = false       # optional: true leaves the background to the terminal
[colors]
background = "#1e1e2e"
foreground = "#cdd6f4"
muted = "#9399b2"
line = "#45475a"
panel = "#181825"
accent = "#89b4fa"
selection = "#45475a"
selection_part = "#313244"
error = "#f38ba8"
warning = "#f9e2af"
number = "#fab387"
string = "#a6e3a1"
bytes = "#fab387"
enum = "#cba6f7"
boolean = "#94e2d5"
offset = "#9399b2"
type = "#89b4fa"
instance = "#9399b2"
```

In the web IDE the theme picker in the header chooses a theme; "System" (the
default) follows the light or dark setting of the system, with the doe Light
and doe Dark themes. The choice is remembered in the browser. "Load a theme
file" reads a file of the format above, which is kept in the browser too (it
is not sent anywhere).

The themes, with the licence of each (all are MIT, and a theme whose licence
does not allow redistribution is not included). A theme is the palette of the
project it names, with a colour made lighter or darker where that was needed for
4.5:1; `themes.toml` has the repository the palette comes from (`source`) and
what was changed (`note`) for each theme.

| Id | Name | Variant | Author | Licence |
|---|---|---|---|---|
| `catppuccin-frappe` | Catppuccin Frappé | dark | Catppuccin | MIT |
| `catppuccin-latte` | Catppuccin Latte | light | Catppuccin | MIT |
| `catppuccin-macchiato` | Catppuccin Macchiato | dark | Catppuccin | MIT |
| `catppuccin-mocha` | Catppuccin Mocha | dark | Catppuccin | MIT |
| `doe-dark` | doe Dark | dark | the doe project | MIT |
| `doe-light` | doe Light | light | the doe project | MIT |
| `dracula` | Dracula | dark | Zeno Rocha | MIT |
| `github-dark` | GitHub Dark | dark | GitHub | MIT |
| `github-light` | GitHub Light | light | GitHub | MIT |
| `gruvbox-dark` | Gruvbox Dark | dark | Pavel Pertsev | MIT |
| `gruvbox-light` | Gruvbox Light | light | Pavel Pertsev | MIT |
| `high-contrast` | High Contrast | dark | the doe project | MIT |
| `high-contrast-light` | High Contrast Light | light | the doe project | MIT |
| `kanagawa-dragon` | Kanagawa Dragon | dark | rebelot | MIT |
| `kanagawa-lotus` | Kanagawa Lotus | light | rebelot | MIT |
| `kanagawa-wave` | Kanagawa Wave | dark | rebelot | MIT |
| `monokai` | Monokai | dark | Wimer Hazenberg | MIT |
| `neobones-dark` | Neobones Dark | dark | mcchrish | MIT |
| `neobones-light` | Neobones Light | light | mcchrish | MIT |
| `nord` | Nord | dark | Sven Greb (Arctic Ice Studio) | MIT |
| `one-dark` | One Dark | dark | GitHub (Atom) | MIT |
| `one-light` | One Light | light | GitHub (Atom) | MIT |
| `seafoam-pastel` | Seafoam Pastel | dark | iTerm2 Color Schemes | MIT |
| `seoul256-dark` | seoul256 Dark | dark | Junegunn Choi | MIT |
| `seoul256-light` | seoul256 Light | light | Junegunn Choi | MIT |
| `seoulbones-dark` | Seoulbones Dark | dark | mcchrish | MIT |
| `seoulbones-light` | Seoulbones Light | light | mcchrish | MIT |
| `solarized-dark` | Solarized Dark | dark | Ethan Schoonover | MIT |
| `solarized-light` | Solarized Light | light | Ethan Schoonover | MIT |
| `tokyo-night` | Tokyo Night | dark | enkia | MIT |
| `vimbones-dark` | Vimbones Dark | dark | mcchrish | MIT |
| `vimbones-light` | Vimbones Light | light | mcchrish | MIT |
| `zenbones-dark` | Zenbones Dark | dark | mcchrish | MIT |
| `zenbones-light` | Zenbones Light | light | mcchrish | MIT |
| `zenburned` | Zenburned | dark | mcchrish | MIT |
| `zenwritten-dark` | Zenwritten Dark | dark | mcchrish | MIT |
| `zenwritten-light` | Zenwritten Light | light | mcchrish | MIT |

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
| `default_types` | `true`, `false` | Whether the directory made by `doe --install-types` (`~/.doe/types`) is searched after `include_paths`. Default `true`; `false` leaves it out. | |
| `theme` | text | The colour theme of `doe --tui`: a built-in id, a file in the `themes` directory next to this file, or a path (see [Themes](#themes)). Default: the terminal's own colours. | `--theme` |
| `transparent` | `true`, `false` | Whether `doe --tui` leaves the background of the screen to the terminal, so that a translucent one shows through (see [Themes](#themes)). Overrides the `transparent` key of a theme file. Default: what the theme says, which is off for the built-in themes. | `--transparent`, `--no-transparent` |
| `radix` | `dec`, `hex`, `bin`, `oct` | The radix in which `doe --tui` first shows integers (`x` changes it in the viewer). Default `dec`. | `--radix` |
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

## Test coverage

The tests are measured for the lines of Rust they run, with
[`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov), and CI fails when
the total is **below 90%**. The threshold is `THRESHOLD` in
[`scripts/coverage`](https://github.com/shroomsword/doe/blob/main/scripts/coverage), the one place it is written, and so is the list
of what is left out of the count, each with its reason (the tests themselves,
benchmarks, the fuzz targets, and the `doe-wasm` glue that only runs in a
browser).

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov        # and jq must be installed
scripts/coverage                    # the same tests as CI; --report-only does not fail below the threshold
xdg-open target/coverage/html/index.html
```

It runs every test of the workspace with all features, so it takes as long as a
build of them with instrumentation and needs several GiB of disk. The two tests
with budgets in time (`crates/doe-tui/tests/performance.rs`) do not run under it,
because instrumented code is slower (`DOE_COVERAGE` is set). Besides the HTML
report it writes `target/coverage/lcov.info` and a summary with the least covered
files.

In CI the `coverage` job puts that summary in the job summary (with how the total
changed against `main`), and keeps the lcov file and the HTML report as the
`coverage` artifact of the run. The badge at the top of this page is the coverage
of `main`: the `pages` workflow measures it on every push to `main` and publishes
`coverage.json` next to the web IDE, where shields.io reads it. The web IDE's
TypeScript is not measured.

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
    "./custom_format.yaml",
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
compiler.load("custom_format", None)?;

// Parse a buffer
let data = std::fs::read("file.bin")?;
let engine = Engine::new(&compiler.registry);
let mut cursor = Cursor::new(&data);
let value = engine.parse_type("custom_format", &mut cursor)?;

// Render
println!("{}", json::render_pretty(&value));
# Ok::<(), Box<dyn std::error::Error>>(())
```

The library has a default feature, `fs`, for everything that reads the filesystem
or the user's configuration: `parse_file`, `load_schema_file`, `doe::config`,
`Cursor::from_file` and `FsSource`. Built with `default-features = false` it has
none of them, and none of their dependencies, and builds for
`wasm32-unknown-unknown`; schemas then come from a `MemorySource` and the input
from a byte slice. CI builds it that way and runs the tests that need no files
under WebAssembly.

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

An error from loading or compiling a schema names what is wrong (a field, a
type, an expression) but not where it is written. `locate_error(text, &error)`
gives the position in the text of the schema (line, column, byte offset and
length) of the field, `type:`, `enum:`, expression, import or type declaration
that the error is about, for an editor to underline; a `SchemaMap` made once from
the text with `SchemaMap::parse` answers for any number of errors. An error from
another schema than the one in `text` has no position.

To know where in the input every value came from (to select the bytes of a
field, or the field of a byte), parse with `parse_type_traced`. It returns the
same value and a tree of `TraceNode`s with the bit range of each field, element
of a repeated field and instance:

```rust,no_run
# use doe::schema::compiler::Compiler;
# use doe::parser::{cursor::Cursor, engine::Engine};
# let mut compiler = Compiler::new(vec![]);
# compiler.load("custom_format", None)?;
# let data = std::fs::read("file.bin")?;
# let engine = Engine::new(&compiler.registry);
let (value, root) = engine.parse_type_traced("custom_format", &mut Cursor::new(&data))?;
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
# compiler.load("custom_format", None)?;
# let data = std::fs::read("file.bin")?;
# let engine = Engine::new(&compiler.registry);
let outcome = engine.parse_type_partial("custom_format", &mut Cursor::new(&data))?;
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
