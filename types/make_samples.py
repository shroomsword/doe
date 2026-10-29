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


if __name__ == "__main__":
    put("sample.tar", tar())
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
