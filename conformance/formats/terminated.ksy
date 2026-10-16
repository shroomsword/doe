meta:
  id: terminated
  title: Values that end at a terminator or are cut from a window
  license: MIT
  encoding: ASCII
doc: |
  `terminator` on bytes and strings, with `include`, `consume` and `eos-error`,
  a `size` together with a `terminator`, and `pad-right`.
seq:
  - id: plain
    type: str
    terminator: 0x2c
  - id: peeked
    type: str
    terminator: 0x2c
    consume: false
  - id: comma
    size: 1
  - id: raw
    terminator: 0x3b
  - id: window
    type: str
    size: 6
    terminator: 0
  - id: window_kept
    size: 4
    terminator: 0x2e
    include: true
  - id: padded
    type: str
    size: 5
    pad-right: 0x20
  - id: padded_cut
    type: str
    size: 6
    terminator: 0x23
    pad-right: 0x40
  - id: rest
    type: str
    terminator: 0x21
    eos-error: false
