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
| WAV | `wav` (reads the INFO tags as ASCII) | **library** (`wav_audio`: the gallery's `wav` reads the text of the tags as ASCII, and the files that people have use UTF-8 in them, so every such file ends with an error about the text; this type reads it as UTF-8) |
| FLAC | none | **library** (`flac`) |
| FLV | none | **library** (`flv`) |
| tzfile | none | **library** (`tzfile`) |
| PSD / PSB | none | **library** (`psd`) |
| cpio | `cpio_old_le` reads the old binary format, little-endian | **library** (`cpio`: newc and crc, odc, and the old binary format in both byte orders, to the first trailer; the gallery's stays) |
| UBI / UBIFS | `ubifs` reads a bare UBIFS image (tested on `mkfs.ubifs` images with every compressor: it reads them whole, and fails on a prefix of one) | **library** (`ubi`: the erase counter and volume identifier headers of every eraseblock and the layout volume's table of volumes; the LEBs of a volume are not joined, so a UBIFS inside is not reached; fastmap is bytes) |
| SquashFS | none | **library** (`squashfs`: the superblock, the id, fragment and export tables, and the root inode and directory, for metadata that is stored or zlib-compressed (compression `gzip`); the other compressions, the inodes of the entries and the file view are out, see the type's comment) |
| gzip | `gzip` reads the header and the compressed data as bytes | **library** (`gzip_file`: the same, and it says its payload is the gunzipped file, so `doe file.tar.gz` shows the tar; see "Files inside files" in the README) |
| zlib stream | none (`process: zlib` inflates a field) | **library** (`zlib_stream`: the header, level, window and dictionary id, the Adler-32, and the inflated data; found by the 21 first-two-byte values that compressors write, about one file in 3000 starts so, and a stream that does not inflate does not fit) |
| Snappy | none | **library** (`snappy`: the framing format's chunks and every compressed block as elements, found by the stream identifier; `snappy::snappy_block` for a raw block, which has no magic; the copies are not expanded) |
| compress (`.Z`), lzop (`.lzo`) | none | **library** (`compress_z`: the flags and the LZW codes as bytes; `lzop`: the header with the flags that gate its fields, and every block up to the end block; found by `1F 9D` and the lzop magic; the compressed data is bytes) |
| LZMA-alone (`.lzma`), lzip (`.lz`) | none | **library** (`lzma_alone`: the 13-byte header, found by name or by a properties byte under 225 and a dictionary of at least 4096, since the format has no magic; `lzip`: the header, the trailer and the last member, found by `LZIP`; the LZMA streams are bytes, and the members between the first and the last are not shown) |
| bzip2, xz, zstd, lz4 | `gzip` only | **library** (`bzip2`, `xz`, `zstd`, `lz4`: the headers and, where the format has them at byte boundaries, the blocks; the compressed data is bytes) |
| COFF object files | `microsoft_pe` has a COFF header | **library** (`coff`) |
| MPEG transport stream | none | **library** (`mpegts`) |
| MP3 / MPEG audio | `id3v1_1`, `id3v2_3`, `id3v2_4` for the tags | **library** (`mp3`: layer III frames; the tags are the gallery's) |
| CAB | none | **library** (`cab`) |
| ZIP | `zip` | **library** (`zip_archive`: read from the central directory, so streamed archives and the central extended timestamp that Info-ZIP writes read; the gallery's `zip` cannot read those, see below) |
| 7z | none | **library** (`sevenzip`: the signature header; the next header is bytes; an identifier cannot start with a digit) |
| TGA | `tga` | **library** (`tga_image`: the gallery's `tga` reads an extension area at the offset in the footer even when it is zero, which is how Pillow, ImageMagick and others say there is none, so it reads the pixels as an author name and comments; this type reads the area only when there is one) |
| Mozilla MAR | `mozilla_mar` | **library** (`mar_archive`: the gallery's definition always reads the size of the file, the signatures and the sections after the offset of the index; the files of the Python package `mar` have none of them, and the first file starts at byte 8) |
| UEFI signature list (`.esl`) | `efivar_signature_list` | **library** (`efi_signature_list`: the gallery's definition is of the file of an EFI variable, which starts with 4 bytes of attributes; the lists that efitools writes do not) |
| TIFF | `exif` reads the TIFF structure (header, IFDs) | not needed: use `exif` |
| HEIF / AVIF / MP4 (ISO base media) | `quicktime_mov` nests the boxes but decodes only `ftyp`, `tkhd` and `mvhd` | **library** (`mp4`: the movie, tracks, sample tables and entries, tags and fragments decoded, the samples not; the gallery's `quicktime_mov` stays for QuickTime `.mov`) |
| Matroska / WebM | none | out: EBML ids and sizes are variable-length numbers whose width is in their first bits; the schema language has no way to say that without a type per width, which is the `vlq` kind of workaround and not worth it for 100+ element ids |
| PDF | none | **library** (`pdf`, in layers as far as the language goes: the header, the end of the file read as lines from the last 1024 bytes, the classic cross-reference table and trailer, and the objects it points to as tokens; no stream data, cross-reference streams, `/Prev` chain or typed views, because the language has no search and no look-up of a key in a dictionary; the type's comment lists the limits) |
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
