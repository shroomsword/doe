//! The [`Value`] type: the output tree produced by the binary parser.
//!
//! [`Value`] is the currency of the `doe` library — the parser produces it
//! and the renderers consume it.  It is intentionally decoupled from both
//! the schema IR and the renderers so that either side can be swapped out
//! independently.
//!
//! # Structure preservation
//!
//! Struct fields are stored as `Vec<(String, Value)>` rather than
//! `HashMap<String, Value>`.  This preserves the declaration order from the
//! schema, which matters for two reasons:
//!
//! 1. **Deterministic output** — text and JSON renderers emit fields in
//!    schema order, matching reader expectations.
//! 2. **Expression correctness** — size and repeat-count expressions can
//!    only reference fields that appear *earlier* in the sequence; the parser
//!    relies on ordered insertion to evaluate them correctly.

use std::fmt;

// ─────────────────────────────────────────────────────────────────────────────
// Value tree
// ─────────────────────────────────────────────────────────────────────────────

/// A parsed value.
///
/// Fields are stored in a `Vec<(String, Value)>` rather than a `HashMap`
/// so that field order is preserved — both for deterministic output and
/// because field order matters when earlier fields feed into later size
/// expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// An unsigned integer (u1 / u2 / u4 / u8, bits).
    UInt(u64),
    /// A signed integer (s1 / s2 / s4 / s8).
    SInt(i64),
    /// An IEEE 754 floating-point value.
    Float(f64),
    /// A boolean: a one-bit field (`b1`).
    Bool(bool),
    /// A raw byte sequence.
    Bytes(Vec<u8>),
    /// A byte sequence too long to keep, as read by `bytes` with a `size:`
    /// above [`MAX_INLINE_BYTES`](crate::parser::engine::MAX_INLINE_BYTES).
    Blob {
        /// Offset of the first byte from the start of the input.
        offset: u64,
        /// Length in bytes.
        len: u64,
        /// The first bytes (at most
        /// [`BLOB_PREVIEW_LEN`](crate::parser::engine::BLOB_PREVIEW_LEN)).
        preview: Vec<u8>,
    },
    /// A decoded string.
    Str(String),
    /// An integer with an optional symbolic name from an enum.
    Enum {
        /// The numeric discriminant.
        value: u64,
        /// `None` when the discriminant has no matching variant.
        name: Option<String>,
    },
    /// A struct-like composite type.
    Struct {
        /// The fully-qualified type name (e.g. `"png::chunk"`).
        type_name: String,
        /// Ordered field name → value pairs.
        fields: Vec<(String, Value)>,
    },
    /// A repeated field (from `repeat: eos` / `repeat: expr`).
    Array(Vec<Value>),
    /// A field whose `if:` expression evaluated to false.
    Absent,
    /// An entry of `instances`, computed or read on demand. It appears only
    /// as a field of a struct, after the fields of `seq`.
    Instance {
        /// The value.
        value: Box<Value>,
        /// Where it was read from, in bytes from the start of the input;
        /// `None` for a computed value.
        offset: Option<u64>,
    },
}

impl Value {
    /// Returns the integer representation of this value, if applicable.
    /// Used by the expression evaluator when a parsed field is referenced in
    /// a size or repeat-count expression.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::UInt(n) => Some(n.cast_signed()),
            Value::SInt(n) => Some(*n),
            Value::Enum { value, .. } => Some(value.cast_signed()),
            Value::Instance { value, .. } => value.as_int(),
            _ => None,
        }
    }

    /// Returns the byte length of this value, if applicable.
    pub fn byte_len(&self) -> Option<usize> {
        match self {
            Value::Bytes(b) => Some(b.len()),
            Value::Blob { len, .. } => usize::try_from(*len).ok(),
            Value::Instance { value, .. } => value.byte_len(),
            Value::Str(s) => Some(s.len()),
            _ => None,
        }
    }

    /// Returns `true` if this value is a truthy integer (non-zero).
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::UInt(n) => *n != 0,
            Value::SInt(n) => *n != 0,
            Value::Enum { value, .. } => *value != 0,
            Value::Bool(b) => *b,
            Value::Instance { value, .. } => value.is_truthy(),
            _ => false,
        }
    }

    /// Convenience: does this value represent the "no data" sentinel?
    pub fn is_absent(&self) -> bool {
        match self {
            Value::Absent => true,
            Value::Instance { value, .. } => value.is_absent(),
            _ => false,
        }
    }

    /// Returns the type name as a short string, useful for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::UInt(_) => "uint",
            Value::SInt(_) => "sint",
            Value::Float(_) => "float",
            Value::Bool(_) => "bool",
            Value::Bytes(_) | Value::Blob { .. } => "bytes",
            Value::Str(_) => "str",
            Value::Enum { .. } => "enum",
            Value::Struct { .. } => "struct",
            Value::Array(_) => "array",
            Value::Absent => "absent",
            Value::Instance { value, .. } => value.type_name(),
        }
    }
}

/// Single-line display representation of a value, used by the text renderer.
///
/// | Variant | Example output |
/// |---------|---------------|
/// | `UInt(255)` | `255` |
/// | `SInt(-1)` | `-1` |
/// | `Float(2.75)` | `2.75` |
/// | `Bool(true)` | `true` |
/// | `Bytes` | `<8 bytes>` |
/// | `Blob` | `<1048576 bytes at 0x40>` |
/// | `Str("hi")` | `"hi"` |
/// | `Enum { value: 1, name: Some("data") }` | `data (1)` |
/// | `Enum { value: 99, name: None }` | `99` |
/// | `Struct { type_name: "png::chunk", .. }` | `<png::chunk>` |
/// | `Array` (3 items) | `[3 items]` |
/// | `Absent` | `<absent>` |
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::UInt(n) => write!(f, "{n}"),
            Value::SInt(n) => write!(f, "{n}"),
            Value::Float(v) => write!(f, "{v}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Bytes(b) => write!(f, "<{} bytes>", b.len()),
            Value::Blob { offset, len, .. } => write!(f, "<{len} bytes at {offset:#x}>"),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::Enum {
                value,
                name: Some(n),
            } => write!(f, "{n} ({value})"),
            Value::Enum { value, name: None } => write!(f, "{value}"),
            Value::Struct { type_name, .. } => write!(f, "<{type_name}>"),
            Value::Array(items) => write!(f, "[{} items]", items.len()),
            Value::Absent => write!(f, "<absent>"),
            Value::Instance { value, .. } => write!(f, "{value}"),
        }
    }
}
