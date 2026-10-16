meta:
  id: encodings
  title: Strings in the encodings other than UTF-8, ASCII and ISO-8859-1
  license: MIT
doc: |
  Strings in UTF-16 and in the legacy encodings that the reference decodes.
  UTF-32 and CP437 are left out: the JavaScript runtime of the reference
  decodes through TextDecoder, which knows neither.
seq:
  - id: utf16le
    type: str
    size: 6
    encoding: UTF-16LE
  - id: utf16be
    type: str
    size: 4
    encoding: UTF-16BE
  - id: cp1252
    type: str
    size: 3
    encoding: CP1252
  - id: latin2
    type: str
    size: 2
    encoding: ISO-8859-2
  - id: sjis
    type: str
    size: 4
    encoding: Shift_JIS
  - id: gbk
    type: str
    size: 4
    encoding: GBK
  - id: big5
    type: str
    size: 2
    encoding: Big5
  - id: eucjp
    type: str
    size: 2
    encoding: EUC-JP
  - id: euckr
    type: str
    size: 2
    encoding: EUC-KR
