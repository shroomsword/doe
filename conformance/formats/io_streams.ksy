meta:
  id: io_streams
  title: Instances read from another stream, and parent
  license: MIT
  endian: le
doc: |
  An instance read with `io:` from the whole input (`_root._io`), from the
  stream of the enclosing type (`_parent._io`) and from the stream a value was
  parsed from (`slot._io`), and `parent: _parent` on a field.
seq:
  - id: head
    type: u1
  - id: slot
    type: slot
    size: 3
  - id: wrapper
    type: wrapper
  - id: tail
    type: u1
instances:
  slot_second:
    io: slot._io
    pos: 1
    type: u1
  slot_rest:
    io: slot._io
    pos: 1
    size-eos: true
  slot_first_via_type:
    io: slot._io
    pos: 0
    type: u1
types:
  slot: {}
  wrapper:
    seq:
      - id: w
        type: u1
      - id: sized
        type: sized
        size: 2
        parent: _parent
    types:
      sized:
        seq:
          - id: s
            type: u1
        instances:
          from_root:
            io: _root._io
            pos: 0
            type: u1
          from_parent_stream:
            io: _parent._io
            pos: 0
            type: u1
          head_via_parent:
            value: _parent.head
