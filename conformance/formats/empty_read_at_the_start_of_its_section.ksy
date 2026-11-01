meta:
  id: empty_read_at_the_start_of_its_section
  title: A read of nothing at the place where the section around it starts
  license: MIT
  endian: le
doc: |
  The list of entries of a section is read at `ofs` for `len` bytes. One of
  its entries has an instance that reads the same type, from the same stream, at
  the same place, for no bytes (`size: 0`): the list is empty, and it ends where
  the section starts (MCAP's summary offset has one for an index that the file
  has none of). It is a different read from the section's, which has `len`
  bytes, so it is read, and it is an empty list.
seq:
  - id: ofs
    type: u1
  - id: len
    type: u1
  - id: filler
    size: 2
instances:
  section:
    pos: ofs
    size: len
    type: list
types:
  list:
    seq:
      - id: items
        type: entry
        repeat: eos
  entry:
    seq:
      - id: at
        type: u1
      - id: n
        type: u1
    instances:
      group:
        io: _root._io
        pos: at
        size: n
        type: list
