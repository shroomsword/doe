meta:
  id: gallery_syntax
  title: Syntax that the gallery definitions use
  license: MIT
  endian: le
doc: |
  The small forms of the Kaitai Struct language that real definitions use:
  the metadata keys of its tools, `contents` made of strings and bytes, a field
  without an id, binary and octal literals, `_` in numbers, a float that ends
  in a point, a unary plus, a list that ends in a comma, enum members that are
  negative or are called `true`, and `valid` that mentions its own field.
-webide-representation: '{magic}'
seq:
  - id: magic
    contents: ['Rar!', 0x1a, 0x07]
    -orig-id: magic
  - id: kind
    type: s1
    enum: kinds
  - id: flag
    type: u1
    enum: switch
  - size: 2
  - id: len
    type: u1
    valid:
      min: 1
      max: len + 1
  - id: mask
    type: u1
instances:
  is_eof:
    value: kind == kinds::eof
    -webide-parse-mode: eager
  bits:
    value: (mask & 0b1000_0001) | 0o17
  big:
    value: 0xffff_ffff - 1_000
  scaled:
    value: len * 24000.
  signed:
    value: '(len < 0 ? -1 : +1) * len'
  table_size:
    value: '[[0x00, 0x01], [0x02, 0x03],].size'
types:
  unused:
    -orig-id: Unused
    to-string: '"unused"'
    seq:
      - id: x
        type: u1
enums:
  kinds:
    -1:
      id: eof
      -orig-id: EOF
    1: one
  switch:
    0x2a: true
    0x20: false
