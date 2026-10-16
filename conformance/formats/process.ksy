meta:
  id: process
  title: Fields whose bytes are transformed before they are read
  license: MIT
  endian: le
doc: |
  `process:` with `xor` (an integer and a byte sequence from an earlier
  field), `rol`, `ror`, and a user type read from the processed bytes. `zlib`
  is left out: the JavaScript runtime of the reference needs a library for it.
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
types:
  inner:
    seq:
      - id: x
        type: u1
      - id: y
        type: u2
