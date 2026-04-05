//! Compiled Intermediate Representation.
//!
//! The IR is what the binary parser consumes.  All type references are
//! resolved, all expression strings are pre-parsed into ASTs, and all
//! enum keys are decoded from their raw YAML string form into `u64`.
//!
//! Types are stored in a flat registry keyed by a fully-qualified name:
//! - Top-level types use their bare id, e.g. `"png"`.
//! - Inline types are prefixed by their parent: `"png::chunk"`.
//! - Doubly-nested: `"png::chunk::segment"`.

use std::collections::HashMap;

use indexmap::IndexMap;

use crate::schema::expr::Expr;

// ─────────────────────────────────────────────────────────────────────────────
// Type registry
// ─────────────────────────────────────────────────────────────────────────────

/// The compiled, resolved representation of all loaded schemas.
#[derive(Debug, Default)]
pub struct TypeRegistry {
    /// All known types, keyed by fully-qualified name.
    pub types: IndexMap<String, CompiledType>,
}

impl TypeRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn get(&self, name: &str) -> Option<&CompiledType> {
        self.types.get(name)
    }

    pub fn insert(&mut self, name: String, ty: CompiledType) {
        self.types.insert(name, ty);
    }

    pub fn contains(&self, name: &str) -> bool {
        self.types.contains_key(name)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Compiled type
// ─────────────────────────────────────────────────────────────────────────────

/// A fully compiled struct-like type.
#[derive(Debug, Clone)]
pub struct CompiledType {
    /// Fully-qualified name, e.g. `"png"` or `"png::chunk"`.
    pub name: String,

    /// Optional human-readable description.
    pub doc: Option<String>,

    /// Default byte order for this type's fields.
    pub endian: Endian,

    /// Ordered list of fields.
    pub fields: Vec<CompiledField>,

    /// Enum definitions visible within this type (merged from imports +
    /// inline `enums:` blocks).
    pub enums: HashMap<String, CompiledEnum>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Compiled field
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CompiledField {
    pub id: String,
    pub doc: Option<String>,
    pub kind: FieldKind,
    pub repeat: RepeatMode,
    /// Compiled `if:` predicate, if present.
    pub if_expr: Option<Expr>,
    /// Name of the enum to apply, if present.
    pub enum_ref: Option<String>,
}

#[derive(Debug, Clone)]
pub enum FieldKind {
    /// A fixed-width unsigned integer.
    UInt { width: IntWidth, endian: EndianOverride },
    /// A fixed-width signed integer.
    SInt { width: IntWidth, endian: EndianOverride },
    /// IEEE 754 float.
    Float { width: FloatWidth },
    /// Raw byte sequence.
    Bytes { size: SizeExpr },
    /// String with optional terminator or explicit size.
    Str { size: StrSize, encoding: Encoding },
    /// Bit field.
    Bits { bit_size: u8 },
    /// A reference to another compiled type by its fully-qualified name.
    TypeRef { type_name: String },
    /// Fixed literal content — parser reads and asserts exact bytes.
    Contents(Vec<u8>),
}

/// Integer byte widths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntWidth { W8, W16, W32, W64 }

impl IntWidth {
    pub fn bytes(self) -> usize {
        match self { IntWidth::W8 => 1, IntWidth::W16 => 2, IntWidth::W32 => 4, IntWidth::W64 => 8 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatWidth { F32, F64 }

impl FloatWidth {
    pub fn bytes(self) -> usize {
        match self { FloatWidth::F32 => 4, FloatWidth::F64 => 8 }
    }
}

/// Endianness selection for a specific field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndianOverride {
    /// Use the type's (or schema's) default.
    Inherit,
    Little,
    Big,
}

/// Byte order resolved to a concrete value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endian {
    Little,
    Big,
}

impl Default for Endian {
    fn default() -> Self { Endian::Little }
}

/// How many bytes to consume for a `bytes` or `str` field.
#[derive(Debug, Clone)]
pub enum SizeExpr {
    /// A compile-time constant.
    Literal(usize),
    /// An expression evaluated at parse time.
    Dynamic(Expr),
}

/// String termination/size strategy.
#[derive(Debug, Clone)]
pub enum StrSize {
    /// Read until this byte value (inclusive; the terminator is consumed but
    /// not included in the string value).
    Terminator(u8),
    /// Read exactly this many bytes.
    Fixed(SizeExpr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Ascii,
    Latin1,
}

impl Default for Encoding {
    fn default() -> Self { Encoding::Utf8 }
}

/// Repetition mode for a field.
#[derive(Debug, Clone)]
pub enum RepeatMode {
    /// Appear exactly once (default).
    Once,
    /// Repeat until end of the containing stream.
    Eos,
    /// Repeat a count determined at parse time.
    Count(Expr),
    /// Repeat until the most recently parsed value satisfies a predicate.
    Until(Expr),
}

// ─────────────────────────────────────────────────────────────────────────────
// Compiled enum
// ─────────────────────────────────────────────────────────────────────────────

/// An enum: maps integer discriminant → variant name.
#[derive(Debug, Clone, Default)]
pub struct CompiledEnum {
    pub variants: HashMap<u64, String>,
}

impl CompiledEnum {
    pub fn lookup(&self, v: u64) -> Option<&str> {
        self.variants.get(&v).map(String::as_str)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::expr::{Expr, BinaryOp};

    // ── TypeRegistry ─────────────────────────────────────────────────────────

    #[test]
    fn registry_insert_and_get() {
        let mut reg = TypeRegistry::new();
        let ty = CompiledType {
            name: "foo".into(),
            doc: None,
            endian: Endian::Little,
            fields: vec![],
            enums: HashMap::new(),
        };
        reg.insert("foo".into(), ty);
        assert!(reg.contains("foo"));
        assert!(!reg.contains("bar"));
        assert_eq!(reg.get("foo").unwrap().name, "foo");
    }

    #[test]
    fn registry_default_is_empty() {
        let reg = TypeRegistry::new();
        assert!(reg.types.is_empty());
    }

    // ── IntWidth ─────────────────────────────────────────────────────────────

    #[test]
    fn int_width_bytes() {
        assert_eq!(IntWidth::W8.bytes(),  1);
        assert_eq!(IntWidth::W16.bytes(), 2);
        assert_eq!(IntWidth::W32.bytes(), 4);
        assert_eq!(IntWidth::W64.bytes(), 8);
    }

    // ── FloatWidth ───────────────────────────────────────────────────────────

    #[test]
    fn float_width_bytes() {
        assert_eq!(FloatWidth::F32.bytes(), 4);
        assert_eq!(FloatWidth::F64.bytes(), 8);
    }

    // ── CompiledEnum ─────────────────────────────────────────────────────────

    #[test]
    fn compiled_enum_lookup_hit() {
        let mut e = CompiledEnum::default();
        e.variants.insert(0, "none".into());
        e.variants.insert(1, "header".into());
        e.variants.insert(2, "data".into());
        assert_eq!(e.lookup(1), Some("header"));
        assert_eq!(e.lookup(2), Some("data"));
    }

    #[test]
    fn compiled_enum_lookup_miss() {
        let e = CompiledEnum::default();
        assert_eq!(e.lookup(42), None);
    }

    // ── Endian defaults ───────────────────────────────────────────────────────

    #[test]
    fn endian_default_is_little() {
        let e: Endian = Default::default();
        assert_eq!(e, Endian::Little);
    }

    // ── FieldKind construction ────────────────────────────────────────────────

    #[test]
    fn field_kind_uint_inherit() {
        let fk = FieldKind::UInt { width: IntWidth::W32, endian: EndianOverride::Inherit };
        assert!(matches!(fk, FieldKind::UInt { width: IntWidth::W32, endian: EndianOverride::Inherit }));
    }

    #[test]
    fn field_kind_typeref() {
        let fk = FieldKind::TypeRef { type_name: "png::chunk".into() };
        if let FieldKind::TypeRef { type_name } = &fk {
            assert_eq!(type_name, "png::chunk");
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn field_kind_contents() {
        let fk = FieldKind::Contents(vec![0x89, 0x50, 0x4e, 0x47]);
        if let FieldKind::Contents(b) = &fk {
            assert_eq!(b, &[0x89, 0x50, 0x4e, 0x47]);
        } else {
            panic!("wrong variant");
        }
    }

    // ── RepeatMode ────────────────────────────────────────────────────────────

    #[test]
    fn repeat_mode_once_is_default_convention() {
        // There's no Default impl to test, but we verify the variant exists.
        let r = RepeatMode::Once;
        assert!(matches!(r, RepeatMode::Once));
    }

    #[test]
    fn repeat_mode_count_holds_expr() {
        let expr = Expr::Int(5);
        let r = RepeatMode::Count(expr.clone());
        if let RepeatMode::Count(e) = r {
            assert_eq!(e, expr);
        } else {
            panic!("wrong variant");
        }
    }

    // ── SizeExpr ─────────────────────────────────────────────────────────────

    #[test]
    fn size_expr_literal() {
        let s = SizeExpr::Literal(16);
        assert!(matches!(s, SizeExpr::Literal(16)));
    }

    #[test]
    fn size_expr_dynamic_holds_expr() {
        // field ref: `length`
        let e = Expr::Field(vec!["length".into()]);
        let s = SizeExpr::Dynamic(e.clone());
        if let SizeExpr::Dynamic(inner) = s {
            assert_eq!(inner, e);
        } else {
            panic!("wrong variant");
        }
    }

    // ── StrSize ───────────────────────────────────────────────────────────────

    #[test]
    fn str_size_terminator() {
        let s = StrSize::Terminator(0x00);
        assert!(matches!(s, StrSize::Terminator(0)));
    }

    #[test]
    fn str_size_fixed_literal() {
        let s = StrSize::Fixed(SizeExpr::Literal(8));
        if let StrSize::Fixed(SizeExpr::Literal(n)) = s {
            assert_eq!(n, 8);
        } else {
            panic!("wrong variant");
        }
    }

    // ── CompiledField construction ────────────────────────────────────────────

    #[test]
    fn compiled_field_no_repeat_no_if() {
        let f = CompiledField {
            id: "magic".into(),
            doc: None,
            kind: FieldKind::Bytes { size: SizeExpr::Literal(4) },
            repeat: RepeatMode::Once,
            if_expr: None,
            enum_ref: None,
        };
        assert_eq!(f.id, "magic");
        assert!(f.if_expr.is_none());
        assert!(matches!(f.repeat, RepeatMode::Once));
    }

    #[test]
    fn compiled_field_with_if_and_enum() {
        let cond = Expr::Binary(
            BinaryOp::Gt,
            Box::new(Expr::Field(vec!["version".into()])),
            Box::new(Expr::Int(2)),
        );
        let f = CompiledField {
            id: "extra".into(),
            doc: None,
            kind: FieldKind::UInt { width: IntWidth::W32, endian: EndianOverride::Inherit },
            repeat: RepeatMode::Once,
            if_expr: Some(cond),
            enum_ref: Some("record_type".into()),
        };
        assert!(f.if_expr.is_some());
        assert_eq!(f.enum_ref.as_deref(), Some("record_type"));
    }

    // ── CompiledType ──────────────────────────────────────────────────────────

    #[test]
    fn compiled_type_with_multiple_fields() {
        let fields = vec![
            CompiledField {
                id: "length".into(),
                doc: None,
                kind: FieldKind::UInt { width: IntWidth::W32, endian: EndianOverride::Inherit },
                repeat: RepeatMode::Once,
                if_expr: None,
                enum_ref: None,
            },
            CompiledField {
                id: "data".into(),
                doc: None,
                kind: FieldKind::Bytes {
                    size: SizeExpr::Dynamic(Expr::Field(vec!["length".into()])),
                },
                repeat: RepeatMode::Once,
                if_expr: None,
                enum_ref: None,
            },
        ];
        let ty = CompiledType {
            name: "chunk".into(),
            doc: Some("A data chunk".into()),
            endian: Endian::Big,
            fields,
            enums: HashMap::new(),
        };
        assert_eq!(ty.fields.len(), 2);
        assert_eq!(ty.endian, Endian::Big);
        assert_eq!(ty.doc.as_deref(), Some("A data chunk"));
    }

    // ── Encoding default ──────────────────────────────────────────────────────

    #[test]
    fn encoding_default_is_utf8() {
        let e: Encoding = Default::default();
        assert_eq!(e, Encoding::Utf8);
    }
}
