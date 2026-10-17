meta:
  id: endian_switch
  title: Byte order chosen by a value
  license: MIT
  encoding: ASCII
doc: |
  A record whose byte order comes from a byte-order mark read earlier: with a
  `switch-on` in `meta.endian` of a type, a default case, a type that inherits
  the choice, a type that does not use it, and a parameter as the selector.
seq:
  - id: mark
    type: str
    size: 2
  - id: first
    type: record
  - id: second
    type: param_record(mark)
types:
  record:
    meta:
      endian:
        switch-on: _root.mark
        cases:
          '"II"': le
          '"MM"': be
    seq:
      - id: a
        type: u2
      - id: b
        type: u4
      - id: forced
        type: u2be
      - id: inner
        type: inner
    types:
      inner:
        seq:
          - id: c
            type: u2
  param_record:
    params:
      - id: order
        type: str
    meta:
      endian:
        switch-on: order
        cases:
          '"II"': le
          _: be
    seq:
      - id: d
        type: u2
