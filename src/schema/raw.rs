//! Raw schema types: a direct serde mirror of the YAML format.
//!
//! Nothing here is validated or resolved. The compiler transforms these into
//! the typed IR in `ir.rs`. Keeping the two representations separate means
//! serde handles all the YAML quirks while the compiler handles all the logic.

use std::collections::HashMap;
use std::path::PathBuf;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

// ── Top-level schema ─────────────────────────────────────────────────────────

/// A complete schema file, corresponding to one `.yaml` file on disk.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawSchema {
    /// The canonical identifier for this type (e.g. `png`).
    pub id: String,

    /// Human-readable description.
    #[serde(default)]
    pub doc: Option<String>,

    /// File-level metadata (endianness, etc.).
    #[serde(default)]
    pub meta: RawMeta,

    /// List of schema names or relative paths to import.
    /// A bare name like `"zlib_block"` is resolved through include paths.
    /// A path starting with `"./"` or `"../"` is resolved relative to this file.
    #[serde(default)]
    pub imports: Vec<String>,

    /// The top-level field sequence — the root struct.
    #[serde(default)]
    pub seq: Vec<RawField>,

    /// Locally-defined sub-types, keyed by name.
    #[serde(default)]
    pub types: IndexMap<String, RawTypeDecl>,

    /// Enum definitions, keyed by name.
    #[serde(default)]
    pub enums: IndexMap<String, RawEnum>,

    /// Not present in the YAML; injected by the loader so downstream code
    /// knows where this schema was resolved from.
    #[serde(skip)]
    pub source_path: Option<PathBuf>,
}

// ── Meta ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawMeta {
    /// Default byte order for multi-byte integer fields.
    /// Individual fields may override this with explicit type names (e.g. `u32be`).
    #[serde(default)]
    pub endian: Option<RawEndian>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RawEndian {
    Le,
    Be,
}

impl Default for RawEndian {
    fn default() -> Self {
        RawEndian::Le
    }
}

// ── Field ────────────────────────────────────────────────────────────────────

/// One field in a `seq` list.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawField {
    /// Field identifier (used in expressions and output).
    pub id: String,

    /// Type name: a primitive name, a locally defined type, or an imported type.
    /// Required unless `contents` is set.
    #[serde(rename = "type")]
    pub type_ref: Option<String>,

    /// Fixed literal contents.  If present, the parser reads exactly this many
    /// bytes and asserts they match.  `type_ref` is ignored when set.
    #[serde(default)]
    pub contents: Option<RawContents>,

    /// Byte count expression.  Required for `bytes` and `str` types.
    /// May be a literal integer or an expression string referencing earlier fields.
    #[serde(default)]
    pub size: Option<StringOrInt>,

    /// For `str`: the byte value that terminates the string (default `0x00` for `strz`).
    #[serde(default)]
    pub terminator: Option<u8>,

    /// For `str`: character encoding.  Defaults to `utf8`.
    #[serde(default)]
    pub encoding: Option<RawEncoding>,

    /// For `bits`: how many bits to consume.
    #[serde(default)]
    pub bit_size: Option<u8>,

    /// Repetition mode.
    #[serde(default)]
    pub repeat: Option<RawRepeat>,

    /// Expression that evaluates to the repeat count when `repeat: expr`.
    #[serde(rename = "repeat-expr", default)]
    pub repeat_expr: Option<String>,

    /// Expression that evaluates to a bool.  Field is skipped when false.
    #[serde(rename = "if", default)]
    pub if_expr: Option<String>,

    /// Name of an enum defined in the nearest enclosing `enums:` block.
    #[serde(rename = "enum", default)]
    pub enum_ref: Option<String>,

    /// Human-readable description.
    #[serde(default)]
    pub doc: Option<String>,
}

/// The `contents` field can be a hex string, a byte array, or a plain string.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RawContents {
    Bytes(Vec<u8>),
    Str(String),
}

/// Serde helper: accept either a quoted string (`"length"`) or a bare integer (`12`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StringOrInt {
    Int(u64),
    Str(String),
}

impl StringOrInt {
    #[allow(dead_code)]
    pub fn as_str(&self) -> std::borrow::Cow<'_, str> {
        match self {
            StringOrInt::Int(n) => std::borrow::Cow::Owned(n.to_string()),
            StringOrInt::Str(s) => std::borrow::Cow::Borrowed(s),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RawEncoding {
    Utf8,
    Ascii,
    Latin1,
}

impl Default for RawEncoding {
    fn default() -> Self {
        RawEncoding::Utf8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RawRepeat {
    /// Repeat until end of stream.
    Eos,
    /// Repeat a fixed number of times given by `repeat-expr`.
    Expr,
    /// Repeat until the last parsed value satisfies `repeat-until`.
    Until,
}

// ── Sub-type declaration ─────────────────────────────────────────────────────

/// An inline type definition inside a schema's `types:` block.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawTypeDecl {
    #[serde(default)]
    pub doc: Option<String>,

    #[serde(default)]
    pub seq: Vec<RawField>,

    /// Types can nest types.
    #[serde(default)]
    pub types: IndexMap<String, RawTypeDecl>,

    /// Types can also define local enums.
    #[serde(default)]
    pub enums: IndexMap<String, RawEnum>,
}

// ── Enum definition ──────────────────────────────────────────────────────────

/// An enum definition: a map from integer discriminant to variant name.
/// The key may be decimal (`0`, `1`) or hex (`0x00`, `0x4d`).
/// We store it as a raw string-keyed map and parse keys during compilation.
pub type RawEnum = HashMap<String, String>;

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    fn parse(yaml: &str) -> RawSchema {
        serde_yaml::from_str(yaml).expect("YAML parse failed")
    }

    // ── Basic round-trip ─────────────────────────────────────────────────────

    #[test]
    fn minimal_schema_round_trip() {
        let yaml = indoc! {r#"
            id: simple
            seq:
              - id: value
                type: u32
        "#};
        let s = parse(yaml);
        assert_eq!(s.id, "simple");
        assert_eq!(s.seq.len(), 1);
        assert_eq!(s.seq[0].id, "value");
        assert_eq!(s.seq[0].type_ref.as_deref(), Some("u32"));
    }

    #[test]
    fn schema_with_doc_and_meta() {
        let yaml = indoc! {r#"
            id: be_file
            doc: "A big-endian file format"
            meta:
              endian: be
            seq: []
        "#};
        let s = parse(yaml);
        assert_eq!(s.doc.as_deref(), Some("A big-endian file format"));
        assert_eq!(s.meta.endian, Some(RawEndian::Be));
    }

    #[test]
    fn meta_endian_little_endian() {
        let yaml = indoc! {r#"
            id: le_file
            meta:
              endian: le
            seq: []
        "#};
        let s = parse(yaml);
        assert_eq!(s.meta.endian, Some(RawEndian::Le));
    }

    #[test]
    fn default_meta_has_no_endian() {
        let yaml = indoc! {r#"
            id: no_meta
            seq: []
        "#};
        let s = parse(yaml);
        assert!(s.meta.endian.is_none());
    }

    // ── Field attributes ─────────────────────────────────────────────────────

    #[test]
    fn field_with_size_integer() {
        let yaml = indoc! {r#"
            id: buf
            seq:
              - id: data
                type: bytes
                size: 16
        "#};
        let s = parse(yaml);
        let f = &s.seq[0];
        assert_eq!(f.type_ref.as_deref(), Some("bytes"));
        assert!(matches!(&f.size, Some(StringOrInt::Int(16))));
    }

    #[test]
    fn field_with_size_expression() {
        let yaml = indoc! {r#"
            id: var_buf
            seq:
              - id: length
                type: u32
              - id: data
                type: bytes
                size: length
        "#};
        let s = parse(yaml);
        let f = &s.seq[1];
        assert!(matches!(&f.size, Some(StringOrInt::Str(e)) if e == "length"));
    }

    #[test]
    fn field_with_terminator_and_encoding() {
        let yaml = indoc! {r#"
            id: strs
            seq:
              - id: name
                type: str
                terminator: 0
                encoding: ascii
        "#};
        let s = parse(yaml);
        let f = &s.seq[0];
        assert_eq!(f.terminator, Some(0));
        assert_eq!(f.encoding, Some(RawEncoding::Ascii));
    }

    #[test]
    fn field_with_repeat_eos() {
        let yaml = indoc! {r#"
            id: list
            seq:
              - id: items
                type: u8
                repeat: eos
        "#};
        let s = parse(yaml);
        assert_eq!(s.seq[0].repeat, Some(RawRepeat::Eos));
    }

    #[test]
    fn field_with_repeat_expr() {
        let yaml = indoc! {r#"
            id: counted
            seq:
              - id: count
                type: u16
              - id: items
                type: u8
                repeat: expr
                repeat-expr: count
        "#};
        let s = parse(yaml);
        let f = &s.seq[1];
        assert_eq!(f.repeat, Some(RawRepeat::Expr));
        assert_eq!(f.repeat_expr.as_deref(), Some("count"));
    }

    #[test]
    fn field_with_if_expression() {
        let yaml = indoc! {r#"
            id: conditional
            seq:
              - id: flags
                type: u8
              - id: extra
                type: u32
                if: flags > 0
        "#};
        let s = parse(yaml);
        assert_eq!(s.seq[1].if_expr.as_deref(), Some("flags > 0"));
    }

    #[test]
    fn field_with_enum_ref() {
        let yaml = indoc! {r#"
            id: tagged
            seq:
              - id: kind
                type: u8
                enum: record_type
            enums:
              record_type:
                "0": unknown
                "1": header
                "2": data
        "#};
        let s = parse(yaml);
        assert_eq!(s.seq[0].enum_ref.as_deref(), Some("record_type"));
        assert!(s.enums.contains_key("record_type"));
        let e = &s.enums["record_type"];
        assert_eq!(e.get("1").map(String::as_str), Some("header"));
    }

    #[test]
    fn field_bit_size() {
        let yaml = indoc! {r#"
            id: bitfield
            seq:
              - id: flags
                type: bits
                bit_size: 4
        "#};
        let s = parse(yaml);
        assert_eq!(s.seq[0].bit_size, Some(4));
    }

    #[test]
    fn field_with_doc() {
        let yaml = indoc! {r#"
            id: documented
            seq:
              - id: magic
                type: bytes
                size: 4
                doc: "File magic number"
        "#};
        let s = parse(yaml);
        assert_eq!(s.seq[0].doc.as_deref(), Some("File magic number"));
    }

    // ── Sub-types ────────────────────────────────────────────────────────────

    #[test]
    fn inline_type_declaration() {
        let yaml = indoc! {r#"
            id: outer
            seq:
              - id: hdr
                type: header
            types:
              header:
                seq:
                  - id: magic
                    type: u32
                  - id: version
                    type: u16
        "#};
        let s = parse(yaml);
        assert!(s.types.contains_key("header"));
        let hdr = &s.types["header"];
        assert_eq!(hdr.seq.len(), 2);
        assert_eq!(hdr.seq[0].id, "magic");
    }

    #[test]
    fn nested_type_declaration() {
        let yaml = indoc! {r#"
            id: deeply_nested
            seq:
              - id: top
                type: outer_type
            types:
              outer_type:
                seq:
                  - id: inner
                    type: inner_type
                types:
                  inner_type:
                    seq:
                      - id: value
                        type: u32
        "#};
        let s = parse(yaml);
        let outer = &s.types["outer_type"];
        assert!(outer.types.contains_key("inner_type"));
        let inner = &outer.types["inner_type"];
        assert_eq!(inner.seq[0].id, "value");
    }

    #[test]
    fn type_with_local_enum() {
        let yaml = indoc! {r#"
            id: typed_enum
            seq:
              - id: record
                type: record_t
            types:
              record_t:
                seq:
                  - id: kind
                    type: u8
                    enum: kind_t
                enums:
                  kind_t:
                    "0": eof
                    "1": data
        "#};
        let s = parse(yaml);
        let rt = &s.types["record_t"];
        assert!(rt.enums.contains_key("kind_t"));
    }

    // ── Imports ──────────────────────────────────────────────────────────────

    #[test]
    fn imports_parsed() {
        let yaml = indoc! {r#"
            id: with_imports
            imports:
              - zlib_block
              - ./local_helper.yaml
            seq: []
        "#};
        let s = parse(yaml);
        assert_eq!(s.imports.len(), 2);
        assert_eq!(s.imports[0], "zlib_block");
        assert_eq!(s.imports[1], "./local_helper.yaml");
    }

    // ── Contents field ───────────────────────────────────────────────────────

    #[test]
    fn contents_as_byte_array() {
        let yaml = indoc! {r#"
            id: magic_file
            seq:
              - id: magic
                contents: [0x89, 0x50, 0x4e, 0x47]
        "#};
        let s = parse(yaml);
        let f = &s.seq[0];
        assert!(matches!(&f.contents, Some(RawContents::Bytes(b)) if b == &[0x89, 0x50, 0x4e, 0x47]));
    }

    #[test]
    fn contents_as_string() {
        let yaml = indoc! {r#"
            id: magic_str
            seq:
              - id: magic
                contents: "RIFF"
        "#};
        let s = parse(yaml);
        assert!(matches!(&s.seq[0].contents, Some(RawContents::Str(s)) if s == "RIFF"));
    }

    // ── Realistic example: PNG-like ───────────────────────────────────────────

    #[test]
    fn png_like_schema() {
        let yaml = indoc! {r#"
            id: png
            doc: "PNG image file"
            meta:
              endian: be
            seq:
              - id: signature
                contents: [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]
              - id: chunks
                type: chunk
                repeat: eos
            types:
              chunk:
                seq:
                  - id: length
                    type: u32be
                  - id: type
                    type: str
                    size: 4
                    encoding: ascii
                  - id: body
                    type: bytes
                    size: length
                  - id: crc
                    type: u32be
        "#};
        let s = parse(yaml);
        assert_eq!(s.id, "png");
        assert_eq!(s.meta.endian, Some(RawEndian::Be));

        // root fields
        assert_eq!(s.seq.len(), 2);
        assert!(matches!(&s.seq[0].contents, Some(RawContents::Bytes(_))));
        assert_eq!(s.seq[1].repeat, Some(RawRepeat::Eos));

        // chunk sub-type
        let chunk = &s.types["chunk"];
        assert_eq!(chunk.seq.len(), 4);
        assert_eq!(chunk.seq[0].type_ref.as_deref(), Some("u32be"));
        assert_eq!(chunk.seq[2].type_ref.as_deref(), Some("bytes"));
        // body size references the length field
        assert!(matches!(&chunk.seq[2].size, Some(StringOrInt::Str(e)) if e == "length"));
    }

    // ── Realistic example: ELF-like ───────────────────────────────────────────

    #[test]
    fn elf_like_schema() {
        let yaml = indoc! {r#"
            id: elf
            doc: "ELF executable"
            seq:
              - id: header
                type: elf_header
            types:
              elf_header:
                seq:
                  - id: magic
                    contents: [0x7f, 0x45, 0x4c, 0x46]
                  - id: bitness
                    type: u8
                    enum: bits_t
                  - id: endian
                    type: u8
                    enum: endian_t
                  - id: ei_version
                    type: u8
                  - id: abi
                    type: u8
                  - id: pad
                    type: bytes
                    size: 8
                  - id: e_type
                    type: u16le
                    enum: obj_type_t
                  - id: machine
                    type: u16le
                  - id: version
                    type: u32le
                enums:
                  bits_t:
                    "1": b32
                    "2": b64
                  endian_t:
                    "1": le
                    "2": be
                  obj_type_t:
                    "0": no_file
                    "1": rel
                    "2": exec
                    "3": dyn
                    "4": core
        "#};
        let s = parse(yaml);
        let hdr = &s.types["elf_header"];
        assert_eq!(hdr.seq.len(), 9);
        assert!(hdr.enums.contains_key("bits_t"));
        assert!(hdr.enums.contains_key("obj_type_t"));
        assert_eq!(hdr.enums["obj_type_t"].get("2").map(String::as_str), Some("exec"));
    }

    // ── Error cases ───────────────────────────────────────────────────────────

    #[test]
    fn missing_id_fails() {
        let yaml = indoc! {r#"
            seq:
              - id: value
                type: u32
        "#};
        let result: Result<RawSchema, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_field_fails() {
        let yaml = indoc! {r#"
            id: bad
            unknown_key: true
            seq: []
        "#};
        let result: Result<RawSchema, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "deny_unknown_fields should reject unknown_key");
    }

    #[test]
    fn field_without_id_fails() {
        let yaml = indoc! {r#"
            id: bad
            seq:
              - type: u32
        "#};
        let result: Result<RawSchema, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "field missing id should fail");
    }

    // ── StringOrInt helper ───────────────────────────────────────────────────

    #[test]
    fn string_or_int_as_str() {
        assert_eq!(StringOrInt::Int(42).as_str().as_ref(), "42");
        assert_eq!(StringOrInt::Str("foo".into()).as_str().as_ref(), "foo");
    }
}
