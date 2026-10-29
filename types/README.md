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
here; *next* means in scope and not written yet; the others are left out, with the reason.

| Format | Gallery | Here |
|--------|---------|------|
| tar | none | **library** (`tar`) |
| ar, and `.deb` (an ar archive) | none | **library** (`ar`) |
| pcapng | `pcap` only | **library** (`pcapng`) |
| WebAssembly | none | **library** (`wasm`) |
| NumPy `.npy` | none | **library** (`npy`) |
| ELF | `elf` | the gallery's: the library's own type was removed |
| WOFF / WOFF2 | `ttf` for TrueType and OpenType | **library** (`woff`, `woff2`: the headers and, for WOFF, the table directory) |
| FLAC | none | **library** (`flac`) |
| FLV | none | **library** (`flv`) |
| tzfile | none | **library** (`tzfile`) |
| PSD / PSB | none | **library** (`psd`) |
| bzip2, xz, zstd, lz4 | `gzip` only | **library** (`bzip2`, `xz`, `zstd`, `lz4`: the headers and, where the format has them at byte boundaries, the blocks; the compressed data is bytes) |
| COFF object files | `microsoft_pe` has a COFF header | next (as a file of its own) |
| MPEG transport stream | none | next (the 188-byte packets) |
| MP3 / MPEG audio | `id3v1_1`, `id3v2_3`, `id3v2_4` for the tags | next (the frame headers) |
| CAB | none | next (header and folders) |
| 7z | none | next (the signature header) |
| TIFF | `exif` reads the TIFF structure (header, IFDs) | not needed: use `exif` |
| HEIF / AVIF / MP4 (ISO base media) | `quicktime_mov` reads the box structure | not needed: use `quicktime_mov` for the boxes |
| Matroska / WebM | none | out: EBML ids and sizes are variable-length numbers whose width is in their first bits; the schema language has no way to say that without a type per width, which is the `vlq` kind of workaround and not worth it for 100+ element ids |
| PDF | none | out: a text format with cross-reference tables found by searching from the end; a parser, not a layout |
| Parquet, Avro, CBOR | none | out: self-describing or schema-driven encodings (Thrift, varints, recursive values); the layout depends on the data |
| git objects and packs | none | out: zlib-compressed objects and variable-length pack entry headers |
| Protocol Buffers, MessagePack, BSON | in the gallery | in the gallery |

A format that is a good fit for the gallery can be offered upstream; the project does not open pull requests on
other repositories by itself.
