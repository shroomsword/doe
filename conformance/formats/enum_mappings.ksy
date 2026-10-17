meta:
  id: enum_mappings
  title: Enum members written as mappings
  license: MIT
doc: |
  Enum members as `value: name` and as `value: {id: name, doc: ...}`.
seq:
  - id: kind
    type: u1
    enum: kind
  - id: other
    type: u1
    enum: kind
instances:
  is_second:
    value: kind == kind::second
enums:
  kind:
    1: first
    2:
      id: second
      doc: The second member.
      doc-ref: https://example.org/second
    3:
      id: third
