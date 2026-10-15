meta:
  id: import_dep
  title: A type that other definitions import
  license: MIT
  endian: le
doc: |
  Imported by import_main. It has a type of its own, and a parameter.
seq:
  - id: value
    type: u2
  - id: tail
    type: pair
types:
  pair:
    seq:
      - id: first
        type: u1
      - id: second
        type: u1
    instances:
      both:
        value: first * 256 + second
