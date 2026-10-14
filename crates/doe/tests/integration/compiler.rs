use doe::DoeError;
use doe::schema::compiler::*;
use doe::schema::ir::{Encoding, Endian, EndianOverride, FloatWidth, SizeExpr, TypeRegistry};
use doe::schema::ir::{FieldKind, IntWidth, RepeatMode, StrSize};
use doe::schema::raw::RawSchema;
use indoc::indoc;

fn compile(yaml: &str) -> TypeRegistry {
    let schema: RawSchema = RawSchema::from_yaml_str(yaml).expect("YAML parse");
    compile_schema(&schema).expect("compile")
}

fn compile_err(yaml: &str) -> DoeError {
    let schema: RawSchema = RawSchema::from_yaml_str(yaml).expect("YAML parse");
    compile_schema(&schema).expect_err("expected compile error")
}

// ── Primitive integer types ───────────────────────────────────────────────

#[test]
fn compile_u8_field() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: x
            type: u1
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
            type: u4be
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
            type: u8le
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
            type: s4
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
            type: s2be
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
        ("u1", IntWidth::W8),
        ("u2", IntWidth::W16),
        ("u4", IntWidth::W32),
        ("u8", IntWidth::W64),
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
            type: f4
    "});
    let ty = reg.get("t").unwrap();
    assert!(matches!(
        ty.fields[0].kind,
        FieldKind::Float {
            width: FloatWidth::F32,
            endian: EndianOverride::Inherit
        }
    ));
}

#[test]
fn compile_f64_field() {
    let reg = compile(indoc! {r"
        id: t
        seq:
          - id: x
            type: f8
    "});
    let ty = reg.get("t").unwrap();
    assert!(matches!(
        ty.fields[0].kind,
        FieldKind::Float {
            width: FloatWidth::F64,
            endian: EndianOverride::Inherit
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
            type: u4
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
            type: u1
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
            type: u2
          - id: items
            type: u1
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
            type: u1
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
            type: u1
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
            type: u1
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
            type: u1
          - id: extra
            type: u4
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
            type: u1
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
            type: u1
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
            type: u1
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
                type: u4
              - id: version
                type: u2
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
                type: u1
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
                    type: u4
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
            type: u4
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
            type: u4
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
            type: u4
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
            type: u4
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
                type: u4be
              - id: type
                type: str
                size: 4
                encoding: ascii
              - id: body
                type: bytes
                size: length
              - id: crc
                type: u4be
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
                type: u1
                enum: bits_t
              - id: endian_byte
                type: u1
                enum: endian_t
              - id: pad
                type: bytes
                size: 9
              - id: e_type
                type: u2le
                enum: obj_type_t
              - id: machine
                type: u2le
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
    let mut schema_a: RawSchema = RawSchema::from_yaml_str(indoc! {r"
        id: my_type
        seq:
          - id: x
            type: u1
    "})
    .unwrap();
    schema_a.source_path = Some(PathBuf::from("/types/a.yaml"));

    let mut schema_b: RawSchema = RawSchema::from_yaml_str(indoc! {r"
        id: my_type
        seq:
          - id: y
            type: u2
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
    let mut schema: RawSchema = RawSchema::from_yaml_str(indoc! {r"
        id: shared
        seq:
          - id: val
            type: u1
    "})
    .unwrap();
    schema.source_path = Some(PathBuf::from("/types/shared.yaml"));

    let mut compiler = Compiler::new(vec![]);
    compiler.process_schema(&schema).unwrap();
    // Processing again with the same source path should succeed silently.
    compiler.process_schema(&schema).unwrap();
    assert!(compiler.registry.contains("shared"));
}

// ── Every primitive type name ─────────────────────────────────────────────

fn field_kind(type_name: &str) -> FieldKind {
    let reg = compile(&format!("id: t\nseq:\n  - id: x\n    type: {type_name}\n"));
    reg.get("t").unwrap().fields[0].kind.clone()
}

#[test]
fn every_integer_type_name_compiles_to_its_width_and_endian() {
    let cases = [
        ("u2be", IntWidth::W16, EndianOverride::Big, false),
        ("u4be", IntWidth::W32, EndianOverride::Big, false),
        ("u8be", IntWidth::W64, EndianOverride::Big, false),
        ("s2", IntWidth::W16, EndianOverride::Inherit, true),
        ("s8", IntWidth::W64, EndianOverride::Inherit, true),
        ("s2le", IntWidth::W16, EndianOverride::Little, true),
        ("s4le", IntWidth::W32, EndianOverride::Little, true),
        ("s8le", IntWidth::W64, EndianOverride::Little, true),
        ("s4be", IntWidth::W32, EndianOverride::Big, true),
        ("s8be", IntWidth::W64, EndianOverride::Big, true),
    ];
    for (name, width, endian, signed) in cases {
        match field_kind(name) {
            FieldKind::UInt {
                width: w,
                endian: e,
            } if !signed => {
                assert_eq!((w, e), (width, endian), "{name}");
            }
            FieldKind::SInt {
                width: w,
                endian: e,
            } if signed => {
                assert_eq!((w, e), (width, endian), "{name}");
            }
            other => panic!("{name} compiled to {other:?}"),
        }
    }
}

#[test]
fn int_widths_report_their_size_in_bytes() {
    assert_eq!(IntWidth::W8.bytes(), 1);
    assert_eq!(IntWidth::W16.bytes(), 2);
    assert_eq!(IntWidth::W32.bytes(), 4);
    assert_eq!(IntWidth::W64.bytes(), 8);
    assert_eq!(FloatWidth::F32.bytes(), 4);
    assert_eq!(FloatWidth::F64.bytes(), 8);
}

// ── Field validation ──────────────────────────────────────────────────────

#[test]
fn empty_contents_is_an_error() {
    for contents in ["[]", "\"\""] {
        let err = compile_err(&format!(
            "id: t\nseq:\n  - id: magic\n    contents: {contents}\n"
        ));
        assert!(
            matches!(&err, DoeError::FieldError { message, .. } if message.contains("empty")),
            "{contents}: {err:?}"
        );
    }
}

#[test]
fn size_expression_may_use_operators_and_parent_references() {
    compile(indoc! {r#"
        id: t
        seq:
          - id: len
            type: u1
          - id: data
            type: bytes
            size: "-(len) + len * 2 + 1 && !0"
    "#});
    compile(indoc! {r#"
        id: t
        seq:
          - id: len
            type: u1
          - id: data
            type: bytes
            size: "_parent.len + 1"
    "#});
}

#[test]
fn size_expression_with_forward_reference_inside_operators_is_an_error() {
    for size in ["-later", "1 + later", "len + later"] {
        let err = compile_err(&format!(
            "id: t\nseq:\n  - id: len\n    type: u1\n  - id: data\n    type: bytes\n    size: \"{size}\"\n  - id: later\n    type: u1\n"
        ));
        assert!(
            matches!(&err, DoeError::FieldError { field, .. } if field == "later"),
            "{size}: {err:?}"
        );
    }
}

// ── Type references across nesting levels ─────────────────────────────────

#[test]
fn nested_type_can_reference_a_sibling_and_an_outer_type() {
    let reg = compile(indoc! {r"
        id: outer
        seq:
          - id: a
            type: first
        types:
          first:
            seq:
              - id: s
                type: second
              - id: o
                type: outer_leaf
          second:
            seq:
              - id: x
                type: u1
          outer_leaf:
            seq:
              - id: y
                type: u1
    "});
    let first = reg.get("outer::first").unwrap();
    assert!(matches!(
        &first.fields[0].kind,
        FieldKind::TypeRef { type_name } if type_name == "outer::second"
    ));
    assert!(matches!(
        &first.fields[1].kind,
        FieldKind::TypeRef { type_name } if type_name == "outer::outer_leaf"
    ));
}

// ── Loading from disk: paths, imports, cycles ─────────────────────────────

fn write(dir: &std::path::Path, name: &str, text: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn compiler_loads_bare_name_from_include_path() {
    let dir = tempfile::TempDir::new().unwrap();
    write(dir.path(), "thing.yaml", "id: thing\nseq: []\n");
    let mut compiler = Compiler::new(vec![dir.path().to_owned()]);
    compiler.load("thing", None).unwrap();
    assert!(compiler.registry.contains("thing"));
}

#[test]
fn compiler_accepts_explicit_yaml_file_name_on_include_path() {
    let dir = tempfile::TempDir::new().unwrap();
    write(dir.path(), "thing.yaml", "id: thing\nseq: []\n");
    let mut compiler = Compiler::new(vec![dir.path().to_owned()]);
    compiler.load("thing.yaml", None).unwrap();
    assert!(compiler.registry.contains("thing"));
}

#[test]
fn missing_bare_name_lists_searched_paths() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut compiler = Compiler::new(vec![dir.path().to_owned()]);
    let err = compiler.load("nope", None).unwrap_err();
    assert!(
        matches!(&err, DoeError::SchemaNotFound { name, searched }
            if name == "nope" && searched == &[dir.path().join("nope.yaml")]),
        "{err:?}"
    );
}

#[test]
fn relative_path_is_resolved_against_the_importing_file() {
    let dir = tempfile::TempDir::new().unwrap();
    write(dir.path(), "dep.yaml", "id: dep\nseq: []\n");
    let mut compiler = Compiler::new(vec![]);
    compiler.load("./dep.yaml", Some(dir.path())).unwrap();
    assert!(compiler.registry.contains("dep"));
}

#[test]
fn missing_relative_path_reports_the_candidate() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut compiler = Compiler::new(vec![]);
    let err = compiler.load("../gone.yaml", Some(dir.path())).unwrap_err();
    assert!(
        matches!(&err, DoeError::SchemaNotFound { searched, .. } if searched.len() == 1),
        "{err:?}"
    );
}

#[test]
fn imports_are_loaded_before_the_importing_schema() {
    let dir = tempfile::TempDir::new().unwrap();
    write(
        dir.path(),
        "dep.yaml",
        "id: dep\nseq:\n  - id: v\n    type: u1\n",
    );
    write(
        dir.path(),
        "main.yaml",
        "id: main\nimports: [./dep.yaml]\nseq:\n  - id: d\n    type: dep\n",
    );
    let mut compiler = Compiler::new(vec![]);
    compiler.load("./main.yaml", Some(dir.path())).unwrap();
    assert!(compiler.registry.contains("dep"));
    assert!(compiler.registry.contains("main"));
}

#[test]
fn import_cycles_are_detected() {
    let dir = tempfile::TempDir::new().unwrap();
    write(
        dir.path(),
        "a.yaml",
        "id: a\nimports: [./b.yaml]\nseq: []\n",
    );
    write(
        dir.path(),
        "b.yaml",
        "id: b\nimports: [./a.yaml]\nseq: []\n",
    );
    let mut compiler = Compiler::new(vec![]);
    let err = compiler.load("./a.yaml", Some(dir.path())).unwrap_err();
    assert!(
        matches!(&err, DoeError::ImportCycle { cycle } if cycle.first() == cycle.last()),
        "{err:?}"
    );
}

#[test]
fn load_yaml_reads_and_records_the_source_path() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = write(dir.path(), "s.yaml", "id: s\nseq: []\n");
    let schema = load_yaml(&path).unwrap();
    assert_eq!(schema.id, "s");
    assert_eq!(schema.source_path.as_deref(), Some(path.as_path()));
}

#[test]
fn load_yaml_reports_io_and_yaml_errors() {
    let dir = tempfile::TempDir::new().unwrap();
    assert!(matches!(
        load_yaml(&dir.path().join("missing.yaml")).unwrap_err(),
        DoeError::Io { .. }
    ));
    let bad = write(dir.path(), "bad.yaml", "id: [unterminated\n");
    assert!(matches!(
        load_yaml(&bad).unwrap_err(),
        DoeError::Yaml { .. }
    ));
}

// ── Enum scope ────────────────────────────────────────────────────────────

fn lookup_in(reg: &TypeRegistry, ty: &str, enum_name: &str, value: u64) -> Option<String> {
    reg.get(ty)
        .unwrap()
        .enums
        .get(enum_name)
        .and_then(|e| e.lookup(value))
        .map(str::to_owned)
}

#[test]
fn top_level_enum_is_visible_in_sub_types() {
    let reg = compile(indoc! {r#"
        id: t
        seq:
          - id: inner
            type: child
        types:
          child:
            seq:
              - id: kind
                type: u1
                enum: kind_t
        enums:
          kind_t:
            "1": data
    "#});
    assert_eq!(
        lookup_in(&reg, "t::child", "kind_t", 1).as_deref(),
        Some("data")
    );
}

#[test]
fn top_level_enum_is_visible_in_deeply_nested_types() {
    let reg = compile(indoc! {r#"
        id: t
        seq: []
        types:
          a:
            seq: []
            types:
              b:
                seq:
                  - id: kind
                    type: u1
                    enum: kind_t
        enums:
          kind_t:
            "7": seven
    "#});
    assert_eq!(
        lookup_in(&reg, "t::a::b", "kind_t", 7).as_deref(),
        Some("seven")
    );
}

#[test]
fn inner_enum_shadows_an_enclosing_enum_of_the_same_name() {
    let reg = compile(indoc! {r#"
        id: t
        seq:
          - id: x
            type: u1
            enum: e
        types:
          child:
            seq:
              - id: y
                type: u1
                enum: e
            enums:
              e:
                "1": inner
        enums:
          e:
            "1": outer
            "2": outer_only
    "#});
    assert_eq!(lookup_in(&reg, "t", "e", 1).as_deref(), Some("outer"));
    assert_eq!(
        lookup_in(&reg, "t::child", "e", 1).as_deref(),
        Some("inner")
    );
    // The shadowing enum replaces the outer one entirely, it is not merged with it.
    assert_eq!(lookup_in(&reg, "t::child", "e", 2), None);
}

#[test]
fn enclosing_type_does_not_see_enums_of_its_sub_types() {
    let err = compile_err(indoc! {r#"
        id: t
        seq:
          - id: x
            type: u1
            enum: only_in_child
        types:
          child:
            seq: []
            enums:
              only_in_child:
                "1": a
    "#});
    assert!(matches!(err, DoeError::UnknownEnum { .. }), "{err:?}");
}

#[test]
fn sibling_types_do_not_see_each_others_enums() {
    let err = compile_err(indoc! {r#"
        id: t
        seq: []
        types:
          a:
            seq: []
            enums:
              from_a:
                "1": a
          b:
            seq:
              - id: x
                type: u1
                enum: from_a
    "#});
    assert!(matches!(err, DoeError::UnknownEnum { .. }), "{err:?}");
}

#[test]
fn nested_field_references_must_name_an_earlier_field() {
    // `header` is defined after `body`, so even a dotted path through it is a forward reference.
    let err = compile_err(indoc! {r"
        id: t
        seq:
          - id: body
            type: bytes
            size: header.length
          - id: header
            type: h
        types:
          h:
            seq:
              - id: length
                type: u1
    "});
    assert!(
        matches!(&err, DoeError::FieldError { field, .. } if field == "header"),
        "{err:?}"
    );
}

#[test]
fn nested_field_references_to_earlier_fields_compile() {
    compile(indoc! {r"
        id: t
        seq:
          - id: header
            type: h
          - id: body
            type: bytes
            size: header.length
        types:
          h:
            seq:
              - id: length
                type: u1
    "});
}

#[test]
fn compile_pos_expr() {
    let reg =
        compile("id: t\nseq:\n  - id: a\n    type: u1\n    pos: 4\n  - id: b\n    type: u1\n");
    let ty = reg.get("t").unwrap();
    assert!(ty.fields[0].pos.is_some());
    assert!(ty.fields[1].pos.is_none());
}

#[test]
fn compile_rejects_a_malformed_pos_expr() {
    let err = compile_err("id: t\nseq:\n  - id: a\n    type: u1\n    pos: \"1 +\"\n");
    assert!(matches!(err, DoeError::ExprParse { .. }), "{err:?}");
}

// ── Numeric type names ────────────────────────────────────────────────────

#[test]
fn every_width_endian_and_family_of_numeric_name_compiles() {
    for (suffix, endian) in [
        ("", EndianOverride::Inherit),
        ("le", EndianOverride::Little),
        ("be", EndianOverride::Big),
    ] {
        for (bytes, width) in [
            (1, IntWidth::W8),
            (2, IntWidth::W16),
            (4, IntWidth::W32),
            (8, IntWidth::W64),
        ] {
            let name = format!("u{bytes}{suffix}");
            assert!(
                matches!(field_kind(&name), FieldKind::UInt { width: w, endian: e } if w == width && e == endian),
                "{name}"
            );
            let name = format!("s{bytes}{suffix}");
            assert!(
                matches!(field_kind(&name), FieldKind::SInt { width: w, endian: e } if w == width && e == endian),
                "{name}"
            );
        }
        for (bytes, width) in [(4, FloatWidth::F32), (8, FloatWidth::F64)] {
            let name = format!("f{bytes}{suffix}");
            assert!(
                matches!(field_kind(&name), FieldKind::Float { width: w, endian: e } if w == width && e == endian),
                "{name}"
            );
        }
    }
}

#[test]
fn u8_means_sixty_four_bits() {
    assert!(matches!(
        field_kind("u8"),
        FieldKind::UInt {
            width: IntWidth::W64,
            ..
        }
    ));
    assert!(matches!(
        field_kind("u1"),
        FieldKind::UInt {
            width: IntWidth::W8,
            ..
        }
    ));
}

#[test]
fn old_numeric_names_are_errors_that_name_the_replacement() {
    for (old, new) in [
        ("u16", "u2"),
        ("u32", "u4"),
        ("u64", "u8"),
        ("u16le", "u2le"),
        ("u32be", "u4be"),
        ("u64le", "u8le"),
        ("i8", "s1"),
        ("i16", "s2"),
        ("i32le", "s4le"),
        ("i64be", "s8be"),
        ("f32", "f4"),
        ("f64", "f8"),
    ] {
        let err = compile_err(&format!("id: t\nseq:\n  - id: x\n    type: {old}\n"));
        assert!(
            matches!(&err, DoeError::FieldError { field, message, .. }
                if field == "x" && message.contains(&format!("'{old}' was renamed '{new}'"))),
            "{old}: {err:?}"
        );
    }
}

#[test]
fn other_numeric_looking_names_are_unknown_types() {
    for name in [
        "u3", "u0", "u16x", "s16", "f2", "f16", "i128", "u128", "U4", "uu", "u", "s", "f", "u2LE",
        "i7", "f4xe",
    ] {
        let err = compile_err(&format!("id: t\nseq:\n  - id: x\n    type: {name}\n"));
        assert!(
            matches!(&err, DoeError::UnknownType { type_name, .. } if type_name == name)
                || matches!(&err, DoeError::FieldError { message, .. } if message.contains("was renamed")),
            "{name}: {err:?}"
        );
    }
}

#[test]
fn a_user_type_may_use_an_old_primitive_name() {
    let reg = compile(indoc! {"
        id: t
        seq:
          - id: x
            type: u16
        types:
          u16:
            seq:
              - id: inner
                type: u1
    "});
    assert!(matches!(
        reg.get("t").unwrap().fields[0].kind,
        FieldKind::TypeRef { .. }
    ));
}
