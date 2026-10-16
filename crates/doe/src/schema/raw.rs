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
    /// The canonical identifier for this type (e.g. `png`). Kaitai Struct
    /// files give it as `meta.id`; [`RawSchema::from_yaml_str`] copies it here.
    #[serde(default)]
    pub id: String,

    /// Human-readable description.
    #[serde(default)]
    pub doc: Option<String>,

    /// Where the format is documented. Accepted and ignored.
    #[serde(rename = "doc-ref", default)]
    pub doc_ref: Option<serde_json::Value>,

    /// File-level metadata (endianness, etc.).
    #[serde(default)]
    pub meta: RawMeta,

    /// List of schema names or relative paths to import.
    /// A bare name like `"zlib_block"` is resolved through include paths.
    /// A path starting with `"./"` or `"../"` is resolved relative to this file.
    #[serde(default)]
    pub imports: Vec<String>,

    /// Parameters the type takes, which the field that uses it supplies.
    #[serde(default)]
    pub params: Vec<RawParam>,

    /// The top-level field sequence — the root struct.
    #[serde(default)]
    pub seq: Vec<RawField>,

    /// Locally-defined sub-types, keyed by name.
    #[serde(default)]
    pub types: IndexMap<String, RawTypeDecl>,

    /// Values computed or read on demand, keyed by name.
    #[serde(default)]
    pub instances: IndexMap<String, RawInstance>,

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
        let mut schema: RawSchema = parse_yaml(text)?;
        if schema.id.is_empty() {
            if let Some(id) = &schema.meta.id {
                schema.id.clone_from(id);
            } else {
                // Neither `id` nor `meta.id`: report it the way a missing
                // required field is.
                #[derive(Deserialize)]
                struct NeedsId {
                    #[expect(dead_code, reason = "only here so that its absence is an error")]
                    id: String,
                }
                parse_yaml::<NeedsId>(text)?;
            }
        }
        Ok(schema)
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
    /// The identifier of the type, as Kaitai Struct files give it.
    #[serde(default)]
    pub id: Option<String>,

    /// Schemas to import, as Kaitai Struct files list them; the same as the
    /// top-level `imports`.
    #[serde(default)]
    pub imports: Vec<String>,

    /// The encoding of the strings that do not give one.
    #[serde(default)]
    pub encoding: Option<RawEncoding>,

    /// Default byte order for multi-byte integer fields.
    /// Individual fields may override this with explicit type names (e.g. `u32be`).
    #[serde(default)]
    pub endian: Option<RawEndian>,

    /// Default bit order for bit fields (`b1`..`b64`): `be` takes the most
    /// significant bits of a byte first (the default), `le` the least.
    #[serde(rename = "bit-endian", default)]
    pub bit_endian: Option<RawEndian>,

    /// A title for the format. Accepted and ignored.
    #[serde(default)]
    pub title: Option<serde_json::Value>,

    /// The applications that use the format. Accepted and ignored.
    #[serde(default)]
    pub application: Option<serde_json::Value>,

    /// File extensions of the format. Accepted and ignored.
    #[serde(rename = "file-extension", default)]
    pub file_extension: Option<serde_json::Value>,

    /// Cross-references to other documentation. Accepted and ignored.
    #[serde(default)]
    pub xref: Option<serde_json::Value>,

    /// The license of the format definition. Accepted and ignored.
    #[serde(default)]
    pub license: Option<serde_json::Value>,

    /// The Kaitai Struct version the definition was written for. Accepted
    /// and ignored.
    #[serde(rename = "ks-version", default)]
    pub ks_version: Option<serde_json::Value>,

    /// Compiler options of Kaitai Struct. Accepted and ignored.
    #[serde(rename = "ks-debug", default)]
    pub ks_debug: Option<serde_json::Value>,

    /// Compiler options of Kaitai Struct. Accepted and ignored.
    #[serde(rename = "ks-opaque-types", default)]
    pub ks_opaque_types: Option<serde_json::Value>,

    /// Tags describing the format. Accepted and ignored.
    #[serde(default)]
    pub tags: Option<serde_json::Value>,
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

/// A parameter of a type: a value the field that uses the type passes in.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawParam {
    /// The name the type's expressions use.
    pub id: String,

    /// The type of the value. It documents the parameter and is not enforced.
    #[serde(rename = "type", default)]
    pub type_ref: Option<String>,

    /// Name of the enum the value is a member of. Not enforced.
    #[serde(rename = "enum", default)]
    pub enum_ref: Option<String>,

    /// Human-readable description.
    #[serde(default)]
    pub doc: Option<String>,

    /// Where the field or type is documented. Accepted and ignored.
    #[serde(rename = "doc-ref", default)]
    pub doc_ref: Option<serde_json::Value>,
}

/// One field in a `seq` list.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawField {
    /// Field identifier (used in expressions and output).
    pub id: String,

    /// Type name: a primitive name, a locally defined type, or an imported type;
    /// or a `switch-on` mapping that chooses one by a value.
    /// Required unless `contents` is set.
    #[serde(rename = "type")]
    pub type_ref: Option<RawType>,

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

    /// For `bytes` and `str`: the byte value that terminates the value (default
    /// `0x00` for `strz`).
    #[serde(default)]
    pub terminator: Option<u8>,

    /// With `terminator`: keep the terminator in the value (default `false`).
    #[serde(default)]
    pub include: Option<bool>,

    /// With `terminator`: continue after the terminator (default `true`).
    #[serde(default)]
    pub consume: Option<bool>,

    /// With `terminator` and no `size`: an error when there is no terminator
    /// (default `true`); otherwise the rest of the stream is the value.
    #[serde(rename = "eos-error", default)]
    pub eos_error: Option<bool>,

    /// With `size`: strip trailing bytes of this value from the result.
    #[serde(rename = "pad-right", default)]
    pub pad_right: Option<u8>,

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

    /// What the value read may be; the parse fails if it is anything else. A
    /// bare value stands for `eq`.
    #[serde(default)]
    pub valid: Option<RawValid>,

    /// Name of an enum defined in the nearest enclosing `enums:` block.
    #[serde(rename = "enum", default)]
    pub enum_ref: Option<String>,

    /// Human-readable description.
    #[serde(default)]
    pub doc: Option<String>,

    /// Where the field or type is documented. Accepted and ignored.
    #[serde(rename = "doc-ref", default)]
    pub doc_ref: Option<serde_json::Value>,
}

/// What a field's value may be: a value it must equal, or several rules that
/// must all hold.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawValidRules {
    /// The value must equal this expression.
    #[serde(default)]
    pub eq: Option<ExprText>,

    /// The value must be at least this.
    #[serde(default)]
    pub min: Option<ExprText>,

    /// The value must be at most this.
    #[serde(default)]
    pub max: Option<ExprText>,

    /// The value must equal one of these.
    #[serde(rename = "any-of", default)]
    pub any_of: Option<Vec<ExprText>>,

    /// An expression over `_`, the value, that must be true.
    #[serde(default)]
    pub expr: Option<ExprText>,

    /// The value must be a member of the field's enum.
    #[serde(rename = "in-enum", default)]
    pub in_enum: Option<bool>,
}

impl RawValidRules {
    /// Whether no rule is given.
    pub fn is_empty(&self) -> bool {
        self.eq.is_none()
            && self.min.is_none()
            && self.max.is_none()
            && self.any_of.is_none()
            && self.expr.is_none()
            && self.in_enum.is_none()
    }
}

/// The `valid` of a field: a bare value (`valid: 4`), which stands for `eq`,
/// or a mapping of rules.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum RawValid {
    /// The value must equal this expression.
    Eq(ExprText),
    /// Rules that must all hold.
    Rules(RawValidRules),
}

// Not derived, for the same reason as `RawType`: a bare value of any scalar
// kind must work, which an untagged enum's buffering would not allow.
impl<'de> Deserialize<'de> for RawValid {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ValidVisitor;

        impl<'de> serde::de::Visitor<'de> for ValidVisitor {
            type Value = RawValid;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a value, or a mapping with eq, min, max, any-of, expr or in-enum")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<RawValid, E> {
                Ok(RawValid::Eq(ExprText(v.to_owned())))
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<RawValid, E> {
                Ok(RawValid::Eq(ExprText(v.to_string())))
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<RawValid, E> {
                Ok(RawValid::Eq(ExprText(v.to_string())))
            }

            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<RawValid, E> {
                Ok(RawValid::Eq(ExprText(format!("{v:?}"))))
            }

            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<RawValid, E> {
                Ok(RawValid::Eq(ExprText(v.to_string())))
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<RawValid, A::Error> {
                RawValidRules::deserialize(serde::de::value::MapAccessDeserializer::new(map))
                    .map(RawValid::Rules)
            }
        }

        deserializer.deserialize_any(ValidVisitor)
    }
}

/// One entry of an `instances` mapping: the attributes of a `seq` field (the id
/// is the mapping key) or a `value` expression.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawInstance {
    /// Type name: a primitive name, a locally defined type, or an imported type;
    /// or a `switch-on` mapping that chooses one by a value.
    /// Required unless `contents` is set.
    #[serde(rename = "type")]
    pub type_ref: Option<RawType>,

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

    /// For `bytes` and `str`: the byte value that terminates the value (default
    /// `0x00` for `strz`).
    #[serde(default)]
    pub terminator: Option<u8>,

    /// With `terminator`: keep the terminator in the value (default `false`).
    #[serde(default)]
    pub include: Option<bool>,

    /// With `terminator`: continue after the terminator (default `true`).
    #[serde(default)]
    pub consume: Option<bool>,

    /// With `terminator` and no `size`: an error when there is no terminator
    /// (default `true`); otherwise the rest of the stream is the value.
    #[serde(rename = "eos-error", default)]
    pub eos_error: Option<bool>,

    /// With `size`: strip trailing bytes of this value from the result.
    #[serde(rename = "pad-right", default)]
    pub pad_right: Option<u8>,

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

    /// What the value read may be; the parse fails if it is anything else. A
    /// bare value stands for `eq`.
    #[serde(default)]
    pub valid: Option<RawValid>,

    /// Name of an enum defined in the nearest enclosing `enums:` block.
    #[serde(rename = "enum", default)]
    pub enum_ref: Option<String>,

    /// Human-readable description.
    #[serde(default)]
    pub doc: Option<String>,

    /// Where the field or type is documented. Accepted and ignored.
    #[serde(rename = "doc-ref", default)]
    pub doc_ref: Option<serde_json::Value>,

    /// A value computed from other fields instead of read: an expression, or a
    /// literal (`4`, `true`).
    #[serde(default)]
    pub value: Option<ExprText>,
}

impl RawInstance {
    /// The instance as a `seq` field named `id`, ignoring `value`.
    pub fn to_field(&self, id: &str) -> RawField {
        RawField {
            id: id.to_owned(),
            type_ref: self.type_ref.clone(),
            contents: self.contents.clone(),
            size: self.size.clone(),
            size_eos: self.size_eos,
            terminator: self.terminator,
            include: self.include,
            consume: self.consume,
            eos_error: self.eos_error,
            pad_right: self.pad_right,
            encoding: self.encoding,
            bit_size: self.bit_size,
            repeat: self.repeat,
            repeat_expr: self.repeat_expr.clone(),
            repeat_until: self.repeat_until.clone(),
            if_expr: self.if_expr.clone(),
            pos: self.pos.clone(),
            valid: self.valid.clone(),
            enum_ref: self.enum_ref.clone(),
            doc: self.doc.clone(),
            doc_ref: self.doc_ref.clone(),
        }
    }

    /// Whether the instance has any attribute that reads data.
    pub fn reads(&self) -> bool {
        self.type_ref.is_some()
            || self.contents.is_some()
            || self.size.is_some()
            || self.size_eos.is_some()
            || self.terminator.is_some()
            || self.include.is_some()
            || self.consume.is_some()
            || self.eos_error.is_some()
            || self.pad_right.is_some()
            || self.encoding.is_some()
            || self.bit_size.is_some()
            || self.repeat.is_some()
            || self.repeat_expr.is_some()
            || self.repeat_until.is_some()
            || self.pos.is_some()
            || self.valid.is_some()
    }
}

/// Expression text written as a YAML scalar: a string, or a literal that
/// stands for itself (`4`, `1.5`, `true`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExprText(pub String);

impl<'de> Deserialize<'de> for ExprText {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TextVisitor;

        impl serde::de::Visitor<'_> for TextVisitor {
            type Value = ExprText;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an expression or a literal")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<ExprText, E> {
                Ok(ExprText(v.to_owned()))
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<ExprText, E> {
                Ok(ExprText(v.to_string()))
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<ExprText, E> {
                Ok(ExprText(v.to_string()))
            }

            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<ExprText, E> {
                Ok(ExprText(format!("{v:?}")))
            }

            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<ExprText, E> {
                Ok(ExprText(v.to_string()))
            }
        }

        deserializer.deserialize_any(TextVisitor)
    }
}

/// The `type` of a field: a type name, or a choice between types.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum RawType {
    /// A primitive, local or imported type by name.
    Name(String),
    /// `switch-on` and `cases`: the type depends on the value of an expression.
    Switch(RawSwitch),
}

// Not derived: an untagged enum buffers the input, which turns the integer
// keys of `cases` (`1: a_t`) into numbers that a `String` key then rejects.
impl<'de> Deserialize<'de> for RawType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TypeVisitor;

        impl<'de> serde::de::Visitor<'de> for TypeVisitor {
            type Value = RawType;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a type name, or a mapping with `switch-on` and `cases`")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<RawType, E> {
                Ok(RawType::Name(v.to_owned()))
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<RawType, A::Error> {
                RawSwitch::deserialize(serde::de::value::MapAccessDeserializer::new(map))
                    .map(RawType::Switch)
            }
        }

        deserializer.deserialize_any(TypeVisitor)
    }
}

impl RawType {
    /// The type name, if this is not a `switch-on`.
    pub fn as_name(&self) -> Option<&str> {
        match self {
            RawType::Name(name) => Some(name),
            RawType::Switch(_) => None,
        }
    }
}

/// A `type` that is chosen by a value.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawSwitch {
    /// The expression whose value picks the case.
    #[serde(rename = "switch-on")]
    pub switch_on: String,

    /// Case key (an expression, or `_` for the default) → type name, in the
    /// order they are tried.
    pub cases: IndexMap<String, String>,
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

/// String encoding as written in a schema: `utf8`, `ascii` or `latin1`, or
/// the names Kaitai Struct uses for them (`UTF-8`, `ASCII`, `ISO-8859-1`), in
/// any case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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

impl RawEncoding {
    /// The encoding called `name`, if it is one of the supported ones.
    pub fn from_name(name: &str) -> Option<Self> {
        let normalized: String = name
            .chars()
            .filter(|c| !matches!(c, '-' | '_' | ' '))
            .flat_map(char::to_lowercase)
            .collect();
        match normalized.as_str() {
            "utf8" => Some(RawEncoding::Utf8),
            "ascii" | "usascii" => Some(RawEncoding::Ascii),
            "latin1" | "iso88591" => Some(RawEncoding::Latin1),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for RawEncoding {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        RawEncoding::from_name(&name).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "unsupported encoding '{name}': use utf8, ascii or latin1 (also spelled UTF-8, ASCII, ISO-8859-1)"
            ))
        })
    }
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

    /// Where the field or type is documented. Accepted and ignored.
    #[serde(rename = "doc-ref", default)]
    pub doc_ref: Option<serde_json::Value>,

    /// Parameters the type takes, which the field that uses it supplies.
    #[serde(default)]
    pub params: Vec<RawParam>,

    /// The type's fields, in order.
    #[serde(default)]
    pub seq: Vec<RawField>,

    /// Values computed or read on demand, keyed by name.
    #[serde(default)]
    pub instances: IndexMap<String, RawInstance>,

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
