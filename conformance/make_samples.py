#!/usr/bin/env python3
"""Writes the binary samples of the conformance suite to samples/<format>/.

The samples are small and made up for the formats in formats/; they are
committed, so this script only has to be run to change them.
"""

import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parent / "samples"


def write(format_name, sample, data):
    directory = ROOT / format_name
    directory.mkdir(parents=True, exist_ok=True)
    (directory / f"{sample}.bin").write_bytes(bytes(data))


def ipv4(ihl, dscp, ecn, ident, flags, offset, ttl, proto, checksum, src, dst, options, payload):
    total = ihl * 4 + len(payload)
    head = bytes([(4 << 4) | ihl, (dscp << 2) | ecn])
    head += struct.pack(">HH", total, ident)
    reserved, df, mf = flags
    head += struct.pack(">H", (reserved << 15) | (df << 14) | (mf << 13) | offset)
    head += bytes([ttl, proto]) + struct.pack(">H", checksum)
    return head + bytes(src) + bytes(dst) + bytes(options) + bytes(payload)


write("ipv4_packet", "udp", ipv4(5, 0, 0, 0x1234, (0, 1, 0), 0, 64, 17, 0xBEEF,
                                  (192, 168, 1, 1), (10, 0, 0, 2), b"", b"8 bytes!"))
write("ipv4_packet", "tcp_fragment_with_options", ipv4(6, 46, 1, 0xABCD, (0, 0, 1), 185, 1, 6, 0x0102,
                                                         (127, 0, 0, 1), (255, 255, 255, 255),
                                                         b"\x01\x01\x01\x00", b"tail"))

def tlv(records):
    out = b"TLV1" + bytes([len(records)])
    for kind, body in records:
        out += bytes([kind, len(body)]) + body
    return out


write("tlv_stream", "mixed", tlv([
    (1, b"hello"),
    (2, struct.pack("<I", 1234)),
    (3, bytes([3, 4])),
    (1, b""),
]))
write("tlv_stream", "single_number", tlv([(2, struct.pack("<I", 1_000_000))]))
write("tlv_stream", "empty", tlv([]))


def bmp(width, height, bits, pixels):
    dib = struct.pack("<IiiHHIIiiII", 40, width, height, 1, bits, 0, len(pixels), 2835, 2835, 0, 0)
    offset = 14 + len(dib)
    return b"BM" + struct.pack("<I", offset + len(pixels)) + bytes(4) + struct.pack("<I", offset) + dib + pixels


write("bmp_header", "bottom_up_24bit", bmp(3, 2, 24, bytes(range(24))))
write("bmp_header", "top_down_8bit", bmp(5, -2, 8, bytes(range(100, 116))))


def sub_blocks(*blocks):
    out = b""
    for block in blocks:
        out += bytes([len(block)]) + block
    return out + b"\x00"


write("gif_like", "extension_only",
      b"GIF89a" + struct.pack("<HH", 1, 1) + bytes([0x00, 0, 0])
      + b"\x21\xf9" + sub_blocks(b"\x04\x00\x00\x00") + b"\x3b")
write("gif_like", "with_tables",
      b"GIF87a" + struct.pack("<HH", 2, 2) + bytes([0xF1, 0, 0])
      + bytes(range(1, 13))
      + b"\x2c" + struct.pack("<HHHH", 0, 0, 2, 2) + bytes([0x80]) + bytes([9, 8, 7, 6, 5, 4])
      + bytes([2]) + sub_blocks(b"\x01\x02\x03")
      + b"\x3b")


def chunk(chunk_id, body):
    out = chunk_id + struct.pack("<I", len(body)) + body
    return out + (b"\x00" if len(body) % 2 else b"")


def riff(chunks):
    body = b"WAVE" + b"".join(chunks)
    return b"RIFF" + struct.pack("<I", len(body)) + body


fmt = struct.pack("<HHIIHH", 1, 2, 44100, 176400, 4, 16)
write("riff_wave", "pcm", riff([chunk(b"fmt ", fmt), chunk(b"data", bytes(range(5)))]))
info = b"INFO" + chunk(b"ISFT", b"abc") + chunk(b"ICMT", b"hi")
write("riff_wave", "list_info", riff([chunk(b"fmt ", fmt), chunk(b"LIST", info)]))


def png_chunk(kind, body):
    return struct.pack(">I", len(body)) + kind + body + b"\xde\xad\xbe\xef"


PNG_SIGNATURE = bytes([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
write("png_chunks", "minimal",
      PNG_SIGNATURE
      + png_chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
      + png_chunk(b"IDAT", b"\x08\x1d\x01")
      + png_chunk(b"IEND", b""))
write("png_chunks", "palette_and_text",
      PNG_SIGNATURE
      + png_chunk(b"IHDR", struct.pack(">IIBBBBB", 2, 2, 8, 3, 0, 0, 0))
      + png_chunk(b"PLTE", bytes([255, 0, 0, 0, 255, 0]))
      + png_chunk(b"tEXt", b"Comment\x00hello")
      + png_chunk(b"IEND", b""))
