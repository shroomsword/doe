#!/usr/bin/env python3
"""Makes the sample files in samples/ that the tests of the types in this directory parse.

Every file is generated here, from nothing but this script (no file of another project is copied), so
its licence is the repository's. Run it again to make them again: the files do not change from one run
to the next.

    python3 -I types/make_samples.py
"""

import io
import struct
import tarfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = HERE / "samples"


def put(name: str, data: bytes) -> None:
    OUT.mkdir(exist_ok=True)
    (OUT / name).write_bytes(data)


def tar() -> bytes:
    """A ustar archive with a directory and two files (fixed owner and time)."""
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        for name, data in [("docs/", None), ("docs/hello.txt", b"hello\n"), ("data.bin", bytes(range(10)))]:
            info = tarfile.TarInfo(name)
            info.mtime = 1_700_000_000
            info.uid = 1000
            info.gid = 1000
            info.uname = "doe"
            info.gname = "doe"
            if data is None:
                info.type = tarfile.DIRTYPE
                info.mode = 0o755
                archive.addfile(info)
            else:
                info.size = len(data)
                info.mode = 0o644
                archive.addfile(info, io.BytesIO(data))
    return buffer.getvalue()


def ar() -> bytes:
    """An ar archive of two members, one of an odd size (so it has a padding byte)."""
    out = b"!<arch>\n"
    for name, data in [("hello.txt/", b"hello"), ("bye.txt/", b"goodbye!")]:
        out += (
            name.ljust(16).encode()
            + b"1700000000".ljust(12)
            + b"1000".ljust(6)
            + b"1000".ljust(6)
            + b"100644".ljust(8)
            + str(len(data)).encode().ljust(10)
            + b"`\n"
            + data
        )
        if len(data) % 2:
            out += b"\n"
    return out


def pcapng() -> bytes:
    """A section header, an Ethernet interface and two packets (one enhanced, one simple)."""

    def block(kind: int, body: bytes) -> bytes:
        body += b"\0" * (-len(body) % 4)
        length = len(body) + 12
        return struct.pack("<II", kind, length) + body + struct.pack("<I", length)

    header = block(0x0A0D0D0A, struct.pack("<IHHq", 0x1A2B3C4D, 1, 0, -1))
    interface = block(1, struct.pack("<HHI", 1, 0, 65535))
    packet = bytes.fromhex("ffffffffffff" "020000000001" "0800") + b"doe test packet"
    enhanced = block(6, struct.pack("<IIIII", 0, 0x0005_E2F6, 0x1234_5678, len(packet), len(packet)) + packet)
    simple = block(3, struct.pack("<I", 4) + b"abcd")
    return header + interface + enhanced + simple


def leb128(n: int) -> bytes:
    out = b""
    while True:
        byte = n & 0x7F
        n >>= 7
        if n:
            out += bytes([byte | 0x80])
        else:
            return out + bytes([byte])


def wasm() -> bytes:
    """A module that exports `add`: type, function, export and code sections, and a custom section of 200 bytes
    (so a section's size takes two bytes of LEB128)."""

    def section(kind: int, body: bytes) -> bytes:
        return bytes([kind]) + leb128(len(body)) + body

    types = section(1, bytes([1, 0x60, 2, 0x7F, 0x7F, 1, 0x7F]))
    functions = section(3, bytes([1, 0]))
    exports = section(7, bytes([1, 3]) + b"add" + bytes([0, 0]))
    code = section(10, bytes([1, 7, 0, 0x20, 0, 0x20, 1, 0x6A, 0x0B]))
    name = b"producers"
    custom = section(0, leb128(len(name)) + name + bytes(200 - len(name) - 1))
    return b"\0asm" + struct.pack("<I", 1) + types + functions + exports + code + custom


def npy() -> bytes:
    """A 2 x 3 array of little-endian 16-bit integers, version 1.0 of the format."""
    text = "{'descr': '<i2', 'fortran_order': False, 'shape': (2, 3), }"
    text += " " * (-(10 + len(text) + 1) % 64) + "\n"
    return b"\x93NUMPY" + bytes([1, 0]) + struct.pack("<H", len(text)) + text.encode("latin1") + struct.pack("<6h", 1, 2, 3, -4, -5, -6)


def woff() -> bytes:
    """A WOFF 1 font with two tables (one stored compressed, one not) and no metadata."""
    tables = [(b"cmap", b"\x01\x02\x03\x04" * 5, 40), (b"head", b"\x00" * 24, 24)]
    directory_end = 44 + 20 * len(tables)
    offset = directory_end
    directory = b""
    data = b""
    for tag, body, original in tables:
        directory += tag + struct.pack(">IIII", offset, len(body), original, 0xDEADBEEF)
        padded = body + b"\0" * (-len(body) % 4)
        data += padded
        offset += len(padded)
    total = 44 + len(directory) + len(data)
    header = b"wOFF" + struct.pack(">IIHHIHHIIIII", 0x00010000, total, len(tables), 0, 64, 1, 2, 0, 0, 0, 0, 0)
    return header + directory + data


def woff2() -> bytes:
    """The 48 bytes of a WOFF 2 header, and a few bytes where the directory and the data would be."""
    header = b"wOF2" + struct.pack(">IIHHIIHHIIIII", 0x00010000, 64, 3, 0, 4096, 20, 1, 0, 0, 0, 0, 0, 0)
    return header + bytes(range(16))


def flac() -> bytes:
    """A stream info block, Vorbis comments, padding, and a few bytes where the frames would be."""

    def block(last: bool, kind: int, body: bytes) -> bytes:
        return bytes([(0x80 if last else 0) | kind]) + len(body).to_bytes(3, "big") + body

    # 44100 Hz, 2 channels, 16 bits, 441000 samples (ten seconds)
    packed = (44100 << 44) | (1 << 41) | (15 << 36) | 441000
    info = struct.pack(">HH", 4096, 4096) + (0).to_bytes(3, "big") + (0).to_bytes(3, "big") + packed.to_bytes(8, "big") + bytes(range(16))
    vendor = b"doe test"
    comments = [b"TITLE=Sample", b"ARTIST=doe"]
    vorbis = struct.pack("<I", len(vendor)) + vendor + struct.pack("<I", len(comments))
    for text in comments:
        vorbis += struct.pack("<I", len(text)) + text
    return b"fLaC" + block(False, 0, info) + block(False, 4, vorbis) + block(True, 1, bytes(8)) + b"\xff\xf8\x00\x00frame"


def flv() -> bytes:
    """A header with audio and video, an empty previous-tag size, and two tags (script data, then video)."""

    def tag(kind: int, timestamp: int, body: bytes) -> bytes:
        head = bytes([kind]) + len(body).to_bytes(3, "big") + (timestamp & 0xFFFFFF).to_bytes(3, "big")
        head += bytes([timestamp >> 24]) + (0).to_bytes(3, "big")
        return head + body + struct.pack(">I", 11 + len(body))

    header = b"FLV" + bytes([1, 0b0000_0101]) + struct.pack(">I", 9) + struct.pack(">I", 0)
    return header + tag(18, 0, b"\x02\x00\x0aonMetaData") + tag(9, 0x1_00_00_10, b"\x17\x00\x00\x00\x00")


def tzfile() -> bytes:
    """A version 1 file for a zone with two transitions and two local time types (CET and CEST)."""
    abbreviations = b"CET\0CEST\0"
    counts = struct.pack(">IIIIII", 0, 0, 0, 2, 2, len(abbreviations))
    times = struct.pack(">2i", 1_679_792_400, 1_698_541_200)
    kinds = bytes([1, 0])
    types = struct.pack(">iBB", 3600, 0, 0) + struct.pack(">iBB", 7200, 1, 4)
    return b"TZif" + b"\0" + bytes(15) + counts + times + kinds + types + abbreviations


def psd() -> bytes:
    """A 4 x 2 RGB PSD with one image resource (the resolution, id 1005, with an empty name) and raw image data."""
    resource = b"8BIM" + struct.pack(">H", 1005) + b"\0\0" + struct.pack(">I", 4) + b"\0\x48\0\x48"
    header = b"8BPS" + struct.pack(">H", 1) + bytes(6) + struct.pack(">HIIHH", 3, 2, 4, 8, 3)
    return (
        header
        + struct.pack(">I", 0)
        + struct.pack(">I", len(resource))
        + resource
        + struct.pack(">I", 0)
        + struct.pack(">H", 0)
        + bytes(range(24))
    )


def bzip2() -> bytes:
    import bz2

    return bz2.compress(b"hello hello hello hello\n" * 20, 9)


def xz() -> bytes:
    import lzma

    return lzma.compress(b"hello hello hello hello\n" * 20, format=lzma.FORMAT_XZ, check=lzma.CHECK_CRC32)


def xz_vli(value: int) -> bytes:
    """An xz variable-length integer: seven bits per byte, low group first."""
    out = bytearray()
    while value >= 0x80:
        out.append(value & 0x7F | 0x80)
        value >>= 7
    out.append(value)
    return bytes(out)


def xz_stream(blocks: list[bytes], check: str = "crc32", sizes_in_headers: bool = False, delta: bool = False) -> bytes:
    """An xz stream built from its parts: one block for each item of `blocks` (LZMA2 by Python's liblzma, with the
    compressed and uncompressed sizes in the block header when `sizes_in_headers`; with the delta filter first when
    `delta`), the index, and the footer. `check` is `crc32` or `sha256`."""
    import hashlib
    import lzma
    import zlib

    flags = {"crc32": 1, "sha256": 10}[check]

    def checksum(data: bytes) -> bytes:
        return struct.pack("<I", zlib.crc32(data)) if check == "crc32" else hashlib.sha256(data).digest()

    header_flags = bytes([0, flags])
    out = bytearray(b"\xfd7zXZ\x00" + header_flags + struct.pack("<I", zlib.crc32(header_flags)))
    records = []
    for data in blocks:
        filters = ([{"id": lzma.FILTER_DELTA, "dist": 2}] if delta else []) + [{"id": lzma.FILTER_LZMA2, "dict_size": 1 << 20}]
        packed = lzma.compress(data, format=lzma.FORMAT_RAW, filters=filters)
        # the header: flags, the sizes when they are given, the filter flags (the LZMA2 dictionary 1 MiB is code 16)
        body = b""
        if sizes_in_headers:
            body += xz_vli(len(packed)) + xz_vli(len(data))
        if delta:
            body += xz_vli(0x03) + xz_vli(1) + bytes([1])
        body += xz_vli(0x21) + xz_vli(1) + bytes([16])
        block_flags = (len(filters) - 1) | (0xC0 if sizes_in_headers else 0)
        raw = bytes([block_flags]) + body
        size = (len(raw) + 1 + 4 + 3) // 4 * 4
        head = bytes([size // 4 - 1]) + raw
        head = head.ljust(size - 4, b"\0")
        head += struct.pack("<I", zlib.crc32(head))
        pad = bytes((4 - len(packed) % 4) % 4)
        check_bytes = checksum(data)
        out += head + packed + pad + check_bytes
        records.append((len(head) + len(packed) + len(check_bytes), len(data)))
    index = b"\x00" + xz_vli(len(records)) + b"".join(xz_vli(u) + xz_vli(d) for u, d in records)
    index += bytes((4 - len(index) % 4) % 4)
    index += struct.pack("<I", zlib.crc32(index))
    out += index
    footer_body = struct.pack("<I", len(index) // 4 - 1) + header_flags
    out += struct.pack("<I", zlib.crc32(footer_body)) + footer_body + b"YZ"
    return bytes(out)


def zstd() -> bytes:
    """Two frames: a raw block with a size in the header and a checksum, and a run-length block over two bytes
    with a window descriptor and no size."""
    first = b"\x28\xb5\x2f\xfd" + bytes([0b0010_0100, 5]) + ((5 << 3) | 1).to_bytes(3, "little") + b"hello" + b"\x01\x02\x03\x04"
    # frame_content_size_flag 0, not single segment, no checksum, no dictionary: a window descriptor, no size
    second = b"\x28\xb5\x2f\xfd" + bytes([0b0000_0000, 0x58]) + ((100 << 3) | (1 << 1) | 1).to_bytes(3, "little") + b"x"
    return first + second


def lz4() -> bytes:
    """A frame with the size of the content and its checksum, and one block stored uncompressed."""
    flg = (1 << 6) | (1 << 5) | (1 << 3) | (1 << 2)
    descriptor = bytes([flg, 0x40]) + struct.pack("<Q", 5) + b"\x9e"
    return b"\x04\x22\x4d\x18" + descriptor + struct.pack("<I", 0x8000_0005) + b"hello" + struct.pack("<I", 0) + struct.pack("<I", 0x1234_5678)


def xxh32(data: bytes, seed: int = 0) -> int:
    """XXH32 of up to a few kilobytes, for the header checksum byte of an LZ4 frame."""
    p1, p2, p3, p4, p5 = 2654435761, 2246822519, 3266489917, 668265263, 374761393
    mask = 0xFFFFFFFF

    def rotl(value: int, bits: int) -> int:
        return ((value << bits) | (value >> (32 - bits))) & mask

    index = 0
    if len(data) >= 16:
        v = [(seed + p1 + p2) & mask, (seed + p2) & mask, seed & mask, (seed - p1) & mask]
        while index + 16 <= len(data):
            for lane in range(4):
                word = struct.unpack_from("<I", data, index + lane * 4)[0]
                v[lane] = (rotl((v[lane] + word * p2) & mask, 13) * p1) & mask
            index += 16
        h = (rotl(v[0], 1) + rotl(v[1], 7) + rotl(v[2], 12) + rotl(v[3], 18)) & mask
    else:
        h = (seed + p5) & mask
    h = (h + len(data)) & mask
    while index + 4 <= len(data):
        h = (rotl((h + struct.unpack_from("<I", data, index)[0] * p3) & mask, 17) * p4) & mask
        index += 4
    while index < len(data):
        h = (rotl((h + data[index] * p5) & mask, 11) * p1) & mask
        index += 1
    h ^= h >> 15
    h = (h * p2) & mask
    h ^= h >> 13
    h = (h * p3) & mask
    h ^= h >> 16
    return h


def lz4_block_with_matches() -> tuple[bytes, bytes]:
    """An LZ4 block - a sequence with 6 literals and a match of 24 bytes (its length in the extension byte), then the last
    sequence with 5 literals - and the data it expands to."""
    block = bytes([(6 << 4) | 15]) + b"abcdef" + struct.pack("<H", 6) + bytes([24 - 4 - 15]) + bytes([5 << 4]) + b"12345"
    return block, b"abcdef" * 5 + b"12345"


def lz4_frames() -> tuple[bytes, bytes]:
    """A standard frame with a compressed block and a block stored uncompressed (the header checksum is the real one), and
    a legacy frame with two blocks: one with a match, one of literals whose length needs an extension byte."""
    block, _ = lz4_block_with_matches()
    flg_bd = bytes([0x60, 0x40])
    header = b"\x04\x22\x4d\x18" + flg_bd + bytes([(xxh32(flg_bd) >> 8) & 255])
    standard = header + struct.pack("<I", len(block)) + block + struct.pack("<I", 0x8000_0006) + b"stored" + struct.pack("<I", 0)
    text = b"second legacy block\n"
    literals_only = bytes([0xF0, len(text) - 15]) + text
    legacy = b"\x02\x21\x4c\x18" + struct.pack("<I", len(block)) + block + struct.pack("<I", len(literals_only)) + literals_only
    return standard, legacy


def zstd_seekable() -> bytes:
    """A skippable frame of application data, two data frames (a raw block each) and the seek table of the zstd seekable
    format with a checksum for each entry (arbitrary numbers: the table's checksums are not checked by readers)."""

    def data_frame(content: bytes) -> bytes:
        # single segment, no checksum, a one-byte content size; one raw block that is the last
        return b"\x28\xb5\x2f\xfd" + bytes([0x20, len(content)]) + ((len(content) << 3) | 1).to_bytes(3, "little") + content

    contents = [b"hello zstd seekable\n", b"second frame\n"]
    frames = [data_frame(c) for c in contents]
    skippable = b"\x53\x2a\x4d\x18" + struct.pack("<I", 11) + b"application"
    entries = b"".join(struct.pack("<III", len(f), len(c), 0xC0DE0000 + i) for i, (f, c) in enumerate(zip(frames, contents)))
    table = entries + struct.pack("<IB", len(frames), 0x80) + bytes([0xB1, 0xEA, 0x92, 0x8F])
    seek = b"\x5e\x2a\x4d\x18" + struct.pack("<I", len(table)) + table
    return skippable + frames[0] + frames[1] + seek


def coff() -> bytes:
    """An x86-64 object file with one section (.text, four bytes), two symbols and a string table."""
    ofs_raw = 20 + 40
    ofs_symbols = ofs_raw + 4
    header = struct.pack("<HHIIIHH", 0x8664, 1, 1_700_000_000, ofs_symbols, 2, 0, 0)
    section = b".text".ljust(8, b"\0") + struct.pack("<IIIIIIHHI", 0, 0, 4, ofs_raw, 0, 0, 0, 0, 0x6000_0020)
    code = b"\x90\x90\x90\xc3"
    main = b"main".ljust(8, b"\0") + struct.pack("<IhHBB", 0, 1, 0x20, 2, 0)
    # a name longer than eight bytes: zeros, then an offset into the string table
    long_name = struct.pack("<II", 0, 4) + struct.pack("<IhHBB", 4, 1, 0x20, 3, 0)
    strings = struct.pack("<I", 4 + len(b"a_longer_name\0")) + b"a_longer_name\0"
    return header + section + code + main + long_name + strings


def mpegts() -> bytes:
    """Three packets: a PAT payload, a payload with an adaptation field, and a null packet."""

    def packet(pid: int, control: int, counter: int, adaptation: bytes, payload: bytes, start: bool = False) -> bytes:
        first = (int(start) << 6) | (pid >> 8)
        head = bytes([0x47, first, pid & 0xFF, (control << 4) | counter])
        body = adaptation + payload
        return (head + body).ljust(188, b"\xff")

    pat = packet(0, 1, 0, b"", bytes.fromhex("0000b00d0001c100000001e100") + b"\x00" * 4, start=True)
    pes = packet(0x100, 3, 5, bytes([7]) + b"\x10" + bytes(6), b"\x00\x00\x01\xe0" + b"video")
    null = packet(0x1FFF, 1, 0, b"", b"")
    return pat + pes + null


def mp3() -> bytes:
    """Three MPEG-1 layer III frames, 128 kbit/s at 44.1 kHz, the second with a padding byte."""
    header = 0xFFFB_9000 | 0  # sync, MPEG-1, layer III, no CRC; 128 kbit/s; 44.1 kHz; no padding; stereo
    frames = b""
    for padding in (0, 1, 0):
        head = (header | (padding << 9)).to_bytes(4, "big")
        frames += head + bytes(417 + padding - 4)
    return frames


def cab() -> bytes:
    """A cabinet with one folder that stores two files in one uncompressed block."""
    data = b"hello cabinet\ngoodbye\n"
    block = struct.pack("<IHH", 0, len(data), len(data)) + data
    files = b""
    for size, offset, name in [(14, 0, b"hello.txt"), (8, 14, b"bye.txt")]:
        files += struct.pack("<IIHHHH", size, offset, 0, 0x5A21, 0x6000, 0x20) + name + b"\0"
    ofs_files = 36 + 8
    ofs_data = ofs_files + len(files)
    total = ofs_data + len(block)
    header = b"MSCF" + struct.pack("<IIIIIBBHHHHH", 0, total, 0, ofs_files, 0, 3, 1, 1, 2, 0, 1234, 0)
    folder = struct.pack("<IHH", ofs_data, 1, 0)
    return header + folder + files + block


def sevenzip() -> bytes:
    """An archive with no files: a signature header, no packed streams, and a header of two bytes (kHeader, kEnd)."""
    import zlib

    next_header = b"\x01\x00"
    start = struct.pack("<QQI", 0, len(next_header), zlib.crc32(next_header))
    return b"7z\xbc\xaf\x27\x1c" + bytes([0, 4]) + struct.pack("<I", zlib.crc32(start)) + start + next_header


def tga_image() -> bytes:
    """A TGA file of version 2 with an image id, a colour map of two 24-bit entries, a 2x2 image of indexes, the
    developer directory with one entry, the extension area and the footer."""
    image_id = b"doe"
    color_map = bytes([0, 0, 255, 0, 255, 0])  # blue, green, red order of the entries as stored: B G R
    pixels = bytes([0, 1, 1, 0])
    header = struct.pack("<BBBHHBHHHHBB", len(image_id), 1, 1, 0, 2, 24, 0, 0, 2, 2, 8, 0x20)
    body = header + image_id + color_map + pixels
    developer_data = b"doe-data"
    developer_at = len(body)
    data_at = developer_at + 2 + 10  # the data follows the one entry of the directory
    body += struct.pack("<H", 1) + struct.pack("<HII", 0x0DE0, data_at, len(developer_data)) + developer_data

    def text(value, size):
        return value.encode() + bytes(size - len(value))

    extension = struct.pack("<H", 495) + text("doe authors", 41)
    for line in ("first line", "second line", "", ""):
        extension += text(line, 81)
    extension += struct.pack("<6H", 10, 26, 2024, 13, 45, 30) + text("a job", 41) + struct.pack("<3H", 1, 2, 3)
    extension += text("doe", 41) + struct.pack("<HB", 301, ord("b")) + struct.pack("<I", 0xFF102030)
    extension += struct.pack("<4H", 1, 1, 22, 10) + struct.pack("<III", 0, 0, 0) + bytes([3])
    assert len(extension) == 495, len(extension)
    extension_at = len(body)
    body += extension
    return body + struct.pack("<II", extension_at, developer_at) + b"TRUEVISION-XFILE.\x00"


def wav_audio() -> bytes:
    """PCM format, a few samples, INFO tags (one with a UTF-8 em dash and an odd length, so it is padded) and a bext chunk."""

    def chunk(kind: bytes, body: bytes) -> bytes:
        return kind + struct.pack("<I", len(body)) + body + (b"\0" if len(body) % 2 else b"")

    fmt = struct.pack("<HHIIHH", 1, 1, 8000, 16000, 2, 16)
    samples = struct.pack("<8h", 0, 1000, -1000, 2000, -2000, 3000, -3000, 0)
    info = b"INFO" + chunk(b"INAM", b"Sample\0") + chunk(b"ICMT", "Made by doe — partial".encode() + b"\0")
    bext = (
        b"A test recording".ljust(256, b"\0") + b"doe".ljust(32, b"\0") + b"ref-1".ljust(32, b"\0")
        + b"2026-11-04" + b"12:00:00" + struct.pack("<QH", 480000, 1)
    )
    body = b"WAVE" + chunk(b"fmt ", fmt) + chunk(b"fact", struct.pack("<I", 8)) + chunk(b"bext", bext) + chunk(b"data", samples) + chunk(b"LIST", info)
    return b"RIFF" + struct.pack("<I", len(body)) + body


def pdf_classic(extra: bytes = b"", prev: int | None = None, base: bytes = b"") -> bytes:
    """A classic PDF: a catalog, a page tree, a page with a content stream and a font, an info dictionary (literal
    and hexadecimal strings), the xref table and the trailer. With `base` and `prev`, an incremental update of it."""
    if base:
        out = base + extra
        objects = [(5, b"<< /Type /Annot /Subtype /Text /Contents (added later) >>")]
    else:
        out = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n"
        content = b"BT /F1 12 Tf 72 720 Td (Hello) Tj ET"
        objects = [
            (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
            (2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
            (3, b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>"),
            (4, b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream"),
            (5, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"),
            (6, b"<< /Title (A sample) /Author <646f65> /Producer (doe tests) /CreationDate (D:20261104120000Z) /Pi 3.14159 /Yes true /Nothing null >>"),
        ]
    offsets = {}
    for number, body in objects:
        offsets[number] = len(out)
        out += b"%d 0 obj\n" % number + body + b"\nendobj\n"
    start = len(out)
    first = min(offsets) if base else 0
    entries = b"" if base else b"0000000000 65535 f \n"
    entries += b"".join(b"%010d 00000 n \n" % offsets[n] for n in sorted(offsets))
    count = len(offsets) + (0 if base else 1)
    size = 7
    trailer = b"<< /Size %d /Root 1 0 R" % size + (b" /Info 6 0 R" if not base else b" /Prev %d" % prev) + b" >>"
    return out + b"xref\n%d %d\n" % (first, count) + entries + b"trailer\n" + trailer + b"\nstartxref\n%d\n%%%%EOF\n" % start


def pdf_xref_stream() -> bytes:
    """A PDF 1.5 file whose cross-reference is a stream (the bytes are not a real compressed table: the type does not
    read it, it only has to say so)."""
    out = b"%PDF-1.5\n%\xe2\xe3\xcf\xd3\n"
    one = len(out)
    out += b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n"
    two = len(out)
    out += b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n"
    xref = len(out)
    table = struct.pack(">BHB", 1, one, 0) + struct.pack(">BHB", 1, two, 0)
    out += b"3 0 obj\n<< /Type /XRef /Size 4 /W [1 2 1] /Root 1 0 R /Length %d >>\nstream\n" % len(table) + table + b"\nendstream\nendobj\n"
    return out + b"startxref\n%d\n%%%%EOF\n" % xref
def mp4_box(kind: bytes, body: bytes = b"") -> bytes:
    return struct.pack(">I", 8 + len(body)) + kind + body


def mp4_full(kind: bytes, body: bytes, version: int = 0, flags: int = 0) -> bytes:
    return mp4_box(kind, bytes([version]) + flags.to_bytes(3, "big") + body)


def mp4() -> bytes:
    """A video track (avc1) and an audio track (mp4a) with their sample tables, a tag, a free box and the media data;
    a box with a 64-bit size, a uuid box and an unknown box are in it too."""
    matrix = struct.pack(">9I", 0x10000, 0, 0, 0, 0x10000, 0, 0, 0, 0x40000000)
    mvhd = mp4_full(b"mvhd", struct.pack(">IIII", 0, 0, 1000, 5000) + struct.pack(">IH", 0x10000, 0x100) + bytes(10) + matrix + bytes(24) + struct.pack(">I", 3))

    def tkhd(track: int, duration: int, width: int, height: int, volume: int) -> bytes:
        return mp4_full(
            b"tkhd",
            struct.pack(">IIIII", 0, 0, track, 0, duration) + bytes(8) + struct.pack(">HHHH", 0, 0, volume, 0) + matrix + struct.pack(">II", width << 16, height << 16),
            0,
            3,
        )

    def mdhd(timescale: int, duration: int) -> bytes:
        return mp4_full(b"mdhd", struct.pack(">IIIIHH", 0, 0, timescale, duration, 0x55C4, 0))  # "und"

    def hdlr(kind: bytes, name: bytes) -> bytes:
        return mp4_full(b"hdlr", struct.pack(">I", 0) + kind + bytes(12) + name + b"\0")

    def stbl(entry: bytes, count: int, delta: int, sizes: list[int], offsets: list[int], sync: list[int] | None) -> bytes:
        out = mp4_full(b"stsd", struct.pack(">I", 1) + entry)
        out += mp4_full(b"stts", struct.pack(">III", 1, count, delta))
        if sync is not None:
            out += mp4_full(b"stss", struct.pack(">I", len(sync)) + b"".join(struct.pack(">I", n) for n in sync))
        out += mp4_full(b"stsc", struct.pack(">IIII", 1, 1, count, 1))
        out += mp4_full(b"stsz", struct.pack(">II", 0, len(sizes)) + b"".join(struct.pack(">I", n) for n in sizes))
        out += mp4_full(b"stco", struct.pack(">I", len(offsets)) + b"".join(struct.pack(">I", n) for n in offsets))
        return mp4_box(b"stbl", out)

    avcc = mp4_box(b"avcC", bytes([1, 0x64, 0, 0x1F, 0xFF, 0xE0, 0]))
    video_entry = mp4_box(
        b"avc1",
        bytes(6) + struct.pack(">H", 1) + bytes(16) + struct.pack(">HH", 320, 240) + struct.pack(">II", 0x480000, 0x480000) + bytes(4)
        + struct.pack(">H", 1) + b"doe".ljust(32, b"\0") + struct.pack(">Hh", 24, -1) + avcc,
    )
    esds = mp4_full(b"esds", bytes([3, 3, 0, 0, 0]))
    audio_entry = mp4_box(
        b"mp4a", bytes(6) + struct.pack(">H", 1) + bytes(8) + struct.pack(">HHHH", 2, 16, 0, 0) + struct.pack(">I", 44100 << 16) + esds
    )
    # the media data comes after the movie: the chunk offsets are filled in once the size of the movie is known
    def movie(video_at: int, audio_at: int) -> bytes:
        video = mp4_box(
            b"trak",
            tkhd(1, 5000, 320, 240, 0)
            + mp4_box(b"edts", mp4_full(b"elst", struct.pack(">I", 1) + struct.pack(">IiI", 5000, 0, 0x10000)))
            + mp4_box(
                b"mdia",
                mdhd(25, 125)
                + hdlr(b"vide", b"VideoHandler")
                + mp4_box(
                    b"minf",
                    mp4_full(b"vmhd", bytes(8), 0, 1)
                    + mp4_box(b"dinf", mp4_full(b"dref", struct.pack(">I", 1) + mp4_full(b"url ", b"", 0, 1)))
                    + stbl(video_entry, 5, 5, [10, 4, 4, 4, 4], [video_at], [1]),
                ),
            ),
        )
        audio = mp4_box(
            b"trak",
            tkhd(2, 5000, 0, 0, 0x100)
            + mp4_box(
                b"mdia",
                mdhd(44100, 220500)
                + hdlr(b"soun", b"SoundHandler")
                + mp4_box(b"minf", mp4_full(b"smhd", bytes(4)) + stbl(audio_entry, 4, 1024, [8, 8, 8, 8], [audio_at], None)),
            ),
        )
        tags = mp4_box(
            b"udta",
            mp4_full(
                b"meta",
                mp4_full(b"hdlr", struct.pack(">I", 0) + b"mdir" + bytes(12) + b"\0")
                + mp4_box(b"ilst", mp4_box(b"\xa9nam", mp4_box(b"data", struct.pack(">II", 1, 0) + b"Sample"))),
            ),
        )
        return mp4_box(b"moov", mvhd + video + audio + tags)

    ftyp = mp4_box(b"ftyp", b"isom" + struct.pack(">I", 512) + b"isomiso2avc1mp41")
    extras = (
        mp4_box(b"free", bytes(6))
        + struct.pack(">I", 1) + b"wide" + struct.pack(">Q", 20) + bytes(4)  # a box with a 64-bit size
        + mp4_box(b"uuid", bytes(range(16)) + b"payload")
        + mp4_box(b"zzzz", b"unknown")
    )
    size = len(ftyp) + len(movie(0, 0)) + len(extras)
    video_at = size + 8
    mdat = mp4_box(b"mdat", bytes(range(26)) + bytes(32))
    return ftyp + movie(video_at, video_at + 26) + extras + mdat


def mp4_fragmented() -> bytes:
    """A movie with no samples and one fragment: `mvex`, then `moof` with a track fragment and `mdat`, and a `mfra`."""
    mvhd = mp4_full(b"mvhd", struct.pack(">IIII", 0, 0, 1000, 0) + struct.pack(">IH", 0x10000, 0x100) + bytes(10) + struct.pack(">9I", 0x10000, 0, 0, 0, 0x10000, 0, 0, 0, 0x40000000) + bytes(24) + struct.pack(">I", 2))
    mvex = mp4_box(b"mvex", mp4_full(b"mehd", struct.pack(">I", 4000)) + mp4_full(b"trex", struct.pack(">IIIII", 1, 1, 1000, 0, 0)))
    moov = mp4_box(b"moov", mvhd + mvex)
    tfhd = mp4_full(b"tfhd", struct.pack(">I", 1), 0, 0x020000 | 0x08)  # default base is the moof, default duration present
    tfhd = mp4_full(b"tfhd", struct.pack(">II", 1, 1000), 0, 0x020000 | 0x08)
    tfdt = mp4_full(b"tfdt", struct.pack(">Q", 90000), 1)
    # data offset present, sample duration and size present: 3 samples
    trun_body = struct.pack(">Ii", 3, 0) + b"".join(struct.pack(">II", 1000, size) for size in (4, 5, 6))
    trun = mp4_full(b"trun", trun_body, 0, 0x001 | 0x100 | 0x200)
    traf_len = 8 + len(tfhd) + len(tfdt) + len(trun)
    moof_len = 8 + len(mp4_full(b"mfhd", struct.pack(">I", 7))) + traf_len
    # the data starts after the moof and the 8 bytes of the header of mdat
    trun = mp4_full(b"trun", struct.pack(">Ii", 3, moof_len + 8) + b"".join(struct.pack(">II", 1000, size) for size in (4, 5, 6)), 0, 0x001 | 0x100 | 0x200)
    moof = mp4_box(b"moof", mp4_full(b"mfhd", struct.pack(">I", 7)) + mp4_box(b"traf", tfhd + tfdt + trun))
    mdat = mp4_box(b"mdat", bytes(15))
    mfra = mp4_box(b"mfra", mp4_full(b"mfro", struct.pack(">I", 16)))
    ftyp = mp4_box(b"ftyp", b"iso5" + struct.pack(">I", 1) + b"iso5iso6dash")
    return ftyp + moov + moof + mdat + mfra


def gzip_file(data: bytes, name: bytes = b"", extra: bytes | None = None) -> bytes:
    """One gzip member of `data`: the header (with the name and an extra field when given), raw deflate, CRC-32 and size."""
    import zlib

    flags = (4 if extra is not None else 0) | (8 if name else 0)
    header = struct.pack("<BBBBIBB", 0x1F, 0x8B, 8, flags, 1_700_000_000, 0, 3)
    if extra is not None:
        header += struct.pack("<H", len(extra)) + extra
    if name:
        header += name + b"\0"
    packer = zlib.compressobj(9, zlib.DEFLATED, -15)
    body = packer.compress(data) + packer.flush()
    return header + body + struct.pack("<II", zlib.crc32(data), len(data) & 0xFFFFFFFF)


def cpio_archive(kind: str) -> bytes:
    """A directory, a file with an odd size, a symbolic link and the trailer, in one of the formats: `newc`, `crc`, `odc`,
    `bin_le` or `bin_be` (the old binary format in either byte order)."""
    mtime = 1_700_000_000
    entries = [
        ("docs", 0o040755, b""),
        ("docs/hello.txt", 0o100644, b"hello\n"),
        ("odd", 0o100600, b"odd"),
        ("link", 0o120777, b"docs/hello.txt"),
        ("TRAILER!!!", 0, b""),
    ]
    out = b""
    for inode, (name, mode, data) in enumerate(entries, 1):
        raw = name.encode() + b"\0"
        if kind in ("newc", "crc"):
            magic = b"070701" if kind == "newc" else b"070702"
            check = sum(data) if kind == "crc" else 0
            fields = [inode, mode, 0, 0, 1 if mode & 0o170000 != 0o040000 else 2, mtime, len(data), 8, 1, 0, 0, len(raw), check]
            header = magic + b"".join(b"%08X" % value for value in fields)
            out += header + raw
            out += bytes((4 - len(out) % 4) % 4)
            out += data + bytes((4 - len(data) % 4) % 4)
        elif kind == "odc":
            fields = [(65, 6), (inode, 6), (mode, 6), (0, 6), (0, 6), (1, 6), (0, 6), (mtime, 11), (len(raw), 6), (len(data), 11)]
            out += b"070707" + b"".join(b"%0*o" % (width, value) for value, width in fields) + raw + data
        else:
            order = "<" if kind == "bin_le" else ">"
            words = [0o070707, 7, inode, mode, 0, 0, 1, 0, mtime >> 16, mtime & 0xFFFF, len(raw), len(data) >> 16, len(data) & 0xFFFF]
            out += struct.pack(order + "13H", *words) + raw + bytes(len(raw) % 2) + data + bytes(len(data) % 2)
    return out + bytes((512 - len(out) % 512) % 512)


def squashfs_image(compress: bool = False, extended_root: bool = False) -> bytes:
    """A SquashFS 4.0 image: a root directory with a directory, a file and a symbolic link (the files' data in one fragment),
    and the id, fragment and export tables. The metadata blocks are stored, or zlib streams (what `mksquashfs -comp gzip`
    writes); the root inode is a basic directory inode, or an extended one."""
    import zlib

    def block(data: bytes) -> bytes:
        if compress:
            packed = zlib.compress(data, 9)
            return struct.pack("<H", len(packed)) + packed
        return struct.pack("<H", len(data) | 0x8000) + data

    mtime = 1_700_000_000
    fragment = b"hello squashfs\n" + b"inner file\n"

    def inode(kind: int, permissions: int, number: int, body: bytes) -> bytes:
        return struct.pack("<HHHHII", kind, permissions, 0, 1, mtime, number) + body

    def entry(offset: int, delta: int, kind: int, name: bytes) -> bytes:
        return struct.pack("<HhHH", offset, delta, kind, len(name) - 1) + name

    def listing(base: int, entries: list[bytes]) -> bytes:
        return struct.pack("<III", len(entries) - 1, 0, base) + b"".join(entries)

    # the inodes, in the order of the inode table, each at the offset that the directory entries give
    hello = inode(2, 0o644, 2, struct.pack("<IIII", 0, 0, 0, 15))
    link = inode(3, 0o777, 5, struct.pack("<II", 1, 9) + b"hello.txt")
    inner = inode(2, 0o644, 4, struct.pack("<IIII", 0, 0, 15, 11))
    at_hello, at_link, at_dir, at_inner = 0, len(hello), len(hello) + len(link), len(hello) + len(link) + 32
    # the directory table: the root's listing, then the subdirectory's
    root_listing = listing(
        3,
        [
            entry(at_dir, 0, 1, b"dir"),
            entry(at_hello, -1, 2, b"hello.txt"),
            entry(at_link, 2, 3, b"link"),
        ],
    )
    inner_listing = listing(4, [entry(at_inner, 0, 2, b"inner.txt")])
    # a basic directory inode: block, link count, size + 3, offset in the block, parent
    subdir = inode(1, 0o755, 3, struct.pack("<IIHHI", 0, 2, len(inner_listing) + 3, len(root_listing), 1))
    assert len(subdir) == 32
    inodes = hello + link + subdir + inner
    at_root = len(inodes)
    if extended_root:
        # type 8: link count, size + 3, block, parent, index count, offset, xattr (none)
        inodes += inode(8, 0o755, 1, struct.pack("<IIIIHHI", 3, len(root_listing) + 3, 0, 6, 0, 0, 0xFFFFFFFF))
    else:
        inodes += inode(1, 0o755, 1, struct.pack("<IIHHI", 0, 3, len(root_listing) + 3, 0, 6))

    out = bytearray(96)
    out += fragment
    inode_table = len(out)
    out += block(inodes)
    directory_table = len(out)
    out += block(root_listing + inner_listing)
    # each lookup table: its metadata block, then the index of the blocks' positions
    entries_at = len(out)
    out += block(struct.pack("<QII", 96, len(fragment) | 0x1000000, 0))
    fragment_table = len(out)
    out += struct.pack("<Q", entries_at)
    refs = {1: at_root, 2: at_hello, 3: at_dir, 4: at_inner, 5: at_link}
    refs_at = len(out)
    out += block(b"".join(struct.pack("<Q", refs[number]) for number in range(1, 6)))
    export_table = len(out)
    out += struct.pack("<Q", refs_at)
    ids_at = len(out)
    out += block(struct.pack("<II", 0, 1000))
    id_table = len(out)
    out += struct.pack("<Q", ids_at)

    # the flags: exportable, no xattrs, and (stored) uncompressed inodes, fragments and ids
    flags = 0x80 | 0x200 | (0 if compress else 0x1 | 0x8 | 0x800)
    out[:96] = struct.pack(
        "<IIIIIHHHHHHQQQQQQQQ",
        0x73717368,
        5,
        mtime,
        131072,
        1,
        1,
        17,
        flags,
        2,
        4,
        0,
        at_root,
        len(out),
        id_table,
        0xFFFFFFFFFFFFFFFF,
        inode_table,
        directory_table,
        fragment_table,
        export_table,
    )
    return bytes(out)


def ubi_image() -> bytes:
    """A UBI image of four 16 KiB eraseblocks: the two copies of the layout volume, one block of volume 0 (a dynamic volume
    "rootfs") and one that is free (an erase counter header and nothing else)."""
    import zlib

    size = 16384
    image_seq = 0x1234ABCD

    def crc(data: bytes) -> int:
        # UBI's CRC-32 starts at all ones and is not inverted at the end
        return zlib.crc32(data) ^ 0xFFFFFFFF

    def erase_counter(counter: int) -> bytes:
        head = b"UBI#" + struct.pack(">B3xQIII32x", 1, counter, 64, 128, image_seq)
        return head + struct.pack(">I", crc(head))

    def volume_id(vol_type: int, vol_id: int, lnum: int, sqnum: int, compat: int = 0, data: bytes = b"") -> bytes:
        head = b"UBI!" + struct.pack(">BBBBII4xIII", 1, vol_type, 0, compat, vol_id, lnum, len(data), 0, 0)
        head += struct.pack(">I4xQ12x", zlib.crc32(data) ^ 0xFFFFFFFF if data else 0, sqnum)
        return head + struct.pack(">I", crc(head))

    def record(blocks: int, kind: int, name: bytes) -> bytes:
        body = struct.pack(">IIIBBH", blocks, 0, 0, kind, 0, len(name)) + name.ljust(128, b"\0") + b"\0" + bytes(23)
        return body + struct.pack(">I", crc(body))

    table = record(3, 1, b"rootfs") + record(0, 0, b"")
    table = table.ljust(172 * 94, b"\xff")

    def block(counter: int, header: bytes | None, data: bytes = b"") -> bytes:
        out = erase_counter(counter).ljust(64, b"\xff")
        if header is not None:
            out = out.ljust(64) + header
            out = out.ljust(128, b"\xff") + data
        return out.ljust(size, b"\xff")

    payload = b"UBIFS data of the volume" * 4
    return (
        block(2, volume_id(1, 0x7FFFEFFF, 0, 1, compat=4), table)
        + block(3, volume_id(1, 0x7FFFEFFF, 1, 2, compat=4), table)
        + block(7, volume_id(1, 0, 0, 3, data=payload), payload)
        + block(1, None)
    )


def lzma_alone(sized: bool = False) -> bytes:
    """A .lzma file as liblzma writes it (the size field is all ones, the stream ends with a marker); with `sized`, the
    size field says the real size, as a file written by a tool that knew it does."""
    import lzma

    data = b"hello hello hello hello\n" * 20
    out = lzma.compress(data, format=lzma.FORMAT_ALONE, filters=[{"id": lzma.FILTER_LZMA1, "preset": 6, "dict_size": 1 << 20}])
    if sized:
        out = out[:5] + struct.pack("<Q", len(data)) + out[13:]
    return out


def lzip_member(data: bytes) -> bytes:
    """One lzip member (version 1) of `data`: an LZMA stream with lc=3, lp=0, pb=2 and an end marker, as lzip writes it."""
    import lzma
    import zlib

    filters = [{"id": lzma.FILTER_LZMA1, "lc": 3, "lp": 0, "pb": 2, "dict_size": 1 << 20}]
    stream = lzma.compress(data, format=lzma.FORMAT_RAW, filters=filters)
    member = b"LZIP" + bytes([1, 20]) + stream
    return member + struct.pack("<IQQ", zlib.crc32(data), len(data), len(member) + 20)


def zlib_stream(level: int = 6, wbits: int = 15, dictionary: bytes | None = None) -> bytes:
    """A zlib stream of a short text, as Python's zlib writes it at a level and window size, optionally with a preset dictionary."""
    import zlib

    data = b"hello zlib hello zlib hello zlib\n" * 30
    if dictionary is None:
        return zlib.compress(data, level, wbits)
    packer = zlib.compressobj(level, zlib.DEFLATED, wbits, zdict=dictionary)
    return packer.compress(data) + packer.flush()


def crc32c_masked(data: bytes) -> int:
    """The CRC-32C (Castagnoli) of `data` as the Snappy framing format stores it: rotated right by 15 bits and added to 0xa282ead8."""
    crc = 0xFFFFFFFF
    for byte in data:
        crc ^= byte
        for _ in range(8):
            crc = (crc >> 1) ^ (0x82F63B78 if crc & 1 else 0)
    crc ^= 0xFFFFFFFF
    return (((crc >> 15) | (crc << 17)) + 0xA282EAD8) & 0xFFFFFFFF


def snappy_block() -> tuple[bytes, bytes]:
    """A raw Snappy block with a literal (short, and one with a length byte) and a copy of each of the three kinds, and the
    data it expands to."""
    out = bytearray()
    elements = b""

    def literal(data: bytes) -> None:
        nonlocal elements
        n = len(data) - 1
        elements += bytes([n << 2]) + data if n < 60 else bytes([60 << 2, n]) + data
        out.extend(data)

    def copy(kind: int, offset: int, length: int) -> None:
        nonlocal elements
        if kind == 1:
            elements += bytes([1 | (length - 4) << 2 | (offset >> 8) << 5, offset & 255])
        elif kind == 2:
            elements += bytes([2 | (length - 1) << 2]) + struct.pack("<H", offset)
        else:
            elements += bytes([3 | (length - 1) << 2]) + struct.pack("<I", offset)
        for _ in range(length):
            out.append(out[-offset])

    literal(b"hello ")
    copy(1, 6, 6)
    literal(b"abcdefghij" * 7)
    copy(2, 76, 20)
    copy(3, 100, 10)
    literal(b"the end\n")
    length = len(out)
    varint = bytes([length & 127 | 128, length >> 7]) if length >= 128 else bytes([length])
    return varint + elements, bytes(out)


def snappy_stream() -> bytes:
    """A framed stream: the identifier, a compressed chunk, an uncompressed one, padding, a skippable chunk, and a second
    stream (its identifier and an uncompressed chunk)."""

    def chunk(kind: int, body: bytes) -> bytes:
        return bytes([kind]) + struct.pack("<I", len(body))[:3] + body

    ident = chunk(0xFF, b"sNaPpY")
    block, data = snappy_block()
    plain = b"stored as it is\n"
    return (
        ident
        + chunk(0x00, struct.pack("<I", crc32c_masked(data)) + block)
        + chunk(0x01, struct.pack("<I", crc32c_masked(plain)) + plain)
        + chunk(0xFE, bytes(5))
        + chunk(0x85, b"skip")
        + ident
        + chunk(0x01, struct.pack("<I", crc32c_masked(b"second stream\n")) + b"second stream\n")
    )
def compress_z(maxbits: int = 16, block_mode: bool = True) -> bytes:
    """A .Z file of a short text: every byte as a literal code of 9 bits (no repeats, so the dictionary is only read, never
    used), packed least significant bit first, as compress does."""
    text = b"compress me, abcdefg\n"
    bits = 0
    count = 0
    out = bytearray()
    for byte in text:
        bits |= byte << count
        count += 9
        while count >= 8:
            out.append(bits & 255)
            bits >>= 8
            count -= 8
    if count:
        out.append(bits & 255)
    return b"\x1f\x9d" + bytes([maxbits | (0x80 if block_mode else 0)]) + bytes(out)


def lzop_file(checksum: str = "adler32", name: bytes = b"sample.txt", extra: bytes | None = None, method: int = 1) -> bytes:
    """An lzop file of version 1.04 with two blocks and the end block: one compressed (an LZO1X stream of a literal run
    and one copy, written by hand) and one stored, with the checksums that `checksum` names (`adler32` or `crc32`)."""
    import zlib

    def check(data: bytes) -> int:
        return zlib.adler32(data) if checksum == "adler32" else zlib.crc32(data)

    flags = 0x03000000 | 0x8 | (0x40 if extra is not None else 0)
    if checksum == "adler32":
        flags |= 0x1 | 0x2
    else:
        flags |= 0x100 | 0x200 | 0x1000
    head = struct.pack(">HHHBB", 0x1040, 0x20A0, 0x0940, method, 5 if method == 1 else 9)
    head += struct.pack(">IIII", flags, 0o100644, 1_700_000_000, 0) + bytes([len(name)]) + name
    out = bytearray(b"\x89LZO\x00\r\n\x1a\n" + head + struct.pack(">I", check(head)))
    if extra is not None:
        field = struct.pack(">I", len(extra)) + extra
        out += field + struct.pack(">I", check(field))
    # "abc", then a copy of 8 bytes from 3 bytes back, then the end marker: "abcabcabcab"
    packed = bytes([17 + 3]) + b"abc" + bytes([0xE8, 0x00]) + bytes([0x11, 0, 0])
    plain = b"abcabcabcab"
    stored = b"stored as is\n"
    first = struct.pack(">II", len(plain), len(packed)) + struct.pack(">I", check(plain)) + struct.pack(">I", check(packed)) + packed
    second = struct.pack(">II", len(stored), len(stored)) + struct.pack(">I", check(stored)) + stored
    return bytes(out) + first + second + struct.pack(">I", 0)


def mar_archive() -> bytes:
    """A MAR file of the newer form: the size of the file, one signature, the product information block, two files
    and the index."""
    files = [("a.txt", 0o644, b"hello mar\n"), ("dir/b.bin", 0o755, bytes(range(20)))]
    signature = struct.pack(">II", 1, 4) + b"sig!"
    product = struct.pack(">II", 8 + 96, 1) + b"release".ljust(64, b"\0") + b"1.2.3".ljust(32, b"\0")
    start = 8 + 8 + 4 + len(signature) + 4 + len(product)
    body, index_entries, at = b"", b"", start
    for name, mode, data in files:
        index_entries += struct.pack(">III", at, len(data), mode) + name.encode() + b"\0"
        body += data
        at += len(data)
    index = struct.pack(">I", len(index_entries)) + index_entries
    total = start + len(body) + len(index)
    head = b"MAR1" + struct.pack(">IQI", start + len(body), total, 1) + signature + struct.pack(">I", 1) + product
    return head + body + index


def zip_archive() -> bytes:
    """An archive written the way Info-ZIP's `zip` writes one: a directory and a file with an extended timestamp
    (0x5455) that has the modification and access times in the local header but only the modification time in the
    central directory, a deflated file, and a file with a data descriptor (flag bit 3, zeros in its local header)."""
    import zlib

    def dos(year, month, day, hour, minute, second):
        return ((hour << 11) | (minute << 5) | (second // 2)), (((year - 1980) << 9) | (month << 5) | day)

    time, date = dos(2026, 9, 19, 13, 18, 44)
    modified = 1_789_000_000
    local_extra = struct.pack("<HHBII", 0x5455, 9, 3, modified, modified) + struct.pack("<HHBBIBI", 0x7875, 11, 1, 4, 0, 4, 0)
    central_extra = struct.pack("<HHBI", 0x5455, 5, 3, modified) + struct.pack("<HHBBIBI", 0x7875, 11, 1, 4, 0, 4, 0)

    def raw_deflate(data):
        compressor = zlib.compressobj(9, zlib.DEFLATED, -15)
        return compressor.compress(data) + compressor.flush()

    # name, method, flags, data as stored, uncompressed data, unix mode
    files = [
        ("docs/", 0, 0, b"", b"", 0o040755),
        ("docs/hello.txt", 0, 0, b"hello\n", b"hello\n", 0o100644),
        ("data.bin", 8, 0, raw_deflate(bytes(range(10)) * 20), bytes(range(10)) * 20, 0o100644),
        ("stream.txt", 8, 8, raw_deflate(b"streamed " * 8), b"streamed " * 8, 0o100644),
    ]
    out = b""
    central = b""
    for name, method, flags, stored, plain, mode in files:
        crc = zlib.crc32(plain)
        offset = len(out)
        encoded = name.encode()
        descriptor = flags & 8
        out += struct.pack(
            "<4sHHHHHIIIHH", b"PK\x03\x04", 10 if method == 0 else 20, flags, method, time, date,
            0 if descriptor else crc, 0 if descriptor else len(stored), 0 if descriptor else len(plain), len(encoded), len(local_extra),
        )
        out += encoded + local_extra + stored
        if descriptor:
            out += struct.pack("<4sIII", b"PK\x07\x08", crc, len(stored), len(plain))
        central += struct.pack(
            "<4sHHHHHHIIIHHHHHII", b"PK\x01\x02", (3 << 8) | 30, 10 if method == 0 else 20, flags, method, time, date,
            crc, len(stored), len(plain), len(encoded), len(central_extra), 0, 0, 0, mode << 16, offset,
        )
        central += encoded + central_extra
    end = struct.pack("<4sHHHHIIH", b"PK\x05\x06", 0, 0, len(files), len(files), len(central), len(out), 0)
    return out + central + end


if __name__ == "__main__":
    put("sample.tar", tar())
    put("sample.tar.gz", gzip_file(tar(), name=b"sample.tar"))
    put("sample.txt.gz", gzip_file(b"plain text, not an archive\n" * 3, extra=b"XYdata"))
    put("sample.a", ar())
    put("sample.pcapng", pcapng())
    put("sample.wasm", wasm())
    put("sample.npy", npy())
    put("sample.woff", woff())
    put("sample.woff2", woff2())
    put("sample.flac", flac())
    put("sample.flv", flv())
    put("sample.tzif", tzfile())
    put("sample.psd", psd())
    put("sample.bz2", bzip2())
    put("sample.xz", xz())
    text = b"hello hello hello hello\n" * 20
    put("sample_blocks.xz", xz_stream([text, b"second block\n" * 30, b"third\n"], sizes_in_headers=True))
    put("sample_delta.xz", xz_stream([bytes(range(200)) * 3], delta=True))
    put("sample_two_streams.xz", xz() + xz_stream([text, text[:100]], check="sha256"))
    put("sample.zst", zstd())
    put("sample.lz4", lz4())
    standard, legacy = lz4_frames()
    put("sample_blocks.lz4", standard)
    put("sample_legacy.lz4", legacy)
    put("sample_mixed.lz4", b"\x52\x2a\x4d\x18" + struct.pack("<I", 4) + b"meta" + standard + legacy)
    put("sample_seekable.zst", zstd_seekable())
    put("sample.obj", coff())
    put("sample.ts", mpegts())
    put("sample.mp3", mp3())
    put("sample.cab", cab())
    put("sample.7z", sevenzip())
    put("sample.zip", zip_archive())
    put("sample.tga", tga_image())
    put("sample.mar", mar_archive())
    for kind, name in (("newc", "sample.cpio"), ("crc", "sample_crc.cpio"), ("odc", "sample_odc.cpio"), ("bin_le", "sample_bin_le.cpio"), ("bin_be", "sample_bin_be.cpio")):
        put(name, cpio_archive(kind))
    put("sample.squashfs", squashfs_image())
    put("sample_gzip.squashfs", squashfs_image(compress=True))
    put("sample_extended.squashfs", squashfs_image(extended_root=True))
    put("sample.ubi", ubi_image())
    put("sample.lzma", lzma_alone())
    put("sample_sized.lzma", lzma_alone(sized=True))
    put("sample.lz", lzip_member(b"hello hello hello hello\n" * 20))
    put("sample_two.lz", lzip_member(b"hello hello hello hello\n" * 20) + lzip_member(b"second member\n" * 50))
    put("sample.zlib", zlib_stream())
    put("sample_fastest.zlib", zlib_stream(1))
    put("sample_fast.zlib", zlib_stream(4))
    put("sample_maximum.zlib", zlib_stream(9, 12))
    put("sample_dictionary.zlib", zlib_stream(6, 15, b"hello zlib"))
    put("sample.sz", snappy_stream())
    put("sample.block", snappy_block()[0])
    put("sample.Z", compress_z())
    put("sample_12bits.Z", compress_z(12, block_mode=False))
    put("sample.lzo", lzop_file())
    put("sample_crc32.lzo", lzop_file("crc32", b"", extra=b"extra data", method=3))
    put("sample.wav", wav_audio())
    classic = pdf_classic()
    put("sample.pdf", classic)
    put("sample_xrefstream.pdf", pdf_xref_stream())
    previous = int(classic.rsplit(b"startxref\n", 1)[1].split(b"\n")[0])
    put("sample_updated.pdf", pdf_classic(prev=previous, base=classic))
    put("sample.mp4", mp4())
    put("sample_fragmented.mp4", mp4_fragmented())
