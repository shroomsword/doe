meta:
  id: import_cycle_b
  title: A definition that imports one that imports it back (b)
  license: MIT
  endian: le
  imports:
    - import_cycle_a
doc: |
  Two definitions that refer to each other's types: a chain that goes on while
  the tag is not zero.
seq:
  - id: tag
    type: u1
  - id: next
    type: import_cycle_a
    if: tag != 0
