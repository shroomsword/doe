use std::collections::HashMap;

use doe::schema::expr::{BinaryOp, Expr};
use doe::schema::ir::*;
use doe::schema::ir::{Encoding, Endian, EndianOverride, FloatWidth, SizeExpr, TypeRegistry};

// ── TypeRegistry ─────────────────────────────────────────────────────────

#[test]
fn registry_insert_and_get() {
    let mut reg = TypeRegistry::new();
    let ty = CompiledType {
        name: "foo".into(),
        doc: None,
        endian: Endian::Little,
        bit_endian: Endian::Big,
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
    assert_eq!(IntWidth::W8.bytes(), 1);
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
    let e = Endian::default();
    assert_eq!(e, Endian::Little);
}

// ── FieldKind construction ────────────────────────────────────────────────

#[test]
fn field_kind_uint_inherit() {
    let fk = FieldKind::UInt {
        width: IntWidth::W32,
        endian: EndianOverride::Inherit,
    };
    assert!(matches!(
        fk,
        FieldKind::UInt {
            width: IntWidth::W32,
            endian: EndianOverride::Inherit
        }
    ));
}

#[test]
fn field_kind_typeref() {
    let fk = FieldKind::TypeRef {
        type_name: "png::chunk".into(),
        size: None,
    };
    if let FieldKind::TypeRef { type_name, .. } = &fk {
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
        kind: FieldKind::Bytes {
            size: SizeExpr::Literal(4),
        },
        repeat: RepeatMode::Once,
        if_expr: None,
        pos: None,
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
        kind: FieldKind::UInt {
            width: IntWidth::W32,
            endian: EndianOverride::Inherit,
        },
        repeat: RepeatMode::Once,
        if_expr: Some(cond),
        pos: None,
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
            kind: FieldKind::UInt {
                width: IntWidth::W32,
                endian: EndianOverride::Inherit,
            },
            repeat: RepeatMode::Once,
            if_expr: None,
            pos: None,
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
            pos: None,
            enum_ref: None,
        },
    ];
    let ty = CompiledType {
        name: "chunk".into(),
        doc: Some("A data chunk".into()),
        endian: Endian::Big,
        bit_endian: Endian::Big,
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
    let e = Encoding::default();
    assert_eq!(e, Encoding::Utf8);
}
