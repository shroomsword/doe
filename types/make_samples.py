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
    put("sample.zst", zstd())
    put("sample.lz4", lz4())
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
    put("sample.wav", wav_audio())
    classic = pdf_classic()
    put("sample.pdf", classic)
    put("sample_xrefstream.pdf", pdf_xref_stream())
    previous = int(classic.rsplit(b"startxref\n", 1)[1].split(b"\n")[0])
    put("sample_updated.pdf", pdf_classic(prev=previous, base=classic))
    put("sample.mp4", mp4())
    put("sample_fragmented.mp4", mp4_fragmented())
