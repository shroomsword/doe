meta:
  id: gif_like
  title: Blocks of a GIF image
  license: MIT
  endian: le
doc: |
  The header, the screen descriptor with its packed flags, the color table
  and the blocks of a GIF file, which run until a trailer.
seq:
  - id: header
    type: header
  - id: logical_screen
    type: logical_screen
  - id: global_color_table
    size: logical_screen.color_table_bytes
    if: logical_screen.has_color_table
  - id: blocks
    type: block
    repeat: until
    repeat-until: _.block_type == block_type_t::end_of_file
types:
  header:
    seq:
      - id: magic
        contents: 'GIF'
      - id: version
        type: str
        size: 3
        encoding: ASCII
  logical_screen:
    seq:
      - id: width
        type: u2
      - id: height
        type: u2
      - id: has_color_table
        type: b1
      - id: color_resolution
        type: b3
      - id: is_sorted
        type: b1
      - id: color_table_size_exp
        type: b3
      - id: background_color_index
        type: u1
      - id: pixel_aspect_ratio
        type: u1
    instances:
      color_table_entries:
        value: 'has_color_table ? (1 << (color_table_size_exp + 1)) : 0'
      color_table_bytes:
        value: color_table_entries * 3
  block:
    seq:
      - id: block_type
        type: u1
        enum: block_type_t
      - id: body
        type:
          switch-on: block_type
          cases:
            'block_type_t::extension': extension
            'block_type_t::local_image_descriptor': image_descriptor
  extension:
    seq:
      - id: label
        type: u1
      - id: subblocks
        type: subblock
        repeat: until
        repeat-until: _.len == 0
  image_descriptor:
    seq:
      - id: left
        type: u2
      - id: top
        type: u2
      - id: width
        type: u2
      - id: height
        type: u2
      - id: has_local_table
        type: b1
      - id: interlaced
        type: b1
      - id: is_sorted
        type: b1
      - id: reserved
        type: b2
      - id: local_table_size_exp
        type: b3
      - id: local_color_table
        size: local_table_bytes
        if: has_local_table
      - id: lzw_min_code_size
        type: u1
      - id: subblocks
        type: subblock
        repeat: until
        repeat-until: _.len == 0
    instances:
      local_table_bytes:
        value: 'has_local_table ? (1 << (local_table_size_exp + 1)) * 3 : 0'
  subblock:
    seq:
      - id: len
        type: u1
      - id: data
        size: len
enums:
  block_type_t:
    0x21: extension
    0x2c: local_image_descriptor
    0x3b: end_of_file
