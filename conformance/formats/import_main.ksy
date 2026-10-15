meta:
  id: import_main
  title: A definition that imports another
  license: MIT
  endian: le
  imports:
    - import_dep
doc: |
  Uses the type of an imported definition and a type nested in it.
seq:
  - id: count
    type: u1
  - id: deps
    type: import_dep
    repeat: expr
    repeat-expr: count
  - id: extra
    type: import_dep::pair
