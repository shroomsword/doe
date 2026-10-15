meta:
  id: repeat_until_eof
  endian: le
seq:
  - id: items
    type: u1
    repeat: until
    repeat-until: _io.eof or _ == 0
