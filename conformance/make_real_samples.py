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


if __name__ == "__main__":
    make_cpio()
    make_ssh()
    make_pickle()
    make_der()
    make_iso()
