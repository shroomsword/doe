meta:
  id: value_instance_enum
  endian: le
seq:
  - id: a
    type: u1
instances:
  k:
    value: 'a == 1 ? kind_t::one : kind_t::two'
    enum: kind_t
enums:
  kind_t:
    1: one
    2: two
