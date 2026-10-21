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
    assert!(matches!(
        ty.fields[0].kind,
        FieldKind::Bits { bit_size: 4, .. }
    ));
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
    if let FieldKind::TypeRef { type_name, .. } = &outer.fields[0].kind {
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
        FieldKind::TypeRef { type_name, .. } if type_name == "outer::second"
    ));
    assert!(matches!(
        &first.fields[1].kind,
        FieldKind::TypeRef { type_name, .. } if type_name == "outer::outer_leaf"
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
        if name == "nope" && searched == &[
            dir.path().join("nope.yaml"),
            dir.path().join("nope.yml"),
            dir.path().join("nope.ksy"),
        ]),
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
fn schemas_may_import_each_other() {
    let dir = tempfile::TempDir::new().unwrap();
    write(
        dir.path(),
        "a.yaml",
        indoc! {"
            id: a
            imports: [./b.yaml]
            seq:
              - id: body
                type: b
            enums:
              kinds:
                1: one
        "},
    );
    write(
        dir.path(),
        "b.yaml",
        indoc! {"
            id: b
            imports: [./a.yaml]
            seq:
              - id: first
                type: u1
                enum: a::kinds
              - id: again
                type: a
                if: first == a::kinds::one
        "},
    );
    let mut compiler = Compiler::new(vec![]);
    compiler.load("./a.yaml", Some(dir.path())).unwrap();
    assert!(compiler.registry.contains("a") && compiler.registry.contains("b"));
    // Starting from the other one gives the same.
    let mut compiler = Compiler::new(vec![]);
    compiler.load("./b.yaml", Some(dir.path())).unwrap();
    assert!(compiler.registry.contains("a") && compiler.registry.contains("b"));
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

// ── Bit field type names ──────────────────────────────────────────────────

#[test]
fn bit_type_names_compile_to_bit_fields() {
    for (suffix, endian) in [
        ("", EndianOverride::Inherit),
        ("le", EndianOverride::Little),
        ("be", EndianOverride::Big),
    ] {
        for bits in [1u8, 3, 8, 12, 31, 32, 63, 64] {
            let name = format!("b{bits}{suffix}");
            assert!(
                matches!(field_kind(&name), FieldKind::Bits { bit_size, endian: e } if bit_size == bits && e == endian),
                "{name}"
            );
        }
    }
}

#[test]
fn the_bits_type_is_a_bit_field_that_inherits_the_bit_order() {
    let reg = compile("id: t\nseq:\n  - id: x\n    type: bits\n    bit_size: 5\n");
    assert!(matches!(
        reg.get("t").unwrap().fields[0].kind,
        FieldKind::Bits {
            bit_size: 5,
            endian: EndianOverride::Inherit
        }
    ));
}

#[test]
fn bit_sizes_outside_one_to_sixty_four_are_errors() {
    for size in [0, 65, 200] {
        let err = compile_err(&format!(
            "id: t\nseq:\n  - id: x\n    type: bits\n    bit_size: {size}\n"
        ));
        assert!(
            matches!(&err, DoeError::FieldError { message, .. } if message.contains("1 to 64 bits")),
            "{size}: {err:?}"
        );
    }
}

#[test]
fn other_b_names_are_unknown_types() {
    for name in [
        "b0", "b65", "b01", "b100", "b4x", "b4LE", "B4", "b", "bits4", "b4lele",
    ] {
        let err = compile_err(&format!("id: t\nseq:\n  - id: x\n    type: {name}\n"));
        assert!(
            matches!(&err, DoeError::UnknownType { type_name, .. } if type_name == name),
            "{name}: {err:?}"
        );
    }
}

#[test]
fn the_bit_order_defaults_to_big_and_nested_types_inherit_it() {
    let reg = compile(
        "id: t\nseq:\n  - id: a\n    type: u1\ntypes:\n  inner:\n    seq:\n      - id: b\n        type: u1\n",
    );
    assert_eq!(reg.get("t").unwrap().bit_endian, Endian::Big);
    assert_eq!(reg.get("t::inner").unwrap().bit_endian, Endian::Big);
    let reg = compile(
        "id: t\nmeta:\n  bit-endian: le\nseq:\n  - id: a\n    type: u1\ntypes:\n  inner:\n    seq:\n      - id: b\n        type: u1\n",
    );
    assert_eq!(reg.get("t").unwrap().bit_endian, Endian::Little);
    assert_eq!(reg.get("t::inner").unwrap().bit_endian, Endian::Little);
    // The bit order does not change the byte order.
    assert_eq!(reg.get("t").unwrap().endian, Endian::Little);
}

// ── size and size-eos on user types ───────────────────────────────────────

fn sized_field(attrs: &str) -> FieldKind {
    let reg = compile(&format!(
        "id: t\nseq:\n  - id: x\n    type: rec\n    {attrs}\ntypes:\n  rec:\n    seq: []\n"
    ));
    reg.get("t").unwrap().fields[0].kind.clone()
}

#[test]
fn size_on_a_user_type_compiles_to_a_window() {
    assert!(matches!(
        sized_field("size: 8"),
        FieldKind::TypeRef {
            size: Some(SizeExpr::Literal(8)),
            ..
        }
    ));
    assert!(matches!(
        sized_field("size-eos: true"),
        FieldKind::TypeRef {
            size: Some(SizeExpr::Eos),
            ..
        }
    ));
    // `size-eos: false` is the same as leaving it out.
    assert!(matches!(
        sized_field("size-eos: false"),
        FieldKind::TypeRef { size: None, .. }
    ));
    let reg = compile(
        "id: t\nseq:\n  - id: n\n    type: u1\n  - id: x\n    type: rec\n    size: n * 2\ntypes:\n  rec:\n    seq: []\n",
    );
    assert!(matches!(
        reg.get("t").unwrap().fields[1].kind,
        FieldKind::TypeRef {
            size: Some(SizeExpr::Dynamic(_)),
            ..
        }
    ));
}

#[test]
fn a_type_without_size_has_no_window() {
    let reg = compile("id: t\nseq:\n  - id: x\n    type: rec\ntypes:\n  rec:\n    seq: []\n");
    assert!(matches!(
        reg.get("t").unwrap().fields[0].kind,
        FieldKind::TypeRef { size: None, .. }
    ));
}

#[test]
fn size_expressions_on_user_types_may_only_use_earlier_fields() {
    let err = compile_err(
        "id: t\nseq:\n  - id: x\n    type: rec\n    size: later\n  - id: later\n    type: u1\ntypes:\n  rec:\n    seq: []\n",
    );
    assert!(
        matches!(&err, DoeError::FieldError { message, .. } if message.contains("later")),
        "{err:?}"
    );
}

#[test]
fn size_and_size_eos_together_are_an_error() {
    for type_name in ["rec", "bytes", "str"] {
        let err = compile_err(&format!(
            "id: t\nseq:\n  - id: x\n    type: {type_name}\n    size: 4\n    size-eos: true\ntypes:\n  rec:\n    seq: []\n"
        ));
        assert!(
            matches!(&err, DoeError::FieldError { message, .. } if message.contains("cannot be used together")),
            "{type_name}: {err:?}"
        );
    }
}

#[test]
fn bytes_and_str_accept_size_eos() {
    for type_name in ["bytes", "str"] {
        let reg = compile(&format!(
            "id: t\nseq:\n  - id: x\n    type: {type_name}\n    size-eos: true\n"
        ));
        let kind = &reg.get("t").unwrap().fields[0].kind;
        assert!(
            matches!(
                kind,
                FieldKind::Bytes {
                    size: SizeExpr::Eos
                }
            ) || matches!(
                kind,
                FieldKind::Str {
                    size: StrSize::Fixed(SizeExpr::Eos),
                    ..
                }
            ),
            "{type_name}: {kind:?}"
        );
    }
}

#[test]
fn fixed_size_types_do_not_take_a_size() {
    for type_name in ["u1", "u4le", "s2", "f8", "b4", "b12le"] {
        for attrs in ["size: 4", "size-eos: true"] {
            let err = compile_err(&format!(
                "id: t\nseq:\n  - id: x\n    type: {type_name}\n    {attrs}\n"
            ));
            assert!(
                matches!(&err, DoeError::FieldError { message, .. } if message.contains("fixed size")),
                "{type_name} {attrs}: {err:?}"
            );
        }
    }
}

// ── Enum members in expressions ───────────────────────────────────────────

const ENUM_SCHEMA: &str = indoc! {"
    id: t
    seq:
      - id: kind
        type: u1
        enum: kind_t
      - id: extra
        type: u1
        if: kind == kind_t::data
    enums:
      kind_t:
        1: data
        0x10: control
"};

fn if_expr_of(schema: &str, field: usize) -> doe::schema::expr::Expr {
    let reg = compile(schema);
    reg.get("t").unwrap().fields[field].if_expr.clone().unwrap()
}

#[test]
fn enum_members_are_replaced_by_their_values() {
    use doe::schema::expr::{BinaryOp, Expr};
    let ast = if_expr_of(ENUM_SCHEMA, 1);
    let Expr::Binary(BinaryOp::Eq, _, rhs) = ast else {
        panic!("expected a comparison, got {ast:?}");
    };
    assert_eq!(*rhs, Expr::Int(1));
    let hex = ENUM_SCHEMA.replace("kind_t::data", "kind_t::control");
    let Expr::Binary(_, _, rhs) = if_expr_of(&hex, 1) else {
        panic!("expected a comparison");
    };
    assert_eq!(*rhs, Expr::Int(16));
}

#[test]
fn enum_members_are_resolved_in_every_kind_of_expression() {
    let schema = indoc! {"
        id: t
        seq:
          - id: a
            type: u1
          - id: sized
            type: bytes
            size: 'a == k::one ? 4 : 2'
          - id: counted
            type: u1
            repeat: expr
            repeat-expr: 'k::two + [k::one, k::two].max'
          - id: until
            type: u1
            repeat: until
            repeat-until: _ == k::two
          - id: placed
            type: u1
            pos: k::two * 2
            if: '!(a == k::two)'
        enums:
          k:
            1: one
            2: two
    "};
    let reg = compile(schema);
    let ty = reg.get("t").unwrap();
    for field in &ty.fields[1..] {
        let text = format!("{field:?}");
        assert!(
            !text.contains("Enum("),
            "{} still has an enum member: {text}",
            field.id
        );
    }
}

#[test]
fn enum_members_see_the_enums_of_enclosing_types() {
    let schema = indoc! {"
        id: t
        seq:
          - id: inner
            type: rec
        types:
          rec:
            seq:
              - id: a
                type: u1
              - id: b
                type: u1
                if: a == outer_t::yes
              - id: c
                type: u1
                if: a == inner_t::yes
            enums:
              inner_t:
                9: yes
        enums:
          outer_t:
            4: yes
    "};
    let reg = compile(schema);
    let rec = reg.get("t::rec").unwrap();
    assert!(rec.fields[1].if_expr.is_some());
    assert!(rec.fields[2].if_expr.is_some());
}

#[test]
fn unknown_enums_and_members_are_schema_errors() {
    let unknown_enum = ENUM_SCHEMA.replace("kind_t::data", "nope_t::data");
    let err = compile_err(&unknown_enum);
    assert!(
        matches!(&err, DoeError::UnknownEnum { enum_name, .. } if enum_name == "nope_t"),
        "{err:?}"
    );
    let unknown_member = ENUM_SCHEMA.replace("kind_t::data", "kind_t::nope");
    let err = compile_err(&unknown_member);
    assert!(err.to_string().contains("no member 'nope'"), "{err}");
    let unknown_type = ENUM_SCHEMA.replace("kind_t::data", "nope::kind_t::data");
    let err = compile_err(&unknown_type);
    assert!(
        matches!(&err, DoeError::UnknownType { type_name, .. } if type_name == "nope"),
        "{err:?}"
    );
    let unknown_qualified = ENUM_SCHEMA.replace("kind_t::data", "t::kind_t::nope");
    let err = compile_err(&unknown_qualified);
    assert!(err.to_string().contains("no member 'nope'"), "{err}");
}

#[test]
fn enum_members_may_be_qualified_with_the_type_that_declares_the_enum() {
    // `right` is compiled after `left`, and `later` after `right`: the order
    // of the declarations does not matter.
    let schema = indoc! {"
        id: t
        seq:
          - id: a
            type: right
        types:
          right:
            seq:
              - id: y
                type: u1
                if: y == left::e::one or y == later::inner::f::two or y == t::e0::zero
          left:
            enums:
              e:
                1: one
          later:
            types:
              inner:
                enums:
                  f:
                    2: two
        enums:
          e0:
            0: zero
    "};
    let reg = compile(schema);
    let field = &reg.get("t::right").unwrap().fields[0];
    let text = format!("{:?}", field.if_expr);
    assert!(
        text.contains("Int(1)") && text.contains("Int(2)") && text.contains("Int(0)"),
        "{text}"
    );
}

#[test]
fn an_enum_of_a_sibling_type_is_not_visible() {
    let schema = indoc! {"
        id: t
        seq:
          - id: a
            type: left
          - id: b
            type: right
        types:
          left:
            seq:
              - id: x
                type: u1
                enum: e
            enums:
              e:
                1: one
          right:
            seq:
              - id: y
                type: u1
                if: y == e::one
    "};
    let err = compile_err(schema);
    assert!(
        matches!(
            err,
            DoeError::UnknownEnum { .. } | DoeError::FieldError { .. }
        ),
        "{err:?}"
    );
}

#[test]
fn size_expressions_may_use_root_parent_and_io() {
    for size in ["_io.size", "_io.size - _io.pos", "_root.n", "_parent.n"] {
        let schema = format!(
            "id: t\nseq:\n  - id: n\n    type: u1\n  - id: inner\n    type: rec\ntypes:\n  rec:\n    seq:\n      - id: x\n        type: bytes\n        size: '{size}'\n"
        );
        compile(&schema);
    }
}

// ── switch-on ─────────────────────────────────────────────────────────────

const SWITCH: &str = indoc! {"
    id: t
    seq:
      - id: kind
        type: u1
        enum: kind_t
      - id: body
        type:
          switch-on: kind
          cases:
            1: a_t
            kind_t::data: u2le
            _: bytes
        size: 4
    types:
      a_t:
        seq:
          - id: x
            type: u1
    enums:
      kind_t:
        2: data
"};

#[test]
fn a_switch_compiles_to_its_cases() {
    use doe::schema::expr::Expr;
    let reg = compile(SWITCH);
    let FieldKind::Switch {
        on,
        size,
        cases,
        default,
    } = &reg.get("t").unwrap().fields[1].kind
    else {
        panic!("expected a switch");
    };
    assert_eq!(*on, Expr::Field(vec!["kind".into()]));
    assert!(matches!(size, Some(SizeExpr::Literal(4))));
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[0].0, Expr::Int(1));
    assert!(matches!(cases[0].1, FieldKind::TypeRef { size: None, .. }));
    // The enum member is replaced by its value.
    assert_eq!(cases[1].0, Expr::Int(2));
    assert!(matches!(
        cases[1].1,
        FieldKind::UInt {
            width: IntWidth::W16,
            endian: EndianOverride::Little
        }
    ));
    // `bytes` fills the window the switch has.
    assert!(matches!(
        default.as_deref(),
        Some(FieldKind::Bytes {
            size: SizeExpr::Eos
        })
    ));
}

#[test]
fn a_switch_without_a_size_leaves_bytes_to_size_themselves() {
    let schema = SWITCH.replace("    size: 4\n", "");
    let err = compile_err(&schema);
    assert!(
        matches!(&err, DoeError::FieldError { message, .. } if message.contains("requires 'size'")),
        "{err:?}"
    );
    compile(&schema.replace("_: bytes", "_: u1"));
}

#[test]
fn malformed_switches_are_errors_that_name_the_field() {
    let cases = [
        (
            "empty cases",
            "id: t\nseq:\n  - id: kind\n    type: u1\n  - id: body\n    type:\n      switch-on: kind\n      cases: {}\n".to_owned(),
        ),
        ("unknown type", SWITCH.replace("1: a_t", "1: nope_t")),
        (
            "unknown enum",
            SWITCH.replace("kind_t::data: u2le", "nope_t::data: u2le"),
        ),
        (
            "unknown member",
            SWITCH.replace("kind_t::data: u2le", "kind_t::nope: u2le"),
        ),
        (
            "later field",
            SWITCH.replace("switch-on: kind", "switch-on: later"),
        ),
        (
            "bad expression",
            SWITCH.replace("switch-on: kind", "switch-on: '1 +'"),
        ),
    ];
    for (what, schema) in cases {
        let err = compile_err(&schema);
        let text = err.to_string();
        assert!(
            text.contains("body")
                || text.contains("nope")
                || text.contains("later")
                || text.contains("switch"),
            "{what}: {text}"
        );
    }
}

#[test]
fn size_and_size_eos_on_a_switch_are_checked_like_other_fields() {
    let both = SWITCH.replace("size: 4", "size: 4\n    size-eos: true");
    let err = compile_err(&both);
    assert!(err.to_string().contains("cannot be used together"), "{err}");
    let eos = SWITCH.replace("size: 4", "size-eos: true");
    let reg = compile(&eos);
    assert!(matches!(
        reg.get("t").unwrap().fields[1].kind,
        FieldKind::Switch {
            size: Some(SizeExpr::Eos),
            ..
        }
    ));
}

#[test]
fn a_switch_inside_a_repeat_and_with_an_enum_compiles() {
    let schema = SWITCH.replace("size: 4", "size: 4\n    repeat: eos\n    enum: kind_t");
    compile(&schema);
}

#[test]
fn a_field_with_a_size_and_no_type_is_bytes() {
    let reg = compile("id: t\nseq:\n  - id: a\n    size: 4\n  - id: b\n    size-eos: true\n");
    let ty = reg.get("t").unwrap();
    assert!(matches!(
        ty.fields[0].kind,
        FieldKind::Bytes {
            size: SizeExpr::Literal(4)
        }
    ));
    assert!(matches!(
        ty.fields[1].kind,
        FieldKind::Bytes {
            size: SizeExpr::Eos
        }
    ));
    let err = compile_err("id: t\nseq:\n  - id: a\n    doc: nothing\n");
    assert!(err.to_string().contains("neither"), "{err}");
}

// ── instances ─────────────────────────────────────────────────────────────

const WITH_INSTANCES: &str = indoc! {"
    id: t
    seq:
      - id: n
        type: u1
      - id: body
        type: bytes
        size: total
    instances:
      table:
        pos: n
        type: u1
        repeat: expr
        repeat-expr: total
      total:
        value: n * 2
        if: n > 0
      later:
        value: total + table.size
"};

#[test]
fn instances_compile_to_reads_and_values_with_their_dependencies() {
    use doe::schema::ir::InstanceKind;
    let reg = compile(WITH_INSTANCES);
    let ty = reg.get("t").unwrap();
    // A `seq` field that uses an instance names it.
    assert_eq!(ty.fields[1].uses_instances, ["total"]);
    assert_eq!(ty.fields[0].uses_instances.len(), 0);
    let ids: Vec<&str> = ty.instances.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["table", "total", "later"]);
    let InstanceKind::Read(table) = &ty.instances[0].kind else {
        panic!("expected a read instance");
    };
    assert!(table.pos.is_some());
    assert_eq!(table.uses_instances, ["total"]);
    let InstanceKind::Value {
        if_expr,
        uses_instances,
        ..
    } = &ty.instances[1].kind
    else {
        panic!("expected a value instance");
    };
    assert!(if_expr.is_some());
    assert_eq!(uses_instances.len(), 0);
    let InstanceKind::Value { uses_instances, .. } = &ty.instances[2].kind else {
        panic!("expected a value instance");
    };
    assert_eq!(*uses_instances, ["total", "table"]);
}

#[test]
fn instances_of_nested_types_are_compiled_with_them() {
    let reg = compile(indoc! {"
        id: t
        seq:
          - id: x
            type: rec
        types:
          rec:
            seq:
              - id: a
                type: u1
            instances:
              twice:
                value: a * 2
    "});
    assert_eq!(reg.get("t::rec").unwrap().instances.len(), 1);
    assert!(reg.get("t").unwrap().instances.is_empty());
}

#[test]
fn malformed_instances_are_schema_errors_that_name_the_instance() {
    let cases = [
        (
            "same name as a field",
            "id: t\nseq:\n  - id: a\n    type: u1\ninstances:\n  a:\n    value: 1\n",
            "both a field of 'seq' and an instance",
        ),
        (
            "value with a type",
            "id: t\nseq:\n  - id: a\n    type: u1\ninstances:\n  x:\n    value: 1\n    type: u1\n",
            "cannot be combined",
        ),
        (
            "value with pos",
            "id: t\nseq:\n  - id: a\n    type: u1\ninstances:\n  x:\n    value: 1\n    pos: 0\n",
            "cannot be combined",
        ),
        (
            "nothing to read or compute",
            "id: t\nseq:\n  - id: a\n    type: u1\ninstances:\n  x:\n    doc: nothing\n",
            "needs 'value'",
        ),
        (
            "bad expression",
            "id: t\nseq:\n  - id: a\n    type: u1\ninstances:\n  x:\n    value: '1 +'\n",
            "'value'",
        ),
        (
            "unknown field",
            "id: t\nseq:\n  - id: a\n    type: u1\ninstances:\n  x:\n    value: nope + 1\n",
            "nope",
        ),
        (
            "unknown type",
            "id: t\nseq:\n  - id: a\n    type: u1\ninstances:\n  x:\n    type: nope_t\n",
            "nope_t",
        ),
        (
            "unknown enum",
            "id: t\nseq:\n  - id: a\n    type: u1\ninstances:\n  x:\n    value: 1\n    enum: nope_t\n",
            "nope_t",
        ),
    ];
    for (what, schema, expected) in cases {
        let err = compile_err(schema);
        assert!(err.to_string().contains(expected), "{what}: {err}");
    }
}

#[test]
fn a_seq_field_may_use_an_instance_that_comes_later_but_not_a_later_field() {
    compile(
        "id: t\nseq:\n  - id: b\n    type: bytes\n    size: n\ninstances:\n  n:\n    value: 3\n",
    );
    let err = compile_err(
        "id: t\nseq:\n  - id: b\n    type: bytes\n    size: n\n  - id: n\n    type: u1\n",
    );
    assert!(err.to_string().contains('n'), "{err}");
}

// ── valid ─────────────────────────────────────────────────────────────────

fn valid_of(rules: &str, extra: &str) -> doe::schema::ir::CompiledValid {
    let schema = format!(
        "id: t\nseq:\n  - id: n\n    type: u1\n  - id: x\n    type: u1\n{extra}    valid:{rules}\nenums:\n  kind_t:\n    1: one\n"
    );
    compile(&schema).get("t").unwrap().fields[1]
        .valid
        .clone()
        .unwrap()
}

#[test]
fn a_bare_valid_is_eq() {
    let valid = valid_of(" 5", "");
    assert_eq!(valid.eq, Some(doe::schema::expr::Expr::Int(5)));
    assert!(valid.min.is_none() && valid.max.is_none() && valid.expr.is_none());
    assert_eq!(valid.any_of.len(), 0);
    assert!(!valid.in_enum);
}

#[test]
fn valid_rules_compile_to_expressions() {
    use doe::schema::expr::Expr;
    let valid = valid_of(
        "\n      min: 1\n      max: n\n      any-of: [1, 2]\n      expr: '_ > 0'\n",
        "",
    );
    assert_eq!(valid.min, Some(Expr::Int(1)));
    assert_eq!(valid.max, Some(Expr::Field(vec!["n".into()])));
    assert_eq!(valid.any_of, [Expr::Int(1), Expr::Int(2)]);
    assert!(valid.expr.is_some());
    let in_enum = valid_of("\n      in-enum: true\n", "    enum: kind_t\n");
    assert!(in_enum.in_enum);
    // Enum members work in the rules.
    let member = valid_of(" 'kind_t::one'", "");
    assert_eq!(member.eq, Some(Expr::Int(1)));
}

#[test]
fn malformed_valid_is_a_schema_error_that_names_the_field() {
    let cases = [
        ("empty", "\n      {}", "", "needs a value"),
        (
            "empty any-of",
            "\n      any-of: []",
            "",
            "'any-of' is empty",
        ),
        (
            "in-enum without an enum",
            "\n      in-enum: true",
            "",
            "needs 'enum'",
        ),
        ("bad expression", "\n      min: '1 +'", "", "valid"),
        ("unknown field", "\n      min: nope", "", "nope"),
        ("later field", "\n      min: later", "", "later"),
        ("unknown enum member", " 'kind_t::nope'", "", "nope"),
    ];
    for (what, rules, extra, expected) in cases {
        let schema = format!(
            "id: t\nseq:\n  - id: x\n    type: u1\n{extra}    valid:{rules}\n  - id: later\n    type: u1\nenums:\n  kind_t:\n    1: one\n"
        );
        let err = compile_err(&schema);
        assert!(err.to_string().contains(expected), "{what}: {err}");
    }
}

#[test]
fn valid_may_not_be_combined_with_value_on_an_instance() {
    let err = compile_err(
        "id: t\nseq:\n  - id: a\n    type: u1\ninstances:\n  x:\n    value: 1\n    valid: 1\n",
    );
    assert!(err.to_string().contains("cannot be combined"), "{err}");
    compile(
        "id: t\nseq:\n  - id: a\n    type: u1\ninstances:\n  x:\n    type: u1\n    pos: 0\n    valid: 1\n",
    );
}

#[test]
fn an_instance_used_by_valid_is_computed_first() {
    let reg = compile(
        "id: t\nseq:\n  - id: a\n    type: u1\n    valid:\n      max: limit\ninstances:\n  limit:\n    value: 9\n",
    );
    assert_eq!(reg.get("t").unwrap().fields[0].uses_instances, ["limit"]);
}

// ── params ────────────────────────────────────────────────────────────────

const PARAMS: &str = indoc! {"
    id: t
    seq:
      - id: n
        type: u1
      - id: first
        type: block(n * 2, 'a')
      - id: second
        type: block(4, \"b, c\")
    types:
      block:
        params:
          - id: len
            type: u4
          - id: label
            type: str
        seq:
          - id: data
            type: bytes
            size: len
"};

#[test]
fn a_type_with_params_is_called_with_arguments() {
    use doe::schema::expr::Expr;
    let reg = compile(PARAMS);
    let block = reg.get("t::block").unwrap();
    let ids: Vec<&str> = block.params.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["len", "label"]);
    let FieldKind::TypeRef {
        type_name, args, ..
    } = &reg.get("t").unwrap().fields[1].kind
    else {
        panic!("expected a type");
    };
    assert_eq!(type_name, "t::block");
    assert_eq!(args.len(), 2);
    assert!(matches!(args[0], Expr::Binary(..)));
    assert_eq!(args[1], Expr::Str("a".into()));
    // A comma inside a string does not split the arguments.
    let FieldKind::TypeRef { args, .. } = &reg.get("t").unwrap().fields[2].kind else {
        panic!("expected a type");
    };
    assert_eq!(args, &[Expr::Int(4), Expr::Str("b, c".into())]);
}

#[test]
fn arguments_may_contain_calls_brackets_and_nested_commas() {
    let schema = PARAMS.replace(
        "block(4, \\\"b, c\\\")",
        "block([1, 2].max + n, 'x'.substring(0, 1))",
    );
    let reg = compile(&schema);
    let FieldKind::TypeRef { args, .. } = &reg.get("t").unwrap().fields[2].kind else {
        panic!("expected a type");
    };
    assert_eq!(args.len(), 2);
}

#[test]
fn params_work_in_switch_cases_and_in_imported_style_types() {
    let schema = indoc! {"
        id: t
        seq:
          - id: kind
            type: u1
          - id: body
            type:
              switch-on: kind
              cases:
                1: block(3)
                _: block(5)
        types:
          block:
            params:
              - id: len
            seq:
              - id: data
                type: bytes
                size: len
    "};
    compile(schema);
}

#[test]
fn wrong_numbers_of_arguments_are_errors_that_name_the_type() {
    for (what, call, expected) in [
        ("none", "block", "takes 2 parameters, but 0 were given"),
        ("too few", "block(1)", "takes 2 parameters, but 1 was given"),
        (
            "too many",
            "block(1, 2, 3)",
            "takes 2 parameters, but 3 were given",
        ),
    ] {
        let schema = PARAMS
            .replace("block(n * 2, 'a')", call)
            .replace("block(4, \"b, c\")", "block(1, 'z')");
        let err = compile_err(&schema);
        assert!(err.to_string().contains(expected), "{what}: {err}");
    }
    let err =
        compile_err("id: t\nseq:\n  - id: a\n    type: plain(1)\ntypes:\n  plain:\n    seq: []\n");
    assert!(
        err.to_string()
            .contains("takes 0 parameters, but 1 was given"),
        "{err}"
    );
}

#[test]
fn malformed_calls_are_schema_errors() {
    for (call, expected) in [
        ("block(1, 2", "not closed"),
        ("block(1,, 2)", "empty"),
        ("block(1, )", "empty"),
        ("block(1 + , 2)", "argument"),
        ("block((1, 2)", "unbalanced"),
        ("block('a, 2)", "quotes"),
    ] {
        let schema = PARAMS.replace("block(n * 2, 'a')", call);
        let err = compile_err(&schema);
        assert!(err.to_string().contains(expected), "{call}: {err}");
    }
}

#[test]
fn arguments_on_a_primitive_are_an_error() {
    for call in ["u4(1)", "bytes(2)", "b3(1)"] {
        let schema = format!("id: t\nseq:\n  - id: a\n    type: {call}\n    size: 1\n");
        let err = compile_err(&schema);
        assert!(
            err.to_string().contains("takes 0 parameters"),
            "{call}: {err}"
        );
    }
}

#[test]
fn arguments_may_only_use_earlier_fields_instances_and_params() {
    let later = PARAMS.replace("block(n * 2, 'a')", "block(later, 'a')") + "";
    let schema = later.replace(
        "      - id: second\n",
        "      - id: later\n        type: u1\n      - id: second\n",
    );
    // `later` is defined after `first`, so `first` cannot use it.
    let err = compile_err(&schema.replace("block(later, 'a')", "block(missing, 'a')"));
    assert!(err.to_string().contains("missing"), "{err}");
    let ok = "id: t\nseq:\n  - id: a\n    type: rec(inst)\ninstances:\n  inst:\n    value: 3\ntypes:\n  rec:\n    params:\n      - id: p\n    seq:\n      - id: b\n        type: bytes\n        size: p\n";
    let reg = compile(ok);
    assert_eq!(reg.get("t").unwrap().fields[0].uses_instances, ["inst"]);
}

#[test]
fn params_may_not_share_a_name_with_fields_instances_or_each_other() {
    let cases = [
        (
            "a field",
            "params:\n      - id: len\n    seq:\n      - id: len\n        type: u1\n",
            "both a parameter",
        ),
        (
            "an instance",
            "params:\n      - id: len\n    seq: []\n    instances:\n      len:\n        value: 1\n",
            "both a parameter",
        ),
        (
            "each other",
            "params:\n      - id: len\n      - id: len\n    seq: []\n",
            "declared twice",
        ),
    ];
    for (what, body, expected) in cases {
        let schema = format!("id: t\nseq: []\ntypes:\n  inner:\n    {body}");
        let err = compile_err(&schema);
        assert!(err.to_string().contains(expected), "{what}: {err}");
    }
}

#[test]
fn params_can_be_used_in_every_kind_of_expression_of_the_type() {
    let schema = indoc! {"
        id: t
        seq:
          - id: x
            type: rec(2, 'ab')
        types:
          rec:
            params:
              - id: count
              - id: label
            seq:
              - id: items
                type: u1
                repeat: expr
                repeat-expr: count
              - id: tail
                type: bytes
                size: label.length
                if: count > 0
              - id: checked
                type: u1
                valid:
                  max: count * 100
            instances:
              doubled:
                value: count * 2
    "};
    compile(schema);
}

// ── Kaitai Struct files ───────────────────────────────────────────────────

#[test]
fn a_schema_without_an_id_or_with_differing_ids_is_an_error() {
    let mut schema = RawSchema::from_yaml_str("id: a\nseq: []\n").unwrap();
    schema.id.clear();
    let err = compile_schema(&schema).unwrap_err();
    assert!(err.to_string().contains("no id"), "{err}");
    let mut compiler = Compiler::new(vec![]);
    let err = compiler.process_schema(&schema).unwrap_err();
    assert!(err.to_string().contains("no id"), "{err}");
    let differing = RawSchema::from_yaml_str("id: a\nmeta:\n  id: b\nseq: []\n").unwrap();
    let err = Compiler::new(vec![])
        .process_schema(&differing)
        .unwrap_err();
    assert!(
        err.to_string().contains("'a'") && err.to_string().contains("'b'"),
        "{err}"
    );
}

#[test]
fn imports_may_be_listed_in_meta_and_files_may_be_ksy() {
    let dir = tempfile::TempDir::new().unwrap();
    write(
        dir.path(),
        "dep.ksy",
        "meta:\n  id: dep\nseq:\n  - id: x\n    type: u1\n",
    );
    let main = write(
        dir.path(),
        "main.ksy",
        "meta:\n  id: main\n  imports:\n    - dep\nseq:\n  - id: d\n    type: dep\n",
    );
    let mut compiler = Compiler::new(vec![]);
    compiler.process_schema(&load_yaml(&main).unwrap()).unwrap();
    assert!(compiler.registry.contains("dep") && compiler.registry.contains("main"));
}

#[test]
fn a_bare_import_is_looked_for_beside_the_importing_file_then_in_the_include_paths() {
    let beside = tempfile::TempDir::new().unwrap();
    let elsewhere = tempfile::TempDir::new().unwrap();
    write(beside.path(), "near.ksy", "meta:\n  id: near\nseq: []\n");
    write(elsewhere.path(), "far.ksy", "meta:\n  id: far\nseq: []\n");
    let main = write(
        beside.path(),
        "main.yaml",
        "id: main\nimports:\n  - near\n  - far\nseq: []\n",
    );
    let mut compiler = Compiler::new(vec![elsewhere.path().to_owned()]);
    compiler.process_schema(&load_yaml(&main).unwrap()).unwrap();
    assert!(compiler.registry.contains("near") && compiler.registry.contains("far"));
}

#[test]
fn a_leading_slash_means_the_include_paths_only() {
    let beside = tempfile::TempDir::new().unwrap();
    let include = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(include.path().join("common")).unwrap();
    write(
        &include.path().join("common"),
        "riff.ksy",
        "meta:\n  id: riff\nseq: []\n",
    );
    // A file of the same name beside the importer is not the one meant.
    std::fs::create_dir_all(beside.path().join("common")).unwrap();
    write(
        &beside.path().join("common"),
        "riff.ksy",
        "meta:\n  id: wrong\nseq: []\n",
    );
    let main = write(
        beside.path(),
        "main.yaml",
        "id: main\nimports:\n  - /common/riff\nseq: []\n",
    );
    let mut compiler = Compiler::new(vec![include.path().to_owned()]);
    compiler.process_schema(&load_yaml(&main).unwrap()).unwrap();
    assert!(compiler.registry.contains("riff"));
    assert!(!compiler.registry.contains("wrong"));
    let err = Compiler::new(vec![])
        .process_schema(&load_yaml(&main).unwrap())
        .unwrap_err();
    assert!(matches!(err, DoeError::SchemaNotFound { .. }), "{err:?}");
}

#[test]
fn meta_encoding_is_the_default_of_the_strings_that_give_none() {
    use doe::schema::ir::Encoding;
    let reg = compile(indoc! {"
        id: t
        meta:
          encoding: ASCII
        seq:
          - id: a
            type: str
            size: 1
          - id: b
            type: str
            size: 1
            encoding: latin1
          - id: c
            type: strz
        types:
          inner:
            seq:
              - id: d
                type: str
                size: 1
    "});
    let encoding =
        |type_name: &str, index: usize| match &reg.get(type_name).unwrap().fields[index].kind {
            FieldKind::Str { encoding, .. } => *encoding,
            other => panic!("expected a string, got {other:?}"),
        };
    assert_eq!(encoding("t", 0), Encoding::Ascii);
    assert_eq!(encoding("t", 1), Encoding::Latin1);
    assert_eq!(encoding("t", 2), Encoding::Ascii);
    assert_eq!(encoding("t::inner", 0), Encoding::Ascii);
}

// ── imports from memory ──────────────────────────────────────────────────────

mod memory {
    use std::path::{Path, PathBuf};

    use doe::DoeError;
    use doe::schema::compiler::Compiler;
    use doe::schema::source::{MemorySource, SchemaSource};

    fn schema(id: &str) -> String {
        format!("meta:\n  id: {id}\nseq: []\n")
    }

    fn schema_importing(id: &str, imports: &[&str]) -> String {
        let list = imports
            .iter()
            .fold(String::new(), |list, i| list + "  - " + i + "\n");
        format!("meta:\n  id: {id}\n  imports:\n{list}seq: []\n")
    }

    /// Compile `main` from `files`, with `include` as the include paths.
    fn compile(
        files: MemorySource,
        include: &[&str],
        main: &str,
    ) -> Result<doe::schema::ir::TypeRegistry, DoeError> {
        let include = include.iter().map(PathBuf::from).collect();
        let mut compiler = Compiler::with_source(include, Box::new(files));
        let root = compiler.read_schema(Path::new(main))?;
        compiler.process_schema(&root)?;
        Ok(compiler.registry)
    }

    #[test]
    fn paths_are_compared_as_a_filesystem_would() {
        let mut files = MemorySource::new();
        files.insert("a/b.ksy", "x");
        assert!(files.exists(Path::new("a/b.ksy")));
        assert!(files.exists(Path::new("./a/b.ksy")));
        assert!(files.exists(Path::new("a/./b.ksy")));
        assert!(files.exists(Path::new("a/c/../b.ksy")));
        assert!(!files.exists(Path::new("b.ksy")));
        assert!(!files.exists(Path::new("../a/b.ksy")));
        assert_eq!(files.read(Path::new("a/c/../b.ksy")).unwrap(), "x");
        assert!(matches!(
            files.read(Path::new("nope")),
            Err(DoeError::Io { .. })
        ));
        assert_eq!((files.len(), files.is_empty()), (1, false));
    }

    #[test]
    fn a_name_is_looked_for_beside_the_importing_file_then_in_the_include_paths() {
        let files = MemorySource::new()
            .with_file("proj/main.ksy", schema_importing("main", &["near", "far"]))
            .with_file("proj/near.ksy", schema("near"))
            .with_file("lib/far.ksy", schema("far"));
        let registry = compile(files, &["lib"], "proj/main.ksy").unwrap();
        assert!(registry.contains("near") && registry.contains("far"));
    }

    #[test]
    fn a_relative_name_is_relative_to_the_importing_file() {
        let files = MemorySource::new()
            .with_file(
                "proj/sub/main.ksy",
                schema_importing("main", &["./sibling.ksy", "../up/other.ksy"]),
            )
            .with_file("proj/sub/sibling.ksy", schema("sibling"))
            .with_file("proj/up/other.ksy", schema("other"));
        let registry = compile(files, &[], "proj/sub/main.ksy").unwrap();
        assert!(registry.contains("sibling") && registry.contains("other"));
    }

    #[test]
    fn a_leading_slash_means_the_include_paths_only() {
        let files = MemorySource::new()
            .with_file("proj/main.ksy", schema_importing("main", &["/common/riff"]))
            .with_file("lib/common/riff.ksy", schema("riff"))
            .with_file("proj/common/riff.ksy", schema("wrong"));
        let registry = compile(files, &["lib"], "proj/main.ksy").unwrap();
        assert!(registry.contains("riff"));
        assert!(!registry.contains("wrong"));
    }

    #[test]
    fn the_extension_is_tried_and_a_given_one_is_kept() {
        let files = MemorySource::new()
            .with_file("main.ksy", schema_importing("main", &["a", "b.yaml"]))
            .with_file("a.yaml", schema("a"))
            .with_file("b.yaml", schema("b"));
        let registry = compile(files, &[], "main.ksy").unwrap();
        assert!(registry.contains("a") && registry.contains("b"));
    }

    #[test]
    fn schemas_may_import_each_other() {
        let files = MemorySource::new()
            .with_file("a.ksy", schema_importing("a", &["b"]))
            .with_file("b.ksy", schema_importing("b", &["a"]));
        let registry = compile(files, &[], "a.ksy").unwrap();
        assert!(registry.contains("a") && registry.contains("b"));
    }

    #[test]
    fn a_missing_import_names_it_and_what_was_searched() {
        let files =
            MemorySource::new().with_file("proj/main.ksy", schema_importing("main", &["gone"]));
        let err = compile(files, &["lib"], "proj/main.ksy").unwrap_err();
        let DoeError::SchemaNotFound { name, searched } = err else {
            panic!("not SchemaNotFound: {err:?}");
        };
        assert_eq!(name, "gone");
        let searched: Vec<String> = searched.iter().map(|p| p.display().to_string()).collect();
        assert!(
            searched
                .iter()
                .any(|p| p.contains("proj") && p.contains("gone.ksy"))
        );
        assert!(
            searched
                .iter()
                .any(|p| p.contains("lib") && p.contains("gone.ksy"))
        );
    }

    #[test]
    fn the_same_id_from_two_files_is_still_an_error() {
        let files = MemorySource::new()
            .with_file("main.ksy", schema_importing("main", &["x", "y"]))
            .with_file("x.ksy", schema("same"))
            .with_file("y.ksy", schema("same"));
        let err = compile(files, &[], "main.ksy").unwrap_err();
        assert!(matches!(err, DoeError::DuplicateType { .. }), "{err:?}");
    }

    #[test]
    fn a_file_that_is_not_a_schema_names_its_path() {
        let files = MemorySource::new().with_file("bad.ksy", "seq: [unclosed");
        let err = compile(files, &[], "bad.ksy").unwrap_err();
        let DoeError::Yaml { path, .. } = err else {
            panic!("not Yaml: {err:?}");
        };
        assert_eq!(path, Path::new("bad.ksy"));
    }
}

#[test]
fn a_compiler_keeps_its_auto_traits() {
    fn all<T: Send + Sync + std::panic::UnwindSafe + std::panic::RefUnwindSafe>() {}
    all::<Compiler>();
    all::<doe::schema::source::MemorySource>();
    all::<doe::schema::source::FsSource>();
}
