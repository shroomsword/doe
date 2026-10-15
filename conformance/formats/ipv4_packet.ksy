meta:
  id: ipv4_packet
  title: IPv4 packet header
  license: MIT
  endian: be
doc: |
  The fixed header of an IPv4 packet, its options and its payload. A format
  made of bit fields that share bytes and span them.
seq:
  - id: version
    type: b4
    valid: 4
  - id: header_len_words
    type: b4
  - id: dscp
    type: b6
  - id: ecn
    type: b2
  - id: total_length
    type: u2
  - id: identification
    type: u2
  - id: reserved
    type: b1
  - id: dont_fragment
    type: b1
  - id: more_fragments
    type: b1
  - id: fragment_offset
    type: b13
  - id: ttl
    type: u1
  - id: protocol
    type: u1
    enum: protocol_t
  - id: header_checksum
    type: u2
  - id: src_address
    size: 4
  - id: dst_address
    size: 4
  - id: options
    size: header_len - 20
  - id: body
    size: total_length - header_len
instances:
  header_len:
    value: header_len_words * 4
  is_fragment:
    value: more_fragments or fragment_offset != 0
enums:
  protocol_t:
    1: icmp
    6: tcp
    17: udp
