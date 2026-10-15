meta:
  id: riff_wave
  title: A RIFF WAVE file
  license: MIT
  endian: le
doc: |
  Chunks identified by a four-character code, with string cases, padding to
  an even size, and a LIST chunk that contains chunks of the same type.
seq:
  - id: riff_id
    contents: 'RIFF'
  - id: file_size
    type: u4
  - id: wave_id
    contents: 'WAVE'
  - id: chunks
    type: chunk
    repeat: eos
types:
  chunk:
    seq:
      - id: chunk_id
        type: str
        size: 4
        encoding: ASCII
      - id: len
        type: u4
      - id: body
        type:
          switch-on: chunk_id
          cases:
            '"fmt "': fmt_body
            '"LIST"': list_body
        size: len
      - id: pad_byte
        size: 1
        if: len % 2 == 1
  fmt_body:
    seq:
      - id: audio_format
        type: u2
        enum: audio_format_t
      - id: channels
        type: u2
      - id: sample_rate
        type: u4
      - id: byte_rate
        type: u4
      - id: block_align
        type: u2
      - id: bits_per_sample
        type: u2
    instances:
      bytes_per_sample:
        value: bits_per_sample / 8
  list_body:
    seq:
      - id: list_type
        type: str
        size: 4
        encoding: ASCII
      - id: subchunks
        type: chunk
        repeat: eos
enums:
  audio_format_t:
    1: pcm
    3: ieee_float
