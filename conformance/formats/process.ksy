meta:
  id: process
  title: Fields whose bytes are transformed before they are read
  license: MIT
  endian: le
doc: |
  `process:` with `xor` (an integer and a byte sequence from an earlier
  field), `rol`, `ror`, `zlib`, and a user type read from the processed bytes.
seq:
  - id: a
    size: 4
    process: xor(0xa5)
  - id: key
    size: 2
  - id: b
    size: 5
    process: xor(key)
  - id: l
    size: 2
    process: rol(3)
  - id: r
    size: 2
    process: ror(3)
  - id: body
    type: inner
    size: 3
    process: xor(0xff)
  - id: packed_len
    type: u1
  - id: packed
    type: inner
    size: packed_len
    process: zlib
  - id: text_len
    type: u1
  - id: text
    size: text_len
    process: zlib
  - id: tail
    type: u1
types:
  inner:
    seq:
      - id: x
        type: u1
      - id: y
        type: u2
