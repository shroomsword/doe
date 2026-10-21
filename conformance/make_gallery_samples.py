#!/usr/bin/env python3
"""Writes the binary samples for the gallery definitions in formats/gallery/ to
samples/gallery/<dir>/<name>/.

The samples are small and made up; they are committed, so this script only has
to be run to change them. The ELF files are built with the C compiler of the
machine it runs on (`cc`), so running it again gives other bytes there. The
Mach-O files are made with `clang` for the macOS targets (it needs no linker for
objects); the Java class and the PE files are built by hand, as no compiler for
them is needed.
"""

import gzip
import io
import os
import struct
import subprocess
import tempfile
import zipfile
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent / "samples" / "gallery"


def write(path, sample, data):
    directory = ROOT / path
    directory.mkdir(parents=True, exist_ok=True)
    (directory / f"{sample}.bin").write_bytes(bytes(data))


# ── zip ──────────────────────────────────────────────────────────────────────

def make_zip(compression, names):
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w", compression) as archive:
        for name, data in names:
            info = zipfile.ZipInfo(name, date_time=(2020, 5, 17, 13, 45, 30))
            info.compress_type = compression
            info.external_attr = 0o644 << 16
            archive.writestr(info, data)
    return out.getvalue()


write("archive/zip", "deflated", make_zip(zipfile.ZIP_DEFLATED, [
    ("hello.txt", b"hello hello hello hello\n" * 5),
    ("dir/bytes.bin", bytes(range(256))),
]))
write("archive/zip", "stored", make_zip(zipfile.ZIP_STORED, [("a.txt", b"abc")]))

# ── gzip ─────────────────────────────────────────────────────────────────────

buffer = io.BytesIO()
with gzip.GzipFile(filename="hello.txt", mode="wb", fileobj=buffer, mtime=1589723130) as handle:
    handle.write(b"hello hello hello hello\n" * 5)
write("archive/gzip", "named", buffer.getvalue())

# ── png ──────────────────────────────────────────────────────────────────────

def chunk(kind, body):
    crc = zlib.crc32(kind + body) & 0xFFFFFFFF
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", crc)


PNG = b"\x89PNG\r\n\x1a\n"


def idat(rows):
    return chunk(b"IDAT", zlib.compress(b"".join(b"\x00" + row for row in rows)))


write("image/png", "rgba", PNG + b"".join([
    chunk(b"IHDR", struct.pack(">IIBBBBB", 2, 2, 8, 6, 0, 0, 0)),
    chunk(b"gAMA", struct.pack(">I", 45455)),
    chunk(b"pHYs", struct.pack(">IIB", 2835, 2835, 1)),
    chunk(b"tIME", struct.pack(">HBBBBB", 2020, 5, 17, 13, 45, 30)),
    chunk(b"tEXt", b"Title\x00Two by two"),
    idat([bytes([255, 0, 0, 255, 0, 255, 0, 255]), bytes([0, 0, 255, 255, 255, 255, 255, 128])]),
    chunk(b"IEND", b""),
]))
write("image/png", "palette", PNG + b"".join([
    chunk(b"IHDR", struct.pack(">IIBBBBB", 4, 1, 8, 3, 0, 0, 0)),
    chunk(b"PLTE", bytes([255, 0, 0, 0, 255, 0, 0, 0, 255])),
    chunk(b"tRNS", bytes([255, 128, 0])),
    chunk(b"sRGB", b"\x00"),
    idat([bytes([0, 1, 2, 1])]),
    chunk(b"IEND", b""),
]))

# ── bmp ──────────────────────────────────────────────────────────────────────

def bmp(header, palette, pixels):
    ofs = 14 + len(header) + len(palette)
    return b"BM" + struct.pack("<IHHI", ofs + len(pixels), 0, 0, ofs) + header + palette + pixels


def info_header(width, height, bits, compression=0, colors=0, extra=b""):
    return struct.pack("<IiiHHIIiiII", 40 + len(extra), width, height, 1, bits, compression, 0, 2835, 2835, colors, 0) + extra


rows24 = b"".join(bytes([b, g, r] * 2) + b"\x00\x00" for b, g, r in [(0, 0, 255), (255, 0, 0)])
write("image/bmp", "rgb24", bmp(info_header(2, 2, 24), b"", rows24))
palette = b"".join(bytes([b, g, r, 0]) for b, g, r in [(0, 0, 0), (255, 255, 255), (0, 0, 255), (255, 0, 0)])
write("image/bmp", "indexed8", bmp(info_header(4, 2, 8, colors=4), palette, bytes([0, 1, 2, 3, 3, 2, 1, 0])))
write("image/bmp", "bitfields32", bmp(
    info_header(1, 1, 32, compression=3, extra=struct.pack("<IIII", 0xFF0000, 0xFF00, 0xFF, 0xFF000000)),
    b"", struct.pack("<I", 0x80FF8040)))
write("image/bmp", "top_down16", bmp(info_header(2, -2, 16), b"", struct.pack("<HHHH", 0x7C00, 0x03E0, 0x001F, 0x7FFF)))

# ── gif ──────────────────────────────────────────────────────────────────────

gif = b"GIF89a" + struct.pack("<HHBBB", 2, 2, 0x80 | 0x01, 0, 0)
gif += bytes([0, 0, 0, 255, 255, 255])
gif += b"\x21\xf9\x04\x01\x0a\x00\x00\x00"
gif += b"\x2c" + struct.pack("<HHHHB", 0, 0, 2, 2, 0) + b"\x02\x02\x44\x01\x00"
gif += b"\x21\xfe\x04test\x00"
gif += b"\x3b"
write("image/gif", "small", gif)

# ── elf ──────────────────────────────────────────────────────────────────────

with tempfile.TemporaryDirectory() as work:
    source = os.path.join(work, "x.c")
    Path(source).write_text(
        "int counter = 3;\n"
        "static int helper(int a) { return a * 2; }\n"
        "int add(int a, int b) { return helper(a) + b + counter; }\n"
    )
    obj = os.path.join(work, "x.o")
    subprocess.run(["cc", "-c", "-O0", "-fno-asynchronous-unwind-tables", "-fno-ident",
                    "-fno-stack-protector", "-o", obj, source], check=True)
    write("executable/elf", "object", Path(obj).read_bytes())
    start = os.path.join(work, "t.c")
    Path(start).write_text("void _start(void) { for (;;) {} }\n")
    exe = os.path.join(work, "t")
    subprocess.run(["cc", "-static", "-nostdlib", "-O0", "-fno-asynchronous-unwind-tables",
                    "-fno-ident", "-Wl,--build-id=none", "-Wl,-z,norelro", "-Wl,-z,noseparate-code",
                    "-o", exe, start], check=True)
    write("executable/elf", "static_exe", Path(exe).read_bytes())

# ── wav ──────────────────────────────────────────────────────────────────────

def riff(chunks):
    body = b"WAVE" + b"".join(chunks)
    return b"RIFF" + struct.pack("<I", len(body)) + body


def riff_chunk(kind, body):
    return kind + struct.pack("<I", len(body)) + body + (b"\x00" if len(body) % 2 else b"")


def wav_format(tag, channels, rate, bits):
    align = channels * bits // 8
    return riff_chunk(b"fmt ", struct.pack("<HHIIHH", tag, channels, rate, rate * align, align, bits))


samples16 = struct.pack("<8h", 0, 1000, 2000, 1000, 0, -1000, -2000, -1000)
write("media/wav", "pcm16", riff([wav_format(1, 1, 8000, 16), riff_chunk(b"data", samples16)]))
write("media/wav", "stereo8_list", riff([
    wav_format(1, 2, 11025, 8),
    riff_chunk(b"LIST", b"INFO" + riff_chunk(b"ISFT", b"doe\x00")),
    riff_chunk(b"data", bytes([128, 130, 132, 134, 136, 138, 140])),
]))

# ── pcap and the network layers ──────────────────────────────────────────────

def ipv4(protocol, payload, source=(10, 0, 0, 1), destination=(10, 0, 0, 2)):
    header = struct.pack(">BBHHHBBH4s4s", 0x45, 0, 20 + len(payload), 0x1234, 0x4000, 64,
                         protocol, 0, bytes(source), bytes(destination))
    return header + payload


def udp(source, destination, payload):
    return struct.pack(">HHHH", source, destination, 8 + len(payload), 0) + payload


def tcp(source, destination, flags, payload=b""):
    return struct.pack(">HHIIBBHHH", source, destination, 1000, 2000, 5 << 4, flags, 8192, 0, 0) + payload


def ethernet(payload, ether_type=0x0800):
    return bytes.fromhex("0a0b0c0d0e0f") + bytes.fromhex("010203040506") + struct.pack(">H", ether_type) + payload


frame_udp = ethernet(ipv4(17, udp(5353, 53, b"query")))
frame_tcp = ethernet(ipv4(6, tcp(40000, 80, 0x18, b"GET / HTTP/1.0\r\n\r\n")))
write("network/ethernet_frame", "udp", frame_udp)
write("network/ethernet_frame", "tcp", frame_tcp)


def pcap(linktype, packets):
    out = struct.pack("<IHHiIII", 0xA1B2C3D4, 2, 4, 0, 0, 65535, linktype)
    for number, packet in enumerate(packets):
        out += struct.pack("<IIII", 1589723130 + number, 250000 * number, len(packet), len(packet)) + packet
    return out


write("network/pcap", "ethernet", pcap(1, [frame_udp, frame_tcp]))

# ── jpeg ─────────────────────────────────────────────────────────────────────

def segment(marker, body):
    return bytes([0xFF, marker]) + struct.pack(">H", len(body) + 2) + body


write("image/jpeg", "baseline", b"".join([
    b"\xff\xd8",
    segment(0xE0, b"JFIF\x00" + struct.pack(">BBBHHBB", 1, 1, 0, 72, 72, 0, 0)),
    segment(0xFE, b"made by hand"),
    segment(0xDB, bytes([0]) + bytes(range(1, 65))),
    segment(0xC0, struct.pack(">BHHB", 8, 8, 8, 1) + bytes([1, 0x11, 0])),
    segment(0xC4, bytes([0x00]) + bytes([0, 1] + [0] * 14) + bytes([0])),
    segment(0xDA, bytes([1, 1, 0x00, 0, 63, 0])),
    bytes([0x12, 0x34, 0xFF, 0x00, 0x56, 0xFF, 0xD0, 0x78]),
    b"\xff\xd9",
]))

# ── dbf ──────────────────────────────────────────────────────────────────────

def dbf(columns, rows):
    header = struct.pack("<BBBBIHH", 3, 120, 5, 17, len(rows), 32 + 32 * len(columns) + 1,
                         1 + sum(length for _, _, length in columns))
    header += bytes(20)
    for name, kind, length in columns:
        header += name.encode().ljust(11, b"\x00") + kind + struct.pack("<IBB", 0, length, 0) + bytes(14)
    header += b"\x0d"
    records = b"".join(b" " + b"".join(value.ljust(length) for value, (_, _, length) in zip(row, columns))
                       for row in rows)
    return header + records + b"\x1a"


write("database/dbf", "people", dbf(
    [("NAME", b"C", 8), ("AGE", b"N", 3)],
    [(b"Ada", b"36"), (b"Grace", b"85")],
).replace(b" Ada", b"*Ada", 1))

# ── mach_o ───────────────────────────────────────────────────────────────────

with tempfile.TemporaryDirectory() as work:
    source = os.path.join(work, "x.c")
    Path(source).write_text(
        "int counter = 3;\n"
        "static int helper(int a) { return a * 2; }\n"
        "int add(int a, int b) { return helper(a) + b + counter; }\n"
    )
    for arch, target in (("x86_64", "x86_64-apple-macos11"), ("arm64", "arm64-apple-macos11")):
        obj = os.path.join(work, f"{arch}.o")
        subprocess.run(["clang", "-target", target, "-c", "-O0", "-fno-asynchronous-unwind-tables",
                        "-o", obj, source], check=True)
        write("executable/mach_o", f"object_{arch}", Path(obj).read_bytes())

# ── java_class ───────────────────────────────────────────────────────────────

def utf8(text):
    data = text.encode()
    return b"\x01" + struct.pack(">H", len(data)) + data


def java_attribute(name_index, body):
    return struct.pack(">HI", name_index, len(body)) + body


# class Hello { int x; static final long BIG = 5L; public int get() { return x; } }
pool = [
    utf8("Hello"),                                    # 1
    b"\x07" + struct.pack(">H", 1),                   # 2 Class Hello
    utf8("java/lang/Object"),                         # 3
    b"\x07" + struct.pack(">H", 3),                   # 4 Class Object
    utf8("x"),                                        # 5
    utf8("I"),                                        # 6
    utf8("get"),                                      # 7
    utf8("()I"),                                      # 8
    utf8("Code"),                                     # 9
    utf8("LineNumberTable"),                          # 10
    utf8("BIG"),                                      # 11
    utf8("J"),                                        # 12
    b"\x05" + struct.pack(">q", 5),                   # 13 Long (takes two entries: 13, 14)
    utf8("ConstantValue"),                            # 15
    b"\x0c" + struct.pack(">HH", 5, 6),               # 16 NameAndType x:I
    b"\x09" + struct.pack(">HH", 2, 16),              # 17 Fieldref Hello.x
    utf8("SourceFile"),                               # 18
    utf8("Hello.java"),                               # 19
    utf8("hello"),                                    # 20
    b"\x08" + struct.pack(">H", 20),                  # 21 String
    b"\x04" + struct.pack(">f", 1.5),                 # 22 Float
]
code = bytes([0x2A, 0xB4, 0x00, 0x11, 0xAC])          # aload_0, getfield #17, ireturn
code_attribute = java_attribute(9, b"".join([
    struct.pack(">HHI", 1, 1, len(code)), code,
    struct.pack(">H", 0),                              # no exception table
    struct.pack(">H", 1),
    java_attribute(10, struct.pack(">HHH", 1, 0, 3)),
]))
write("executable/java_class", "hello", b"".join([
    b"\xca\xfe\xba\xbe", struct.pack(">HHH", 0, 52, len(pool) + 2),   # the long takes two entries
    *pool,
    struct.pack(">HHHH", 0x0021, 2, 4, 0),             # public super; this, super, no interfaces
    struct.pack(">H", 2),
    struct.pack(">HHHH", 0x0000, 5, 6, 0),
    struct.pack(">HHHH", 0x0018, 11, 12, 1) + java_attribute(15, struct.pack(">H", 13)),
    struct.pack(">H", 1),
    struct.pack(">HHHH", 0x0001, 7, 8, 1) + code_attribute,
    struct.pack(">H", 1),
    java_attribute(18, struct.pack(">H", 19)),
]))

# ── microsoft_pe ─────────────────────────────────────────────────────────────

def pe(plus):
    code = bytes([0x31, 0xC0, 0xC3]) + bytes(13)       # xor eax, eax; ret
    headers_size = 0x200
    optional = struct.pack("<HBBIIIII", 0x20B if plus else 0x10B, 14, 0, 0x200, 0, 0, 0x1000, 0x1000)
    if not plus:
        optional += struct.pack("<I", 0x2000)           # base_of_data
    optional += struct.pack("<Q" if plus else "<I", 0x140000000 if plus else 0x400000)
    optional += struct.pack("<IIHHHHHHIIIIHH", 0x1000, 0x200, 6, 0, 0, 0, 6, 0, 0, 0x2000, headers_size, 0, 3, 0x8160)
    optional += struct.pack("<QQQQ" if plus else "<IIII", 0x100000, 0x1000, 0x100000, 0x1000)
    optional += struct.pack("<II", 0, 16) + bytes(16 * 8)
    coff = struct.pack("<HHIIIHH", 0x8664 if plus else 0x14C, 1, 1589723130, 0, 0, len(optional),
                       0x0022 if plus else 0x0102)
    section = b".text".ljust(8, b"\x00") + struct.pack("<IIIIIIHHI", len(code), 0x1000, 0x200, headers_size,
                                                        0, 0, 0, 0, 0x60000020)
    mz = b"MZ" + bytes(0x3A) + struct.pack("<I", 0x40)
    header = mz + b"PE\x00\x00" + coff + optional + section
    return header.ljust(headers_size, b"\x00") + code.ljust(0x200, b"\x00")


write("executable/microsoft_pe", "pe32_plus", pe(True))
write("executable/microsoft_pe", "pe32", pe(False))
