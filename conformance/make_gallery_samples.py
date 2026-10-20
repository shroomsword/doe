#!/usr/bin/env python3
"""Writes the binary samples for the gallery definitions in formats/gallery/ to
samples/gallery/<dir>/<name>/.

The samples are small and made up; they are committed, so this script only has
to be run to change them. The ELF files are built with the C compiler of the
machine it runs on (`cc`), so running it again gives other bytes there.
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
