meta:
  id: bit_enums
  title: Enums on bit fields, including a single bit
  license: MIT
doc: |
  A single bit (`b1`) is a boolean, unless it has an enum: then it is an
  integer and the enum applies to it.
seq:
  - id: a
    type: b4
    enum: kind
  - id: b
    type: b3
    enum: kind
  - id: c
    type: b1
    enum: flag
    valid:
      in-enum: true
  - id: d
    type: b4le
    enum: kind
instances:
  is_one:
    value: a == kind::one
  c_is_on:
    value: c == flag::on
enums:
  kind:
    1: one
    2: two
  flag:
    0: off
    1: on
