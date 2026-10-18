meta:
  id: qualified_field_enums
  title: Fields and instances whose enum is declared in another type
  license: MIT
doc: |
  `enum: type::name` on a field and on an instance, with `valid: in-enum`
  and a comparison with a member written `type::name::member`.
seq:
  - id: kind
    type: u1
    enum: kinds_holder::kinds
  - id: other
    type: u1
    enum: kinds_holder::kinds
    valid:
      in-enum: true
  - id: nested
    type: user
instances:
  is_control:
    value: kind == kinds_holder::kinds::control
  again:
    pos: 1
    type: u1
    enum: kinds_holder::kinds
types:
  kinds_holder:
    enums:
      kinds:
        1: data
        2: control
  user:
    seq:
      - id: k
        type: u1
        enum: kinds_holder::kinds
