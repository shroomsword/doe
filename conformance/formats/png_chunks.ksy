meta:
  id: png_chunks
  title: Chunks of a PNG image
  license: MIT
  endian: be
doc: |
  The signature and the chunks of a PNG file, up to the last one, with the
  body of a few of them decoded and the rest kept as bytes.
seq:
  - id: signature
    contents: [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]
  - id: chunks
    type: chunk
    repeat: until
    repeat-until: _.type == "IEND"
types:
  chunk:
    seq:
      - id: len
        type: u4
      - id: type
        type: str
        size: 4
        encoding: ASCII
      - id: body
        type:
          switch-on: type
          cases:
            '"IHDR"': ihdr_chunk
            '"PLTE"': plte_chunk
            '"tEXt"': text_chunk
        size: len
      - id: crc
        size: 4
  ihdr_chunk:
    seq:
      - id: width
        type: u4
        valid:
          min: 1
      - id: height
        type: u4
        valid:
          min: 1
      - id: bit_depth
        type: u1
      - id: color_type
        type: u1
        enum: color_type_t
      - id: compression_method
        type: u1
        valid: 0
      - id: filter_method
        type: u1
        valid: 0
      - id: interlace_method
        type: u1
  plte_chunk:
    seq:
      - id: entries
        type: rgb
        repeat: eos
  rgb:
    seq:
      - id: r
        type: u1
      - id: g
        type: u1
      - id: b
        type: u1
  text_chunk:
    seq:
      - id: keyword
        type: strz
        encoding: ASCII
      - id: text
        type: str
        size-eos: true
        encoding: ASCII
enums:
  color_type_t:
    0: greyscale
    2: truecolor
    3: indexed
    4: greyscale_alpha
    6: truecolor_alpha
