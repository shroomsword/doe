#!/usr/bin/env python3
"""Writes sample files for the gallery definitions in samples/gallery/<dir>/<name>/
with the command line tools that write the formats in the world (msgfmt, dtc,
mkimage, cc65, xar, zck, sfdisk, mkfs.vfat, cryptsetup, gpg, ffmpeg, ...), so
that each file is what that tool wrote and not something built to fit doe.
`make_real_samples.py` does the same for the first formats, and
`make_real_samples_libs.py` with Python libraries. The plan for the rest is
real_files_plan.tsv.

    python3 conformance/make_real_samples_tools.py [FORMAT...]

Needs the tools on the PATH (a Debian or Ubuntu has them as packages: gettext,
device-tree-compiler, u-boot-tools, cc65, zmakebas, xar, zchunk,
android-sdk-libsparse-utils, mkbootimg, fdisk, parted, dosfstools, mtools,
cryptsetup-bin, efitools, openssl, gnupg, msitools, xfonts-base,
binutils-mingw-w64-x86-64, php-cli, zisofs-tools, e2fsprogs) and, in the
Python of the script, `mar` and `imageio-ffmpeg` (or `ffmpeg` on the PATH),
`dbf`, `pyshp`, `websockets` and `scapy`. A format whose tool is missing is
skipped, with a message. The files are committed, so this only has to be run to
change them. Each file is at most 64 KiB and has nothing in it that has a
licence: the contents are the words in this script.
"""

import gzip
import hashlib
import os
import shutil
import ssl
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent / "samples" / "gallery"
# real files that the gallery's definition cannot read but a type of the library does
WITHOUT_REFERENCE = Path(__file__).resolve().parent / "samples_without_reference"
LIMIT = 64 * 1024
made = []
skipped = []


def put(path, sample, data, root=None):
    data = bytes(data)
    assert 0 < len(data) <= LIMIT, f"{path}/{sample} is {len(data)} bytes"
    directory = (root or ROOT) / path
    directory.mkdir(parents=True, exist_ok=True)
    (directory / f"{sample}.bin").write_bytes(data)
    if path not in made:
        made.append(path)


def run(command, **kwargs):
    kwargs.setdefault("capture_output", True)
    # the tools that put the date in the file put this one (running the script again gives the same bytes for them)
    kwargs["env"] = {"SOURCE_DATE_EPOCH": "1700000000", **(kwargs.get("env") or os.environ)}
    return subprocess.run(command, check=True, **kwargs).stdout


def need(tool):
    if shutil.which(tool) is None:
        raise Missing(tool)


class Missing(Exception):
    pass


TEXT = ("decoder of everything reads binary files described by schemas. " * 40).encode()
BYTES = bytes((i * 37 + i // 7) % 256 for i in range(3000))


# ── gettext ──────────────────────────────────────────────────────────────────
def make_gettext():
    need("msgfmt")
    po = (
        'msgid ""\nmsgstr ""\n"Project-Id-Version: doe 1\\n"\n"Language: sv\\n"\n"Content-Type: text/plain; charset=UTF-8\\n"\n'
        '"Plural-Forms: nplurals=2; plural=(n != 1);\\n"\n\n'
        'msgid "decoder"\nmsgstr "avkodare"\n\nmsgid "everything"\nmsgstr "allt"\n\n'
        'msgctxt "menu"\nmsgid "open"\nmsgstr "öppna"\n\n'
        'msgid "one file"\nmsgid_plural "%d files"\nmsgstr[0] "en fil"\nmsgstr[1] "%d filer"\n'
    )
    with tempfile.TemporaryDirectory() as tmp:
        (Path(tmp) / "x.po").write_text(po, encoding="utf-8")
        run(["msgfmt", "-o", str(Path(tmp) / "x.mo"), str(Path(tmp) / "x.po")])
        put("database/gettext_mo", "msgfmt_plural", (Path(tmp) / "x.mo").read_bytes())


# ── device tree ──────────────────────────────────────────────────────────────
def make_dtb():
    need("dtc")
    dts = """/dts-v1/;
/ {
    compatible = "doe,sample";
    #address-cells = <1>;
    #size-cells = <1>;
    model = "Doe sample board";
    chosen { bootargs = "console=ttyS0,115200"; };
    cpus { #address-cells = <1>; #size-cells = <0>;
        cpu@0 { device_type = "cpu"; reg = <0>; compatible = "arm,cortex-a9"; };
        cpu@1 { device_type = "cpu"; reg = <1>; compatible = "arm,cortex-a9"; }; };
    memory@80000000 { device_type = "memory"; reg = <0x80000000 0x10000000>; };
    uart0: serial@1000 { compatible = "ns16550a"; reg = <0x1000 0x100>; interrupts = <5 1>; status = "okay"; };
    flags { enabled; table = <1 2 3 0xdeadbeef>; bytes = [00 11 22 33]; names = "a", "b", "c"; link = <&uart0>; };
};
"""
    with tempfile.TemporaryDirectory() as tmp:
        (Path(tmp) / "x.dts").write_text(dts)
        run(["dtc", "-I", "dts", "-O", "dtb", "-o", str(Path(tmp) / "x.dtb"), str(Path(tmp) / "x.dts")])
        put("hardware/dtb", "dtc_board", (Path(tmp) / "x.dtb").read_bytes())
        run(["dtc", "-I", "dts", "-O", "dtb", "-@", "-o", str(Path(tmp) / "y.dtb"), str(Path(tmp) / "x.dts")])
        put("hardware/dtb", "dtc_board_symbols", (Path(tmp) / "y.dtb").read_bytes())


# ── U-Boot ───────────────────────────────────────────────────────────────────
def make_uimage():
    need("mkimage")
    with tempfile.TemporaryDirectory() as tmp:
        data = Path(tmp) / "data"
        data.write_bytes(TEXT[:1500])
        for name, args in [
            ("arm_linux_kernel", ["-A", "arm", "-O", "linux", "-T", "kernel", "-C", "none", "-a", "0x80008000", "-e", "0x80008000"]),
            ("mips_ramdisk", ["-A", "mips", "-O", "linux", "-T", "ramdisk", "-C", "none", "-a", "0", "-e", "0"]),
        ]:
            image = Path(tmp) / f"{name}.uimg"
            run(["mkimage", *args, "-n", "doe sample", "-d", str(data), str(image)])
            put("firmware/uimage", f"mkimage_{name}", image.read_bytes())


# ── cc65: NES, 6502 ──────────────────────────────────────────────────────────
ASM_6502 = """
.export _main
@HEADER@
.segment "CODE"
_main:
    ldx #$00
loop:
    lda table,x
    clc
    adc #$01
    sta $0200,x
    inx
    cpx #$08
    bne loop
    lda #$80
    bit $2002
    bpl done
    jsr sub
    jmp (vector)
sub:
    pha
    php
    asl a
    rol a
    lsr a
    ror a
    plp
    pla
    rts
done:
    brk
    nop
    rti
vector: .word _main
table:  .byte 1,2,3,4,5,6,7,8
"""


HEADER_NES = '.segment "HEADER"\n.byte "NES", $1A, 2, 1, 0, 0\n'


def make_cc65():
    need("cl65")
    with tempfile.TemporaryDirectory() as tmp:
        source = Path(tmp) / "x.s"
        source.write_text(ASM_6502.replace("@HEADER@", HEADER_NES))
        out = Path(tmp) / "x.nes"
        run(["cl65", "-t", "nes", "-o", str(out), str(source)])
        put("firmware/ines", "cl65_nrom", out.read_bytes())
        raw = Path(tmp) / "x.bin"
        source.write_text(ASM_6502.replace("@HEADER@", ""))
        run(["cl65", "-t", "none", "-o", str(raw), str(source)])
        put("machine_code/code_6502", "cl65_program", raw.read_bytes())


# ── ZX Spectrum ──────────────────────────────────────────────────────────────
def make_tap():
    need("zmakebas")
    basic = '10 REM doe sample\n20 LET a=1\n30 FOR i=1 TO 10\n40 PRINT i;" ";a*i\n50 NEXT i\n60 PAUSE 0\n'
    with tempfile.TemporaryDirectory() as tmp:
        (Path(tmp) / "x.bas").write_text(basic)
        run(["zmakebas", "-n", "doe", "-a", "10", "-o", str(Path(tmp) / "x.tap"), str(Path(tmp) / "x.bas")])
        put("filesystem/zx_spectrum_tap", "zmakebas_program", (Path(tmp) / "x.tap").read_bytes())


# ── archives ─────────────────────────────────────────────────────────────────
def make_xar():
    need("xar")
    with tempfile.TemporaryDirectory() as tmp:
        tree = Path(tmp) / "tree"
        (tree / "dir").mkdir(parents=True)
        (tree / "a.txt").write_bytes(TEXT[:300])
        (tree / "dir" / "b.bin").write_bytes(BYTES[:500])
        for name, args in [("gzip", []), ("none", ["--compression=none"])]:
            out = Path(tmp) / f"{name}.xar"
            run(["xar", "-c", "-f", str(out), *args, "a.txt", "dir"], cwd=tree)
            put("archive/xar", f"xar_{name}", out.read_bytes())


def make_zchunk():
    need("zck")
    with tempfile.TemporaryDirectory() as tmp:
        data = Path(tmp) / "data"
        data.write_bytes(TEXT[:2000] + BYTES + TEXT[:1500])
        run(["zck", "-o", str(Path(tmp) / "x.zck"), str(data)], cwd=tmp)
        put("archive/zchunk", "zck_small", (Path(tmp) / "x.zck").read_bytes())


def make_zisofs():
    need("mkzftree")
    with tempfile.TemporaryDirectory() as tmp:
        source = Path(tmp) / "in"
        source.mkdir()
        (source / "data").write_bytes(TEXT)
        out = Path(tmp) / "out"
        run(["mkzftree", "-z", "9", str(source), str(out)])
        put("archive/zisofs", "mkzftree_text", (out / "data").read_bytes())


def make_mar():
    mar = Path(sys.executable).with_name("mar")
    if not mar.exists():
        raise Missing("mar (pip install mar)")
    with tempfile.TemporaryDirectory() as tmp:
        (Path(tmp) / "a.txt").write_bytes(TEXT[:400])
        (Path(tmp) / "b.bin").write_bytes(BYTES[:700])
        out = Path(tmp) / "x.mar"
        run([str(mar), "-c", str(out), "a.txt", "b.bin"], cwd=tmp)
        put("archive/mozilla_mar", "mar_two_files", out.read_bytes(), WITHOUT_REFERENCE)


def make_android():
    with tempfile.TemporaryDirectory() as tmp:
        kernel = Path(tmp) / "kernel"
        kernel.write_bytes(BYTES[:2000])
        ramdisk = Path(tmp) / "ramdisk"
        ramdisk.write_bytes(TEXT[:1000])
        if shutil.which("mkbootimg"):
            dtb = Path(tmp) / "x.dtb"
            (Path(tmp) / "x.dts").write_text('/dts-v1/;\n/ { model = "doe"; };\n')
            run(["dtc", "-I", "dts", "-O", "dtb", "-o", str(dtb), str(Path(tmp) / "x.dts")])
            for version in (0, 1, 2):
                out = Path(tmp) / f"boot{version}.img"
                extra = ["--dtb", str(dtb)] if version == 2 else []
                run(["mkbootimg", "--kernel", str(kernel), "--ramdisk", str(ramdisk), "--cmdline", "console=ttyS0", "--header_version", str(version), *extra, "-o", str(out)])
                put("archive/android_img", f"mkbootimg_v{version}", out.read_bytes())
        else:
            skipped.append("mkbootimg")
        if shutil.which("img2simg") and shutil.which("mke2fs"):
            tree = Path(tmp) / "tree"
            tree.mkdir()
            (tree / "hello.txt").write_text("hello\n")
            raw = Path(tmp) / "ext4.img"
            run(["mke2fs", "-q", "-t", "ext4", "-d", str(tree), str(raw), "1M"])
            out = Path(tmp) / "x.simg"
            run(["img2simg", str(raw), str(out)])
            put("archive/android_sparse", "img2simg_ext4", out.read_bytes())
        else:
            skipped.append("img2simg")


# ── disks ────────────────────────────────────────────────────────────────────
def make_partition_tables():
    need("sfdisk")
    with tempfile.TemporaryDirectory() as tmp:
        mbr = Path(tmp) / "mbr.img"
        mbr.write_bytes(bytes(64 * 1024))
        run(["sfdisk", "--label", "dos", str(mbr)], input=b"label: dos\nlabel-id: 0xd0e0d0e0\nunit: sectors\nstart=8, size=40, type=83\nstart=48, size=40, type=82\nstart=88, size=32, type=b, bootable\n")
        put("filesystem/mbr_partition_table", "sfdisk_three", mbr.read_bytes()[:512])
        gpt = Path(tmp) / "gpt.img"
        gpt.write_bytes(bytes(64 * 1024))
        run(
            ["sfdisk", "--label", "gpt", str(gpt)],
            input=b"label: gpt\nlabel-id: 11111111-2222-3333-4444-555555555555\nfirst-lba: 40\nstart=40, size=40, name=data\nstart=80, size=14, name=boot, type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B\n",
        )
        put("filesystem/gpt_partition_table", "sfdisk_two", gpt.read_bytes())
        if os.environ.get("DOE_APM") and shutil.which("parted"):
            apm = Path(tmp) / "apm.img"
            apm.write_bytes(bytes(1024 * 1024))
            run(["parted", "-s", str(apm), "mklabel", "mac", "mkpart", "primary", "hfs", "64s", "100s"])
            put("filesystem/apm_partition_table", "parted_one", apm.read_bytes()[:65536])  # the map is at the start
        else:
            skipped.append("parted")


def make_vfat():
    need("mkfs.vfat")
    need("mcopy")
    with tempfile.TemporaryDirectory() as tmp:
        image = Path(tmp) / "x.img"
        run(["mkfs.vfat", "-C", "-F", "12", "-n", "DOE", "-s", "1", "-r", "16", "-f", "1", str(image), "48"])
        (Path(tmp) / "hello.txt").write_text("Hello, FAT.\n")
        (Path(tmp) / "DATA.TXT").write_text("short names\n")
        run(["mcopy", "-i", str(image), str(Path(tmp) / "hello.txt"), str(Path(tmp) / "DATA.TXT"), "::"])
        put("filesystem/vfat", "mkfs_fat12", image.read_bytes())


def make_luks():
    need("cryptsetup")
    with tempfile.TemporaryDirectory() as tmp:
        key = Path(tmp) / "key"
        key.write_bytes(b"doe sample passphrase")
        for kind, keep in [("luks1", 4096), ("luks2", 16384)]:
            image = Path(tmp) / f"{kind}.img"
            image.write_bytes(bytes(20 * 1024 * 1024))
            run(["cryptsetup", "luksFormat", "-q", "--type", kind, "--pbkdf", "pbkdf2", "--pbkdf-force-iterations", "1000", "--key-file", str(key), str(image)])
            put(f"filesystem/luks", f"cryptsetup_{kind}", image.read_bytes()[:keep])
            image.unlink()


# ── fonts, security ──────────────────────────────────────────────────────────
def make_pcf():
    fonts = Path(os.environ.get("PCF_DIR", "/usr/share/fonts/X11/misc"))
    candidates = sorted(fonts.glob("*.pcf.gz")) if fonts.is_dir() else []
    if not candidates:
        raise Missing("xfonts-base (the .pcf.gz fonts of /usr/share/fonts/X11/misc)")
    # the fonts of X.Org's misc-fixed are in the public domain; the smallest ones fit a sample
    for path in candidates:
        data = gzip.decompress(path.read_bytes())
        if len(data) <= LIMIT and path.name.startswith(("5x7", "6x10", "6x12", "6x13.", "7x13.")):
            put("font/pcf_font", f"xfonts_{path.name.split('.')[0].lower()}", data)


def make_efi():
    need("cert-to-efi-sig-list")
    with tempfile.TemporaryDirectory() as tmp:
        key = Path(tmp) / "key.pem"
        cert = Path(tmp) / "cert.pem"
        run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", str(key), "-out", str(cert), "-days", "3650", "-subj", "/CN=doe sample/"])
        out = Path(tmp) / "x.esl"
        run(["cert-to-efi-sig-list", "-g", "11111111-2222-3333-4444-555555555555", str(cert), str(out)])
        put("security/efivar_signature_list", "efitools_x509", out.read_bytes(), WITHOUT_REFERENCE)


def make_cpio():
    need("cpio")
    with tempfile.TemporaryDirectory() as tmp:
        tree = Path(tmp) / "tree"
        (tree / "dir").mkdir(parents=True)
        (tree / "dir" / "a.txt").write_bytes(b"hello\n")
        (tree / "b").write_bytes(b"odd")
        os.symlink("a.txt", tree / "dir" / "link")
        for path in (tree / "dir" / "a.txt", tree / "b", tree / "dir", tree):
            os.utime(path, (1700000000, 1700000000))
        names = "\n".join(sorted(str(p.relative_to(tree)) for p in tree.rglob("*"))).encode() + b"\n"
        for sample, fmt in (("gnu_newc", "newc"), ("gnu_crc", "crc"), ("gnu_odc", "odc"), ("gnu_binary", "bin")):
            out = run(["cpio", "-o", "-H", fmt, "--quiet", "--reproducible", "-R", "0:0"], input=names, cwd=tree)
            put("archive/cpio", sample, out, WITHOUT_REFERENCE)


def make_lzma_alone():
    need("xz")
    for sample, preset in (("xz_preset6", "-6"), ("xz_preset0", "-0")):
        out = run(["xz", "--format=lzma", "-c", preset], input=TEXT)
        put("archive/lzma_alone", sample, out, WITHOUT_REFERENCE)


def make_lzip():
    need("lzip")
    # lzip shrinks the dictionary to the size of the data, so the input is 126 kB of the text (a member of about 180
    # bytes): the dictionary of 64 KiB, one of 100000 bytes (coded as 2^17 less three sixteenths of an eighth of it),
    # and two members joined as `cat` does (the second of other data and the smallest dictionary)
    big = TEXT * 50
    put("archive/lzip", "lzip_dict64k", run(["lzip", "-c", "-s", "64KiB"], input=big), WITHOUT_REFERENCE)
    put("archive/lzip", "lzip_dict100000", run(["lzip", "-c", "-s", "100000"], input=big), WITHOUT_REFERENCE)
    both = run(["lzip", "-c", "-9"], input=big) + run(["lzip", "-c", "-0"], input=BYTES)
    put("archive/lzip", "lzip_two_members", both, WITHOUT_REFERENCE)


def make_zlib_stream():
    need("git")
    # (each level gets other content: git does not write an object that it has) a loose object of a git repository is a zlib stream ("blob SIZE", a NUL and the content), whatever the level
    with tempfile.TemporaryDirectory() as tmp:
        run(["git", "init", "-q", tmp])
        for sample, level in (("git_blob", "-1"), ("git_blob_level1", "1"), ("git_blob_level9", "9")):
            oid = run(["git", "-C", tmp, "-c", f"core.looseCompression={level}", "hash-object", "-w", "--stdin"], input=level.encode() + TEXT).decode().strip()
            put("archive/zlib_stream", sample, (Path(tmp) / ".git" / "objects" / oid[:2] / oid[2:]).read_bytes(), WITHOUT_REFERENCE)


def make_snappy():
    try:
        import cramjam
    except ImportError:
        raise Missing("cramjam (pip install cramjam: the Rust snap crate)") from None
    # a chunk holds at most 64 KiB: the text fills the first, and the bytes of a hash chain, which do not compress, are
    # most of the second, which is then written as an uncompressed chunk
    noise = b"".join(hashlib.sha256(bytes([i])).digest() for i in range(100))[:3000]
    framed = bytes(cramjam.snappy.compress(TEXT * 26 + noise))
    put("archive/snappy", "cramjam_framed", framed, WITHOUT_REFERENCE)
    put("archive/snappy", "cramjam_framed_small", bytes(cramjam.snappy.compress(TEXT)), WITHOUT_REFERENCE)
    put("archive/snappy", "cramjam_raw_block", bytes(cramjam.snappy.compress_raw(TEXT)), WITHOUT_REFERENCE)


def make_gpg():
    need("gpg")
    with tempfile.TemporaryDirectory() as tmp:
        env = {**os.environ, "GNUPGHOME": tmp}
        agent = shutil.which("gpg-agent")
        if agent:
            Path(tmp, "gpg.conf").write_text("")
            Path(tmp, "gpg-agent.conf").write_text("allow-loopback-pinentry\n")
            Path(tmp, "gpg.conf").write_text(f"agent-program {agent}\n")
        os.chmod(tmp, 0o700)
        run(["gpg", "--batch", "--pinentry-mode", "loopback", "--passphrase", "", "--quick-gen-key", "Doe Sample <doe@example.org>", "rsa2048", "sign", "never"], env=env)
        put("security/openpgp_message", "gpg_public_key", run(["gpg", "--export"], env=env))
        run(["gpg", "--batch", "--pinentry-mode", "loopback", "--passphrase", "", "-o", str(Path(tmp) / "x.sig"), "--sign", "--compress-algo", "none", str(Path(tmp) / "x.sig")], env=env) if False else None
        (Path(tmp) / "msg").write_bytes(b"signed by doe\n")
        run(["gpg", "--batch", "--pinentry-mode", "loopback", "--passphrase", "", "--compress-algo", "none", "-o", str(Path(tmp) / "msg.gpg"), "--sign", str(Path(tmp) / "msg")], env=env)
        put("security/openpgp_message", "gpg_signed_message", (Path(tmp) / "msg.gpg").read_bytes())


def make_msi():
    need("msibuild")
    table = "Property\tValue\ns72\tl0\nProperty\tProperty\nProductName\tDoe sample\nManufacturer\tdoe\nProductVersion\t1.0.0\n"
    with tempfile.TemporaryDirectory() as tmp:
        (Path(tmp) / "Property.idt").write_text(table)
        out = Path(tmp) / "x.msi"
        run(["msibuild", str(out), "-i", str(Path(tmp) / "Property.idt")], cwd=tmp)
        put("serialization/microsoft_cfb", "msibuild_property", out.read_bytes())


def make_windres():
    exe = shutil.which("x86_64-w64-mingw32-windres") or shutil.which("windres")
    if not exe:
        raise Missing("windres")
    rc = '#include <windows.h>\nSTRINGTABLE { 1, "doe" 2, "sample" }\n1 VERSIONINFO\nFILEVERSION 1,0,0,0\nBEGIN\n BLOCK "StringFileInfo"\n BEGIN\n  BLOCK "040904B0"\n  BEGIN\n   VALUE "FileDescription", "doe sample\\0"\n  END\n END\n BLOCK "VarFileInfo"\n BEGIN\n  VALUE "Translation", 0x409, 1200\n END\nEND\n'
    with tempfile.TemporaryDirectory() as tmp:
        (Path(tmp) / "x.rc").write_text(rc.replace("#include <windows.h>\n", "").replace("STRINGTABLE {", "STRINGTABLE\nBEGIN").replace('"sample" }', '"sample"\nEND'))
        out = Path(tmp) / "x.res"
        run([exe, "-O", "res", "-i", str(Path(tmp) / "x.rc"), "-o", str(out)])
        put("windows/windows_resource_file", "windres_strings_version", out.read_bytes())


# ── serialization ────────────────────────────────────────────────────────────
def make_php():
    exe = shutil.which("php") or shutil.which("php8.4") or shutil.which("php8.3")
    if not exe:
        raise Missing("php")
    code = 'echo serialize(["name"=>"doe","numbers"=>[1,2,3,70000,-5],"pi"=>3.25,"ok"=>true,"none"=>null,"nested"=>["a"=>["b"=>"c"]]]);'
    put("serialization/php_serialized_value", "php_array", run([exe, "-r", code]))
    code = 'class Sample { public $a = 1; protected $b = "two"; private $c = [3]; } echo serialize(new Sample);'
    put("serialization/php_serialized_value", "php_object", run([exe, "-r", code]))


def make_dbf_shapefile():
    import dbf
    import shapefile

    with tempfile.TemporaryDirectory() as tmp:
        table = dbf.Table(str(Path(tmp) / "x.dbf"), "name C(12); size N(8,2); count N(5,0); seen D; ok L", dbf_type="db3")
        table.open(dbf.READ_WRITE)
        import datetime

        for index, name in enumerate(["alpha", "beta", "gamma"]):
            table.append((name, 1.5 * (index + 1), index * 100, datetime.date(2020, 5, 17 + index), index % 2 == 0))
        table.close()
        put("database/dbf", "dbf_library_three_rows", (Path(tmp) / "x.dbf").read_bytes())
        base = str(Path(tmp) / "x")
        with shapefile.Writer(base, shapeType=shapefile.POINT) as writer:
            writer.field("NAME", "C", 10)
            for index, (x, y) in enumerate([(1.0, 2.0), (3.5, -4.25), (10.0, 20.0)]):
                writer.point(x, y)
                writer.record(f"p{index}")
        put("geospatial/shapefile_main", "pyshp_points", Path(base + ".shp").read_bytes())
        put("geospatial/shapefile_index", "pyshp_points", Path(base + ".shx").read_bytes())
        with shapefile.Writer(base + "_polygon", shapeType=shapefile.POLYGON) as writer:
            writer.field("NAME", "C", 10)
            writer.poly([[[0, 0], [0, 5], [5, 5], [5, 0], [0, 0]], [[1, 1], [2, 1], [2, 2], [1, 1]]])
            writer.record("square")
        put("geospatial/shapefile_main", "pyshp_polygon", Path(base + "_polygon.shp").read_bytes())
        put("geospatial/shapefile_index", "pyshp_polygon", Path(base + "_polygon.shx").read_bytes())


# ── media (ffmpeg) ───────────────────────────────────────────────────────────
def ffmpeg():
    exe = shutil.which("ffmpeg")
    if exe:
        return exe
    try:
        import imageio_ffmpeg

        return imageio_ffmpeg.get_ffmpeg_exe()
    except ImportError:
        raise Missing("ffmpeg (or pip install imageio-ffmpeg)")


def make_ffmpeg():
    exe = ffmpeg()
    video = ["-f", "lavfi", "-i", "testsrc=size=32x32:rate=5:duration=0.6"]
    audio = ["-f", "lavfi", "-i", "sine=frequency=440:duration=0.2:sample_rate=8000"]
    quiet = ["-v", "error", "-y", "-bitexact", "-fflags", "+bitexact"]
    with tempfile.TemporaryDirectory() as tmp:
        def encode(name, sample, path, args):
            out = Path(tmp) / name
            try:
                run([exe, *quiet, *args, str(out)])
            except subprocess.CalledProcessError as error:
                skipped.append(f"{path}: {error.stderr.decode().strip().splitlines()[-1] if error.stderr else 'ffmpeg failed'}")
                return
            put(path, sample, out.read_bytes())

        encode("x.mov", "ffmpeg_mjpeg", "media/quicktime_mov", [*video, "-c:v", "mjpeg", "-q:v", "10", "-f", "mov"])
        encode("x.ivf", "ffmpeg_vp8", "media/vp8_duck_ivf", [*video, "-c:v", "libvpx", "-b:v", "50k", "-f", "ivf"])
        encode("x.ogg", "ffmpeg_vorbis", "media/ogg", [*audio, "-c:a", "libvorbis", "-q:a", "0", "-f", "ogg"])
        encode("x.au", "ffmpeg_pcm", "media/au", [*audio, "-c:a", "pcm_s16be", "-f", "au"])
        encode("x.voc", "ffmpeg_pcm_u8", "media/creative_voice_file", [*audio, "-c:a", "pcm_u8", "-f", "voc"])


def make_avi():
    # not in ALL: the definition of the gallery does not skip the padding byte after a chunk of odd size (#278); ffmpeg
    # writes the "Lavf" text of its ISFT entry, 13 bytes, and a pad byte after it
    exe = ffmpeg()
    video = ["-f", "lavfi", "-i", "testsrc=size=32x32:rate=5:duration=0.6"]
    audio = ["-f", "lavfi", "-i", "sine=frequency=440:duration=0.2:sample_rate=8000"]
    quiet = ["-v", "error", "-y", "-bitexact", "-fflags", "+bitexact"]
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / "x.avi"
        run([exe, *quiet, *video, *audio, "-c:v", "mjpeg", "-q:v", "10", "-c:a", "pcm_s16le", "-f", "avi", str(out)])
        put("media/avi", "ffmpeg_mjpeg_pcm", out.read_bytes())


# ── network (the libraries write the frames) ─────────────────────────────────
def make_websocket():
    from websockets.frames import Frame, Opcode

    put("network/websocket", "websockets_text", Frame(Opcode.TEXT, b"hello doe", True).serialize(mask=False))
    # a text frame that a client sends is masked, and the definition decodes the masked bytes: the text is not valid (#279)
    put("network/websocket", "websockets_text_masked", Frame(Opcode.TEXT, b"hello doe", True).serialize(mask=True))
    put("network/websocket", "websockets_binary", Frame(Opcode.BINARY, bytes(range(200)), True).serialize(mask=False))
    put("network/websocket", "websockets_ping", Frame(Opcode.PING, b"ok", True).serialize(mask=True))


def client_hello_body(record):
    # OpenSSL writes a TLS record (5 bytes: type 22, version, length) with a handshake message in it (4 bytes: type 1,
    # length); the gallery's definition is of the body of the ClientHello alone, so the 9 bytes are cut off
    assert record[0] == 22 and record[5] == 1 and len(record) == 5 + int.from_bytes(record[3:5], "big"), "not one ClientHello record"
    return record[9:]


def make_tls():
    # OpenSSL writes the ClientHello that a client sends first: a memory BIO, no network
    context = ssl.create_default_context()
    incoming, outgoing = ssl.MemoryBIO(), ssl.MemoryBIO()
    connection = context.wrap_bio(incoming, outgoing, server_hostname="example.org")
    try:
        connection.do_handshake()
    except ssl.SSLWantReadError:
        pass
    put("network/tls_client_hello", "openssl_example_org", client_hello_body(outgoing.read()))
    context.minimum_version = ssl.TLSVersion.TLSv1_2
    context.maximum_version = ssl.TLSVersion.TLSv1_2
    incoming, outgoing = ssl.MemoryBIO(), ssl.MemoryBIO()
    connection = context.wrap_bio(incoming, outgoing, server_hostname="doe.test")
    try:
        connection.do_handshake()
    except ssl.SSLWantReadError:
        pass
    put("network/tls_client_hello", "openssl_tls12", client_hello_body(outgoing.read()))


ALL = [
    make_gettext, make_dtb, make_uimage, make_cc65, make_tap, make_xar, make_zchunk, make_zisofs, make_android,
    make_partition_tables, make_gpg, make_msi, make_windres, make_php,
    make_dbf_shapefile, make_ffmpeg, make_websocket, make_tls, make_mar, make_efi, make_cpio, make_lzma_alone, make_lzip, make_zlib_stream, make_snappy,
]

# Not made: files that the gallery's definitions cannot read (#278). The functions stay so that they can be run when the
# definitions are fixed. make_vfat: the volume label entry that mkfs.fat writes has a date of zero, which `dos_datetime`
# does not allow; make_pcf: the tables of an X11 font are big-endian and the definition reads them as little-endian; the
# AVI of ffmpeg (see make_avi): the chunks of odd size have a padding byte that the definition does not skip; make_luks:
# a LUKS1 header has 8 key slots of 256000 bytes each, past the limit of a sample; the Apple map of make_partition_tables
# (run with DOE_APM=1): the map of parted lists the free space up to the end of the disk, and the definition reads each partition, so a sample is as large
# as the disk. The MAR of `mar` and the EFI list of efitools are written to samples_without_reference/ and are read by
# the library types `mar_archive` and `efi_signature_list`.

if __name__ == "__main__":
    failed = []
    for function in ALL:
        try:
            function()
        except Missing as error:
            skipped.append(f"{function.__name__}: needs {error}")
        except Exception as error:  # report them all, so that one tool that breaks does not hide the rest
            detail = error.stderr.decode().strip()[-300:] if isinstance(error, subprocess.CalledProcessError) and error.stderr else ""
            failed.append(f"{function.__name__}: {type(error).__name__}: {error} {detail}")
    print("written:", ", ".join(sorted(made)))
    if skipped:
        print("skipped:", *skipped, sep="\n  ")
    if failed:
        print("failed:", *failed, sep="\n  ")
        sys.exit(1)
