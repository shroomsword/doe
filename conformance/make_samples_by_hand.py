#!/usr/bin/env python3
"""Builds valid sample files for gallery formats that no tool in the repository's
environment writes, byte by byte from the specification of each format (the RFC,
the format's own documentation), and not from the Kaitai definition: the
definitions are only read to see which of the format's variants they cover.
Their rows in gallery_samples.tsv say `crafted`.

    python3 conformance/make_samples_by_hand.py [section ...]

A section is a directory of the gallery (`network`); without one, all are made.
The files are committed, so this only has to be run to change them. They are
small, and made of nothing that has a licence: the words and numbers in this
script.
"""

import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent / "samples" / "gallery"
LIMIT = 64 * 1024


def put(path, sample, data):
    data = bytes(data)
    assert len(data) <= LIMIT, f"{path}/{sample} is {len(data)} bytes"
    directory = ROOT / path
    directory.mkdir(parents=True, exist_ok=True)
    (directory / f"{sample}.bin").write_bytes(data)


def checksum(data):
    """The Internet checksum of RFC 1071."""
    if len(data) % 2:
        data += b"\0"
    total = sum(struct.unpack(f">{len(data) // 2}H", data))
    while total >> 16:
        total = (total & 0xFFFF) + (total >> 16)
    return ~total & 0xFFFF


# ── network ──────────────────────────────────────────────────────────────────

def udp(source, destination, payload, pseudo=None):
    length = 8 + len(payload)
    header = struct.pack(">HHHH", source, destination, length, 0) + payload
    if pseudo:
        value = checksum(pseudo + struct.pack(">H", length) + header) or 0xFFFF
        header = header[:6] + struct.pack(">H", value) + header[8:]
    return header


def tcp(source, destination, seq, ack, flags, payload=b"", options=b"", window=65535):
    offset = (20 + len(options)) // 4
    return (
        struct.pack(">HHIIBBHHH", source, destination, seq, ack, offset << 4, flags, window, 0, 0)
        + options
        + payload
    )


def ipv4(protocol, payload, source=(192, 0, 2, 1), destination=(198, 51, 100, 7), options=b"", ident=0x1C46, ttl=64):
    ihl = (20 + len(options)) // 4
    header = struct.pack(
        ">BBHHHBBH4s4s",
        0x40 | ihl, 0, ihl * 4 + len(payload), ident, 0x4000, ttl, protocol, 0,
        bytes(source), bytes(destination),
    ) + options
    value = checksum(header)
    header = header[:10] + struct.pack(">H", value) + header[12:]
    return header + payload


def ipv6(next_header, payload, source=None, destination=None, hop_limit=64):
    source = source or bytes.fromhex("20010db8000000000000000000000001")
    destination = destination or bytes.fromhex("20010db8000000000000000000000002")
    first = (6 << 28) | (0 << 20) | 0x12345
    return struct.pack(">IHBB", first, len(payload), next_header, hop_limit) + source + destination + payload


def icmp_echo(kind, ident, seq, data):
    body = struct.pack(">BBHHH", kind, 0, 0, ident, seq) + data
    return body[:2] + struct.pack(">H", checksum(body)) + body[4:]


def rtp(payload_type, seq, timestamp, ssrc, payload, marker=0, csrc=(), extension=None, padding=0):
    first = (2 << 6) | ((1 if padding else 0) << 5) | ((1 if extension else 0) << 4) | len(csrc)
    header = struct.pack(">BBHII", first, (marker << 7) | payload_type, seq, timestamp, ssrc)
    for c in csrc:
        header += struct.pack(">I", c)
    if extension:
        ext_id, words = extension
        header += struct.pack(">HH", ext_id, len(words) // 4) + words
    tail = (b"\0" * (padding - 1) + bytes([padding])) if padding else b""
    return header + payload + tail


def ws_frame(opcode, payload, fin=True, mask=None):
    first = (0x80 if fin else 0) | opcode
    length = len(payload)
    if length <= 125:
        head = struct.pack(">BB", first, (0x80 if mask else 0) | length)
    elif length < 65536:
        head = struct.pack(">BBH", first, (0x80 if mask else 0) | 126, length)
    else:
        head = struct.pack(">BBQ", first, (0x80 if mask else 0) | 127, length)
    if mask:
        key = bytes(mask)
        masked = bytes(b ^ key[i % 4] for i, b in enumerate(payload))
        return head + key + masked
    return head + payload


def dns_name(name):
    out = b""
    for label in name.rstrip(".").split("."):
        out += bytes([len(label)]) + label.encode()
    return out + b"\0"


def dns_header(ident, flags, qd, an, ns, ar):
    return struct.pack(">HHHHHH", ident, flags, qd, an, ns, ar)


def tls_client_hello():
    random = bytes(range(28))
    ciphers = [0x1301, 0x1302, 0xC02B, 0xC02F, 0x009C]
    body = b"\x03\x03" + struct.pack(">I", 1700000000) + random
    session = bytes(range(32))
    body += bytes([len(session)]) + session
    body += struct.pack(">H", 2 * len(ciphers)) + b"".join(struct.pack(">H", c) for c in ciphers)
    body += b"\x01\x00"  # one compression method: null
    host = b"example.com"
    sni_list = b"\x00" + struct.pack(">H", len(host)) + host
    sni = struct.pack(">H", len(sni_list)) + sni_list
    protocols = [b"h2", b"http/1.1"]
    alpn_list = b"".join(bytes([len(p)]) + p for p in protocols)
    alpn = struct.pack(">H", len(alpn_list)) + alpn_list
    groups = struct.pack(">H", 4) + struct.pack(">HH", 0x001D, 0x0017)  # supported_groups, not looked into
    extensions = b""
    for kind, ext in [(0, sni), (10, groups), (16, alpn), (43, b"\x02\x03\x04")]:
        extensions += struct.pack(">HH", kind, len(ext)) + ext
    return body + struct.pack(">H", len(extensions)) + extensions


def network():
    pseudo4 = bytes((192, 0, 2, 1)) + bytes((198, 51, 100, 7)) + b"\x00\x11"
    datagram = udp(40000, 53, b"\x12\x34\x01\x00\x00\x01\x00\x00\x00\x00\x00\x00", pseudo4)
    put("network/udp_datagram", "dns_query", datagram)
    put("network/udp_datagram", "empty", udp(5000, 5001, b""))

    put(
        "network/tcp_segment",
        "syn",
        tcp(51000, 443, 1000, 0, 0x02, options=struct.pack(">BBHBBBBBBBB", 2, 4, 1460, 4, 2, 1, 3, 3, 7, 0, 0)),
    )
    put("network/tcp_segment", "data", tcp(443, 51000, 5000, 1001, 0x18, b"HTTP/1.1 200 OK\r\n\r\n"))
    put("network/tcp_segment", "fin_ack", tcp(51000, 443, 1001, 5019, 0x11))

    put("network/icmp_packet", "echo", icmp_echo(8, 0x1234, 1, bytes(range(32))))
    put("network/icmp_packet", "echo_reply", icmp_echo(0, 0x1234, 1, bytes(range(32))))
    # destination unreachable (port unreachable) and time exceeded carry the start of the packet that caused them
    original = ipv4(17, udp(40000, 9, b"hi"))[:28]
    for sample, kind, code in [("port_unreachable", 3, 3), ("ttl_exceeded", 11, 0)]:
        message = struct.pack(">BBHI", kind, code, 0, 0) + original
        put("network/icmp_packet", sample, message[:2] + struct.pack(">H", checksum(message)) + message[4:])

    put("network/ipv4_packet", "udp", ipv4(17, datagram))
    put("network/ipv4_packet", "tcp", ipv4(6, tcp(51000, 80, 1, 0, 0x02)))
    put("network/ipv4_packet", "icmp", ipv4(1, icmp_echo(8, 1, 1, b"ping")))
    # a router alert option (RFC 2113): type 0x94, length 4, value 0
    put("network/ipv4_packet", "options", ipv4(17, udp(1, 2, b"x"), options=b"\x94\x04\x00\x00"))

    put("network/ipv6_packet", "udp", ipv6(17, udp(546, 547, b"solicit")))
    put("network/ipv6_packet", "tcp", ipv6(6, tcp(40000, 22, 7, 0, 0x02)))
    # No hop-by-hop sample: the definition reads the length of that header as bytes
    # minus one, where RFC 8200 counts 8-byte units after the first 8, so it cannot
    # read a real one.

    put("network/rtp_packet", "pcmu", rtp(0, 100, 160000, 0xDEADBEEF, bytes(160)))
    put("network/rtp_packet", "marker_csrc", rtp(96, 7, 90000, 0x01020304, b"frame", marker=1, csrc=(11, 22)))
    put("network/rtp_packet", "extension_padding", rtp(8, 9, 320, 5, b"abcdefgh", extension=(0xBEDE, b"\x10\xaa\x00\x00"), padding=4))

    put("network/websocket", "text", ws_frame(1, b"Hello"))
    put("network/websocket", "masked_binary", ws_frame(2, bytes(range(200)), mask=(0x37, 0xFA, 0x21, 0x3D)))
    put("network/websocket", "long", ws_frame(2, bytes(range(256)) + bytes(44)))
    put(
        "network/websocket",
        "fragmented",
        ws_frame(1, b"Hel", fin=False) + ws_frame(0, b"lo, ", fin=False, mask=(1, 2, 3, 4)) + ws_frame(0, b"world"),
    )
    put("network/websocket", "ping", ws_frame(9, b"ping", mask=(9, 8, 7, 6)))

    # DNS (RFC 1035)
    put("network/dns_packet", "query", dns_header(0x1A2B, 0x0100, 1, 0, 0, 0) + dns_name("www.example.com") + struct.pack(">HH", 1, 1))
    question = dns_name("www.example.com") + struct.pack(">HH", 1, 1)
    pointer = b"\xc0\x0c"  # the name at offset 12
    answers = (
        pointer + struct.pack(">HHIH", 5, 1, 300, len(dns_name("web.example.com"))) + dns_name("web.example.com")
        + dns_name("web.example.com") + struct.pack(">HHIH4s", 1, 1, 60, 4, bytes((93, 184, 216, 34)))
        + pointer + struct.pack(">HHIH", 28, 1, 60, 16) + bytes.fromhex("20010db8000000000000000000000001")
    )
    put(
        "network/dns_packet",
        "response",
        dns_header(0x1A2B, 0x8180, 1, 3, 0, 0) + question + answers,
    )
    mx = struct.pack(">H", 10) + dns_name("mail.example.com")
    txt = b"".join(bytes([len(t)]) + t for t in (b"v=spf1 -all", b"hello"))
    soa = (
        dns_name("ns1.example.com") + dns_name("hostmaster.example.com")
        + struct.pack(">IIIII", 2024010101, 7200, 3600, 1209600, 300)
    )
    srv = struct.pack(">HHH", 1, 5, 5060) + dns_name("sip.example.com")
    records = b""
    for kind, rdata in [(15, mx), (16, txt), (6, soa), (33, srv), (2, dns_name("ns1.example.com"))]:
        records += dns_name("example.com") + struct.pack(">HHIH", kind, 1, 3600, len(rdata)) + rdata
    put(
        "network/dns_packet",
        "records",
        dns_header(7, 0x8400, 1, 5, 0, 0) + dns_name("example.com") + struct.pack(">HH", 255, 1) + records,
    )

    put("network/tls_client_hello", "example", tls_client_hello())

    # rtpdump (rtptools): a text line, then a binary header, then each packet with its own small header
    header = b"#!rtpplay1.0 224.2.0.1/5004\n" + struct.pack(">IIIHH", 1700000000, 250000, 0xC0000201, 5004, 0)
    packets = b""
    for index in range(3):
        body = rtp(0, index, index * 160, 0x11223344, bytes(40))
        packets += struct.pack(">HHI", 8 + len(body), len(body), index * 20000) + body
    put("network/rtpdump", "three_packets", header + packets)

    # hashcat: the handshake of a network (WPA) in the two files that hashcat reads
    def handshake(essid, seed):
        mac_ap = bytes([0x00, 0x11, 0x22, 0x33, 0x44, seed])
        mac_station = bytes([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, seed])
        nonce_ap = bytes((i * 7 + seed) & 0xFF for i in range(32))
        nonce_station = bytes((i * 5 + seed) & 0xFF for i in range(32))
        eapol = bytes((i + seed) & 0xFF for i in range(121))
        keymic = bytes(range(seed, seed + 16))
        return essid, mac_ap, mac_station, nonce_ap, nonce_station, eapol, keymic

    def hccapx_record(essid, mac_ap, mac_station, nonce_ap, nonce_station, eapol, keymic, keyver=2):
        # the first bit says whether the replay counter was ignored, the other seven which messages were paired
        return (
            b"HCPX" + struct.pack("<I", 4) + bytes([(0 << 7) | 2, len(essid)])
            + essid.ljust(32, b"\0") + bytes([keyver]) + keymic + mac_ap + nonce_ap + mac_station
            + nonce_station + struct.pack("<H", len(eapol)) + eapol.ljust(256, b"\0")
        )

    put("network/hccapx", "two_networks", hccapx_record(*handshake(b"doe-net", 1)) + hccapx_record(*handshake(b"second", 2), keyver=1))

    def hccap_record(essid, mac_ap, mac_station, nonce_ap, nonce_station, eapol, keymic, keyver=2):
        return (
            essid.ljust(36, b"\0") + mac_ap + mac_station + nonce_station + nonce_ap
            + eapol.ljust(256, b"\0") + struct.pack("<II", len(eapol), keyver) + keymic
        )

    put("network/hccap", "one_network", hccap_record(*handshake(b"doe-net", 3)))

    # DIME (draft-nielsen-dime-02): 12 bytes of header, then options, id, type and data, each padded to 4 bytes
    def dime_record(first, last, chunk, type_format, ident, kind, data, options=b""):
        head = bytes([(1 << 3) | (first << 2) | (last << 1) | chunk, type_format << 4])
        head += struct.pack(">HHHI", len(options), len(ident), len(kind), len(data))

        def pad(blob):
            return blob + b"\0" * (-len(blob) % 4)

        return head + pad(options) + pad(ident) + pad(kind) + pad(data)

    soap = b"<soap:Envelope xmlns:soap='http://www.w3.org/2003/05/soap-envelope'/>"
    option = struct.pack(">HH", 1, 5) + b"hello"
    put(
        "network/dime_message",
        "soap_and_attachment",
        dime_record(1, 0, 0, 1, b"uuid:0001", b"application/soap+xml", soap, options=option)
        + dime_record(0, 1, 0, 2, b"cid:attachment", b"http://example.com/blob", bytes(range(10))),
    )

    # a Bitcoin transaction as it was serialised before segwit: one signed input, two outputs
    def der(r, s):
        return b"\x30" + bytes([len(r) + len(s) + 4]) + b"\x02" + bytes([len(r)]) + r + b"\x02" + bytes([len(s)]) + s

    signature = der(bytes(range(1, 33)), bytes(range(33, 65)))
    script_sig = bytes([len(signature) + 1]) + signature + b"\x01" + b"\x41" + b"\x04" + bytes(range(64, 96)) + bytes(range(96, 128))
    vin = bytes(range(32)) + struct.pack("<IB", 1, len(script_sig)) + script_sig + b"\xff\xff\xff\xff"
    pay_to_key_hash = b"\x76\xa9\x14" + bytes(range(20)) + b"\x88\xac"
    vouts = b"".join(
        struct.pack("<QB", amount, len(pay_to_key_hash)) + pay_to_key_hash for amount in (50_000_000, 1_234_567)
    )
    put("network/bitcoin_transaction", "signed", struct.pack("<IB", 1, 1) + vin + bytes([2]) + vouts + struct.pack("<I", 0))

    # Microsoft Network Monitor 2 capture (.cap): a header with the start time, the frames, and the table of the frames
    frames_in = [
        (ROOT / "network/ethernet_frame/tcp.bin").read_bytes(),
        (ROOT / "network/ethernet_frame/udp.bin").read_bytes(),
    ]
    frames_blob = b""
    table = b""
    position = 72
    for index, frame in enumerate(frames_in):
        table += struct.pack("<I", position + len(frames_blob))
        frames_blob += struct.pack("<QII", index * 1500, len(frame), len(frame)) + frame
    table_offset = position + len(frames_blob)
    nm_header = b"GMBU" + bytes([0, 2]) + struct.pack("<H", 1)  # version 2.0, Ethernet
    nm_header += struct.pack("<8H", 2024, 5, 1, 6, 12, 30, 15, 250)  # SYSTEMTIME: year, month, day of week, day, hour, minute, second, ms
    nm_header += struct.pack("<II", table_offset, len(table)) + bytes(40)
    assert len(nm_header) == 72, len(nm_header)
    put("network/microsoft_network_monitor_v2", "two_frames", nm_header + frames_blob + table)

    # SOME/IP (AUTOSAR, Specification of SOME/IP Message Format): a header of 16 bytes, then the payload
    def some_ip(service, method, client, session, kind, payload, protocol=1, interface=1, code=0):
        return struct.pack(">HHIHHBBBB", service, method, len(payload) + 8, client, session, protocol, interface, kind, code) + payload

    request = some_ip(0x1234, 0x0001, 0x0100, 7, 0x00, b"\x00\x00\x00\x2a")
    response = some_ip(0x1234, 0x0001, 0x0100, 7, 0x80, b"\x00\x00\x00\x2b\x01")
    event = some_ip(0x1234, 0x8001, 0, 1, 0x02, b"temperature")
    put("network/some_ip/some_ip", "request", request)
    put("network/some_ip/some_ip_container", "three_messages", request + response + event)

    # SOME/IP-SD: flags, entries (16 bytes: find, offer, subscribe, subscribe acknowledgement) and options
    def sd_entry(kind, first, second, count_first, count_second, service, instance, major, ttl, tail):
        return struct.pack(">BBBBHHBBHB", kind, first, second, (count_first << 4) | count_second, service, instance, major, ttl >> 16, ttl & 0xFFFF, 0)[:-1] + tail

    entries = (
        sd_entry(0x00, 0, 0, 0, 0, 0x1234, 0xFFFF, 0xFF, 3, struct.pack(">I", 0xFFFFFFFF))  # find
        + sd_entry(0x01, 0, 1, 1, 1, 0x1234, 0x0001, 1, 0xFFFFFF, struct.pack(">I", 5))  # offer, with two options
        + sd_entry(0x06, 2, 0, 1, 0, 0x1234, 0x0001, 1, 3, bytes([0, 0x80, 0x00, 0x10]))  # subscribe, initial data requested
        + sd_entry(0x07, 2, 0, 1, 0, 0x1234, 0x0001, 1, 3, bytes([0, 0x00, 0x00, 0x10]))  # subscribe acknowledgement
    )
    endpoint = struct.pack(">HBB4sBBH", 9, 0x04, 0, bytes((192, 168, 0, 10)), 0, 0x11, 30490)
    endpoint6 = struct.pack(">HBB16sBBH", 21, 0x06, 0, bytes(range(16)), 0, 0x06, 30501)
    multicast = struct.pack(">HBB4sBBH", 9, 0x14, 0, bytes((224, 244, 224, 245)), 0, 0x11, 30490)
    balancing = struct.pack(">HBBHH", 5, 0x02, 0, 10, 20)
    config = b"".join(bytes([len(s)]) + s for s in (b"hostname=ecu", b"abc=")) + b"\0"
    configuration = struct.pack(">HBB", len(config) + 1, 0x01, 0) + config
    options = endpoint + endpoint6 + multicast + balancing + configuration
    sd_payload = bytes([0xC0, 0, 0, 0]) + struct.pack(">I", len(entries)) + entries + struct.pack(">I", len(options)) + options
    put("network/some_ip/some_ip_sd", "offer_and_subscribe", sd_payload)
    put("network/some_ip/some_ip", "service_discovery", struct.pack(">HHIHHBBBB", 0xFFFF, 0x8100, len(sd_payload) + 8, 0, 1, 1, 1, 0x02, 0) + sd_payload)


# ── media ────────────────────────────────────────────────────────────────────

def media():
    # Sun/NeXT audio (.au): the header is 24 bytes and a comment, then the samples
    def au(encoding, rate, channels, data, comment=b"doe sample\0", unknown_size=False):
        comment = comment + b"\0" * (-len(comment) % 4)
        size = 0xFFFFFFFF if unknown_size else len(data)
        return b".snd" + struct.pack(">IIIII", 24 + len(comment), size, encoding, rate, channels) + comment + data

    samples16 = b"".join(struct.pack(">h", int(12000 * ((i % 8) - 3.5) / 3.5)) for i in range(64))
    put("media/au", "linear16", au(3, 8000, 1, samples16))
    put("media/au", "mulaw_unknown_size", au(1, 8000, 1, bytes(range(0, 255, 4)), unknown_size=True))
    put("media/au", "stereo_no_comment", au(3, 44100, 2, samples16, comment=b""))

    # ID3v1.1 tag at the end of an MP3: one silent MPEG-1 Layer III frame (128 kbit/s, 44.1 kHz: 417 bytes)
    frame = b"\xff\xfb\x90\x64" + bytes(413)
    tag = b"TAG" + b"doe sample".ljust(30, b"\0") + b"the doe authors".ljust(30, b"\0") + b"samples".ljust(30, b"\0") + b"2024" + b"made for the tests".ljust(30, b"\0") + bytes([13])
    put("media/id3v1_1", "silent_frame", frame * 3 + tag)

    # Creative Voice File: a header, then blocks; the terminator has no size
    def block(kind, body):
        return bytes([kind]) + struct.pack("<I", len(body))[:3] + body

    voc = b"Creative Voice File\x1a" + struct.pack("<HHH", 26, 0x010A, ~0x010A + 0x1234 & 0xFFFF)
    voc += block(1, bytes([156, 0]) + bytes(range(64)))  # sound data: 8-bit PCM at 1000000 / (256 - 156) = 10 kHz
    voc += block(3, struct.pack("<HB", 1000, 156))  # silence
    voc += block(4, struct.pack("<H", 7))  # marker
    voc += block(6, struct.pack("<H", 2))  # repeat start
    voc += block(7, b"")  # repeat end
    voc += block(5, b"a text block\0")
    voc += block(8, struct.pack("<HBB", 65536 - 64, 0, 0))  # extra info
    voc += block(1, bytes([156, 0]) + bytes(16))
    voc += block(9, struct.pack("<IBBH", 22050, 16, 1, 4) + bytes(4) + bytes(64))  # sound data, new format
    voc += b"\x00"  # terminator
    put("media/creative_voice_file", "blocks", voc)

    # binary STL: 80 header bytes, a count, 50 bytes a triangle (a tetrahedron)
    points = [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0)]
    faces = [(0, 2, 1), (0, 1, 3), (0, 3, 2), (1, 2, 3)]
    triangles = b""
    for a, b, c in faces:
        pa, pb, pc = points[a], points[b], points[c]
        u = [pb[i] - pa[i] for i in range(3)]
        v = [pc[i] - pa[i] for i in range(3)]
        n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
        length = sum(x * x for x in n) ** 0.5
        n = [x / length for x in n]
        triangles += struct.pack("<12fH", *n, *pa, *pb, *pc, 0)
    put("media/stl", "tetrahedron", b"binary STL made for the tests".ljust(80, b" ") + struct.pack("<I", 4) + triangles)

    # Standard MIDI file, format 1: a tempo track and a track of notes (no running status)
    def vlq(value):
        out = [value & 0x7F]
        value >>= 7
        while value:
            out.append(0x80 | (value & 0x7F))
            value >>= 7
        return bytes(reversed(out))

    def track(events):
        body = b"".join(vlq(delta) + data for delta, data in events)
        return b"MTrk" + struct.pack(">I", len(body)) + body

    tempo = track([
        (0, b"\xff\x03\x05tempo"),
        (0, b"\xff\x51\x03\x07\xa1\x20"),
        (0, b"\xff\x58\x04\x04\x02\x18\x08"),
        (0, b"\xff\x2f\x00"),
    ])
    notes = track([
        (0, b"\xc0\x00"),
        (0, b"\xb0\x07\x64"),
        (0, b"\x90\x3c\x40"),
        (480, b"\x80\x3c\x40"),
        (0, b"\xe0\x00\x40"),
        (0, b"\xf0\x04\x7e\x7f\x09\xf7"),
        (0, b"\xa0\x3c\x10"),
        (0, b"\xd0\x20"),
        (16384, b"\x90\x40\x50"),
        (240, b"\x80\x40\x00"),
        (0, b"\xff\x2f\x00"),
    ])
    put("media/standard_midi_file", "two_tracks", b"MThd" + struct.pack(">IHHh", 6, 1, 2, 480) + tempo + notes)

    # AVI: RIFF with a header list (main header, a video stream) and the frames; uncompressed 2x2 24-bit frames
    def chunk(kind, body):
        assert len(body) % 2 == 0
        return kind + struct.pack("<I", len(body)) + body

    def lst(kind, body):
        return chunk(b"LIST", kind + body)

    frame_a = (b"\x00\x00\xff\x00\xff\x00" + b"\x00\x00") + (b"\xff\x00\x00\xff\xff\xff" + b"\x00\x00")
    frame_b = bytes(reversed(frame_a))
    avih = struct.pack("<IIIIIIIIII16s", 33333, 1000, 0, 0x10, 2, 0, 1, 16, 2, 2, bytes(16))
    strh = struct.pack("<4s4sIHHIIIIIIIIhhhh", b"vids", b"DIB ", 0, 0, 0, 0, 1, 30, 0, 2, 16, 0xFFFFFFFF, 0, 0, 0, 2, 2)
    strf = struct.pack("<IiiHHIIiiII", 40, 2, 2, 1, 24, 0, 16, 0, 0, 0, 0)
    hdrl = lst(b"hdrl", chunk(b"avih", avih) + lst(b"strl", chunk(b"strh", strh) + chunk(b"strf", strf)))
    movi = lst(b"movi", chunk(b"00db", frame_a) + chunk(b"00db", frame_b))
    index = chunk(b"idx1", b"".join(struct.pack("<4sIII", b"00db", 0x10, 4 + 24 * i + 0, 16) for i in range(2)))
    body = b"AVI " + hdrl + movi + index
    put("media/avi", "two_frames", b"RIFF" + struct.pack("<I", len(body)) + body)

    # Ogg (RFC 3533): pages with a CRC-32 of their own kind (polynomial 0x04C11DB7, no reflection, no final xor)
    table = []
    for byte in range(256):
        crc = byte << 24
        for _ in range(8):
            crc = ((crc << 1) ^ 0x04C11DB7) & 0xFFFFFFFF if crc & 0x80000000 else (crc << 1) & 0xFFFFFFFF
        table.append(crc)

    def ogg_crc(data):
        crc = 0
        for byte in data:
            crc = ((crc << 8) & 0xFFFFFFFF) ^ table[((crc >> 24) ^ byte) & 0xFF]
        return crc

    def ogg_page(flags, granule, serial, number, packets):
        lacing = b""
        for packet in packets:
            lacing += b"\xff" * (len(packet) // 255) + bytes([len(packet) % 255])
        header = b"OggS\0" + bytes([flags]) + struct.pack("<QIII", granule, serial, number, 0) + bytes([len(lacing)]) + lacing
        page = header + b"".join(packets)
        return page[:22] + struct.pack("<I", ogg_crc(page)) + page[26:]

    stream = (
        ogg_page(0x02, 0, 0x1234, 0, [b"\x01doe-codec" + bytes(20)])
        + ogg_page(0x00, 0, 0x1234, 1, [b"\x03comments", bytes(range(256)) * 2])
        + ogg_page(0x04, 960, 0x1234, 2, [bytes(40), bytes(60)])
    )
    put("media/ogg", "three_pages", stream)

    # ID3v2 tags (id3.org): a header with a synchsafe size, frames, and zero padding
    def synchsafe(value):
        return bytes([(value >> 21) & 0x7F, (value >> 14) & 0x7F, (value >> 7) & 0x7F, value & 0x7F])

    def id3(version, frames, padding=64, footer=False):
        body = b"".join(frames) + bytes(padding)
        flags = 0x10 if footer else 0
        tag = b"ID3" + bytes([version, 0, flags]) + synchsafe(len(body)) + body
        if footer:
            tag += b"3DI" + bytes([version, 0, flags]) + synchsafe(len(body))
        return tag

    def v3_frame(name, data):
        return name + struct.pack(">I", len(data)) + b"\0\0" + data

    def v4_frame(name, data):
        return name + synchsafe(len(data)) + b"\0\0" + data

    put(
        "media/id3v2_3",
        "tags",
        id3(
            3,
            [
                v3_frame(b"TIT2", b"\0" + b"A title"),
                v3_frame(b"TPE1", b"\0" + b"An artist"),
                v3_frame(b"TALB", b"\0" + b"An album"),
                v3_frame(b"TYER", b"\0" + b"2024"),
                v3_frame(b"TRCK", b"\0" + b"3/12"),
                v3_frame(b"COMM", b"\0" + b"eng" + b"\0" + b"a comment of some length, so that it is the longest frame of the tag"),
            ],
        ),
    )
    put(
        "media/id3v2_4",
        "tags",
        id3(
            4,
            [
                v4_frame(b"TIT2", b"\x03" + "A títle".encode()),
                v4_frame(b"TPE1", b"\x03" + b"An artist"),
                v4_frame(b"TDRC", b"\x03" + b"2024-05-06"),
                v4_frame(b"COMM", b"\x03" + b"eng" + b"\0" + b"a comment of some length, so that it is the longest frame of the tag"),
            ],
            padding=0,
            footer=True,  # with a footer, the tag has no padding (the reference runtime cannot read the padding of the definition)
        ),
    )

    # MagicaVoxel .vox (version 150): a MAIN chunk whose children are the size, the voxels and the palette
    def chunk(kind, content=b"", children=b""):
        return kind + struct.pack("<II", len(content), len(children)) + content + children

    voxels = [(0, 0, 0, 1), (1, 0, 0, 2), (1, 1, 0, 3), (2, 2, 2, 255)]
    palette = b"".join(bytes((i, 255 - i, (i * 7) & 0xFF, 255)) for i in range(256))
    children = (
        chunk(b"SIZE", struct.pack("<III", 3, 3, 3))
        + chunk(b"XYZI", struct.pack("<I", len(voxels)) + b"".join(bytes(v) for v in voxels))
        + chunk(b"RGBA", palette)
    )
    put("media/magicavoxel_vox", "small_model", b"VOX " + struct.pack("<I", 150) + chunk(b"MAIN", b"", children))


# ── image ────────────────────────────────────────────────────────────────────

def png(width, height, pixels):
    """An 8-bit RGBA PNG; pixels is a list of rows of (r, g, b, a)."""
    import zlib

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))

    raw = b"".join(b"\0" + b"".join(bytes(p) for p in row) for row in pixels)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


def image():
    # Windows icon: a directory, then an image each as a PNG or as a DIB without a file header
    small = png(2, 2, [[(255, 0, 0, 255), (0, 255, 0, 255)], [(0, 0, 255, 255), (255, 255, 255, 128)]])
    # a 2x2 32-bit DIB: a BITMAPINFOHEADER with the height of the image and the mask together, bottom row first, then the AND mask
    dib = struct.pack("<IiiHHIIiiII", 40, 2, 4, 1, 32, 0, 0, 0, 0, 0, 0)
    dib += b"".join(bytes((b, g, r, a)) for (r, g, b, a) in [(0, 0, 255, 255), (255, 255, 255, 128), (255, 0, 0, 255), (0, 255, 0, 255)])
    dib += b"\0\0\0\0" * 2
    images = [(2, 2, 0, 1, 32, small), (2, 2, 0, 1, 32, dib)]
    directory = b"\0\0\1\0" + struct.pack("<H", len(images))
    offset = len(directory) + 16 * len(images)
    entries = b""
    for width, height, colors, planes, bpp, blob in images:
        entries += struct.pack("<BBBBHHII", width, height, colors, 0, planes, bpp, len(blob), offset)
        offset += len(blob)
    put("image/ico", "png_and_dib", directory + entries + b"".join(i[5] for i in images))

    # Targa: a header of 18 bytes, the image id, a colour map, the pixels, and (TGA 2.0) a footer of 26 bytes.
    # The first two have no footer, as the original format; a footer with no extension area (offset 0)
    # is not read by the definition, which reads the extension area at offset 0

    def tga(kind, map_type, map_entries, map_depth, width, height, depth, descriptor, ident, colour_map, pixels, footer):
        head = struct.pack("<BBBHHBHHHHBB", len(ident), map_type, kind, 0, map_entries, map_depth, 0, 0, width, height, depth, descriptor)
        return head + ident + colour_map + pixels + footer

    rows = [(255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 0)]  # BGR order in the file
    truecolor = b"".join(bytes((b, g, r)) for r, g, b in rows)
    put("image/tga", "truecolor", tga(2, 0, 0, 0, 2, 2, 24, 0x20, b"", b"", truecolor, b""))
    palette = b"".join(bytes((b, g, r)) for r, g, b in [(0, 0, 0), (255, 0, 0), (0, 255, 0), (0, 0, 255)])
    put("image/tga", "color_mapped", tga(1, 1, 4, 24, 2, 2, 8, 0x20, b"doe", palette, bytes([0, 1, 2, 3]), b""))
    # with an extension area (TGA 2.0): 495 bytes that the footer points to
    def text(value, size):
        return value.encode().ljust(size, b"\0")

    extension = (
        struct.pack("<H", 495)
        + text("doe authors", 41)
        + b"".join(text(f"comment {i}", 81) for i in range(4))
        + struct.pack("<6H", 10, 26, 2024, 12, 0, 0)  # month, day, year, hour, minute, second
        + text("job", 41)
        + struct.pack("<3H", 0, 1, 2)  # hours, minutes, seconds
        + text("doe", 41)
        + struct.pack("<HB", 100, 32)  # version 1.00, letter ' '
        + struct.pack("<I", 0)  # key colour
        + struct.pack("<HH", 1, 1)  # pixel aspect ratio, numerator and denominator
        + struct.pack("<HH", 22, 10)  # gamma 2.2
        + struct.pack("<3I", 0, 0, 0)  # offsets of the colour correction table, the postage stamp, the scan line table
        + b"\3"  # attributes: useful alpha
    )
    assert len(extension) == 495, len(extension)
    image_bytes = tga(2, 0, 0, 0, 2, 2, 24, 0x20, b"", b"", truecolor, b"")
    ext_offset = len(image_bytes)
    put("image/tga", "extension_area", image_bytes + extension + struct.pack("<II", ext_offset, 0) + b"TRUEVISION-XFILE.\0")

    # PCX: a header of 128 bytes, run-length coded scan lines, and for 256 colours a palette at the end
    def rle(line):
        out = b""
        i = 0
        while i < len(line):
            run = 1
            while i + run < len(line) and line[i + run] == line[i] and run < 63:
                run += 1
            if run > 1 or line[i] >= 0xC0:
                out += bytes([0xC0 | run, line[i]])
            else:
                out += bytes([line[i]])
            i += run
        return out

    def pcx_header(width, height, planes, bytes_per_line):
        head = struct.pack("<BBBBHHHHHH", 0x0A, 5, 1, 8, 0, 0, width - 1, height - 1, 72, 72)
        head += bytes(48) + b"\0" + bytes([planes]) + struct.pack("<HHHH", bytes_per_line, 1, width, height)
        return head.ljust(128, b"\0")

    indexes = [bytes([0, 0, 0, 0, 5, 7, 7, 200]), bytes([1, 2, 3, 4, 4, 4, 4, 255])]
    palette256 = b"\x0c" + b"".join(bytes((i, 255 - i, (i * 3) & 255)) for i in range(256))
    put("image/pcx", "indexed256", pcx_header(8, 2, 1, 8) + b"".join(rle(r) for r in indexes) + palette256)
    reds, greens, blues = bytes([255] * 4 + [0] * 4), bytes([0] * 4 + [255] * 4), bytes([0, 64, 128, 255] * 2)
    planes = b"".join(rle(r) for r in (reds, greens, blues))
    put("image/pcx", "truecolor_planes", pcx_header(8, 1, 3, 8) + planes)

    # DCX: the magic, a list of offsets ended by 0, and a PCX file at each
    # (the definition looks for the palette of 256 colours at the end of the whole file, so that page is the last)
    first = pcx_header(8, 1, 3, 8) + planes
    second = pcx_header(8, 2, 1, 8) + b"".join(rle(r) for r in indexes) + palette256
    start = 4 + 4 * 3
    put(
        "image/pcx_dcx",
        "two_pages",
        b"\xb1\x68\xde\x3a" + struct.pack("<III", start, start + len(first), 0) + first + second,
    )

    # GIMP brush (.gbr, version 2): the size of the header, the header, then the pixels
    name = b"doe brush\0"
    header_size = 28 + len(name)
    gimp = struct.pack(">IIIII", header_size, 2, 4, 4, 1) + b"GIMP" + struct.pack(">I", 25) + name
    put("image/gimp_brush", "gray_4x4", gimp + bytes((i * 17) & 0xFF for i in range(16)))

    # PlayStation TIM: 16-bit colour, and 4-bit with a colour lookup table
    def tim_block(x, y, width, height, data):
        return struct.pack("<IHHHH", 12 + len(data), x, y, width, height) + data

    pixels16 = b"".join(struct.pack("<H", (r << 10) | (g << 5) | b) for r, g, b in [(31, 0, 0), (0, 31, 0), (0, 0, 31), (31, 31, 31)] * 2)
    put("image/psx_tim", "direct_16bit", b"\x10\0\0\0" + struct.pack("<I", 2) + tim_block(640, 0, 4, 2, pixels16))
    clut = b"".join(struct.pack("<H", (i << 10) | (i << 5) | i) for i in range(16))
    put(
        "image/psx_tim",
        "indexed_4bit",
        b"\x10\0\0\0" + struct.pack("<I", 0b1000) + tim_block(0, 480, 16, 1, clut) + tim_block(640, 256, 2, 2, bytes([0x10, 0x32, 0x54, 0x76, 0x98, 0xBA, 0xDC, 0xFE])),
    )

    # X Window Dump: a header, the colour map, then the pixels (as xwd writes a ZPixmap)
    def xwd(width, height, depth, bits, visual, masks, colors, creator, pixels):
        fields = (
            7, 2, depth, width, height, 0, 1, 32, 1, 32, bits, (width * bits + 31) // 32 * 4, visual,
            *masks, 8, len(colors), len(colors), width, height, 0, 0, 0,
        )
        head = struct.pack(">24I", *fields) + creator.encode() + b"\0"
        table = b"".join(struct.pack(">IHHHBB", n, r, g, b, 7, 0) for n, (r, g, b) in enumerate(colors))
        return struct.pack(">I", 4 + len(head)) + head + table + pixels

    pixel_data = b"".join(bytes((0, r, g, b)) for r, g, b in [(255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 255)])
    put("image/xwd", "truecolor", xwd(2, 2, 24, 32, 4, (0xFF0000, 0xFF00, 0xFF), [], "doe", pixel_data))
    put(
        "image/xwd",
        "pseudocolor",
        xwd(4, 2, 8, 8, 3, (0, 0, 0), [(0, 0, 0), (65535, 0, 0), (0, 65535, 0), (0, 0, 65535)], "doe", bytes([0, 1, 2, 3, 3, 2, 1, 0])),
    )


# ── serialization ────────────────────────────────────────────────────────────

def bson_document(elements):
    """A BSON document (bsonspec.org); elements is a list of (type byte, name, encoded value)."""
    body = b"".join(bytes([kind]) + name.encode() + b"\0" + value for kind, name, value in elements)
    return struct.pack("<i", len(body) + 5) + body + b"\0"


def bson_string(text):
    return struct.pack("<i", len(text.encode()) + 1) + text.encode() + b"\0"


def varint(value):
    out = bytearray()
    while True:
        low = value & 0x7F
        value >>= 7
        if value:
            out.append(low | 0x80)
        else:
            out.append(low)
            return bytes(out)


def serialization():
    inner = bson_document([(0x10, "depth", struct.pack("<i", 2)), (0x08, "deep", b"\1")])
    array = bson_document([(0x10, str(i), struct.pack("<i", v)) for i, v in enumerate((10, 20, 30))])
    elements = [
        (0x01, "double", struct.pack("<d", 3.25)),
        (0x02, "string", bson_string("héllo")),
        (0x03, "document", inner),
        (0x04, "array", array),
        (0x05, "binary", struct.pack("<iB", 4, 0) + b"\x00\x01\x02\x03"),
        (0x05, "uuid", struct.pack("<iB", 16, 4) + bytes(range(16))),
        (0x07, "object_id", bytes.fromhex("507f1f77bcf86cd799439011")),
        (0x08, "false", b"\0"),
        (0x08, "true", b"\1"),
        (0x09, "datetime", struct.pack("<q", 1700000000000)),
        (0x0A, "null", b""),
        (0x0B, "regex", b"^doe\0i\0"),
        (0x0D, "javascript", bson_string("function () { return 1; }")),
        (0x0E, "symbol", bson_string("sym")),
        (0x10, "int32", struct.pack("<i", -42)),
        (0x11, "timestamp", struct.pack("<II", 1, 1700000000)),
        (0x12, "int64", struct.pack("<q", 1 << 40)),
        # no decimal128: the definition reads its significand as 49 bits, which the reference runtime cannot (32 at most)
        (0x7F, "max_key", b""),
    ]
    put("serialization/bson", "all_types", bson_document(elements))
    put("serialization/bson", "empty", bson_document([]))

    # MessagePack (msgpack.org): fixints, sized ints, floats, strings, arrays and maps of each size
    def mp_str(text):
        raw = text.encode()
        if len(raw) < 32:
            return bytes([0xA0 | len(raw)]) + raw
        if len(raw) < 256:
            return b"\xd9" + bytes([len(raw)]) + raw
        return b"\xda" + struct.pack(">H", len(raw)) + raw

    numbers = [
        b"\x00", b"\x7f", b"\xcc\xff", b"\xcd\x01\x00", b"\xce\x00\x01\x11\x70", b"\xcf" + struct.pack(">Q", 1 << 40),
        b"\xd0\x80", b"\xd1\xff\x00", b"\xd2" + struct.pack(">i", -70000), b"\xd3" + struct.pack(">q", -5_000_000_000),
        b"\xca" + struct.pack(">f", 1.5), b"\xcb" + struct.pack(">d", -2.25), b"\xc0", b"\xc2", b"\xc3",
    ]
    mapped = b"\x83" + mp_str("name") + mp_str("doe") + mp_str("numbers") + b"\xdc" + struct.pack(">H", len(numbers)) + b"".join(numbers) + mp_str("nested") + b"\x81" + mp_str("long") + mp_str("x" * 300)
    put("serialization/msgpack", "document", mapped)
    put("serialization/msgpack", "small_array", b"\x93\x01\x02\x03")
    put("serialization/msgpack", "map16", b"\xde" + struct.pack(">H", 2) + mp_str("a") + b"\x01" + mp_str("b") + b"\xdd" + struct.pack(">I", 1) + b"\xc3")

    # PHP serialize() output
    put(
        "serialization/php_serialized_value",
        "array",
        b'a:5:{i:0;s:3:"foo";i:1;d:1.5;s:1:"k";b:1;s:4:"null";N;i:4;a:2:{i:0;i:-7;i:1;b:0;}}',
    )
    put(
        "serialization/php_serialized_value",
        "object",
        b'O:8:"stdClass":2:{s:1:"a";i:1;s:5:"\0*\0pr";s:2:"hi";}',
    )

    # protocol buffers wire format: varint, 64-bit, length-delimited (a string and a nested message), 32-bit
    def key(field, wire):
        return varint(field << 3 | wire)

    nested = key(1, 0) + varint(150) + key(2, 2) + varint(3) + b"abc"
    message = (
        key(1, 0) + varint(300)
        + key(2, 2) + varint(len("testing")) + b"testing"
        + key(3, 2) + varint(len(nested)) + nested
        + key(4, 1) + struct.pack("<Q", 1 << 40)
        + key(5, 5) + struct.pack("<f", 1.5)
        + key(6, 0) + varint((1 << 49) - 1)
    )
    put("serialization/google_protobuf", "message", message)

    # Chrome resource pack (.pak), versions 4 and 5: a table of (id, offset) and then the resources
    def pak(version):
        bodies = [(100, b"<html>one</html>"), (101, b"two"), (7, b"\x00\x01\x02")]
        if version == 4:
            header = struct.pack("<IIB", 4, len(bodies), 1)
            table_size = 6 * (len(bodies) + 1)
            aliases = b""
        else:
            header = struct.pack("<IB3xHH", 5, 1, len(bodies), 1)
            aliases = struct.pack("<HH", 200, 0)  # id 200 is another name for the first resource
            table_size = 6 * (len(bodies) + 1)
        offset = len(header) + table_size + len(aliases)
        table = b""
        for ident, body in bodies:
            table += struct.pack("<HI", ident, offset)
            offset += len(body)
        table += struct.pack("<HI", 0, offset)
        return header + table + aliases + b"".join(body for _, body in bodies)

    put("serialization/chrome_pak", "version4", pak(4))
    put("serialization/chrome_pak", "version5", pak(5))


# ── game ─────────────────────────────────────────────────────────────────────

def game():
    # Quake PACK: "PACK", the offset and size of the directory, the files, the directory (56-byte names)
    files = [("maps/start.bsp", bytes(range(64))), ("progs/player.mdl", b"IDPO" + bytes(20)), ("readme.txt", b"hello pak\n")]
    body = b""
    directory = b""
    for name, data in files:
        directory += name.encode().ljust(56, b"\0") + struct.pack("<II", 12 + len(body), len(data))
        body += data
    put("game/quake_pak", "three_files", b"PACK" + struct.pack("<II", 12 + len(body), len(directory)) + body + directory)

    # Doom WAD: a header, the lumps, and the directory at the end; a map with one room
    vertexes = b"".join(struct.pack("<hh", x, y) for x, y in [(0, 0), (256, 0), (256, 256), (0, 256)])
    sidedefs = b"".join(struct.pack("<hh8s8s8sh", 0, 0, b"-", b"-", b"STARTAN3", 0) for _ in range(4))
    linedefs = b"".join(struct.pack("<7H", i, (i + 1) % 4, 1, 0, 0, i, 0xFFFF) for i in range(4))
    things = struct.pack("<hhHHH", 128, 128, 90, 1, 7) + struct.pack("<hhHHH", 64, 64, 0, 2001, 7)
    sectors = struct.pack("<hh8s8shHH", 0, 128, b"FLOOR4_8", b"CEIL3_5", 160, 0, 0)
    # a blockmap of 1x1 block: origin, size, one offset (in words), then 0, the linedefs, -1
    blockmap = struct.pack("<hhhh", 0, 0, 1, 1) + struct.pack("<H", 5) + struct.pack("<7h", 0, 0, 1, 2, 3, -1, 0)[:12]
    lumps = [
        ("E1M1", b""), ("THINGS", things), ("LINEDEFS", linedefs), ("SIDEDEFS", sidedefs),
        ("VERTEXES", vertexes), ("SECTORS", sectors), ("BLOCKMAP", blockmap), ("PLAYPAL", bytes(range(48))),
    ]
    data = b""
    entries = b""
    for name, blob in lumps:
        entries += struct.pack("<ii8s", 12 + len(data), len(blob), name.encode().ljust(8, b"\0"))
        data += blob
    for magic in (b"IWAD", b"PWAD"):
        put("game/doom_wad", magic.decode().lower(), magic + struct.pack("<ii", len(lumps), 12 + len(data)) + data + entries)

    # Dune II PAK: the directory is a list of (offset, name), ended by a zero offset; the first offset is the size of the directory
    names = [("INTRO.HSQ", b"intro data"), ("DUNE.CFG", b"cfg"), ("LAST", b"\x01\x02\x03\x04\x05")]
    directory_size = sum(4 + len(n) + 1 for n, _ in names) + 4
    out = b""
    offset = directory_size
    for name, blob in names:
        out += struct.pack("<I", offset) + name.encode() + b"\0"
        offset += len(blob)
    out += struct.pack("<I", 0) + b"".join(blob for _, blob in names)
    put("game/dune_2_pak", "three_files", out)

    # Minecraft NBT (uncompressed, as it is inside a level.dat before gzip): a named compound of every tag
    def name(text):
        return struct.pack(">H", len(text.encode())) + text.encode()

    def tag(kind, text, payload):
        return bytes([kind]) + name(text) + payload

    compound = (
        tag(1, "byte", struct.pack(">b", -5))
        + tag(2, "short", struct.pack(">h", 1000))
        + tag(3, "int", struct.pack(">i", -100000))
        + tag(4, "long", struct.pack(">q", 1 << 40))
        + tag(5, "float", struct.pack(">f", 1.5))
        + tag(6, "double", struct.pack(">d", -0.25))
        + tag(7, "bytes", struct.pack(">i", 4) + b"\x00\x01\x02\x03")
        + tag(8, "string", name("héllo"))
        + tag(9, "ints", bytes([3]) + struct.pack(">i", 3) + struct.pack(">3i", 1, 2, 3))
        + tag(9, "empty_list", bytes([0]) + struct.pack(">i", 0))
        + tag(9, "compounds", bytes([10]) + struct.pack(">i", 2) + tag(1, "a", b"\x01") + b"\0" + tag(8, "b", name("x")) + b"\0")
        + tag(10, "inner", tag(3, "depth", struct.pack(">i", 1)) + b"\0")
        + tag(11, "int_array", struct.pack(">i", 2) + struct.pack(">2i", 7, -7))
        + tag(12, "long_array", struct.pack(">i", 2) + struct.pack(">2q", 1, -1))
        + b"\0"
    )
    put("game/minecraft_nbt", "level", tag(10, "Data", compound[:-1] + b"\0"))

    import zlib

    # Fallout (big-endian): the folders and the files in them, the index first and the data after
    def pstr1(text):
        return bytes([len(text)]) + text.encode()

    def fallout1():
        folders = ["DATA", "ART"]
        files = {"DATA": [("A.TXT", b"first file"), ("B.BIN", bytes(range(30)))], "ART": [("C.FRM", b"third")]}
        index_size = 16 + sum(len(pstr1(f)) for f in folders) + sum(16 + sum(len(pstr1(n)) + 16 for n, _ in files[f]) for f in folders)
        data, entries, position = b"", b"", index_size
        out = struct.pack(">IIII", len(folders), 0, 0, 1700000000) + b"".join(pstr1(f) for f in folders)
        for f in folders:
            out += struct.pack(">IIII", len(files[f]), 0, 0x10, 1700000000)
            for name, blob in files[f]:
                out += pstr1(name) + struct.pack(">IIII", 32, position + len(data), len(blob), len(blob))
                data += blob
        return out + data

    put("game/fallout_dat", "two_folders", fallout1())

    # Fallout 2: the data first, then the index and a footer (index size, file size), all little-endian
    def fallout2():
        raw = b"stored without compression"
        text = b"packed with zlib " * 20
        packed = zlib.compress(text)
        data = raw + packed
        index = struct.pack("<I", 2)
        for name, flags, unpacked, stored, offset in [("DATA/A.TXT", 0, len(raw), len(raw), 0), ("DATA/B.TXT", 1, len(text), len(packed), len(raw))]:
            index += struct.pack("<I", len(name)) + name.encode() + struct.pack("<BIII", flags, unpacked, stored, offset)
        total = len(data) + len(index) + 8
        return data + index + struct.pack("<II", len(index), total)

    put("game/fallout2_dat", "raw_and_zlib", fallout2())

    # FTL: a count, the offsets of the entries, and each entry (size, name length, name, bytes)
    entries = [("data/a.xml", b"<a/>"), ("img/b.png", bytes(range(50)))]
    offsets_end = 4 + 4 * 3
    out, blobs, position = struct.pack("<I", 3), b"", offsets_end
    for name, data in entries:
        out += struct.pack("<I", position + len(blobs))
        blobs += struct.pack("<II", len(data), len(name)) + name.encode() + data
    out += struct.pack("<I", 0)  # an entry that is not used
    put("game/ftl_dat", "two_files", out + blobs)

    # Heaps .pak: a header with a tree of directories and files, then the data
    def heaps_entry(name, body, is_dir):
        return bytes([len(name)]) + name.encode() + bytes([1 if is_dir else 0]) + body

    def heaps_file(offset, data):
        return struct.pack("<II", offset, len(data)) + zlib.crc32(data).to_bytes(4, "little")

    first, second = b"hello heaps\n", bytes(range(40))
    tree = heaps_entry(
        "",
        struct.pack("<I", 2)
        + heaps_entry("readme.txt", heaps_file(0, first), False)
        + heaps_entry("sub", struct.pack("<I", 1) + heaps_entry("data.bin", heaps_file(len(first), second), False), True),
        True,
    )
    header_size = 16 + len(tree)
    put("game/heaps_pak", "tree", b"PAK" + bytes([0]) + struct.pack("<II", header_size, len(first) + len(second)) + tree + b"DATA" + first + second)

    # Heroes of Might and Magic: an AGG archive (entries, the files, the names) and a BMP (a size and 8-bit pixels)
    files = [("ONE.BMP", b"\x00\x00\x02\x00\x02\x00" + bytes([1, 2, 3, 4])), ("TWO.TXT", b"text of two")]
    entries_size = 2 + 14 * len(files)
    out, blobs, position = struct.pack("<H", len(files)), b"", entries_size
    for index, (name, blob) in enumerate(files):
        out += struct.pack("<HIII", 0x1000 + index, position + len(blobs), len(blob), len(blob))
        blobs += blob
    names = b"".join(name.encode().ljust(15, b"\0") for name, _ in files)
    put("game/heroes_of_might_and_magic_agg", "two_files", out + blobs + names)
    put("game/heroes_of_might_and_magic_bmp", "four_pixels", files[0][1])

    # Gran Turismo VOL: a header with the offsets (sector 0x800 each), a directory of entries and the files
    offsets = [0, 0x800, 0x1000, 0x1800, 0x2000]
    header = b"GTFS\0\0\0\0" + struct.pack("<HH", len(offsets), 2) + bytes(4) + b"".join(struct.pack("<I", o) for o in offsets)
    directory = (
        struct.pack("<IHB", 1700000000, 2, 0) + b"first.dat".ljust(25, b"\0")
        + struct.pack("<IHB", 1700000001, 3, 0x80) + b"second.dat".ljust(25, b"\0")
    )
    vol = header.ljust(0x800, b"\0") + directory.ljust(0x800, b"\0") + b"first file".ljust(0x800, b"\0") + bytes(range(100)).ljust(0x800, b"\0")
    put("game/gran_turismo_vol", "two_files", vol)


# ── filesystem ───────────────────────────────────────────────────────────────

def chs(lba):
    """CHS of a block for a disk of 255 heads and 63 sectors, as an MBR holds it."""
    cylinder, rest = divmod(lba, 255 * 63)
    head, sector = divmod(rest, 63)
    sector += 1
    cylinder = min(cylinder, 1023)
    return bytes([head, (sector & 0x3F) | ((cylinder >> 2) & 0xC0), cylinder & 0xFF])


def mbr_entry(status, kind, start, count):
    return bytes([status]) + chs(start) + bytes([kind]) + chs(start + count - 1) + struct.pack("<II", start, count)


def filesystem():
    import zlib
    import uuid

    # MBR: boot code, four entries (two used), and the signature
    boot = bytes([0xEB, 0x3C, 0x90]) + b"doe sample".ljust(443, b"\0")
    entries = mbr_entry(0x80, 0x0C, 2048, 20480) + mbr_entry(0x00, 0x83, 22528, 40960) + bytes(32)
    put("filesystem/mbr_partition_table", "two_partitions", boot + entries + b"\x55\xaa")

    # GPT (UEFI specification): a protective MBR, the header at LBA 1, 128 entries of 128 bytes at LBA 2, the same at the end
    sectors = 80
    entries_sectors = 32
    types = [uuid.UUID("c12a7328-f81f-11d2-ba4b-00a0c93ec93b"), uuid.UUID("0fc63daf-8483-4772-8e79-3d69d8477de4")]
    entry_blob = b""
    for index, (kind, name) in enumerate(zip(types, ("EFI System", "Linux data"))):
        first = 34 + index * 10
        entry_blob += kind.bytes_le + uuid.UUID(int=0x1000 + index).bytes_le + struct.pack("<QQQ", first, first + 9, 0) + name.encode("utf-16-le").ljust(72, b"\0")
    entry_blob = entry_blob.ljust(128 * 128, b"\0")

    def header(current, backup, entries_lba):
        head = b"EFI PART" + struct.pack("<IIII", 0x00010000, 92, 0, 0)
        head += struct.pack("<QQQQ", current, backup, 34, sectors - 34)
        head += uuid.UUID(int=0xD0E).bytes_le + struct.pack("<QIII", entries_lba, 128, 128, zlib.crc32(entry_blob))
        crc = zlib.crc32(head)
        return (head[:16] + struct.pack("<I", crc) + head[20:]).ljust(512, b"\0")

    protective = bytes(446) + mbr_entry(0, 0xEE, 1, sectors - 1) + bytes(48) + b"\x55\xaa"
    disk = (
        protective + header(1, sectors - 1, 2) + entry_blob + bytes((sectors - 34 - 33) * 512)
        + entry_blob + header(sectors - 1, 1, sectors - 33)
    )
    assert len(disk) == sectors * 512, len(disk)
    put("filesystem/gpt_partition_table", "two_partitions", disk)

    # Apple partition map: the driver descriptor at block 0, the map from block 1
    def apm_entry(count, start, size, name, kind, status):
        return (
            b"PM\0\0"
            + struct.pack(">III", count, start, size)
            + name.encode().ljust(32, b"\0")
            + kind.encode().ljust(32, b"\0")
            + struct.pack(">IIIIII", 0, size, status, 0, 0, 0)  # data start (in the partition) and size, status, boot code start and size, load address
            + bytes(4) + struct.pack(">I", 0) + bytes(4) + struct.pack(">I", 0)  # reserved, entry point, reserved, checksum
            + b"68000".ljust(16, b"\0")
        ).ljust(512, b"\0")

    blocks = 16
    map_entries = [
        apm_entry(3, 1, 3, "Apple", "Apple_partition_map", 0x3),
        apm_entry(3, 4, 8, "disk image", "Apple_HFS", 0x40000033),
        apm_entry(3, 12, 4, "Extra", "Apple_Free", 0x0),
    ]
    image = b"ER" + bytes(510) + b"".join(map_entries) + bytes((blocks - 4) * 512)
    put("filesystem/apm_partition_table", "three_entries", image)

    # ZX Spectrum tape: blocks of (length, flag, data, checksum); a program and its data, then code
    def tap_block(flag, data):
        payload = bytes([flag]) + data
        check = 0
        for byte in payload:
            check ^= byte
        return struct.pack("<H", len(payload) + 1) + payload + bytes([check])

    def tap_header(kind, name, length, param1, param2):
        return tap_block(0, bytes([kind]) + name.encode().ljust(10, b" ") + struct.pack("<HHH", length, param1, param2))

    basic = b"\x00\x0a\x0d\x00\xf5 \"Hello\"\x0d"
    tape = tap_header(0, "hello", len(basic), 10, len(basic)) + tap_block(0xFF, basic)
    code = bytes(range(16))
    tape += tap_header(3, "codeblock", len(code), 32768, 0) + tap_block(0xFF, code)
    tape += tap_block(0xFF, b"headerless")
    put("filesystem/zx_spectrum_tap", "program_and_code", tape)


# ── firmware ─────────────────────────────────────────────────────────────────

def firmware():
    import zlib

    # UF2 (Microsoft's USB Flashing Format): blocks of 512 bytes, each with 256 bytes of a program
    blocks = 3
    out = b""
    for number in range(blocks):
        payload = bytes((number * 16 + i) & 0xFF for i in range(256))
        out += struct.pack("<IIIIIIII", 0x0A324655, 0x9E5D5157, 0x2000, 0x10000000 + number * 256, 256, number, blocks, 0xE48BFF56)
        out += payload.ljust(476, b"\0") + struct.pack("<I", 0x0AB16F30)
    put("firmware/uf2", "rp2040", out)

    # iNES: a header of 16 bytes, one bank of PRG ROM (16 KiB), one of CHR ROM (8 KiB), and a title
    header = b"NES\x1a" + bytes([1, 1, 0x01, 0x00, 0, 0, 0]) + bytes(5)
    prg = bytes((i * 7) & 0xFF for i in range(16384))
    chr_rom = bytes((i * 3) & 0xFF for i in range(8192))
    put("firmware/ines", "mapper0", header + prg + chr_rom + b"DOE TEST ROM")

    # U-Boot legacy image: a header of 64 bytes (with the CRC-32 of the header, computed with that field zero), then the data
    data = b"\xde\xad\xbe\xef" * 64
    fields = struct.pack(">IIIIIIIBBBB32s", 0x27051956, 0, 1700000000, len(data), 0x80008000, 0x80008000, zlib.crc32(data), 5, 2, 2, 0, b"Linux-doe-sample")
    header_crc = zlib.crc32(fields)
    put("firmware/uimage", "kernel", fields[:4] + struct.pack(">I", header_crc) + fields[8:] + data)

    # Broadcom TRX: a header of 28 bytes (the offsets of up to three parts), the parts, and an ASUS tail
    parts = [b"loader " * 10, b"kernel " * 40, b"rootfs " * 100]
    tail = bytes([1, 2, 3, 4]) + b"RT-AC68U".ljust(12, b"\0") + struct.pack("<8H", 0, 0, 1, 1, 2, 2, 3, 3) + bytes(32)
    offsets, position = [], 28
    for part in parts:
        offsets.append(position)
        position += len(part)
    offsets.append(0)
    body = b"".join(parts) + tail
    after_crc = struct.pack("<HH", 0, 1) + struct.pack("<4I", *offsets) + body
    length = 12 + len(after_crc)
    trx = b"HDR0" + struct.pack("<II", length, zlib.crc32(after_crc)) + after_crc
    put("firmware/broadcom_trx", "asus_tail", trx)


# ── hardware ─────────────────────────────────────────────────────────────────

def hardware():
    # Flattened device tree (Devicetree Specification, chapter 5): a header, the memory reservations,
    # the structure block and the strings block, as dtc writes them
    strings = bytearray()
    offsets = {}

    def string_offset(name):
        if name not in offsets:
            offsets[name] = len(strings)
            strings.extend(name.encode() + b"\0")
        return offsets[name]

    def pad4(blob):
        return blob + b"\0" * (-len(blob) % 4)

    def node(name, properties, children=()):
        out = struct.pack(">I", 1) + pad4(name.encode() + b"\0")
        for prop, value in properties:
            out += struct.pack(">III", 3, len(value), string_offset(prop)) + pad4(value)
        for child in children:
            out += child
        return out + struct.pack(">I", 2)

    def cells(*values):
        return b"".join(struct.pack(">I", v) for v in values)

    def text(*values):
        return b"".join(v.encode() + b"\0" for v in values)

    tree = node(
        "",
        [("#address-cells", cells(2)), ("#size-cells", cells(2)), ("compatible", text("doe,sample-board", "doe,generic")), ("model", text("Doe sample board"))],
        [
            node("chosen", [("bootargs", text("console=ttyS0,115200")), ("stdout-path", text("/serial@10000000"))]),
            node("memory@80000000", [("device_type", text("memory")), ("reg", cells(0, 0x80000000, 0, 0x10000000))]),
            node("cpus", [("#address-cells", cells(1)), ("#size-cells", cells(0))], [
                node("cpu@0", [("device_type", text("cpu")), ("reg", cells(0)), ("clock-frequency", cells(1000000000))]),
            ]),
            node("serial@10000000", [("compatible", text("ns16550a")), ("reg", cells(0, 0x10000000, 0, 0x100)), ("status", text("okay"))]),
        ],
    )
    structure = tree + struct.pack(">I", 9)
    reservations = struct.pack(">QQ", 0x7FF00000, 0x100000) + struct.pack(">QQ", 0, 0)
    header_size = 40
    off_rsv = header_size
    off_struct = off_rsv + len(reservations)
    off_strings = off_struct + len(structure)
    total = off_strings + len(strings)
    header = struct.pack(">10I", 0xD00DFEED, total, off_struct, off_strings, off_rsv, 17, 16, 0, len(strings), len(structure))
    put("hardware/dtb", "board", header + reservations + structure + bytes(strings))


# ── log ──────────────────────────────────────────────────────────────────────

def log():
    # AIX utmp: records of 0x2a0 bytes (big-endian)
    def aix(user, ident, device, pid, kind, when, host):
        return (
            user.ljust(256, b"\0") + ident.ljust(14, b"\0") + device.ljust(64, b"\0") + struct.pack(">QhqHH", pid, kind, when, 0, 0)
            + host.ljust(256, b"\0") + struct.pack(">i", 0) + bytes(8) + bytes(24)
        )

    out = aix(b"root", b"si", b"vty0", 1, 8, 1700000000, b"") + aix(b"doe", b"p0", b"pts/0", 4242, 7, 1700000100, b"host.example") + aix(b"", b"", b"", 0, 0, 0, b"")
    put("log/aix_utmp", "three_records", out)

    # sudo's timestamp file (/var/run/sudo/ts/USER): records of version 2 (and one of version 1)
    def flags(any_uid=0, disabled=0):
        # two bytes: the first holds six reserved bits and then anyuid, disabled
        return bytes([(any_uid << 1) | disabled, 0])

    def record_v2(kind, uid, sid, start, now, extra=None):
        body = struct.pack("<H", kind) + flags() + struct.pack("<II", uid, sid) + struct.pack("<qq", *start) + struct.pack("<qq", *now)
        if extra is not None:
            body += struct.pack("<I", extra)
        return struct.pack("<HH", 2, len(body) + 4) + body

    def record_v1(kind, uid, sid, now, extra=None):
        body = struct.pack("<H", kind) + flags(any_uid=1) + struct.pack("<II", uid, sid) + struct.pack("<qq", *now)
        if extra is not None:
            body += struct.pack("<I", extra)
        return struct.pack("<HH", 1, len(body) + 4) + body

    out = record_v2(1, 1000, 1, (1700000000, 0), (1700000300, 500))
    out += record_v2(2, 1000, 2000, (1700000000, 1), (1700000400, 2), extra=0x8800)
    out += record_v2(3, 1000, 2000, (1700000000, 1), (1700000400, 2), extra=4321)
    out += record_v1(2, 0, 7, (1700000500, 3), extra=0x8801)
    put("log/sudoers_ts", "records", out)


# ── archive ──────────────────────────────────────────────────────────────────

def archive():
    # Android sparse image (system/core/libsparse): a header, then chunks of four kinds
    block = 4096
    chunks = (
        struct.pack("<HHII", 0xCAC1, 0, 1, 12 + block) + bytes((i * 5) & 0xFF for i in range(block))  # raw
        + struct.pack("<HHII", 0xCAC2, 0, 2, 12 + 4) + struct.pack("<I", 0xDEADBEEF)  # fill
        + struct.pack("<HHII", 0xCAC3, 0, 3, 12)  # don't care
        + struct.pack("<HHII", 0xCAC4, 0, 0, 12 + 4) + struct.pack("<I", 0x12345678)  # crc32
    )
    header = struct.pack("<IHHHHIIII", 0xED26FF3A, 1, 0, 28, 12, block, 6, 4, 0)
    put("archive/android_sparse", "four_chunks", header + chunks)

    # Mozilla Archive (MAR): a header, a signature, a section of product information, the files, and the index at the end
    files = [("update.manifest", b'type "partial"\n'), ("bin/app", bytes(range(100)))]
    signature = struct.pack(">II", 1, 16) + bytes(range(16))
    product = b"release".ljust(64, b"\0") + b"1.2.3".ljust(32, b"\0")
    section = struct.pack(">II", 8 + len(product), 1) + product
    head_size = 4 + 4 + 8 + 4 + len(signature) + 4 + len(section)
    body = b""
    index = b""
    for name, data in files:
        index += struct.pack(">III", head_size + len(body), len(data), 0o644) + name.encode() + b"\0"
        body += data
    index_offset = head_size + len(body)
    total = index_offset + 4 + len(index)
    mar = b"MAR1" + struct.pack(">IQI", index_offset, total, 1) + signature + struct.pack(">I", 1) + section + body + struct.pack(">I", len(index)) + index
    assert len(mar) == total
    put("archive/mozilla_mar", "update", mar)

    # Android boot loader images of three makers: a header, then the images
    asus = b"BOOTLDR!" + struct.pack("<HHI", 2, 0, 0)
    for chunk_id, body in [(b"IFWI!!!!", bytes(range(30))), (b"DROIDBT!", b"droidboot"), (b"SPLASHS!", bytes(8))]:
        asus += chunk_id + struct.pack("<IBBBB", len(body), 1, 0, 0, 0) + body
    put("archive/android_bootldr_asus", "three_images", asus)

    names = [("sbl1", b"first boot loader"), ("aboot", bytes(range(40)))]
    huawei_meta_size = 76
    image_header_size = 80 * 3
    first_body = huawei_meta_size + image_header_size
    huawei = b"\x3c\xd6\x1a\xce" + struct.pack("<HH", 1, 2) + b"HUAWEI-1.0".ljust(64, b"\0") + struct.pack("<HH", huawei_meta_size, image_header_size)
    position = first_body
    for name, body in names:
        huawei += name.encode().ljust(72, b"\0") + struct.pack("<II", position, len(body))
        position += len(body)
    huawei += bytes(80)  # an entry that is not used
    put("archive/android_bootldr_huawei", "two_images", huawei + b"".join(body for _, body in names))

    qcom_images = [("kernel", bytes(range(100))), ("modem", b"modem firmware")]
    qcom_start = 16 + 68 * len(qcom_images)
    qcom = b"BOOTLDR!" + struct.pack("<III", len(qcom_images), qcom_start, sum(len(b) for _, b in qcom_images))
    for name, body in qcom_images:
        qcom += name.encode().ljust(64, b"\0") + struct.pack("<I", len(body))
    put("archive/android_bootldr_qcom", "two_images", qcom + b"".join(body for _, body in qcom_images))

    # Android device tree table (dtbo.img): a header, the entries, and the device trees (the one of the dtb sample, twice)
    tree = (ROOT / "hardware/dtb/board.bin").read_bytes()
    count = 2
    first_tree = 32 + 32 * count
    entries = b"".join(struct.pack(">IIIIIIII", len(tree), first_tree + i * len(tree), 100 + i, 0, 0, 0, 0, 0) for i in range(count))
    dto = b"\xd7\xb7\xab\x1e" + struct.pack(">IIIIIII", first_tree + count * len(tree), 32, 32, count, 32, 4096, 0)
    put("archive/android_dto", "two_trees", dto + entries + tree * count)

    # Android boot image (header version 2): a page of 2048 bytes with the header, then the kernel, ramdisk and device tree, each padded to a page
    page = 2048
    kernel, ramdisk, second, dtb = bytes(range(200)) * 3, b"ramdisk " * 40, b"", tree
    os_version = (10 << 25) | (24 << 4) | 5  # 10.0.0, May 2024
    header = b"ANDROID!" + struct.pack(
        "<10I", len(kernel), 0x10008000, len(ramdisk), 0x11000000, len(second), 0x10F00000, 0x10000100, page, 2, os_version
    )
    header += b"doe".ljust(16, b"\0") + b"console=ttyS0".ljust(512, b"\0") + bytes(32) + bytes(1024)
    header += struct.pack("<IQ", 0, 0) + struct.pack("<I", 1660) + struct.pack("<IQ", len(dtb), 0x11F00000)

    def paged(blob):
        return blob.ljust((len(blob) + page - 1) // page * page, b"\0")

    put("archive/android_img", "boot_v2", paged(header) + paged(kernel) + paged(ramdisk) + paged(dtb))

    # Respack (a resource pack of a game): "RS", 8 bytes, the size of the JSON, its MD5 in hex, the JSON
    import hashlib

    document = b'{"name": "doe", "version": 1, "files": ["a.png", "b.ogg"]}'
    put("archive/respack", "pack", b"RS" + bytes(8) + struct.pack("<I", len(document)) + hashlib.md5(document).hexdigest().encode() + document)


# ── executable ───────────────────────────────────────────────────────────────

def executable():
    import zlib

    # DOS MZ executable: a header of two paragraphs with a relocation, and a program that exits (mov ax, 4C00h; int 21h)
    code = bytes([0xB8, 0x00, 0x4C, 0xCD, 0x21]) + bytes(3)
    header_size = 32
    total = header_size + len(code)
    header = b"MZ" + struct.pack("<HHHHHHHHHHHHH", total % 512, (total + 511) // 512, 1, 2, 0, 0xFFFF, 0, 0x100, 0, 0, 0, 0x1C, 0)
    header += struct.pack("<HH", 1, 0)  # the one relocation: offset 1 in segment 0
    put("executable/dos_mz", "exit", header.ljust(header_size, b"\0") + code)

    # fat Mach-O (what lipo writes): a header, an entry for each architecture, and the objects at aligned offsets,
    # made of the two objects that clang wrote for the Mach-O samples
    objects = [
        (0x01000007, 3, (ROOT / "executable/mach_o/object_x86_64.bin").read_bytes()),
        (0x0100000C, 0, (ROOT / "executable/mach_o/object_arm64.bin").read_bytes()),
    ]
    align = 12
    out = struct.pack(">II", 0xCAFEBABE, len(objects))
    offset = 4096
    entries = b""
    for cpu, sub, blob in objects:
        entries += struct.pack(">IIIII", cpu, sub, offset, len(blob), align)
        offset += (len(blob) + 4095) // 4096 * 4096
    out += entries
    position = 4096
    out = out.ljust(position, b"\0")
    for cpu, sub, blob in objects:
        out += blob.ljust((len(blob) + 4095) // 4096 * 4096, b"\0")
    put("executable/mach_o_fat", "two_architectures", out)

    # SWF (Adobe Flash): a header, a rectangle, the frame rate and count, and tags
    def rect(x_max, y_max):
        bits = max(x_max, y_max).bit_length() + 1
        value = bits
        for field in (0, x_max, 0, y_max):
            value = (value << bits) | field
        total_bits = 5 + 4 * bits
        padded = value << (-total_bits % 8)
        return padded.to_bytes((total_bits + 7) // 8, "big")

    def tag(code, body):
        if len(body) < 0x3F:
            return struct.pack("<H", code << 6 | len(body)) + body
        return struct.pack("<Hi", code << 6 | 0x3F, len(body)) + body

    abc = struct.pack("<I", 1) + b"doe\0" + bytes(range(70))
    symbols = struct.pack("<H", 2) + struct.pack("<H", 1) + b"Main\0" + struct.pack("<H", 2) + b"Other\0"
    tags = (
        tag(69, struct.pack("<I", 0x08))  # file attributes: uses the network
        + tag(9, bytes([255, 128, 0]))  # background colour
        + tag(65, struct.pack("<HH", 256, 15))  # script limits
        + tag(82, abc)
        + tag(76, symbols)
        + tag(1, b"")  # show frame
        + tag(0, b"")  # end
    )
    body = rect(11000, 8000) + struct.pack("<HH", 0x1800, 1) + tags
    plain = b"FWS" + bytes([10]) + struct.pack("<I", 8 + len(body)) + body
    put("executable/swf", "plain", plain)
    put("executable/swf", "zlib", b"CWS" + bytes([10]) + struct.pack("<I", 8 + len(body)) + zlib.compress(body))

    # UEFI Terse Executable: a header of 40 bytes, section headers, and the sections' bytes
    stripped = 0x1B8
    section = b".text".ljust(8, b"\0") + struct.pack("<IIIIIIHHI", 64, 0x1000, 64, stripped + 40, 0, 0, 0, 0, 0x60000020)
    te = b"VZ" + struct.pack("<HBBHIIQ", 0x8664, 1, 10, stripped, 0x1000, 0x1000, 0x100000000) + struct.pack("<IIII", 0, 0, 0x2000, 28)
    assert len(te) == 40, len(te)
    put("executable/uefi_te", "efi_application", te + section + bytes((i * 3) & 0xFF for i in range(64)))

    # Android nanoapp header (the CHRE nanoapp, 40 bytes)
    put(
        "executable/android_nanoapp_header",
        "signed",
        struct.pack("<I4sQIIQBB6s", 1, b"NANO", 0x0001020304050607, 0x00010203, 0x5, 0x0000476F6F676C00, 1, 5, bytes(6)),
    )


# ── windows and other small ones ─────────────────────────────────────────────

def small():
    # Windows shell item list (ITEMIDLIST of a shortcut): a root folder, a drive, a directory and a file, ended by a zero length
    def item(code, body):
        data = bytes([code]) + body
        return struct.pack("<H", len(data) + 2) + data

    computer = bytes.fromhex("e04fd020ea3a6910a2d808002b30309d")  # My Computer
    shell_items = (
        item(0x1F, bytes([0x50]) + computer)
        + item(0x2F, b"C:\\" + bytes(19))
        + item(0x31, bytes([0]) + struct.pack("<IIH", 0, 0x58A31B20, 0x10) + b"Users\0")
        + item(0x32, bytes([0]) + struct.pack("<IIH", 1234, 0x58A31B21, 0x20) + b"notes.txt\0")
        + struct.pack("<H", 0)
    )
    put("windows/windows_shell_items", "path", shell_items)

    # Windows shortcut (.lnk, MS-SHLLINK): the header, the item list of the target, the link info and the strings
    flags = 0x01 | 0x02 | 0x04 | 0x08 | 0x10 | 0x20 | 0x40 | 0x80
    header = struct.pack("<I", 0x4C) + bytes([0x01, 0x14, 0x02, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46])
    header += struct.pack("<IIQQQIiIH", flags, 0x20, 132000000000000000, 132000000000000000, 132000000000000000, 1234, 0, 1, 0) + bytes(10)
    assert len(header) == 76, len(header)
    id_list = shell_items
    # the volume label is in UTF-16 (offset 0x14): the reference runtime cannot decode the code page 437 of the ANSI one
    label = "Windows\0".encode("utf-16-le")
    volume = struct.pack("<IIIII", 20 + len(label), 3, 0x1234ABCD, 0x14, 0x14) + label
    local_path = b"C:\\Users\\notes.txt\0"
    suffix = b"\0"
    link_info_header = 28
    link_info = struct.pack(
        "<IIIIIII", link_info_header + len(volume) + len(local_path) + len(suffix), link_info_header, 1,
        link_info_header, link_info_header + len(volume), 0, link_info_header + len(volume) + len(local_path),
    ) + volume + local_path + suffix

    def lnk_string(text):
        return struct.pack("<H", len(text)) + text.encode("utf-16-le")

    lnk = (
        header + struct.pack("<H", len(id_list)) + id_list + link_info
        + lnk_string("A shortcut") + lnk_string("..\\notes.txt") + lnk_string("C:\\Users") + lnk_string("--open") + lnk_string("%SystemRoot%\\system32\\SHELL32.dll")
    )
    put("windows/windows_lnk_file", "notes", lnk)

    # Windows metafile (placeable): the header with its checksum, the metafile header, and the records (sizes in 16-bit words)
    def wmf_record(function, params=b""):
        return struct.pack("<IH", 3 + len(params) // 2, function) + params

    def points(*pairs):
        return struct.pack("<h", len(pairs)) + b"".join(struct.pack("<hh", x, y) for x, y in pairs)

    records = (
        wmf_record(0x020C, struct.pack("<hh", 200, 300))  # window extent
        + wmf_record(0x020B, struct.pack("<hh", 0, 0))  # window origin
        + wmf_record(0x0102, struct.pack("<H", 1))  # transparent background
        + wmf_record(0x0201, bytes([255, 128, 0, 0]))  # background colour
        + wmf_record(0x0106, struct.pack("<H", 2))
        + wmf_record(0x0104, struct.pack("<H", 13))
        + wmf_record(0x0324, points((10, 10), (190, 20), (100, 280)))  # polygon
        + wmf_record(0x0325, points((0, 0), (200, 300)))  # polyline
    )
    eof = wmf_record(0x0000)
    total_words = 9 + (len(records) + len(eof)) // 2
    placeable = b"\xd7\xcd\xc6\x9a" + struct.pack("<Hhhhh", 0, 0, 0, 200, 300) + struct.pack("<H", 1440) + bytes(4)
    checksum = 0
    for (word,) in struct.iter_unpack("<H", placeable):
        checksum ^= word
    placeable += struct.pack("<H", checksum)
    meta_header = struct.pack("<HHHIHIH", 1, 9, 0x0300, total_words, 1, 8, 0)
    put("image/wmf", "polygon", placeable + meta_header + records + eof)

    # GENMIDI.OP2 (the OPL2 instrument patches of Doom): 175 instruments of 36 bytes and 175 names of 32
    def op_settings(i, j):
        return bytes([(i * 3 + j) & 0xFF, (i * 5 + j) & 0xFF, (i * 7 + j) & 0xFF, j & 3, (i + j) & 0xFF, (i * 2 + j) & 0x3F])

    def patch(i, j):
        return op_settings(i, j) + bytes([(i + j) & 0xF]) + op_settings(i, j + 1) + b"\0" + struct.pack("<h", -12 + (i % 24))

    op2 = b"#OPL_II#"
    for i in range(175):
        op2 += struct.pack("<HBB", i & 1, 128 + (i % 5), 0) + patch(i, 0) + patch(i, 1)
    for i in range(175):
        op2 += f"Instrument {i}".encode().ljust(32, b"\0")
    put("media/genmidi_op2", "patches", op2)

    # EDID 1.4 (VESA, 128 bytes): the header, the vendor "DOE", the colour coordinates, the timings and a checksum
    def edid_chroma(points):
        values = [round(v * 1024) for point in points for v in point]  # rx ry gx gy bx by wx wy
        low = 0
        for v in values:
            low = (low << 2) | (v & 3)
        return struct.pack(">H", low) + bytes(v >> 2 for v in values)

    vendor = ((ord("D") - 64) << 10) | ((ord("O") - 64) << 5) | (ord("E") - 64)
    edid = bytes([0, 255, 255, 255, 255, 255, 255, 0]) + struct.pack(">H", vendor) + struct.pack("<HI", 0x1234, 0x01020304)
    edid += bytes([12, 2024 - 1990, 1, 4, 0xA5, 53, 30, 120, 0x2E])  # week, year, version 1.4, digital input, 53x30 cm, gamma 2.2, features
    edid += edid_chroma([(0.640, 0.330), (0.300, 0.600), (0.150, 0.060), (0.3127, 0.3290)])
    edid += bytes([0x21, 0x08, 0x00])  # established timings: 640x480 at 60 and 75 Hz, 800x600 at 60 Hz (bit 0x20 of byte 0 is 640x480 60)
    edid += bytes([0xD1, 0xC0, 0x81, 0x80, 0xA9, 0xC0, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01])  # standard timings
    edid += bytes([0x02, 0x3A, 0x80, 0x18, 0x71, 0x38, 0x2D, 0x40, 0x58, 0x2C, 0x45, 0x00, 0x13, 0x2A, 0x21, 0x00, 0x00, 0x1E])  # 1920x1080 at 60 Hz
    edid += bytes([0, 0, 0, 0xFC, 0]) + b"DOE MONITOR\n " + bytes([0, 0, 0, 0xFD, 0, 50, 76, 30, 81, 20, 0, 10]) + b"    "[:0]
    edid = edid.ljust(126, b"\0") + bytes([0])
    edid += bytes([(-sum(edid)) & 0xFF])
    assert len(edid) == 128, len(edid)
    put("hardware/edid", "monitor", edid)

    # MIFARE Classic 1K dump: sixteen sectors of four blocks; block 0 has the UID, and each sector ends with a trailer.
    # No value block: doe inverts a u4 with ~ in 128 bits, the reference in 32 (#162).
    uid = bytes([0xDE, 0xAD, 0xBE, 0xEF])
    bcc = uid[0] ^ uid[1] ^ uid[2] ^ uid[3]
    dump = b""
    for sector in range(16):
        blocks = [bytes((sector * 16 + i) & 0xFF for i in range(16)) for _ in range(3)]
        if sector == 0:
            blocks[0] = uid + bytes([bcc, 0x08, 0x04, 0x00]) + bytes(8)
        dump += b"".join(blocks) + bytes([0xFF] * 6) + bytes([0xFF, 0x07, 0x80, 0x69]) + bytes([0xFF] * 6)
    put("hardware/mifare/mifare_classic", "one_k", dump)

    # Andes (MediaTek) Wi-Fi firmware: a header of 32 bytes, the instruction memory and the data memory
    ilm, dlm = bytes((i * 3) & 0xFF for i in range(300)), bytes((i * 5) & 0xFF for i in range(100))
    put(
        "firmware/andes_firmware",
        "image",
        struct.pack("<IIHHI", len(ilm), len(dlm), 0x0102, 0x0304, 0) + b"20240501120000\0\0" + ilm + dlm,
    )

    # Quake II model (.md2): a header of 68 bytes with the offsets, a skin name, texture coordinates, a triangle, two frames, GL commands
    verts, frames_n = 3, 2
    skins = [b"models/doe/skin.pcx".ljust(64, b"\0")]
    tex = b"".join(struct.pack("<HH", s, t) for s, t in [(0, 0), (63, 0), (32, 63)])
    triangles = struct.pack("<3H3H", 0, 1, 2, 0, 1, 2)
    frame_size = 40 + 4 * verts
    frames_blob = b""
    for number in range(frames_n):
        frames_blob += struct.pack("<3f3f", 0.1, 0.1, 0.1, -5.0, -5.0, 0.0) + f"stand{number:02d}".encode().ljust(16, b"\0")
        frames_blob += b"".join(bytes([20 * i + number, 10 * i, 40, i]) for i in range(verts))
    gl = struct.pack("<i", 3) + b"".join(struct.pack("<ffI", s, t, i) for i, (s, t) in enumerate([(0.0, 0.0), (1.0, 0.0), (0.5, 1.0)])) + struct.pack("<i", 0)
    offset = 68
    offsets = []
    for blob in (b"".join(skins), tex, triangles, frames_blob, gl):
        offsets.append(offset)
        offset += len(blob)
    md2 = b"IDP2" + struct.pack("<16I", 8, 64, 64, frame_size, 1, verts, 3, 1, len(gl) // 4, frames_n, *offsets, offset)
    put("game/quake2_md2", "triangle", md2 + b"".join(skins) + tex + triangles + frames_blob + gl)

    # Quake model (.mdl, version 6): a header, a skin, texture coordinates, a triangle, and two animation frames
    skin_w, skin_h, verts, tris, frames = 4, 4, 3, 1, 2
    mdl = b"IDPO" + struct.pack("<i", 6) + struct.pack("<3f3ff3f", 0.1, 0.1, 0.1, 0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 12.0)
    mdl += struct.pack("<iiiiiiiif", 1, skin_w, skin_h, verts, tris, frames, 0, 0, 1.0)
    mdl += struct.pack("<i", 0) + bytes((i * 16) & 0xFF for i in range(skin_w * skin_h))
    mdl += b"".join(struct.pack("<iii", 0, s, t) for s, t in [(0, 0), (3, 0), (1, 3)])
    mdl += struct.pack("<iiii", 1, 0, 1, 2)
    for number in range(frames):
        vertices = b"".join(bytes([10 * i + number, 5 * i, 20, i]) for i in range(verts))
        mdl += struct.pack("<i", 0) + bytes([0, 0, 0, 0, 255, 255, 255, 0]) + f"frame{number}".encode().ljust(16, b"\0") + vertices
    put("game/quake_mdl", "triangle", mdl)

    # FastTracker 2 module (.xm): the header, a pattern of 4 channels and 4 rows, and an instrument with one 8-bit sample
    channels, rows = 4, 4
    order_table = bytes([0]) + bytes(255)
    xm_header = struct.pack("<HHHHHHHH", 1, 0, channels, 1, 1, 1, 6, 125) + order_table
    xm = b"Extended Module: " + b"doe sample".ljust(20, b"\0") + b"\x1a" + b"made by hand".ljust(20, b"\0") + bytes([4, 1])
    xm += struct.pack("<I", 4 + len(xm_header)) + xm_header
    cells = b"".join(bytes([49 + c if r == 0 else 0, 1 if r == 0 else 0, 0x40 if r == 0 else 0, 0, 0]) for r in range(rows) for c in range(channels))
    xm += struct.pack("<IBHH", 9, 0, rows, len(cells)) + cells
    envelope = b"".join(struct.pack("<HH", 8 * i, 64 - 4 * i) for i in range(12))
    extra = struct.pack("<I", 40) + bytes(96) + envelope + envelope + bytes([4, 2, 1, 0, 3, 0, 0, 1]) + bytes([1, 0, 0, 0, 0, 0]) + struct.pack("<HH", 256, 0)
    instrument_header = b"sine".ljust(22, b"\0") + bytes([0]) + struct.pack("<H", 1) + extra
    instrument_header += bytes(20)  # the 20 bytes that the header size of real files holds past what the definition reads
    sample_data = bytes(((i * 8) & 0xFF) for i in range(32))
    sample_header = struct.pack("<III", len(sample_data), 0, 0) + bytes([64]) + struct.pack("<b", 0) + bytes([0, 128]) + struct.pack("<b", 0) + bytes([0]) + b"wave".ljust(22, b"\0")
    xm += struct.pack("<I", 4 + len(instrument_header)) + instrument_header + sample_header + sample_data
    put("media/tracker_modules/fasttracker_xm_module", "one_pattern", xm)

    # Scream Tracker 3 module (.s3m): the header, channels, orders, pointers in paragraphs of 16 bytes, an instrument, a pattern, a sample
    def paragraphs(blob):
        return blob + bytes(-len(blob) % 16)

    head_len = 64 + 32 + 4 + 2 + 2 + 32
    instrument_at = (head_len + 15) // 16
    sample = bytes((i * 4) & 0xFF for i in range(64))
    # a note on channel 0, the end of the row (a zero byte), a note with an effect on channel 1, the end of the row
    pattern_cells = bytes([0x20 | 0, 60, 1, 0]) + bytes([0xA0 | 1, 64, 1, 3, 0x0C, 0])
    pattern = struct.pack("<H", 2 + len(pattern_cells)) + pattern_cells
    pattern_at = instrument_at + 5
    sample_at = pattern_at + (len(pattern) + 15) // 16
    instrument = bytes([1]) + b"sample.raw".ljust(12, b"\0") + bytes([0]) + struct.pack("<H", sample_at) + struct.pack("<III", len(sample), 0, 0) + bytes([64, 0, 0, 0]) + struct.pack("<I", 8363) + bytes(12) + b"a sample".ljust(28, b"\0") + b"SCRS"
    assert len(instrument) == 80, len(instrument)
    s3m = b"doe module".ljust(28, b"\0") + b"\x1a" + bytes([16]) + bytes(2) + struct.pack("<HHHHHH", 4, 1, 1, 0, 0x1300, 2) + b"SCRM"
    s3m += bytes([64, 6, 125, 0x80 | 48, 0, 252]) + bytes(8) + struct.pack("<H", 0)
    assert len(s3m) == 64, len(s3m)
    s3m += bytes([0, 8] + [255] * 30)  # channels: the first is on the left, the second on the right, the rest are off
    s3m += bytes([0, 1, 254, 255])  # orders
    s3m += struct.pack("<H", instrument_at) + struct.pack("<H", pattern_at)
    s3m += bytes(0x20 | 3 if i < 2 else 0 for i in range(32))  # channel pans
    s3m = paragraphs(s3m) + paragraphs(instrument) + paragraphs(pattern) + sample
    put("media/tracker_modules/s3m", "one_pattern", s3m)

    # QuickTime / MP4 (ISO 14496-12): atoms of (size, type, body): a file type, the movie with a header and a track, free space, data
    def atom(kind, body):
        return struct.pack(">I", 8 + len(body)) + kind + body

    matrix = struct.pack(">9I", 0x10000, 0, 0, 0, 0x10000, 0, 0, 0, 0x40000000)
    mvhd = atom(b"mvhd", bytes(4) + struct.pack(">IIII", 3_700_000_000, 3_700_000_001, 1000, 5000) + struct.pack(">IH", 0x10000, 0x0100) + bytes(10) + matrix + bytes(24) + struct.pack(">I", 2))
    tkhd = atom(b"tkhd", bytes([0, 0, 0, 3]) + struct.pack(">IIIIIQ", 3_700_000_000, 3_700_000_001, 1, 0, 5000, 0) + struct.pack(">hhhH", 0, 0, 0x0100, 0) + matrix + struct.pack(">II", 320 << 16, 240 << 16))
    track = atom(b"trak", tkhd + atom(b"mdia", atom(b"minf", atom(b"stbl", b""))))
    movie = atom(b"moov", mvhd + track)
    put(
        "media/quicktime_mov",
        "movie",
        atom(b"ftyp", b"isom" + struct.pack(">I", 512) + b"isomiso2mp41") + movie + atom(b"free", bytes(16)) + atom(b"mdat", b"media data " * 8),
    )

    # Blender file (.blend): a header, blocks of 24 bytes of header and a body, the DNA of the structures, and ENDB
    def block(code, body, sdna=0, count=1):
        return code + struct.pack("<I", len(body)) + struct.pack("<Q", 0x7F0000001000 + len(body)) + struct.pack("<II", sdna, count) + body

    names = [b"x", b"y", b"*next", b"name[64]"]
    types = [b"int", b"char", b"Vec", b"Link"]
    lengths = [4, 1, 8, 8 + 64]

    def pad(blob):
        return blob + bytes(-len(blob) % 4)

    dna = b"SDNA" + b"NAME" + struct.pack("<I", len(names)) + pad(b"".join(n + b"\0" for n in names))
    dna += b"TYPE" + struct.pack("<I", len(types)) + pad(b"".join(t + b"\0" for t in types))
    dna += b"TLEN" + pad(struct.pack(f"<{len(lengths)}H", *lengths))
    structs = [(2, [(0, 0), (0, 1)]), (3, [(3, 2), (1, 3)])]  # Vec { int x; int y; } and Link { Link *next; char name[64]; }
    dna += b"STRC" + struct.pack("<I", len(structs))
    for idx_type, fields in structs:
        dna += struct.pack("<HH", idx_type, len(fields)) + b"".join(struct.pack("<HH", t, n) for t, n in fields)
    blend = b"BLENDER-v280" + block(b"REND", bytes(16)) + block(b"DATA", struct.pack("<ii", 3, 4), sdna=1, count=1)
    blend += block(b"DNA1", dna) + block(b"ENDB", b"")
    put("media/blender_blend", "vec", blend)

    # MCAP (a log of messages, mcap.dev): the magic, a header, a schema, a channel, three messages, the end of the data,
    # and a footer without a summary
    def mcap_str(text):
        return struct.pack("<I", len(text)) + text.encode()

    def mcap_record(op, body):
        return bytes([op]) + struct.pack("<Q", len(body)) + body

    magic = b"\x89MCAP0\r\n"
    schema_body = struct.pack("<H", 1) + mcap_str("doe/Point") + mcap_str("jsonschema")
    schema_data = b'{"type":"object","properties":{"x":{"type":"number"}}}'
    schema_body += struct.pack("<I", len(schema_data)) + schema_data
    metadata = mcap_str("frame") + mcap_str("base")
    channel_body = struct.pack("<HH", 1, 1) + mcap_str("/points") + mcap_str("json") + struct.pack("<I", len(metadata)) + metadata
    out = magic + mcap_record(0x01, mcap_str("") + mcap_str("doe samples"))
    out += mcap_record(0x03, schema_body) + mcap_record(0x04, channel_body)
    for sequence in range(3):
        out += mcap_record(0x05, struct.pack("<HIQQ", 1, sequence, 1_700_000_000_000_000_000 + sequence, 1_700_000_000_000_000_000 + sequence) + b'{"x":%d}' % sequence)
    out += mcap_record(0x0F, struct.pack("<I", 0))
    out += mcap_record(0x02, struct.pack("<QQI", 0, 0, 0)) + magic
    put("log/mcap", "three_messages", out)

    # hashcat .restore: the version, the working directory, positions, and the command line (an argument on each line)
    arguments = [b"hashcat", b"-m", b"0", b"hashes.txt", b"words.txt"]
    restore = struct.pack("<I", 0x0000012C) + b"/home/user/work".ljust(256, b"\0") + struct.pack("<II", 1, 0) + bytes(4)
    restore += struct.pack("<QI", 123456, len(arguments)) + bytes(12) + b"".join(a + b"\n" for a in arguments)
    put("log/hashcat_restore", "session", restore)

    # 6502 machine code: fill a screen with spaces, call a routine, add, store and loop (opcodes of the MOS 6502)
    program = bytes(
        [0xA2, 0x00, 0xA9, 0x20, 0x9D, 0x00, 0x04, 0xE8, 0xD0, 0xF8]  # LDX #0; LDA #$20; STA $0400,X; INX; BNE
        + [0x20, 0x20, 0x08]  # JSR $0820
        + [0x18, 0xA5, 0x10, 0x69, 0x05, 0x85, 0x10]  # CLC; LDA $10; ADC #5; STA $10
        + [0xA0, 0x03, 0x88, 0xD0, 0xFD]  # LDY #3; DEY; BNE
        + [0x4C, 0x00, 0x08, 0x60]  # JMP $0800; RTS
    )
    put("machine_code/code_6502", "fill_screen", program)

    # cramfs: a superblock with the root inode, and directories of inodes (a name is padded to 4 bytes). Only
    # directories: the definition reads every inode as a file and a link as well, which only a directory survives.
    import zlib

    def inode(mode, size, name, offset):
        padded = name.encode().ljust((len(name) + 3) // 4 * 4, b"\0")
        return struct.pack("<HHII", mode, 0, size, ((offset // 4) << 6) | (len(padded) // 4)) + padded

    dir_mode = 0o40755
    lib = inode(dir_mode, 0, "lib", 0)
    etc = inode(dir_mode, 0, "etc", 0)
    usr_offset = 76 + len(etc) + 16
    usr = inode(dir_mode, len(lib), "usr", 108)
    assert usr_offset == 108
    root_data = etc + usr
    superblock = (
        struct.pack("<IIII", 0x28CD3D45, 76 + len(root_data) + len(lib), 0, 0) + b"Compressed ROMFS"
        + struct.pack("<IIII", 0, 1, 0, 4) + b"doe".ljust(16, b"\0")
        + inode(dir_mode, len(root_data), "", 76)
    )
    image = superblock + root_data + lib
    image = image[:32] + struct.pack("<I", zlib.crc32(image[:32] + bytes(4) + image[36:])) + image[36:]
    put("filesystem/cramfs", "empty_directories", image)

    # Amlogic eMMC partition table: "MPT", a version, the number of partitions, a checksum, and 40 bytes for each
    def partition(name, size, offset, flags):
        return name.encode().ljust(16, b"\0") + struct.pack("<QQ", size, offset) + struct.pack("<I", flags) + bytes(4)

    parts = [("bootloader", 4 << 20, 0, 1), ("reserved", 64 << 20, 36 << 20, 1), ("cache", 512 << 20, 108 << 20, 2), ("data", 4 << 30, 636 << 20, 4)]
    table = b"".join(partition(*p) for p in parts)
    put("filesystem/amlogic_emmc_partitions", "four", b"MPT\0" + b"01.00.00".ljust(12, b"\0") + struct.pack("<iI", len(parts), sum(table) & 0xFFFFFFFF) + table)


# ── scientific, geospatial, database, security ──────────────────────────────

def others():
    import hashlib
    import zlib

    # Avantes spectrometer (.roh60): floats, the pixel range, the spectrum, and three settings
    first, last = 2.0, 12.0
    values = (
        [0.0, 200.5, 0.0123, -1e-5, 1e-9, 0.0]  # unknown, the wavelength intercept and the four coefficients
        + [0.0] * 9 + [first, last] + [0.0] * 4
        + [float(1000 + 10 * i) for i in range(int(last) - int(first) - 1)]
        + [25.0, 8.0, 3.0]
    )
    put("scientific/spectroscopy/avantes_roh60", "ten_pixels", struct.pack(f"<{len(values)}f", *values))

    # Shapefile index (.shx): a header of 100 bytes and a record of (offset, length) for each shape, in 16-bit words
    records = [(50, 10), (60, 12), (72, 14)]
    shx = struct.pack(">I", 0x0000270A) + bytes(20) + struct.pack(">i", 50 + 4 * len(records)) + struct.pack("<ii", 1000, 3)
    shx += struct.pack("<8d", 0.0, 10.0, 0.0, 5.0, 0.0, 0.0, 0.0, 0.0)  # as the ESRI specification has them: little-endian
    put("geospatial/shapefile_index", "three_shapes", shx + b"".join(struct.pack(">ii", o, n) for o, n in records))

    # InfluxDB TSM file: a header, blocks (CRC-32 and data), an index of the keys, and a footer with the offset of the index
    blocks = [(1_000_000_000, 2_000_000_000, bytes(range(20))), (2_000_000_001, 3_000_000_000, bytes(range(30)))]
    out = b"\x16\xd1\x16\xd1" + bytes([1])
    index_entries = b""
    for low, high, data in blocks:
        index_entries += struct.pack(">QQQI", low, high, len(out), 4 + len(data))
        out += struct.pack(">I", zlib.crc32(data)) + data
    key = b"cpu,host=a#!~#usage"
    index_offset = len(out)
    out += struct.pack(">H", len(key)) + key + bytes([0]) + struct.pack(">H", len(blocks)) + index_entries
    put("database/tsm", "one_key", out + struct.pack(">Q", index_offset))

    # gettext message catalog (.mo) with the hash table of msgfmt, in both byte orders
    def hashpjw(text):
        value = 0
        for byte in text:
            value = ((value << 4) + byte) & 0xFFFFFFFF
            top = value & 0xF0000000
            if top:
                value ^= top >> 24
                value ^= top
        return value

    def gettext_mo(order):
        pairs = sorted([(b"", b"Project-Id-Version: doe\nContent-Type: text/plain; charset=UTF-8\n"), (b"Hello", b"Bonjour"), (b"Goodbye", b"Au revoir"), (b"Yes", b"Oui")])
        n = len(pairs)
        size = 5
        while size < 4 * n // 3 or any(size % d == 0 for d in range(2, int(size**0.5) + 1)):
            size += 1
        table = [0] * size
        for index, (original, _) in enumerate(pairs):
            h = hashpjw(original)
            slot, step = h % size, 1 + h % (size - 2)
            while table[slot]:
                slot = slot - (size - step) if slot >= size - step else slot + step
            table[slot] = index + 1
        header_size = 28
        ofs_originals = header_size
        ofs_translations = ofs_originals + 8 * n
        ofs_hash = ofs_translations + 8 * n
        position = ofs_hash + 4 * size
        originals, translations, blob = b"", b"", b""
        for original, translation in pairs:
            originals += struct.pack(order + "II", len(original), position + len(blob))
            blob += original + b"\0"
        for original, translation in pairs:
            translations += struct.pack(order + "II", len(translation), position + len(blob))
            blob += translation + b"\0"
        magic = b"\xde\x12\x04\x95" if order == "<" else b"\x95\x04\x12\xde"
        head = magic + struct.pack(order + "IIIIII", 0, n, ofs_originals, ofs_translations, size, ofs_hash)
        return head + originals + translations + struct.pack(order + f"{size}I", *table) + blob

    put("database/gettext_mo", "little_endian", gettext_mo("<"))
    put("database/gettext_mo", "big_endian", gettext_mo(">"))

    # EFI variable holding signature lists (db, dbx, ...): the attributes, then lists of signatures
    owner = bytes.fromhex("a1b2c3d4e5f60718293a4b5c6d7e8f90")
    sha256_type = bytes([0x26, 0x16, 0xC4, 0xC1, 0x4C, 0x50, 0x92, 0x40, 0xAC, 0xA9, 0x41, 0xF9, 0x36, 0x93, 0x43, 0x28])
    x509_type = bytes([0xA1, 0x59, 0xC0, 0xA5, 0xE4, 0x94, 0xA7, 0x4A, 0x87, 0xB5, 0xAB, 0x15, 0x5C, 0x2B, 0xF0, 0x72])

    def signature_list(kind, signature_size, signatures):
        body = b"".join(owner + data for data in signatures)
        return kind + struct.pack("<III", 28 + len(body), 0, signature_size) + body

    certificate = (ROOT / "serialization/asn1/asn1_der/certificate.bin").read_bytes()
    hashes = [hashlib.sha256(bytes([i])).digest() for i in range(3)]
    variable = struct.pack("<I", 0x27) + signature_list(sha256_type, 16 + 32, hashes) + signature_list(x509_type, 16 + len(certificate), [certificate])
    put("security/efivar_signature_list", "db", variable)


SECTIONS = {
    "small": small,
    "others": others,
    "network": network, "media": media, "image": image, "serialization": serialization, "game": game, "filesystem": filesystem,
    "firmware": firmware, "hardware": hardware, "log": log, "archive": archive, "executable": executable,
}

if __name__ == "__main__":
    chosen = sys.argv[1:] or list(SECTIONS)
    for name in chosen:
        SECTIONS[name]()
