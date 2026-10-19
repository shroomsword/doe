meta:
  id: import_cycle_a
  title: A definition that imports one that imports it back (a)
  license: MIT
  endian: le
  imports:
    - import_cycle_b
doc: |
  Two definitions that refer to each other's types: a chain that goes on while
  the tag is not zero.
seq:
  - id: tag
    type: u1
  - id: next
    type: import_cycle_b
    if: tag != 0
