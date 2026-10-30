# The type library

Types for file formats that the [Kaitai Struct gallery](https://github.com/kaitai-io/kaitai_struct_formats)
(the submodule `conformance/kaitai_struct_formats`, at the commit the repository is at) does not have. A format
that the gallery has is not copied here: use the gallery's definition (see the main README, *Type library*).

Each type has a comment at its top (what it covers, its limits, the specification), a sample file in
`samples/` made by `make_samples.py` (run `python3 -I types/make_samples.py`; the files are in the repository so
that the tests need no Python, and a test checks that the script makes them) and a test of the values in
`crates/doe/tests/integration/library.rs`.

## The common formats the gallery lacks

Checked against the gallery's file names and `meta.id`s (the commit `f38fd7d`). *Library* means there is a type
here; the others are left out, with the reason (a format that is in scope and not written yet would be marked *next*; there is none now).

| Format | Gallery | Here |
|--------|---------|------|
| tar | none | **library** (`tar`) |
| ar, and `.deb` (an ar archive) | none | **library** (`ar`) |
| pcapng | `pcap` only | **library** (`pcapng`) |
| WebAssembly | none | **library** (`wasm`) |
| NumPy `.npy` | none | **library** (`npy`) |
| ELF | `elf` | not needed: use `elf` |
| WOFF / WOFF2 | `ttf` for TrueType and OpenType | **library** (`woff`, `woff2`: the headers and, for WOFF, the table directory) |
| FLAC | none | **library** (`flac`) |
| FLV | none | **library** (`flv`) |
| tzfile | none | **library** (`tzfile`) |
| PSD / PSB | none | **library** (`psd`) |
| bzip2, xz, zstd, lz4 | `gzip` only | **library** (`bzip2`, `xz`, `zstd`, `lz4`: the headers and, where the format has them at byte boundaries, the blocks; the compressed data is bytes) |
| COFF object files | `microsoft_pe` has a COFF header | **library** (`coff`) |
| MPEG transport stream | none | **library** (`mpegts`) |
| MP3 / MPEG audio | `id3v1_1`, `id3v2_3`, `id3v2_4` for the tags | **library** (`mp3`: layer III frames; the tags are the gallery's) |
| CAB | none | **library** (`cab`) |
| ZIP | `zip` | **library** (`zip_archive`: read from the central directory, so streamed archives and the central extended timestamp that Info-ZIP writes read; the gallery's `zip` cannot read those, see below) |
| 7z | none | **library** (`sevenzip`: the signature header; the next header is bytes; an identifier cannot start with a digit) |
| TIFF | `exif` reads the TIFF structure (header, IFDs) | not needed: use `exif` |
| HEIF / AVIF / MP4 (ISO base media) | `quicktime_mov` reads the box structure | not needed: use `quicktime_mov` for the boxes |
| Matroska / WebM | none | out: EBML ids and sizes are variable-length numbers whose width is in their first bits; the schema language has no way to say that without a type per width, which is the `vlq` kind of workaround and not worth it for 100+ element ids |
| PDF | none | out: a text format with cross-reference tables found by searching from the end; a parser, not a layout |
| Parquet, Avro, CBOR | none | out: self-describing or schema-driven encodings (Thrift, varints, recursive values); the layout depends on the data |
| git objects and packs | none | out: zlib-compressed objects and variable-length pack entry headers |
| Protocol Buffers, MessagePack, BSON | in the gallery | in the gallery |

A format that is a good fit for the gallery can be offered upstream; the project does not open pull requests on
other repositories by itself.

**The gallery's `zip`** reads a file from its start, section by section, and reads a time for each flag of the
extended timestamp extra field (`0x5455`). Info-ZIP's `zip` sets the flags of the access time in the central
directory's copy of the field while it stores only the modification time there (the specification says the central
copy has only that), so the definition asks for 4 bytes where none are left and fails on archives that `zip` writes
on Linux and macOS ("requested 4 bytes, 0 remaining"; the reference compiler's parser fails the same way). A streamed
archive (flag bit 3, zeros in the local headers) cannot be read from the start either. `zip_archive` reads the
central directory instead.
