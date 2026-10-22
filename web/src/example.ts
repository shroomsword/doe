// What the page shows at first, so that there is something to click.

export const EXAMPLE_SCHEMA = `meta:
  id: example
  endian: le
doc: A small example. Edit this schema, or open your own file and schema.
seq:
  - id: magic
    contents: "DOE"
  - id: count
    type: u1
  - id: entries
    type: entry
    repeat: expr
    repeat-expr: count
instances:
  last_text:
    value: entries[count - 1].text
types:
  entry:
    seq:
      - id: kind
        type: u1
        enum: kinds
      - id: len
        type: u1
      - id: text
        type: str
        size: len
        encoding: ASCII
enums:
  kinds:
    1: greeting
    2: farewell
`;

const text = (value: string): number[] => [...value].map((char) => char.charCodeAt(0));

/** "DOE", two entries: a greeting "hello" and a farewell "bye". */
export const EXAMPLE_BYTES = new Uint8Array([...text('DOE'), 2, 1, 5, ...text('hello'), 2, 3, ...text('bye')]);
