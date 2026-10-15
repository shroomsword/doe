meta:
  id: expressions
  title: Expressions over a few fields
  license: MIT
  endian: be
doc: |
  Instances that compute values from a few fields, to compare the arithmetic,
  the comparisons, the strings, the arrays and the conversions of doe's
  expression language with the reference.
seq:
  - id: a
    type: s4
  - id: b
    type: s4
  - id: c
    type: u1
  - id: d
    type: u1
  - id: name
    type: strz
    encoding: UTF-8
  - id: raw
    size: 3
  - id: ratio
    type: f4
  - id: nums
    type: u1
    repeat: expr
    repeat-expr: 4
instances:
  quotient:
    value: a / b
  remainder:
    value: a % b
  neg_quotient:
    value: (0 - a) / (0 - b)
  shifted_left:
    value: c << 2
  shifted_right:
    value: c >> 1
  bit_and:
    value: c & d
  bit_or:
    value: c | d
  bit_xor:
    value: c ^ d
  negated:
    value: -c
  sum:
    value: a + b * 3 - 1
  cmp_lt:
    value: a < b
  cmp_eq:
    value: c == d
  and_or:
    value: c > 5 and d < 5 or false
  not_cmp:
    value: not (c == d)
  tern:
    value: 'c > d ? 100 : 200'
  name_len:
    value: name.length
  name_rev:
    value: name.reverse
  name_sub:
    value: name.substring(1, 3)
  name_cat:
    value: 'name + "!"'
  name_eq:
    value: 'name == "héllo"'
  name_lt:
    value: 'name < "z"'
  num_str:
    value: '"42".to_i'
  hex_str:
    value: '"ff".to_i(16)'
  c_str:
    value: c.to_s
  raw_first:
    value: raw[0]
  raw_len:
    value: raw.size
  raw_last:
    value: raw.last
  nums_min:
    value: nums.min
  nums_max:
    value: nums.max
  nums_first:
    value: nums.first
  nums_last:
    value: nums.last
  nums_size:
    value: nums.size
  nums_second:
    value: nums[1]
  ratio_gt:
    value: ratio > 1.0
  ratio_int:
    value: ratio.to_i
  ratio_sum:
    value: ratio + 1
