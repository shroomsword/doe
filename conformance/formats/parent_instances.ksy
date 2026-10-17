meta:
  id: parent_instances
  title: Instances of an enclosing type, and enum members qualified with a type
  license: MIT
  endian: le
doc: |
  A sub-type that uses instances of the types around it (`_parent.limit`,
  `_parent._parent.is_control`) while they are being parsed, and enum members
  written `type::enum::member` for an enum declared in another type.
seq:
  - id: header
    type: header
  - id: body
    type: body
instances:
  limit:
    value: header.max + 1
  is_control:
    value: header.kind == header::kinds::control
types:
  header:
    seq:
      - id: kind
        type: u1
        enum: kinds
      - id: max
        type: u1
    enums:
      kinds:
        1: data
        2: control
  body:
    seq:
      - id: v
        type: u1
        valid:
          max: _parent.limit
      - id: inner
        type: inner
    types:
      inner:
        seq:
          - id: w
            type: u1
        instances:
          flagged:
            value: _parent._parent.is_control and w == _parent.v - 7
