#!/usr/bin/env python3
"""Writes sample files for the gallery definitions in samples/gallery/<dir>/<name>/
with the libraries and programs that write the formats in the world (Pillow for
the images, pydicom for DICOM, mutagen for ID3, fontTools for TrueType, ...), so
that each file is what that writer wrote and not something built to fit doe.
`make_real_samples.py` does the same with command line tools. The plan for the
rest is real_files_plan.tsv.

    python3 -m venv venv && venv/bin/pip install pillow pydicom fonttools mutagen \\
        msgpack pymongo nbtlib mido mcap protobuf trimesh numpy numpy-stl py-vox-io scapy
    venv/bin/python conformance/make_real_samples_libs.py [FORMAT...]

The files are committed, so this only has to be run to change them. Each file
is at most 64 KiB and has nothing in it that has a licence: the contents are the
words and the pictures made in this script.
"""

import io
import struct
import sys
import tempfile
import wave
from pathlib import Path

ROOT = Path(__file__).resolve().parent / "samples" / "gallery"
LIMIT = 64 * 1024
made = []


def put(path, sample, data):
    data = bytes(data)
    assert 0 < len(data) <= LIMIT, f"{path}/{sample} is {len(data)} bytes"
    directory = ROOT / path
    directory.mkdir(parents=True, exist_ok=True)
    (directory / f"{sample}.bin").write_bytes(data)
    if path not in made:
        made.append(path)


# ── images (Pillow) ──────────────────────────────────────────────────────────
def picture(size=(16, 16), mode="RGB"):
    from PIL import Image, ImageDraw

    image = Image.new("RGB", size, (250, 250, 250))
    draw = ImageDraw.Draw(image)
    w, h = size
    for x in range(w):
        draw.line([(x, 0), (x, h)], fill=(x * 255 // max(w - 1, 1), 64, 255 - x * 255 // max(w - 1, 1)))
    draw.ellipse([w // 4, h // 4, 3 * w // 4, 3 * h // 4], outline=(0, 0, 0))
    return image.convert(mode) if mode != "RGB" else image


def encode(image, fmt, **options):
    out = io.BytesIO()
    image.save(out, format=fmt, **options)
    return out.getvalue()


def make_images():
    from PIL import Image

    rgb = picture()
    put("image/bmp", "pillow_rgb24", encode(rgb, "BMP"))
    put("image/bmp", "pillow_indexed", encode(rgb.convert("P", palette=Image.ADAPTIVE, colors=16), "BMP"))
    put("image/gif", "pillow_still", encode(rgb.convert("P", palette=Image.ADAPTIVE, colors=32), "GIF"))
    frames = [picture((12, 12)).rotate(angle).convert("P", palette=Image.ADAPTIVE, colors=16) for angle in (0, 90, 180)]
    put("image/gif", "pillow_animated", encode(frames[0], "GIF", save_all=True, append_images=frames[1:], duration=100, loop=0))
    put("image/jpeg", "pillow_q75", encode(rgb, "JPEG", quality=75))
    put("image/jpeg", "pillow_progressive_gray", encode(picture((24, 16)).convert("L"), "JPEG", quality=85, progressive=True))
    put("image/png", "pillow_rgb", encode(rgb, "PNG"))
    put("image/png", "pillow_palette", encode(rgb.convert("P", palette=Image.ADAPTIVE, colors=8), "PNG"))
    put("image/png", "pillow_rgba", encode(rgb.convert("RGBA"), "PNG"))
    put("image/webp", "pillow_lossy", encode(rgb, "WEBP", quality=70))
    put("image/webp", "pillow_lossless", encode(rgb, "WEBP", lossless=True))
    put("image/ico", "pillow_sizes", encode(picture((32, 32)), "ICO", sizes=[(16, 16), (32, 32)]))
    put("image/pcx", "pillow_rgb", encode(rgb, "PCX"))
    put("image/pcx", "pillow_indexed", encode(rgb.convert("P", palette=Image.ADAPTIVE, colors=16), "PCX"))


def make_exif():
    from PIL import Image

    exif = Image.Exif()
    exif[0x010F] = "doe"  # Make
    exif[0x0110] = "sample camera"  # Model
    exif[0x0112] = 1  # Orientation
    exif[0x011A] = (72, 1)  # XResolution
    exif[0x0131] = "Pillow"  # Software
    ifd = exif.get_ifd(0x8769)
    ifd[0x829A] = (1, 125)  # ExposureTime
    ifd[0x829D] = (28, 10)  # FNumber
    ifd[0x9003] = "2020:05:17 13:45:30"  # DateTimeOriginal
    put("image/exif", "pillow_tags", exif.tobytes()[6:])  # without the "Exif\0\0" that the JPEG segment has


def make_dicom():
    import pydicom
    from pydicom.dataset import FileDataset, FileMetaDataset
    from pydicom.uid import ExplicitVRLittleEndian, generate_uid

    meta = FileMetaDataset()
    meta.MediaStorageSOPClassUID = "1.2.840.10008.5.1.4.1.1.7"  # secondary capture
    meta.MediaStorageSOPInstanceUID = "1.2.826.0.1.3680043.8.498.1"
    meta.TransferSyntaxUID = ExplicitVRLittleEndian
    meta.ImplementationClassUID = "1.2.826.0.1.3680043.8.498"
    dataset = FileDataset(None, {}, file_meta=meta, preamble=b"\0" * 128)
    dataset.SOPClassUID = meta.MediaStorageSOPClassUID
    dataset.SOPInstanceUID = meta.MediaStorageSOPInstanceUID
    dataset.PatientName = "Doe^Sample"
    dataset.PatientID = "0001"
    dataset.Modality = "OT"
    dataset.StudyInstanceUID = "1.2.826.0.1.3680043.8.498.2"
    dataset.SeriesInstanceUID = "1.2.826.0.1.3680043.8.498.3"
    dataset.Rows = 4
    dataset.Columns = 4
    dataset.SamplesPerPixel = 1
    dataset.PhotometricInterpretation = "MONOCHROME2"
    dataset.BitsAllocated = 8
    dataset.BitsStored = 8
    dataset.HighBit = 7
    dataset.PixelRepresentation = 0
    dataset.PixelData = bytes(range(0, 160, 10))
    out = io.BytesIO()
    pydicom.dcmwrite(out, dataset, enforce_file_format=True)
    put("image/dicom", "pydicom_secondary_capture", out.getvalue())


# ── fonts (fontTools) ────────────────────────────────────────────────────────
def make_ttf():
    from fontTools.fontBuilder import FontBuilder
    from fontTools.pens.ttGlyphPen import TTGlyphPen

    def square():
        pen = TTGlyphPen(None)
        pen.moveTo((100, 0))
        pen.lineTo((100, 700))
        pen.lineTo((600, 700))
        pen.lineTo((600, 0))
        pen.closePath()
        return pen.glyph()

    names = [".notdef", "A", "B"]
    builder = FontBuilder(1000, isTTF=True)
    builder.setupGlyphOrder(names)
    builder.setupCharacterMap({65: "A", 66: "B"})
    builder.setupGlyf({name: square() for name in names})
    builder.setupHorizontalMetrics({name: (700, 100) for name in names})
    builder.setupHorizontalHeader(ascent=800, descent=-200)
    builder.setupNameTable({"familyName": "Doe Sample", "styleName": "Regular"})
    builder.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
    builder.setupPost()
    out = io.BytesIO()
    builder.save(out)
    put("font/ttf", "fonttools_two_squares", out.getvalue())


# ── tags, audio and music ────────────────────────────────────────────────────
def make_id3():
    import mutagen.id3 as id3

    def frames():
        return [
            id3.TIT2(encoding=3, text="Sample"),
            id3.TPE1(encoding=3, text="doe"),
            id3.TALB(encoding=3, text="Samples"),
            id3.TRCK(encoding=3, text="3/9"),
            id3.COMM(encoding=3, lang="eng", desc="", text="a comment"),
        ]

    for version, name in [(3, "id3v2_3")]:  # (the reference parser fails on a v2.4 tag that mutagen writes)
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "x.mp3"
            path.write_bytes(b"")
            tags = id3.ID3()
            for frame in frames():
                tags.add(frame)
            tags.save(path, v2_version=version, v1=0)
            put(f"media/{name}", "mutagen_basic", path.read_bytes())
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "x.mp3"
        path.write_bytes(b"\xff\xfb\x90\x00" + bytes(64))
        tags = id3.ID3()
        tags.add(id3.TIT2(encoding=0, text="Sample"))
        tags.add(id3.TPE1(encoding=0, text="doe"))
        tags.add(id3.TRCK(encoding=0, text="3"))
        tags.add(id3.COMM(encoding=0, lang="eng", desc="", text="a comment"))
        tags.save(path, v1=2, v2_version=4)
        data = path.read_bytes()
        put("media/id3v1_1", "mutagen_tag", data[-128:])


def make_wav():
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "x.wav"
        with wave.open(str(path), "wb") as out:
            out.setnchannels(1)
            out.setsampwidth(2)
            out.setframerate(8000)
            out.writeframes(b"".join(struct.pack("<h", int(8000 * ((i % 16) / 8 - 1))) for i in range(400)))
        put("media/wav", "wave_module_mono16", path.read_bytes())
        with wave.open(str(path), "wb") as out:
            out.setnchannels(2)
            out.setsampwidth(1)
            out.setframerate(11025)
            out.writeframes(bytes((i * 7) % 256 for i in range(200)))
        put("media/wav", "wave_module_stereo8", path.read_bytes())


def make_midi():
    import mido

    midi = mido.MidiFile(type=1, ticks_per_beat=96)
    tempo = mido.MidiTrack()
    tempo.append(mido.MetaMessage("set_tempo", tempo=500000, time=0))
    tempo.append(mido.MetaMessage("time_signature", numerator=4, denominator=4, time=0))
    tempo.append(mido.MetaMessage("end_of_track", time=0))
    notes = mido.MidiTrack()
    notes.append(mido.MetaMessage("track_name", name="doe", time=0))
    notes.append(mido.Message("program_change", program=5, time=0))
    for note in (60, 64, 67):
        notes.append(mido.Message("note_on", note=note, velocity=80, time=0 if note == 60 else 96))
        notes.append(mido.Message("note_off", note=note, velocity=0, time=96))
    notes.append(mido.MetaMessage("end_of_track", time=0))
    midi.tracks.extend([tempo, notes])
    out = io.BytesIO()
    midi.save(file=out)
    put("media/standard_midi_file", "mido_chord", out.getvalue())


def make_vox():
    from pyvox.models import Model, Size, Vox, Voxel
    from pyvox.writer import VoxWriter

    voxels = [Voxel(x, y, z, 1 + (x + y + z) % 8) for x in range(3) for y in range(3) for z in range(2)]
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "x.vox"
        VoxWriter(str(path), Vox([Model(Size(4, 4, 4), voxels)])).write()
        put("media/magicavoxel_vox", "pyvox_cube", path.read_bytes())


# ── serialization ────────────────────────────────────────────────────────────
VALUE = {"name": "doe", "numbers": [1, 2, 3, 70000, -5], "nested": {"ok": True, "none": None}, "pi": 3.25, "blob": b"\x00\x01\x02"}


def make_msgpack():
    import msgpack

    put("serialization/msgpack", "msgpack_value", msgpack.packb(VALUE))
    put("serialization/msgpack", "msgpack_scalars", b"".join(msgpack.packb(v) for v in [0, -1, 300, "text", 2**33, 1.5, [1, "a"], {"k": 1}]))


def make_bson():
    import bson

    put("serialization/bson", "pymongo_document", bson.encode({k: v for k, v in VALUE.items() if k != "pi"} | {"pi": 3.25}))


def make_nbt():
    import nbtlib
    from nbtlib import tag

    root = nbtlib.File(
        {"Data": tag.Compound({"LevelName": tag.String("doe"), "version": tag.Int(19133), "spawn": tag.List[tag.Int]([0, 64, 0]), "flags": tag.ByteArray([1, 0, 1]), "time": tag.Long(123456789012), "ratio": tag.Float(0.5)})},
        gzipped=False,
        root_name="",
    )
    out = io.BytesIO()
    root.write(out) if hasattr(root, "write") else None
    put("game/minecraft_nbt", "nbtlib_level", out.getvalue())


def make_mcap():
    from mcap.writer import Writer

    out = io.BytesIO()
    writer = Writer(out)
    writer.start(profile="", library="doe")
    schema = writer.register_schema(name="doe/Text", encoding="jsonschema", data=b'{"type":"object"}')
    channel = writer.register_channel(topic="/text", message_encoding="json", schema_id=schema)
    for i in range(3):
        writer.add_message(channel_id=channel, log_time=1_700_000_000_000_000_000 + i, publish_time=1_700_000_000_000_000_000 + i, data=b'{"n":%d}' % i)
    writer.finish()
    put("log/mcap", "mcap_text", out.getvalue())


def make_protobuf():
    from google.protobuf import descriptor_pb2, descriptor_pool, message_factory

    proto = descriptor_pb2.FileDescriptorProto(name="doe.proto", package="doe", syntax="proto3")
    message = proto.message_type.add(name="Sample")
    for number, (name, kind) in enumerate([("id", "INT32"), ("name", "STRING"), ("score", "FLOAT"), ("tags", "STRING"), ("flag", "BOOL"), ("big", "SINT64")], start=1):
        field = message.field.add(name=name, number=number, type=getattr(descriptor_pb2.FieldDescriptorProto, f"TYPE_{kind}"))
        field.label = descriptor_pb2.FieldDescriptorProto.LABEL_REPEATED if name == "tags" else descriptor_pb2.FieldDescriptorProto.LABEL_OPTIONAL
    pool = descriptor_pool.DescriptorPool()
    pool.Add(proto)
    cls = message_factory.GetMessageClass(pool.FindMessageTypeByName("doe.Sample"))
    value = cls(id=150, name="doe", score=0.25, tags=["a", "bc"], flag=True, big=-300)
    put("serialization/google_protobuf", "protobuf_sample", value.SerializeToString())


# ── 3D ───────────────────────────────────────────────────────────────────────
def make_gltf():
    import trimesh

    mesh = trimesh.creation.box(extents=(1, 2, 3))
    put("3d/gltf_binary", "trimesh_box", trimesh.exchange.gltf.export_glb(trimesh.Scene(mesh)))


def make_stl():
    import numpy as np
    from stl import Mode, mesh

    data = np.zeros(4, dtype=mesh.Mesh.dtype)
    data["vectors"] = np.array(
        [[[0, 0, 0], [1, 0, 0], [0, 1, 0]], [[0, 0, 0], [0, 0, 1], [1, 0, 0]], [[0, 0, 0], [0, 1, 0], [0, 0, 1]], [[1, 0, 0], [0, 0, 1], [0, 1, 0]]],
        dtype=np.float32,
    )
    solid = mesh.Mesh(data)
    with tempfile.TemporaryDirectory() as tmp:
        for mode, name in [(Mode.BINARY, "binary")]:  # (the definition reads the binary form)
            path = Path(tmp) / f"{name}.stl"
            solid.save(str(path), mode=mode)
            put("media/stl", f"numpy_stl_{name}", path.read_bytes())


# ── Windows ──────────────────────────────────────────────────────────────────
def make_lnk():
    import pylnk3

    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "x.lnk"
        pylnk3.for_file("C:\\Windows\\notepad.exe", str(path), arguments="/c echo doe", description="doe sample link", work_dir="C:\\Windows")
        put("windows/windows_lnk_file", "pylnk3_notepad", path.read_bytes())


# ── network (Scapy builds the packets, the library writes the captures) ─────
def make_network():
    from scapy.all import ICMP, IP, TCP, UDP, DNS, DNSQR, DNSRR, Ether, IPv6, PcapWriter, Raw
    from scapy.layers.rtp import RTP

    ether = Ether(src="02:00:00:00:00:01", dst="02:00:00:00:00:02")
    dns_query = DNS(id=0x1234, rd=1, qd=DNSQR(qname="example.org", qtype="A"))
    dns_answer = DNS(id=0x1234, qr=1, rd=1, ra=1, qd=DNSQR(qname="example.org"), an=DNSRR(rrname="example.org", ttl=300, rdata="192.0.2.7"))
    packets = {
        "tcp_syn": ether / IP(src="192.0.2.1", dst="192.0.2.2", id=1, flags="DF") / TCP(sport=40000, dport=80, flags="S", seq=1000, options=[("MSS", 1460), ("SAckOK", b""), ("WScale", 7)]),
        "tcp_data": ether / IP(src="192.0.2.1", dst="192.0.2.2", id=2, flags="DF") / TCP(sport=40000, dport=80, flags="PA", seq=1001, ack=5001) / Raw(b"GET / HTTP/1.1\r\nHost: example.org\r\n\r\n"),
        "udp_dns_query": ether / IP(src="192.0.2.1", dst="192.0.2.53", id=3) / UDP(sport=53000, dport=53) / dns_query,
        "udp_dns_answer": ether / IP(src="192.0.2.53", dst="192.0.2.1", id=4) / UDP(sport=53, dport=53000) / dns_answer,
        "icmp_echo": ether / IP(src="192.0.2.1", dst="192.0.2.2", id=5) / ICMP(type=8, id=7, seq=1) / Raw(b"doe doe doe"),
        "ipv6_udp": Ether(src="02:00:00:00:00:01", dst="02:00:00:00:00:02") / IPv6(src="2001:db8::1", dst="2001:db8::2") / UDP(sport=5000, dport=5001) / Raw(b"hello"),
        "rtp": ether / IP(src="192.0.2.1", dst="192.0.2.2", id=6) / UDP(sport=5004, dport=5004) / RTP(payload_type=0, sequence=7, timestamp=160, sourcesync=0x11223344) / Raw(bytes(range(40))),
    }
    for name, packet in packets.items():
        put("network/ethernet_frame", f"scapy_{name}", bytes(packet))
    put("network/ipv4_packet", "scapy_tcp_data", bytes(packets["tcp_data"][IP]))
    put("network/ipv6_packet", "scapy_udp", bytes(packets["ipv6_udp"][IPv6]))
    put("network/tcp_segment", "scapy_syn_options", bytes(packets["tcp_syn"][TCP]))
    put("network/udp_datagram", "scapy_dns", bytes(packets["udp_dns_query"][UDP]))
    put("network/icmp_packet", "scapy_echo", bytes(packets["icmp_echo"][ICMP]))
    put("network/dns_packet", "scapy_query", bytes(dns_query))
    put("network/dns_packet", "scapy_answer", bytes(dns_answer))
    put("network/rtp_packet", "scapy_rtp", bytes(packets["rtp"][RTP]))
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "x.pcap"
        writer = PcapWriter(str(path), linktype=1, sync=True)
        for index, packet in enumerate(packets.values()):
            packet.time = 1_700_000_000 + index
            writer.write(packet)
        writer.close()
        put("network/pcap", "scapy_seven_packets", path.read_bytes())


ALL = [
    make_images, make_exif, make_dicom, make_id3, make_wav, make_midi, make_vox, make_msgpack, make_bson, make_nbt,
    make_protobuf, make_gltf, make_stl, make_lnk, make_network,
]

if __name__ == "__main__":
    failed = []
    for function in ALL:
        try:
            function()
        except Exception as error:  # report them all, so that one writer that breaks does not hide the rest
            failed.append(f"{function.__name__}: {type(error).__name__}: {error}")
    print("written:", ", ".join(sorted(made)))
    if failed:
        print("failed:", *failed, sep="\n  ")
        sys.exit(1)
