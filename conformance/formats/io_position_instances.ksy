meta:
  id: io_position_instances
  endian: le
doc: |
  Instances that read where the stream is. The reference works an instance out
  the first time it is used and keeps it; one that nothing uses is worked out
  when the output is made, with the stream where the whole parse left it.
seq:
  - id: items
    type: item
    repeat: expr
    repeat-expr: 3
  - id: tail
    type: u1
types:
  item:
    seq:
      - id: kind
        type: u1
      - id: extra
        type: u1
        if: kind == 1
      - id: forced
        type: u1
        if: kind == 2 and used_by_field >= 0
    instances:
      # nothing in the sequence uses these: worked out when the output is made
      unused_pos:
        value: _io.pos
      unused_eof:
        value: _io.eof
      # uses one of them, so it is worked out when the output is made too
      indirect:
        value: unused_pos + 1
      # used by the condition of a field (for kind 2 only): worked out at that point
      used_by_field:
        value: _io.pos
