//! Raw schema types: a direct serde mirror of the YAML format.
//!
//! These structs are deliberately free of any validation or business logic —
//! they exist only to let `serde-saphyr` deserialise a schema file into a Rust
//! value.  The [`crate::schema::compiler`] module then transforms them into
//! the validated, resolved [`crate::schema::ir`] representation.
//!
//! Keeping the two representations separate means:
//! - `serde` handles all YAML quirks (optional fields, untagged unions, key
//!   renaming) without cluttering the compiler.
//! - The compiler can report precise, field-level errors without fighting
//!   serde's error model.
//!
//! You generally do not need to construct these types directly; use
//! [`crate::load_schema_file`] to parse a YAML file into a [`RawSchema`].

use std::collections::HashMap;
use std::path::PathBuf;

use indexmap::IndexMap;
use serde::de::DeserializeOwned;
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

impl RawSchema {
    /// Parse a schema from YAML text.
    ///
    /// Unknown keys are rejected, duplicate mapping keys are an error, and only
    /// `true` and `false` are booleans, so field ids such as `y`, `n` or `on`
    /// stay strings. The parser enforces limits on document size and alias
    /// expansion, so untrusted input cannot exhaust memory.
    ///
    /// # Errors
    ///
    /// Returns the parser's error, which names the line and column, if the text
    /// is not valid YAML or does not match the schema format.
    pub fn from_yaml_str(text: &str) -> Result<Self, serde_saphyr::Error> {
        parse_yaml(text)
    }
}

/// Deserialize YAML with the settings every `doe` YAML file uses.
pub(crate) fn parse_yaml<T: DeserializeOwned>(text: &str) -> Result<T, serde_saphyr::Error> {
    let mut options = serde_saphyr::Options::default();
    // YAML 1.1 reads `y`, `n`, `yes`, `no`, `on` and `off` as booleans, which
    // would break field ids and expressions such as `size: n`.
    options.strict_booleans = true;
    serde_saphyr::from_str_with_options(text, options)
}

// ── Meta ─────────────────────────────────────────────────────────────────────

/// File-level metadata; currently carries only the default byte order.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawMeta {
    /// Default byte order for multi-byte integer fields.
    /// Individual fields may override this with explicit type names (e.g. `u32be`).
    #[serde(default)]
    pub endian: Option<RawEndian>,

    /// Default bit order for bit fields (`b1`..`b64`): `be` takes the most
    /// significant bits of a byte first (the default), `le` the least.
    #[serde(rename = "bit-endian", default)]
    pub bit_endian: Option<RawEndian>,
}

/// Byte order as written in a schema's `meta.endian` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum RawEndian {
    /// Little-endian (least-significant byte first).
    #[default]
    Le,
    /// Big-endian (most-significant byte first).
    Be,
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

    /// For `bytes`, `str` and user types: the size is whatever is left of the
    /// enclosing window or the input.
    #[serde(rename = "size-eos", default)]
    pub size_eos: Option<bool>,

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

    /// Expression evaluated after each iteration when `repeat: until`.
    /// The most recently parsed value is bound to `_` within the expression.
    /// Iteration stops when the expression evaluates to a non-zero (truthy) value.
    #[serde(rename = "repeat-until", default)]
    pub repeat_until: Option<String>,

    /// Expression that evaluates to a bool.  Field is skipped when false.
    #[serde(rename = "if", default)]
    pub if_expr: Option<String>,

    /// Byte offset to read the field at, from the start of the file (or of the
    /// enclosing sized window). May be a literal or an expression over earlier
    /// fields. The position is restored once the field has been read.
    #[serde(default)]
    pub pos: Option<StringOrInt>,

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
    /// A list of byte values.
    Bytes(Vec<u8>),
    /// A string.
    Str(String),
}

/// Serde helper: accept either a quoted string (`"length"`) or a bare integer (`12`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StringOrInt {
    /// A bare integer.
    Int(u64),
    /// A string, typically an expression.
    Str(String),
}

impl StringOrInt {
    /// The value as text, formatting integers in decimal.
    pub fn as_str(&self) -> std::borrow::Cow<'_, str> {
        match self {
            StringOrInt::Int(n) => std::borrow::Cow::Owned(n.to_string()),
            StringOrInt::Str(s) => std::borrow::Cow::Borrowed(s),
        }
    }
}

/// String encoding as written in a schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum RawEncoding {
    /// UTF-8.
    #[default]
    Utf8,
    /// 7-bit ASCII.
    Ascii,
    /// ISO 8859-1.
    Latin1,
}

/// Repetition mode as written in a schema.
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
    /// Human-readable description.
    #[serde(default)]
    pub doc: Option<String>,

    /// The type's fields, in order.
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
