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


if __name__ == "__main__":
    put("sample.tar", tar())
    put("sample.a", ar())
    put("sample.pcapng", pcapng())
    put("sample.wasm", wasm())
    put("sample.npy", npy())
