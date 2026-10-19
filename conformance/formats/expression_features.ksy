meta:
  id: expression_features
  title: Expressions that the gallery definitions use
  license: MIT
  endian: le
  encoding: ASCII
doc: |
  `_index` in the expressions of a repeated field, in its type's arguments and
  in `repeat-until`; the elements read so far; `x._sizeof`, `_sizeof` and
  `sizeof<type>`; `.as<type>`; and a list of byte literals as a byte sequence.
seq:
  - id: magic
    size: 2
  - id: lens
    type: u1
    repeat: expr
    repeat-expr: 3
  - id: items
    size: lens[_index]
    repeat: expr
    repeat-expr: 3
  - id: groups
    type: 'group(_index != 0 ? groups[_index - 1].total : 0)'
    repeat: until
    repeat-until: not _.more
  - id: kind
    type: u1
  - id: body
    type:
      switch-on: kind
      cases:
        1: one
  - id: name
    type: strz
  - id: tail
    type: u1
instances:
  is_ii:
    value: magic == [0x49, 0x49]
  bytes_size:
    value: '[1, 2, 3].size'
  prims:
    value: sizeof<s4> * 2 + sizeof<b12> + sizeof<one>
  here:
    value: magic._sizeof + tail._sizeof
  one_v:
    value: body.as<one>.v + 1
  header_size:
    value: _root.magic._sizeof + sizeof<group>
types:
  group:
    params:
      - id: before
        type: u4
    seq:
      - id: byte
        type: u1
    instances:
      more:
        value: byte >= 0x80
      total:
        value: before + (byte & 0x7f)
  one:
    seq:
      - id: v
        type: u2
    instances:
      own:
        value: _sizeof
