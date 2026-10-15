meta:
  id: tlv_stream
  title: A stream of type-length-value records
  license: MIT
  endian: le
doc: |
  Records whose body depends on a tag: a switch on an enum, sized windows,
  a parameterized type, a validated field and instances.
seq:
  - id: magic
    contents: [0x54, 0x4c, 0x56, 0x31]
  - id: count
    type: u1
  - id: records
    type: record
    repeat: expr
    repeat-expr: count
instances:
  total_records:
    value: records.size
  last_kind:
    value: records.last.kind
    if: count > 0
types:
  record:
    seq:
      - id: kind
        type: u1
        enum: kind_t
      - id: len
        type: u1
      - id: body
        type:
          switch-on: kind
          cases:
            'kind_t::text': text_body(len)
            'kind_t::number': number_body
            'kind_t::pair': pair_body
        size: len
  text_body:
    params:
      - id: len
        type: u1
    seq:
      - id: text
        type: str
        size: len
        encoding: ASCII
  number_body:
    seq:
      - id: value
        type: u4
        valid:
          max: 1000000
  pair_body:
    seq:
      - id: first
        type: u1
      - id: second
        type: u1
    instances:
      sum:
        value: first + second
enums:
  kind_t:
    1: text
    2: number
    3: pair
