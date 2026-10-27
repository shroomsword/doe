#!/usr/bin/env python3
"""Writes sample files for the gallery definitions in samples/gallery/<dir>/<name>/
with the tools that make the formats in the world, so that each file is what
that tool wrote and not something made to fit doe. The rows of
gallery_samples.tsv with the status `real` are these (and `make_gallery_samples.py`
for the first four).

    python3 conformance/make_real_samples.py

Needs `cpio`, `ssh-keygen`, `openssl`, `genisoimage` and Python 3. The files are
committed, so this only has to be run to change them; running it again gives
other bytes for what has a key, a date or a random number in it. Each file is
at most 64 KiB, and made from nothing that has a licence: the contents are the
words in this script.
"""

import base64
import os
import pickle
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent / "samples" / "gallery"
LIMIT = 64 * 1024


def put(path, sample, data):
    assert len(data) <= LIMIT, f"{path}/{sample} is {len(data)} bytes"
    directory = ROOT / path
    directory.mkdir(parents=True, exist_ok=True)
    (directory / f"{sample}.bin").write_bytes(bytes(data))


def run(command, **kwargs):
    return subprocess.run(command, check=True, capture_output=True, **kwargs).stdout


# ── cpio ─────────────────────────────────────────────────────────────────────
# `cpio -H bin` is the old binary format, in the byte order of the machine.
def make_cpio():
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / "hello.txt").write_text("Hello, cpio.\n")
        (root / "odd.txt").write_text("odd\n")  # an odd length: the entry is padded
        (root / "dir").mkdir()
        (root / "dir" / "nested.txt").write_text("nested\n")
        names = b"hello.txt\nodd.txt\ndir\ndir/nested.txt\n"
        for sample, extra in [("bin", ["-H", "bin"])]:
            data = run(["cpio", "-o", "--quiet", *extra], input=names, cwd=root)
            put("archive/cpio_old_le", sample, data)


# ── ssh public keys ──────────────────────────────────────────────────────────
def make_ssh():
    with tempfile.TemporaryDirectory() as tmp:
        for sample, args in [
            ("ed25519", ["-t", "ed25519"]),
            ("rsa2048", ["-t", "rsa", "-b", "2048"]),
            ("ecdsa256", ["-t", "ecdsa", "-b", "256"]),
        ]:
            key = Path(tmp) / sample
            run(["ssh-keygen", "-q", "-N", "", "-C", "doe-sample", "-f", str(key), *args])
            # the file ssh-keygen writes is "type base64 comment"; the format of the
            # gallery is the key as that base64 holds it
            line = (Path(tmp) / f"{sample}.pub").read_text().split()
            put("security/ssh_public_key", sample, base64.b64decode(line[1]))


# ── python pickle ────────────────────────────────────────────────────────────
def make_pickle():
    value = {
        "name": "doe",
        "numbers": [1, 2, 3, 70000, -5, 2**40],
        "pair": (1.5, None),
        "flags": {True, False},
        "text": "café",
        "blob": b"\x00\x01\x02",
    }
    for protocol in (0, 2, 4):
        # protocol 0 writes a string as Latin-1, which the definition (ascii) and
        # doe, that does not accept a byte that is not ASCII, do not read
        shown = {**value, "text": "cafe"} if protocol == 0 else value
        put("serialization/python_pickle", f"protocol{protocol}", pickle.dumps(shown, protocol=protocol))


# ── ASN.1 DER ────────────────────────────────────────────────────────────────
# A certificate and a request, as OpenSSL writes them in DER.
def make_der():
    with tempfile.TemporaryDirectory() as tmp:
        key = Path(tmp) / "key.pem"
        run(["openssl", "ecparam", "-name", "prime256v1", "-genkey", "-noout", "-out", str(key)])
        subject = ["-subj", "/C=SE/O=doe/CN=sample"]
        certificate = run(
            ["openssl", "req", "-x509", "-new", "-key", str(key), "-days", "3650", "-outform", "DER", *subject]
        )
        put("serialization/asn1/asn1_der", "certificate", certificate)
        request = run(["openssl", "req", "-new", "-key", str(key), "-outform", "DER", *subject])
        put("serialization/asn1/asn1_der", "request", request)


# ── ISO 9660 ─────────────────────────────────────────────────────────────────
def make_iso():
    with tempfile.TemporaryDirectory() as tmp:
        source = Path(tmp) / "tree"
        (source / "docs").mkdir(parents=True)
        (source / "README.TXT").write_text("Hello, ISO 9660.\n")
        (source / "docs" / "A.TXT").write_text("a\n")
        image = Path(tmp) / "out.iso"
        run(
            [
                "genisoimage", "-quiet", "-no-pad", "-V", "DOE", "-publisher", "doe",
                "-o", str(image), str(source),
            ],
            env={**os.environ, "SOURCE_DATE_EPOCH": "1700000000"},
        )
        put("filesystem/iso9660", "small", image.read_bytes())


# ── GRUB font ────────────────────────────────────────────────────────────────
# grub-mkfont makes the PFF2 file from a TrueType font: ten glyphs (A to J) of DejaVu Sans Mono at 8 points. The
# font has the Bitstream Vera licence, which lets a derived font be shared under another name ("doe sample").
def make_grub_font():
    font = Path("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf")
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / "font.pf2"
        run(["grub-mkfont", "-s", "8", "-r", "0x41-0x4a", "--name", "doe sample", "-o", str(out), str(font)])
        put("font/grub2_font", "glyphs_a_to_j", out.read_bytes())


# ── utmp ─────────────────────────────────────────────────────────────────────
# glibc writes the file: utmpxname and pututxline of the C library, called through ctypes.
def make_utmp():
    import ctypes
    import ctypes.util

    class Timeval(ctypes.Structure):
        _fields_ = [("tv_sec", ctypes.c_int32), ("tv_usec", ctypes.c_int32)]

    class ExitStatus(ctypes.Structure):
        _fields_ = [("e_termination", ctypes.c_short), ("e_exit", ctypes.c_short)]

    class Utmpx(ctypes.Structure):
        _fields_ = [
            ("ut_type", ctypes.c_short), ("ut_pid", ctypes.c_int), ("ut_line", ctypes.c_char * 32),
            ("ut_id", ctypes.c_char * 4), ("ut_user", ctypes.c_char * 32), ("ut_host", ctypes.c_char * 256),
            ("ut_exit", ExitStatus), ("ut_session", ctypes.c_int32), ("ut_tv", Timeval),
            ("ut_addr_v6", ctypes.c_int32 * 4), ("unused", ctypes.c_char * 20),
        ]

    libc = ctypes.CDLL(ctypes.util.find_library("c"), use_errno=True)
    assert ctypes.sizeof(Utmpx) == 384, ctypes.sizeof(Utmpx)
    libc.pututxline.argtypes = [ctypes.POINTER(Utmpx)]
    libc.utmpxname.argtypes = [ctypes.c_char_p]
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "utmp"
        path.write_bytes(b"")
        assert libc.utmpxname(str(path).encode()) == 0
        libc.setutxent()
        records = [
            (2, 0, b"~", b"~~", b"reboot", b"6.1.0-doe", 1700000000),
            (1, 0, b"~", b"~~", b"runlevel", b"6.1.0-doe", 1700000010),
            (7, 4242, b"pts/0", b"s/0", b"doe", b"192.0.2.10", 1700000100),
            (8, 4242, b"pts/0", b"s/0", b"", b"", 1700000200),
        ]
        for kind, pid, line, ident, user, host, when in records:
            entry = Utmpx(ut_type=kind, ut_pid=pid, ut_line=line, ut_id=ident, ut_user=user, ut_host=host)
            entry.ut_tv.tv_sec = when
            assert libc.pututxline(ctypes.byref(entry))
        libc.endutxent()
        put("log/glibc_utmp", "four_records", path.read_bytes())


# ── disk images ──────────────────────────────────────────────────────────────
# qemu-img writes an empty virtual disk of 1 MiB (the VMDK it writes has a grain of 64 KiB, too big for a sample).
def make_disk_images():
    with tempfile.TemporaryDirectory() as tmp:
        for kind, path in [("vdi", "filesystem/vdi")]:
            image = Path(tmp) / f"disk.{kind}"
            run(["qemu-img", "create", "-q", "-f", kind, str(image), "1M"])
            put(path, "empty_1m", image.read_bytes())


if __name__ == "__main__":
    make_disk_images()
    make_utmp()
    make_cpio()
    make_ssh()
    make_pickle()
    make_der()
    make_iso()
