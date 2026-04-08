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
    /// An unsigned integer (u8 / u16 / u32 / u64, bits).
    UInt(u64),
    /// A signed integer (i8 / i16 / i32 / i64).
    SInt(i64),
    /// An IEEE 754 floating-point value.
    Float(f64),
    /// A raw byte sequence.
    Bytes(Vec<u8>),
    /// A decoded string.
    Str(String),
    /// An integer with an optional symbolic name from an enum.
    Enum {
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
}

impl Value {
    /// Returns the integer representation of this value, if applicable.
    /// Used by the expression evaluator when a parsed field is referenced in
    /// a size or repeat-count expression.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::UInt(n)              => Some(*n as i64),
            Value::SInt(n)              => Some(*n),
            Value::Enum { value, .. }   => Some(*value as i64),
            _                           => None,
        }
    }

    /// Returns the byte length of this value, if applicable.
    #[allow(dead_code)]
    pub fn byte_len(&self) -> Option<usize> {
        match self {
            Value::Bytes(b) => Some(b.len()),
            Value::Str(s)   => Some(s.len()),
            _               => None,
        }
    }

    /// Returns `true` if this value is a truthy integer (non-zero).
    #[allow(dead_code)]
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::UInt(n)            => *n != 0,
            Value::SInt(n)            => *n != 0,
            Value::Enum { value, .. } => *value != 0,
            _                         => false,
        }
    }

    /// Convenience: does this value represent the "no data" sentinel?
    pub fn is_absent(&self) -> bool {
        matches!(self, Value::Absent)
    }

    /// Returns the type name as a short string, useful for error messages.
    #[allow(dead_code)]
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::UInt(_)    => "uint",
            Value::SInt(_)    => "sint",
            Value::Float(_)   => "float",
            Value::Bytes(_)   => "bytes",
            Value::Str(_)     => "str",
            Value::Enum { .. }=> "enum",
            Value::Struct { ..}=> "struct",
            Value::Array(_)   => "array",
            Value::Absent     => "absent",
        }
    }
}

/// Single-line display representation of a value, used by the text renderer.
///
/// | Variant | Example output |
/// |---------|---------------|
/// | `UInt(255)` | `255` |
/// | `SInt(-1)` | `-1` |
/// | `Float(3.14)` | `3.14` |
/// | `Bytes` | `<8 bytes>` |
/// | `Str("hi")` | `"hi"` |
/// | `Enum { value: 1, name: Some("data") }` | `data (1)` |
/// | `Enum { value: 99, name: None }` | `99` |
/// | `Struct { type_name: "png::chunk", .. }` | `<png::chunk>` |
/// | `Array` (3 items) | `[3 items]` |
/// | `Absent` | `<absent>` |
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::UInt(n)                  => write!(f, "{}", n),
            Value::SInt(n)                  => write!(f, "{}", n),
            Value::Float(v)                 => write!(f, "{}", v),
            Value::Bytes(b)                 => write!(f, "<{} bytes>", b.len()),
            Value::Str(s)                   => write!(f, "{:?}", s),
            Value::Enum { value, name: Some(n) } => write!(f, "{} ({})", n, value),
            Value::Enum { value, name: None }    => write!(f, "{}", value),
            Value::Struct { type_name, .. } => write!(f, "<{}>", type_name),
            Value::Array(items)             => write!(f, "[{} items]", items.len()),
            Value::Absent                   => write!(f, "<absent>"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── as_int ────────────────────────────────────────────────────────────────

    #[test]
    fn as_int_uint() {
        assert_eq!(Value::UInt(42).as_int(), Some(42));
    }
    #[test]
    fn as_int_sint() {
        assert_eq!(Value::SInt(-7).as_int(), Some(-7));
    }
    #[test]
    fn as_int_enum_named() {
        let v = Value::Enum {
            value: 2,
            name: Some("data".into()),
        };
        assert_eq!(v.as_int(), Some(2));
    }
    #[test]
    fn as_int_enum_unnamed() {
        let v = Value::Enum { value: 99, name: None };
        assert_eq!(v.as_int(), Some(99));
    }
    #[test]
    fn as_int_float_is_none() {
        assert_eq!(Value::Float(1.0).as_int(), None);
    }
    #[test]
    fn as_int_bytes_is_none() {
        assert_eq!(Value::Bytes(vec![]).as_int(), None);
    }
    #[test]
    fn as_int_str_is_none() {
        assert_eq!(Value::Str("x".into()).as_int(), None);
    }
    #[test]
    fn as_int_absent_is_none() {
        assert_eq!(Value::Absent.as_int(), None);
    }

    // ── byte_len ──────────────────────────────────────────────────────────────

    #[test]
    fn byte_len_bytes() {
        assert_eq!(Value::Bytes(vec![1, 2, 3]).byte_len(), Some(3));
    }
    #[test]
    fn byte_len_str() {
        assert_eq!(Value::Str("hello".into()).byte_len(), Some(5));
    }
    #[test]
    fn byte_len_uint_is_none() {
        assert_eq!(Value::UInt(1).byte_len(), None);
    }

    // ── is_truthy ─────────────────────────────────────────────────────────────

    #[test]
    fn truthy_nonzero_uint() {
        assert!(Value::UInt(1).is_truthy());
    }
    #[test]
    fn falsy_zero_uint() {
        assert!(!Value::UInt(0).is_truthy());
    }
    #[test]
    fn truthy_nonzero_sint() {
        assert!(Value::SInt(-1).is_truthy());
    }
    #[test]
    fn falsy_zero_sint() {
        assert!(!Value::SInt(0).is_truthy());
    }
    #[test]
    fn truthy_enum_nonzero() {
        assert!(Value::Enum { value: 1, name: None }.is_truthy());
    }
    #[test]
    fn falsy_float() {
        assert!(!Value::Float(1.0).is_truthy());
    }
    #[test]
    fn falsy_bytes() {
        assert!(!Value::Bytes(vec![1]).is_truthy());
    }
    #[test]
    fn falsy_absent() {
        assert!(!Value::Absent.is_truthy());
    }

    // ── is_absent ─────────────────────────────────────────────────────────────

    #[test]
    fn absent_is_absent() {
        assert!(Value::Absent.is_absent());
    }
    #[test]
    fn uint_is_not_absent() {
        assert!(!Value::UInt(0).is_absent());
    }

    // ── type_name ─────────────────────────────────────────────────────────────

    #[test]
    fn type_name_variants() {
        assert_eq!(Value::UInt(0).type_name(), "uint");
        assert_eq!(Value::SInt(0).type_name(), "sint");
        assert_eq!(Value::Float(0.0).type_name(), "float");
        assert_eq!(Value::Bytes(vec![]).type_name(), "bytes");
        assert_eq!(Value::Str("".into()).type_name(), "str");
        assert_eq!(
            Value::Enum {
                value: 0,
                name: None
            }
            .type_name(),
            "enum"
        );
        assert_eq!(
            Value::Struct {
                type_name: "t".into(),
                fields: vec![]
            }
            .type_name(),
            "struct"
        );
        assert_eq!(Value::Array(vec![]).type_name(), "array");
        assert_eq!(Value::Absent.type_name(), "absent");
    }

    // ── Display ───────────────────────────────────────────────────────────────

    #[test]
    fn display_uint() {
        assert_eq!(format!("{}", Value::UInt(255)), "255");
    }
    #[test]
    fn display_sint() {
        assert_eq!(format!("{}", Value::SInt(-1)), "-1");
    }
    #[test]
    fn display_float() {
        assert_eq!(format!("{}", Value::Float(3.14)), "3.14");
    }
    #[test]
    fn display_bytes() {
        assert_eq!(format!("{}", Value::Bytes(vec![0; 8])), "<8 bytes>");
    }
    #[test]
    fn display_str() {
        assert_eq!(format!("{}", Value::Str("hi".into())), "\"hi\"");
    }
    #[test]
    fn display_enum_named() {
        let v = Value::Enum {
            value: 1,
            name: Some("header".into()),
        };
        assert_eq!(format!("{}", v), "header (1)");
    }
    #[test]
    fn display_enum_unnamed() {
        let v = Value::Enum {
            value: 42,
            name: None,
        };
        assert_eq!(format!("{}", v), "42");
    }
    #[test]
    fn display_struct() {
        let v = Value::Struct {
            type_name: "png::chunk".into(),
            fields: vec![],
        };
        assert_eq!(format!("{}", v), "<png::chunk>");
    }
    #[test]
    fn display_array() {
        assert_eq!(
            format!("{}", Value::Array(vec![Value::UInt(1), Value::UInt(2)])),
            "[2 items]"
        );
    }
    #[test]
    fn display_absent() {
        assert_eq!(format!("{}", Value::Absent), "<absent>");
    }

    // ── Clone + PartialEq ─────────────────────────────────────────────────────

    #[test]
    fn clone_and_eq_uint() {
        let v = Value::UInt(7);
        assert_eq!(v.clone(), v);
    }
    #[test]
    fn clone_and_eq_struct() {
        let v = Value::Struct {
            type_name: "t".into(),
            fields: vec![("x".into(), Value::UInt(1))],
        };
        assert_eq!(v.clone(), v);
    }
}
