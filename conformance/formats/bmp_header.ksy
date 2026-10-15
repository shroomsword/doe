meta:
  id: bmp_header
  title: Header of a Windows bitmap
  license: MIT
  endian: le
doc: |
  The file header and the info header of a bitmap, and the pixel data found
  through the offset in the header (an instance with a position).
seq:
  - id: signature
    contents: 'BM'
  - id: file_size
    type: u4
  - id: reserved
    size: 4
  - id: pixel_data_offset
    type: u4
  - id: dib
    type: dib_header
instances:
  pixel_data:
    pos: pixel_data_offset
    size: dib.row_size * dib.abs_height
types:
  dib_header:
    seq:
      - id: header_size
        type: u4
        valid: 40
      - id: width
        type: s4
      - id: height
        type: s4
      - id: planes
        type: u2
      - id: bits_per_pixel
        type: u2
      - id: compression
        type: u4
        enum: compression_t
      - id: image_size
        type: u4
      - id: horizontal_ppm
        type: s4
      - id: vertical_ppm
        type: s4
      - id: colors_used
        type: u4
      - id: colors_important
        type: u4
    instances:
      row_size:
        value: ((bits_per_pixel * width + 31) / 32) * 4
      abs_height:
        value: 'height < 0 ? -height : height'
enums:
  compression_t:
    0: none
    1: rle8
    2: rle4
    3: bitfields
