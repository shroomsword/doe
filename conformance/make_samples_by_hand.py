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


SECTIONS = {"network": network}

if __name__ == "__main__":
    chosen = sys.argv[1:] or list(SECTIONS)
    for name in chosen:
        SECTIONS[name]()
