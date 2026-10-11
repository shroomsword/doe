use doe::DoeError;
use doe::schema::compiler::*;
use doe::schema::ir::{Encoding, Endian, EndianOverride, FloatWidth, SizeExpr, TypeRegistry};
use doe::schema::ir::{FieldKind, IntWidth, RepeatMode, StrSize};
use doe::schema::raw::RawSchema;
use indoc::indoc;

fn compile(yaml: &str) -> TypeRegistry {
    let schema: RawSchema = serde_yaml::from_str(yaml).expect("YAML parse");
    compile_schema(&schema).expect("compile")
}

fn compile_err(yaml: &str) -> DoeError {
    let schema: RawSchema = serde_yaml::from_str(yaml).expect("YAML parse");
    compile_schema(&schema).expect_err("expected compile error")
}

// ── Primitive integer types ───────────────────────────────────────────────

#[test]
fn compile_u8_field() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: x
            type: u8
    "});
    let ty = reg.get("t").unwrap();
    assert!(matches!(
        ty.fields[0].kind,
        FieldKind::UInt {
            width: IntWidth::W8,
            endian: EndianOverride::Inherit
        }
    ));
}

#[test]
fn compile_u32be_field() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: x
            type: u32be
    "});
    let ty = reg.get("t").unwrap();
    assert!(matches!(
        ty.fields[0].kind,
        FieldKind::UInt {
            width: IntWidth::W32,
            endian: EndianOverride::Big
        }
    ));
}

#[test]
fn compile_u64le_field() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: x
            type: u64le
    "});
    let ty = reg.get("t").unwrap();
    assert!(matches!(
        ty.fields[0].kind,
        FieldKind::UInt {
            width: IntWidth::W64,
            endian: EndianOverride::Little
        }
    ));
}

#[test]
fn compile_i32_field() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: x
            type: i32
    "});
    let ty = reg.get("t").unwrap();
    assert!(matches!(
        ty.fields[0].kind,
        FieldKind::SInt {
            width: IntWidth::W32,
            endian: EndianOverride::Inherit
        }
    ));
}

#[test]
fn compile_i16be_field() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: x
            type: i16be
    "});
    let ty = reg.get("t").unwrap();
    assert!(matches!(
        ty.fields[0].kind,
        FieldKind::SInt {
            width: IntWidth::W16,
            endian: EndianOverride::Big
        }
    ));
}

#[test]
fn compile_all_unsigned_widths() {
    for (type_str, expected_width) in &[
        ("u8", IntWidth::W8),
        ("u16", IntWidth::W16),
        ("u32", IntWidth::W32),
        ("u64", IntWidth::W64),
    ] {
        let yaml = format!("id: t\nseq:\n  - id: x\n    type: {type_str}");
        let reg = compile(&yaml);
        let ty = reg.get("t").unwrap();
        if let FieldKind::UInt { width, .. } = ty.fields[0].kind {
            assert_eq!(&width, expected_width, "width mismatch for {type_str}");
        } else {
            panic!("expected UInt for {type_str}");
        }
    }
}

// ── Float types ───────────────────────────────────────────────────────────

#[test]
fn compile_f32_field() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: x
            type: f32
    "});
    let ty = reg.get("t").unwrap();
    assert!(matches!(
        ty.fields[0].kind,
        FieldKind::Float {
            width: FloatWidth::F32
        }
    ));
}

#[test]
fn compile_f64_field() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: x
            type: f64
    "});
    let ty = reg.get("t").unwrap();
    assert!(matches!(
        ty.fields[0].kind,
        FieldKind::Float {
            width: FloatWidth::F64
        }
    ));
}

// ── Bytes and strings ─────────────────────────────────────────────────────

#[test]
fn compile_bytes_literal_size() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: buf
            type: bytes
            size: 16
    "});
    let ty = reg.get("t").unwrap();
    assert!(matches!(
        ty.fields[0].kind,
        FieldKind::Bytes {
            size: SizeExpr::Literal(16)
        }
    ));
}

#[test]
fn compile_bytes_dynamic_size() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: len
            type: u32
          - id: buf
            type: bytes
            size: len
    "});
    let ty = reg.get("t").unwrap();
    assert!(matches!(
        ty.fields[1].kind,
        FieldKind::Bytes {
            size: SizeExpr::Dynamic(_)
        }
    ));
}

#[test]
fn compile_str_with_size() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: name
            type: str
            size: 8
            encoding: ascii
    "});
    let ty = reg.get("t").unwrap();
    if let FieldKind::Str {
        size: StrSize::Fixed(SizeExpr::Literal(8)),
        encoding,
    } = &ty.fields[0].kind
    {
        assert_eq!(*encoding, Encoding::Ascii);
    } else {
        panic!("unexpected field kind: {:?}", ty.fields[0].kind);
    }
}

#[test]
fn compile_strz() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: name
            type: strz
    "});
    let ty = reg.get("t").unwrap();
    assert!(matches!(
        ty.fields[0].kind,
        FieldKind::Str {
            size: StrSize::Terminator(0),
            ..
        }
    ));
}

#[test]
fn compile_str_with_terminator() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: name
            type: str
            terminator: 0x0a
    "});
    let ty = reg.get("t").unwrap();
    assert!(matches!(
        ty.fields[0].kind,
        FieldKind::Str {
            size: StrSize::Terminator(0x0a),
            ..
        }
    ));
}

// ── Bits ─────────────────────────────────────────────────────────────────

#[test]
fn compile_bits_field() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: flags
            type: bits
            bit_size: 4
    "});
    let ty = reg.get("t").unwrap();
    assert!(matches!(ty.fields[0].kind, FieldKind::Bits { bit_size: 4 }));
}

#[test]
fn bits_without_bit_size_is_error() {
    let err = compile_err(indoc! {r"
        id: t
        seq:
          - id: flags
            type: bits
    "});
    assert!(matches!(err, DoeError::FieldError { .. }));
}

// ── Contents ─────────────────────────────────────────────────────────────

#[test]
fn compile_contents_bytes() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: magic
            contents: [0x89, 0x50, 0x4e, 0x47]
    "});
    let ty = reg.get("t").unwrap();
    assert!(
        matches!(&ty.fields[0].kind, FieldKind::Contents(b) if b == &[0x89u8, 0x50, 0x4e, 0x47])
    );
}

#[test]
fn compile_contents_string() {
    let reg = compile(indoc! {r#"
        id: t
        seq:
          - id: magic
            contents: "RIFF"
    "#});
    let ty = reg.get("t").unwrap();
    assert!(matches!(&ty.fields[0].kind, FieldKind::Contents(b) if b == b"RIFF"));
}

// ── Repeat modes ─────────────────────────────────────────────────────────

#[test]
fn compile_repeat_eos() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: items
            type: u8
            repeat: eos
    "});
    let ty = reg.get("t").unwrap();
    assert!(matches!(ty.fields[0].repeat, RepeatMode::Eos));
}

#[test]
fn compile_repeat_expr() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: count
            type: u16
          - id: items
            type: u8
            repeat: expr
            repeat-expr: count
    "});
    let ty = reg.get("t").unwrap();
    assert!(matches!(ty.fields[1].repeat, RepeatMode::Count(_)));
}

#[test]
fn repeat_expr_without_repeat_expr_is_error() {
    let err = compile_err(indoc! {r"
        id: t
        seq:
          - id: items
            type: u8
            repeat: expr
    "});
    assert!(matches!(err, DoeError::FieldError { .. }));
}

#[test]
fn compile_repeat_until() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: items
            type: u8
            repeat: until
            repeat-until: _ == 0
    "});
    let ty = reg.get("t").unwrap();
    assert!(matches!(ty.fields[0].repeat, RepeatMode::Until(_)));
}

#[test]
fn repeat_until_without_repeat_until_is_error() {
    let err = compile_err(indoc! {r"
        id: t
        seq:
          - id: items
            type: u8
            repeat: until
    "});
    assert!(matches!(err, DoeError::FieldError { .. }));
}

// ── Conditional fields ────────────────────────────────────────────────────

#[test]
fn compile_if_expr() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: flags
            type: u8
          - id: extra
            type: u32
            if: flags > 0
    "});
    let ty = reg.get("t").unwrap();
    assert!(ty.fields[1].if_expr.is_some());
}

// ── Enums ─────────────────────────────────────────────────────────────────

#[test]
fn compile_enum_decimal_keys() {
    let reg = compile(indoc! {r#"
        id: t
        seq:
          - id: kind
            type: u8
            enum: kind_t
        enums:
          kind_t:
            "0": eof
            "1": data
            "2": header
    "#});
    let ty = reg.get("t").unwrap();
    let e = ty.enums.get("kind_t").unwrap();
    assert_eq!(e.lookup(0), Some("eof"));
    assert_eq!(e.lookup(1), Some("data"));
    assert_eq!(e.lookup(99), None);
}

#[test]
fn compile_enum_hex_keys() {
    let reg = compile(indoc! {r#"
        id: t
        seq:
          - id: type_field
            type: u8
            enum: types_t
        enums:
          types_t:
            "0x01": text
            "0xff": binary
    "#});
    let ty = reg.get("t").unwrap();
    let e = ty.enums.get("types_t").unwrap();
    assert_eq!(e.lookup(0x01), Some("text"));
    assert_eq!(e.lookup(0xff), Some("binary"));
}

#[test]
fn unknown_enum_ref_is_error() {
    let err = compile_err(indoc! {r"
        id: t
        seq:
          - id: x
            type: u8
            enum: nonexistent
    "});
    assert!(matches!(err, DoeError::UnknownEnum { .. }));
}

// ── Sub-types ─────────────────────────────────────────────────────────────

#[test]
fn compile_inline_subtype() {
    let reg = compile(indoc! {r"
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
    "});
    assert!(reg.contains("outer"));
    assert!(reg.contains("outer::header"));
    let hdr = reg.get("outer::header").unwrap();
    assert_eq!(hdr.fields.len(), 2);
    assert_eq!(hdr.fields[0].id, "magic");
}

#[test]
fn subtype_ref_resolved_correctly() {
    let reg = compile(indoc! {r"
        id: outer
        seq:
          - id: hdr
            type: header
        types:
          header:
            seq:
              - id: val
                type: u8
    "});
    let outer = reg.get("outer").unwrap();
    if let FieldKind::TypeRef { type_name } = &outer.fields[0].kind {
        assert_eq!(type_name, "outer::header");
    } else {
        panic!("expected TypeRef");
    }
}

#[test]
fn compile_nested_subtypes() {
    let reg = compile(indoc! {r"
        id: root
        seq:
          - id: a
            type: level1
        types:
          level1:
            seq:
              - id: b
                type: level2
            types:
              level2:
                seq:
                  - id: val
                    type: u32
    "});
    assert!(reg.contains("root"));
    assert!(reg.contains("root::level1"));
    assert!(reg.contains("root::level1::level2"));
}

// ── Forward-ref validation ────────────────────────────────────────────────

#[test]
fn size_expr_forward_ref_is_error() {
    // `buf` is defined before `len`, so `size: len` is a forward ref.
    let err = compile_err(indoc! {r"
        id: t
        seq:
          - id: buf
            type: bytes
            size: len
          - id: len
            type: u32
    "});
    assert!(
        matches!(err, DoeError::FieldError { .. }),
        "expected FieldError, got {err:?}"
    );
}

#[test]
fn size_expr_back_ref_is_ok() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: len
            type: u32
          - id: buf
            type: bytes
            size: len
    "});
    assert!(reg.contains("t"));
}

// ── Meta endian propagation ───────────────────────────────────────────────

#[test]
fn big_endian_propagated_to_type() {
    let reg = compile(indoc! {r"
        id: be_file
        meta:
          endian: be
        seq:
          - id: val
            type: u32
    "});
    let ty = reg.get("be_file").unwrap();
    assert_eq!(ty.endian, Endian::Big);
}

#[test]
fn little_endian_is_default() {
    let reg = compile(indoc! {r"
        id: le_file
        seq:
          - id: val
            type: u32
    "});
    let ty = reg.get("le_file").unwrap();
    assert_eq!(ty.endian, Endian::Little);
}

// ── Error cases ───────────────────────────────────────────────────────────

#[test]
fn unknown_type_ref_is_error() {
    let err = compile_err(indoc! {r"
        id: t
        seq:
          - id: x
            type: does_not_exist
    "});
    assert!(matches!(err, DoeError::UnknownType { .. }));
}

#[test]
fn field_without_type_or_contents_is_error() {
    let err = compile_err(indoc! {r"
        id: t
        seq:
          - id: x
    "});
    assert!(matches!(err, DoeError::FieldError { .. }));
}

#[test]
fn bytes_without_size_is_error() {
    let err = compile_err(indoc! {r"
        id: t
        seq:
          - id: buf
            type: bytes
    "});
    assert!(matches!(err, DoeError::FieldError { .. }));
}

// ── Realistic schemas ─────────────────────────────────────────────────────

#[test]
fn png_like_compiles() {
    let reg = compile(indoc! {r"
        id: png
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
    "});
    assert!(reg.contains("png"));
    assert!(reg.contains("png::chunk"));

    let chunk = reg.get("png::chunk").unwrap();
    assert_eq!(chunk.fields.len(), 4);

    // body.size should be dynamic (references `length`)
    assert!(matches!(
        chunk.fields[2].kind,
        FieldKind::Bytes {
            size: SizeExpr::Dynamic(_)
        }
    ));
}

#[test]
fn elf_like_compiles() {
    let reg = compile(indoc! {r#"
        id: elf
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
              - id: endian_byte
                type: u8
                enum: endian_t
              - id: pad
                type: bytes
                size: 9
              - id: e_type
                type: u16le
                enum: obj_type_t
              - id: machine
                type: u16le
            enums:
              bits_t:
                "1": b32
                "2": b64
              endian_t:
                "1": le
                "2": be
              obj_type_t:
                "0": no_file
                "2": exec
    "#});
    assert!(reg.contains("elf"));
    assert!(reg.contains("elf::elf_header"));
    let hdr = reg.get("elf::elf_header").unwrap();
    assert_eq!(hdr.enums.len(), 3);
    assert_eq!(hdr.enums["bits_t"].lookup(2), Some("b64"));
    assert_eq!(hdr.enums["obj_type_t"].lookup(2), Some("exec"));
}

// ── Enum keys ─────────────────────────────────────────────────────────────

fn enum_with_key(key: &str) -> String {
    format!("id: t\nseq: []\nenums:\n  e:\n    \"{key}\": v\n")
}

fn enum_lookup(key: &str, value: u64) -> Option<String> {
    let reg = compile(&enum_with_key(key));
    reg.get("t").unwrap().enums["e"]
        .lookup(value)
        .map(str::to_owned)
}

#[test]
fn enum_key_decimal() {
    assert_eq!(enum_lookup("0", 0).as_deref(), Some("v"));
    assert_eq!(enum_lookup("255", 255).as_deref(), Some("v"));
}

#[test]
fn enum_key_hex() {
    assert_eq!(enum_lookup("0x00", 0).as_deref(), Some("v"));
    assert_eq!(enum_lookup("0xff", 255).as_deref(), Some("v"));
    assert_eq!(enum_lookup("0xFF", 255).as_deref(), Some("v"));
    assert_eq!(enum_lookup("0x10", 16).as_deref(), Some("v"));
}

#[test]
fn enum_key_invalid_is_error() {
    for key in ["", "abc", "0xgg"] {
        let err = compile_err(&enum_with_key(key));
        assert!(matches!(err, DoeError::Schema(_)), "key {key:?}");
    }
}

// ── Duplicate type detection ──────────────────────────────────────────────

#[test]
fn duplicate_top_level_id_from_different_files_is_error() {
    // Two separately loaded in-memory schemas with the same id but
    // different source_path values should be rejected.
    use std::path::PathBuf;
    let mut schema_a: RawSchema = serde_yaml::from_str(indoc! {r"
        id: my_type
        seq:
          - id: x
            type: u8
    "})
    .unwrap();
    schema_a.source_path = Some(PathBuf::from("/types/a.yaml"));

    let mut schema_b: RawSchema = serde_yaml::from_str(indoc! {r"
        id: my_type
        seq:
          - id: y
            type: u16
    "})
    .unwrap();
    schema_b.source_path = Some(PathBuf::from("/types/b.yaml"));

    let mut compiler = Compiler::new(vec![]);
    compiler.process_schema(&schema_a).unwrap();
    let err = compiler.process_schema(&schema_b).unwrap_err();
    assert!(
        matches!(err, DoeError::DuplicateType { .. }),
        "expected DuplicateType, got {err:?}"
    );
    if let DoeError::DuplicateType {
        name,
        first,
        second,
    } = err
    {
        assert_eq!(name, "my_type");
        assert!(first.contains("a.yaml"), "first={first}");
        assert!(second.contains("b.yaml"), "second={second}");
    }
}

#[test]
fn same_schema_loaded_twice_via_import_is_idempotent() {
    // The same schema id from the same source is not a duplicate —
    // this happens when a schema is reachable via multiple import paths.
    use std::path::PathBuf;
    let mut schema: RawSchema = serde_yaml::from_str(indoc! {r"
        id: shared
        seq:
          - id: val
            type: u8
    "})
    .unwrap();
    schema.source_path = Some(PathBuf::from("/types/shared.yaml"));

    let mut compiler = Compiler::new(vec![]);
    compiler.process_schema(&schema).unwrap();
    // Processing again with the same source path should succeed silently.
    compiler.process_schema(&schema).unwrap();
    assert!(compiler.registry.contains("shared"));
}
