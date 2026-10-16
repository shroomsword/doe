use doe::parser::cursor::Cursor;
use doe::parser::engine::*;
use doe::schema::compiler::compile_schema;
use doe::schema::ir::TypeRegistry;
use doe::{DoeError, Value};
use indoc::indoc;

// ── Test helpers ─────────────────────────────────────────────────────────

fn engine_from_yaml(yaml: &str) -> (Engine<'static>, &'static TypeRegistry) {
    // Leak the registry so the engine can borrow it with 'static lifetime
    // in tests.  This is only acceptable in test code.
    let schema = doe::schema::raw::RawSchema::from_yaml_str(yaml).unwrap();
    let reg = Box::leak(Box::new(compile_schema(&schema).unwrap()));
    let engine = Engine::new(reg);
    (engine, reg)
}

fn parse(yaml: &str, type_name: &str, data: &[u8]) -> Value {
    let (engine, _) = engine_from_yaml(yaml);
    let mut cursor = Cursor::new(data);
    engine.parse_type(type_name, &mut cursor).unwrap()
}

fn parse_err(yaml: &str, type_name: &str, data: &[u8]) -> DoeError {
    let (engine, _) = engine_from_yaml(yaml);
    let mut cursor = Cursor::new(data);
    engine.parse_type(type_name, &mut cursor).unwrap_err()
}

fn get_field<'a>(value: &'a Value, name: &str) -> &'a Value {
    if let Value::Struct { fields, .. } = value {
        fields
            .iter()
            .find(|(k, _)| k == name)
            .map_or_else(|| panic!("field '{name}' not found"), |(_, v)| v)
    } else {
        panic!("expected Struct, got {value:?}")
    }
}

// ── Unsigned integers ─────────────────────────────────────────────────────

#[test]
fn parse_u8() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: x
            type: u1
    "},
        "t",
        &[0x42],
    );
    assert_eq!(get_field(&v, "x"), &Value::UInt(0x42));
}

#[test]
fn parse_u16_le() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: x
            type: u2le
    "},
        "t",
        &[0x34, 0x12],
    );
    assert_eq!(get_field(&v, "x"), &Value::UInt(0x1234));
}

#[test]
fn parse_u16_be() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: x
            type: u2be
    "},
        "t",
        &[0x12, 0x34],
    );
    assert_eq!(get_field(&v, "x"), &Value::UInt(0x1234));
}

#[test]
fn parse_u32_le() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: x
            type: u4le
    "},
        "t",
        &[0x78, 0x56, 0x34, 0x12],
    );
    assert_eq!(get_field(&v, "x"), &Value::UInt(0x1234_5678));
}

#[test]
fn parse_u32_be() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: x
            type: u4be
    "},
        "t",
        &[0x12, 0x34, 0x56, 0x78],
    );
    assert_eq!(get_field(&v, "x"), &Value::UInt(0x1234_5678));
}

#[test]
fn parse_u64_le() {
    let n: u64 = 0xdead_beef_cafe_0001;
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: x
            type: u8le
    "},
        "t",
        &n.to_le_bytes(),
    );
    assert_eq!(get_field(&v, "x"), &Value::UInt(n));
}

#[test]
fn parse_u8_inherits_meta_endian() {
    // u8 endian doesn't matter, but verify meta: be doesn't break anything
    let v = parse(
        indoc! {r"
        id: t
        meta:
          endian: be
        seq:
          - id: x
            type: u1
    "},
        "t",
        &[0xff],
    );
    assert_eq!(get_field(&v, "x"), &Value::UInt(255));
}

#[test]
fn parse_u16_inherits_be_meta() {
    let v = parse(
        indoc! {r"
        id: t
        meta:
          endian: be
        seq:
          - id: x
            type: u2
    "},
        "t",
        &[0x01, 0x00],
    );
    assert_eq!(get_field(&v, "x"), &Value::UInt(0x0100));
}

// ── Signed integers ───────────────────────────────────────────────────────

#[test]
fn parse_i8_positive() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: x
            type: s1
    "},
        "t",
        &[0x7f],
    );
    assert_eq!(get_field(&v, "x"), &Value::SInt(127));
}

#[test]
fn parse_i8_negative() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: x
            type: s1
    "},
        "t",
        &[0xff],
    );
    assert_eq!(get_field(&v, "x"), &Value::SInt(-1));
}

#[test]
fn parse_i32_le_negative() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: x
            type: s4le
    "},
        "t",
        &(-100i32).to_le_bytes(),
    );
    assert_eq!(get_field(&v, "x"), &Value::SInt(-100));
}

#[test]
fn parse_i64_be_min() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: x
            type: s8be
    "},
        "t",
        &i64::MIN.to_be_bytes(),
    );
    assert_eq!(get_field(&v, "x"), &Value::SInt(i64::MIN));
}

// ── Floats ────────────────────────────────────────────────────────────────

#[test]
fn parse_f32_le() {
    let f: f32 = 1.5;
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: x
            type: f4
    "},
        "t",
        &f.to_le_bytes(),
    );
    if let Value::Float(got) = get_field(&v, "x") {
        assert!((got - 1.5).abs() < 1e-6);
    } else {
        panic!("expected Float");
    }
}

#[test]
fn parse_f64_be() {
    let f: f64 = std::f64::consts::E;
    let v = parse(
        indoc! {r"
        id: t
        meta:
          endian: be
        seq:
          - id: x
            type: f8
    "},
        "t",
        &f.to_be_bytes(),
    );
    if let Value::Float(got) = get_field(&v, "x") {
        assert!((got - std::f64::consts::E).abs() < 1e-15);
    } else {
        panic!("expected Float");
    }
}

// ── Bytes ─────────────────────────────────────────────────────────────────

#[test]
fn parse_bytes_literal_size() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: buf
            type: bytes
            size: 4
    "},
        "t",
        &[0xde, 0xad, 0xbe, 0xef],
    );
    assert_eq!(
        get_field(&v, "buf"),
        &Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef])
    );
}

#[test]
fn parse_bytes_dynamic_size() {
    let mut data = vec![0x03u8]; // length = 3
    data.extend_from_slice(&[0xaa, 0xbb, 0xcc]); // 3 bytes of data
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: length
            type: u1
          - id: buf
            type: bytes
            size: length
    "},
        "t",
        &data,
    );
    assert_eq!(get_field(&v, "buf"), &Value::Bytes(vec![0xaa, 0xbb, 0xcc]));
}

#[test]
fn parse_bytes_zero_length() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: len
            type: u1
          - id: buf
            type: bytes
            size: len
    "},
        "t",
        &[0x00],
    );
    assert_eq!(get_field(&v, "buf"), &Value::Bytes(vec![]));
}

// ── Strings ───────────────────────────────────────────────────────────────

#[test]
fn parse_str_fixed_size() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: name
            type: str
            size: 5
            encoding: ascii
    "},
        "t",
        b"hello",
    );
    assert_eq!(get_field(&v, "name"), &Value::Str("hello".into()));
}

#[test]
fn parse_strz_null_terminated() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: name
            type: strz
    "},
        "t",
        b"hello\x00world",
    );
    assert_eq!(get_field(&v, "name"), &Value::Str("hello".into()));
}

#[test]
fn parse_str_with_terminator() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: line
            type: str
            terminator: 10
    "},
        "t",
        b"line1\nrest",
    );
    assert_eq!(get_field(&v, "line"), &Value::Str("line1".into()));
}

#[test]
fn parse_str_latin1() {
    // byte 0xe9 is 'é' in latin-1
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: s
            type: str
            size: 3
            encoding: latin1
    "},
        "t",
        &[0x63, 0x61, 0xe9],
    ); // "café" without the f
    assert_eq!(get_field(&v, "s"), &Value::Str("caé".into()));
}

#[test]
fn parse_str_utf8() {
    let s = "héllo";
    let bytes = s.as_bytes();
    let yaml = format!(
        indoc! {r"
        id: t
        seq:
          - id: s
            type: str
            size: {}
    "},
        bytes.len()
    );
    let v = parse(&yaml, "t", bytes);
    assert_eq!(get_field(&v, "s"), &Value::Str("héllo".into()));
}

// ── Bits ─────────────────────────────────────────────────────────────────

#[test]
fn parse_bits_4() {
    // 0xab = 0b1010_1011; top 4 bits = 0b1010 = 10
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: nibble
            type: bits
            bit_size: 4
    "},
        "t",
        &[0xab],
    );
    assert_eq!(get_field(&v, "nibble"), &Value::UInt(10));
}

#[test]
fn parse_bits_8() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: byte_val
            type: bits
            bit_size: 8
    "},
        "t",
        &[0xff],
    );
    assert_eq!(get_field(&v, "byte_val"), &Value::UInt(255));
}

#[test]
fn parse_bits_1() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: flag
            type: bits
            bit_size: 1
    "},
        "t",
        &[0x80],
    ); // top bit set
    // A one-bit field is a boolean.
    assert_eq!(get_field(&v, "flag"), &Value::Bool(true));
}

// ── Contents ─────────────────────────────────────────────────────────────

#[test]
fn parse_contents_match() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: magic
            contents: [0x89, 0x50, 0x4e, 0x47]
    "},
        "t",
        &[0x89, 0x50, 0x4e, 0x47],
    );
    assert_eq!(
        get_field(&v, "magic"),
        &Value::Bytes(vec![0x89, 0x50, 0x4e, 0x47])
    );
}

#[test]
fn parse_contents_mismatch_is_error() {
    let err = parse_err(
        indoc! {r"
        id: t
        seq:
          - id: magic
            contents: [0x89, 0x50, 0x4e, 0x47]
    "},
        "t",
        &[0x00, 0x00, 0x00, 0x00],
    );
    assert!(matches!(err, DoeError::Schema(_)));
}

// ── Enums ─────────────────────────────────────────────────────────────────

#[test]
fn parse_enum_known_variant() {
    let v = parse(
        indoc! {r#"
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
    "#},
        "t",
        &[0x01],
    );
    assert_eq!(
        get_field(&v, "kind"),
        &Value::Enum {
            value: 1,
            name: Some("data".into())
        }
    );
}

#[test]
fn parse_enum_unknown_variant() {
    let v = parse(
        indoc! {r#"
        id: t
        seq:
          - id: kind
            type: u1
            enum: kind_t
        enums:
          kind_t:
            "0": eof
    "#},
        "t",
        &[0x99],
    );
    assert_eq!(
        get_field(&v, "kind"),
        &Value::Enum {
            value: 0x99,
            name: None
        }
    );
}

// ── Conditional fields ────────────────────────────────────────────────────

#[test]
fn conditional_field_present() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: flags
            type: u1
          - id: extra
            type: u4le
            if: flags > 0
    "},
        "t",
        &[0x01, 0x78, 0x56, 0x34, 0x12],
    );
    assert_eq!(get_field(&v, "extra"), &Value::UInt(0x1234_5678));
}

#[test]
fn conditional_field_absent() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: flags
            type: u1
          - id: extra
            type: u4le
            if: flags > 0
    "},
        "t",
        &[0x00],
    ); // flags = 0 → extra is absent
    assert_eq!(get_field(&v, "extra"), &Value::Absent);
}

// ── Repeat modes ─────────────────────────────────────────────────────────

#[test]
fn repeat_eos() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: items
            type: u1
            repeat: eos
    "},
        "t",
        &[0x01, 0x02, 0x03],
    );
    if let Value::Array(items) = get_field(&v, "items") {
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], Value::UInt(1));
        assert_eq!(items[2], Value::UInt(3));
    } else {
        panic!("expected Array");
    }
}

#[test]
fn repeat_eos_empty_input() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: items
            type: u1
            repeat: eos
    "},
        "t",
        &[],
    );
    if let Value::Array(items) = get_field(&v, "items") {
        assert_eq!(items.as_slice(), []);
    } else {
        panic!("expected Array");
    }
}

#[test]
fn repeat_expr_count() {
    let data = [0x03u8, 0x0a, 0x0b, 0x0c]; // count=3, items=[10,11,12]
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: count
            type: u1
          - id: items
            type: u1
            repeat: expr
            repeat-expr: count
    "},
        "t",
        &data,
    );
    if let Value::Array(items) = get_field(&v, "items") {
        assert_eq!(items.len(), 3);
        assert_eq!(items[1], Value::UInt(0x0b));
    } else {
        panic!("expected Array");
    }
}

#[test]
fn repeat_expr_zero_count() {
    let data = [0x00u8]; // count=0
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: count
            type: u1
          - id: items
            type: u1
            repeat: expr
            repeat-expr: count
    "},
        "t",
        &data,
    );
    if let Value::Array(items) = get_field(&v, "items") {
        assert_eq!(items.as_slice(), []);
    } else {
        panic!("expected Array");
    }
}

#[test]
fn repeat_until_null_terminator() {
    // Classic C-string style: read bytes until value == 0x00.
    // The terminator is included as the last item in the array.
    let data = [0x41u8, 0x42, 0x43, 0x00]; // "ABC\0"
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: chars
            type: u1
            repeat: until
            repeat-until: _ == 0
    "},
        "t",
        &data,
    );
    if let Value::Array(items) = get_field(&v, "chars") {
        assert_eq!(items.len(), 4); // A, B, C, and the terminator
        assert_eq!(items[0], Value::UInt(0x41));
        assert_eq!(items[3], Value::UInt(0x00)); // terminator included
    } else {
        panic!("expected Array");
    }
}

#[test]
fn repeat_until_sentinel_value() {
    // Read u16le values until one equals 0xFFFF.
    let data: Vec<u8> = vec![
        0x01, 0x00, // 1
        0x02, 0x00, // 2
        0xFF, 0xFF, // sentinel
    ];
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: values
            type: u2le
            repeat: until
            repeat-until: _ == 0xFFFF
    "},
        "t",
        &data,
    );
    if let Value::Array(items) = get_field(&v, "values") {
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], Value::UInt(1));
        assert_eq!(items[1], Value::UInt(2));
        assert_eq!(items[2], Value::UInt(0xFFFF)); // sentinel included
    } else {
        panic!("expected Array");
    }
}

#[test]
fn repeat_until_expression_with_bitwise() {
    // Stop when the high bit of a byte is set.
    let data = [0x01u8, 0x02, 0x83]; // 0x83 has high bit set
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: bytes
            type: u1
            repeat: until
            repeat-until: _ & 0x80
    "},
        "t",
        &data,
    );
    if let Value::Array(items) = get_field(&v, "bytes") {
        assert_eq!(items.len(), 3);
        assert_eq!(items[2], Value::UInt(0x83));
    } else {
        panic!("expected Array");
    }
}

#[test]
fn repeat_until_stops_at_eof_if_condition_never_met() {
    // If the terminator condition is never true, iteration stops at EOF.
    let data = [0x01u8, 0x02, 0x03]; // no zero byte
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: items
            type: u1
            repeat: until
            repeat-until: _ == 0
    "},
        "t",
        &data,
    );
    if let Value::Array(items) = get_field(&v, "items") {
        assert_eq!(items.len(), 3);
    } else {
        panic!("expected Array");
    }
}

// ── Sub-types ─────────────────────────────────────────────────────────────

#[test]
fn parse_subtype() {
    let data = [0x01u8, 0x00, 0x02, 0x00]; // two u16le values
    let v = parse(
        indoc! {r"
        id: outer
        seq:
          - id: point
            type: point_t
        types:
          point_t:
            seq:
              - id: x
                type: u2le
              - id: y
                type: u2le
    "},
        "outer",
        &data,
    );
    let point = get_field(&v, "point");
    assert_eq!(get_field(point, "x"), &Value::UInt(1));
    assert_eq!(get_field(point, "y"), &Value::UInt(2));
}

#[test]
fn parse_repeated_subtype() {
    // 2 points × 4 bytes = 8 bytes, prefixed with count byte
    let v = parse(
        indoc! {r"
        id: outer
        seq:
          - id: count
            type: u1
          - id: points
            type: point_t
            repeat: expr
            repeat-expr: count
        types:
          point_t:
            seq:
              - id: x
                type: u2le
              - id: y
                type: u2le
    "},
        "outer",
        &[2, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00],
    );
    if let Value::Array(points) = get_field(&v, "points") {
        assert_eq!(points.len(), 2);
        assert_eq!(get_field(&points[0], "x"), &Value::UInt(1));
        assert_eq!(get_field(&points[1], "y"), &Value::UInt(4));
    } else {
        panic!("expected Array");
    }
}

// ── Multiple fields ───────────────────────────────────────────────────────

#[test]
fn parse_multiple_fields_sequential() {
    let v = parse(
        indoc! {r"
        id: t
        seq:
          - id: a
            type: u1
          - id: b
            type: u2le
          - id: c
            type: u4le
    "},
        "t",
        &[0x01, 0x03, 0x00, 0x07, 0x00, 0x00, 0x00],
    );
    assert_eq!(get_field(&v, "a"), &Value::UInt(1));
    assert_eq!(get_field(&v, "b"), &Value::UInt(3));
    assert_eq!(get_field(&v, "c"), &Value::UInt(7));
}

// ── Realistic: TGA-like header ────────────────────────────────────────────

#[test]
fn parse_tga_like_header() {
    // Minimal TGA header layout
    let data = vec![
        0x00u8, // id_length
        0x00,   // colormap_type
        0x02,   // image_type (truecolor)
        0x00, 0x00, // colormap_first_entry
        0x00, 0x00, // colormap_length
        0x00, // colormap_entry_size
        0x00, 0x00, // x_origin
        0x00, 0x00, // y_origin
        0x80, 0x02, // width = 640
        0xe0, 0x01, // height = 480
        0x18, // pixel_depth = 24
        0x00, // image_descriptor
    ];
    let v = parse(
        indoc! {r#"
        id: tga_header
        meta:
          endian: le
        seq:
          - id: id_length
            type: u1
          - id: colormap_type
            type: u1
          - id: image_type
            type: u1
            enum: image_type_t
          - id: colormap_first_entry
            type: u2
          - id: colormap_length
            type: u2
          - id: colormap_entry_size
            type: u1
          - id: x_origin
            type: u2
          - id: y_origin
            type: u2
          - id: width
            type: u2
          - id: height
            type: u2
          - id: pixel_depth
            type: u1
          - id: image_descriptor
            type: u1
        enums:
          image_type_t:
            "0": no_image
            "1": colormap
            "2": truecolor
            "3": grayscale
    "#},
        "tga_header",
        &data,
    );

    assert_eq!(get_field(&v, "width"), &Value::UInt(640));
    assert_eq!(get_field(&v, "height"), &Value::UInt(480));
    assert_eq!(get_field(&v, "pixel_depth"), &Value::UInt(24));
    assert_eq!(
        get_field(&v, "image_type"),
        &Value::Enum {
            value: 2,
            name: Some("truecolor".into())
        }
    );
}

// ── Realistic: length-prefixed string list ────────────────────────────────

#[test]
fn parse_length_prefixed_strings() {
    // Format: u8 count, then (u8 len, bytes) for each string
    let data: Vec<u8> = vec![
        0x02, // count = 2
        0x05, // str 0: len = 5
        b'h', b'e', b'l', b'l', b'o', 0x03, // str 1: len = 3
        b'f', b'o', b'o',
    ];
    let v = parse(
        indoc! {r"
        id: string_list
        seq:
          - id: count
            type: u1
          - id: strings
            type: lp_string
            repeat: expr
            repeat-expr: count
        types:
          lp_string:
            seq:
              - id: length
                type: u1
              - id: value
                type: str
                size: length
                encoding: ascii
    "},
        "string_list",
        &data,
    );

    if let Value::Array(strings) = get_field(&v, "strings") {
        assert_eq!(strings.len(), 2);
        assert_eq!(get_field(&strings[0], "value"), &Value::Str("hello".into()));
        assert_eq!(get_field(&strings[1], "value"), &Value::Str("foo".into()));
    } else {
        panic!("expected Array");
    }
}

// ── Error cases ───────────────────────────────────────────────────────────

#[test]
fn unknown_type_name_is_error() {
    let (engine, _reg) = engine_from_yaml(indoc! {r"
        id: t
        seq: []
    "});
    let mut cursor = Cursor::new(&[]);
    assert!(engine.parse_type("nonexistent", &mut cursor).is_err());
}

#[test]
fn buffer_underrun_is_error() {
    let err = parse_err(
        indoc! {r"
        id: t
        seq:
          - id: x
            type: u4le
    "},
        "t",
        &[0x01, 0x02],
    ); // only 2 bytes, need 4
    assert!(matches!(err, DoeError::Schema(_)));
}

// ── String decoding ───────────────────────────────────────────────────────

fn str_schema(encoding: &str, size: usize) -> String {
    format!("id: t\nseq:\n  - id: s\n    type: str\n    size: {size}\n    encoding: {encoding}\n")
}

#[test]
fn decode_ascii_valid() {
    let v = parse(&str_schema("ascii", 5), "t", b"hello");
    assert_eq!(get_field(&v, "s"), &Value::Str("hello".into()));
}

#[test]
fn decode_ascii_high_byte_is_error() {
    parse_err(&str_schema("ascii", 1), "t", &[0x80]);
}

#[test]
fn decode_utf8_valid() {
    let s = "héllo";
    let v = parse(&str_schema("utf8", s.len()), "t", s.as_bytes());
    assert_eq!(get_field(&v, "s"), &Value::Str(s.into()));
}

#[test]
fn decode_utf8_invalid_is_error() {
    parse_err(&str_schema("utf8", 2), "t", &[0xff, 0xfe]);
}

#[test]
fn decode_latin1_high_bytes() {
    // 0xe9 = 'é' in latin-1 = U+00E9
    let v = parse(&str_schema("latin1", 1), "t", &[0xe9]);
    assert_eq!(get_field(&v, "s"), &Value::Str("é".into()));
}

// ── Endianness resolution ─────────────────────────────────────────────────

fn u16_with(meta_endian: &str, field_type: &str) -> Value {
    let yaml =
        format!("id: t\nmeta:\n  endian: {meta_endian}\nseq:\n  - id: x\n    type: {field_type}\n");
    get_field(&parse(&yaml, "t", &[0x01, 0x02]), "x").clone()
}

#[test]
fn inherit_uses_type_default() {
    assert_eq!(u16_with("be", "u2"), Value::UInt(0x0102));
    assert_eq!(u16_with("le", "u2"), Value::UInt(0x0201));
}

#[test]
fn override_ignores_type_default() {
    assert_eq!(u16_with("be", "u2le"), Value::UInt(0x0201));
    assert_eq!(u16_with("le", "u2be"), Value::UInt(0x0102));
}

// ── Every integer width and byte order ────────────────────────────────────

fn one_field(type_name: &str, data: &[u8]) -> Value {
    let yaml = format!("id: t\nseq:\n  - id: x\n    type: {type_name}\n");
    get_field(&parse(&yaml, "t", data), "x").clone()
}

#[test]
fn unsigned_widths_and_byte_orders() {
    assert_eq!(one_field("u2be", &[1, 2]), Value::UInt(0x0102));
    assert_eq!(one_field("u2le", &[1, 2]), Value::UInt(0x0201));
    assert_eq!(one_field("u4be", &[1, 2, 3, 4]), Value::UInt(0x0102_0304));
    assert_eq!(one_field("u4le", &[1, 2, 3, 4]), Value::UInt(0x0403_0201));
    assert_eq!(
        one_field("u8be", &[1, 2, 3, 4, 5, 6, 7, 8]),
        Value::UInt(0x0102_0304_0506_0708)
    );
    assert_eq!(
        one_field("u8le", &[1, 2, 3, 4, 5, 6, 7, 8]),
        Value::UInt(0x0807_0605_0403_0201)
    );
}

#[test]
fn signed_widths_and_byte_orders() {
    assert_eq!(one_field("s1", &[0xff]), Value::SInt(-1));
    assert_eq!(one_field("s2le", &[0xfe, 0xff]), Value::SInt(-2));
    assert_eq!(one_field("s2be", &[0xff, 0xfe]), Value::SInt(-2));
    assert_eq!(
        one_field("s4le", &[0xfd, 0xff, 0xff, 0xff]),
        Value::SInt(-3)
    );
    assert_eq!(
        one_field("s4be", &[0xff, 0xff, 0xff, 0xfd]),
        Value::SInt(-3)
    );
    assert_eq!(one_field("s8le", &(-4i64).to_le_bytes()), Value::SInt(-4));
    assert_eq!(one_field("s8be", &(-4i64).to_be_bytes()), Value::SInt(-4));
}

#[test]
fn signed_types_inherit_meta_endian() {
    for (endian, bytes) in [("be", [0xff, 0xfe]), ("le", [0xfe, 0xff])] {
        let yaml = format!("id: t\nmeta:\n  endian: {endian}\nseq:\n  - id: x\n    type: s2\n");
        assert_eq!(get_field(&parse(&yaml, "t", &bytes), "x"), &Value::SInt(-2));
    }
}

#[test]
fn floats_follow_meta_endian() {
    let single_be = 1.5f32.to_be_bytes();
    let double_le = (-2.5f64).to_le_bytes();
    let yaml = "id: t\nmeta:\n  endian: be\nseq:\n  - id: x\n    type: f4\n";
    assert_eq!(
        get_field(&parse(yaml, "t", &single_be), "x"),
        &Value::Float(1.5)
    );
    let yaml = "id: t\nmeta:\n  endian: le\nseq:\n  - id: x\n    type: f8\n";
    assert_eq!(
        get_field(&parse(yaml, "t", &double_le), "x"),
        &Value::Float(-2.5)
    );
}

// ── Bits, repeats and sizes: edge cases ───────────────────────────────────

#[test]
fn bits_wider_than_a_u64_are_masked_to_64_bits() {
    let yaml = "id: t\nseq:\n  - id: x\n    type: bits\n    bit_size: 64\n";
    assert_eq!(
        get_field(&parse(yaml, "t", &[0xff; 8]), "x"),
        &Value::UInt(u64::MAX)
    );
}

#[test]
fn negative_repeat_count_is_an_error() {
    let yaml = indoc! {r"
        id: t
        seq:
          - id: n
            type: s1
          - id: items
            type: u1
            repeat: expr
            repeat-expr: n
    "};
    let err = parse_err(yaml, "t", &[0xff, 1, 2]);
    assert!(
        matches!(&err, DoeError::FieldError { message, .. } if message.contains("negative")),
        "{err:?}"
    );
}

#[test]
fn negative_size_is_an_error() {
    let yaml = indoc! {r"
        id: t
        seq:
          - id: n
            type: s1
          - id: data
            type: bytes
            size: n
    "};
    let err = parse_err(yaml, "t", &[0xff, 1, 2]);
    assert!(
        matches!(&err, DoeError::Schema(m) if m.contains("negative")),
        "{err:?}"
    );
}

#[test]
fn huge_repeat_count_does_not_preallocate_or_panic() {
    let yaml = indoc! {r"
        id: t
        seq:
          - id: n
            type: u8
          - id: items
            type: u1
            repeat: expr
            repeat-expr: n
    "};
    let mut data = i64::MAX.to_le_bytes().to_vec();
    data.extend_from_slice(&[1, 2, 3]);
    assert!(matches!(parse_err(yaml, "t", &data), DoeError::Schema(_)));
}

#[test]
fn huge_byte_size_is_a_buffer_error_not_a_panic() {
    let yaml = indoc! {r"
        id: t
        seq:
          - id: n
            type: u8
          - id: data
            type: bytes
            size: n
    "};
    let mut data = i64::MAX.to_le_bytes().to_vec();
    data.push(0);
    assert!(matches!(parse_err(yaml, "t", &data), DoeError::Schema(_)));
}

#[test]
fn repeat_until_over_non_integer_values_stops_at_end_of_input() {
    let yaml = indoc! {r#"
        id: t
        seq:
          - id: names
            type: strz
            repeat: until
            repeat-until: "0"
    "#};
    let v = parse(yaml, "t", b"ab\0cd\0");
    let Value::Array(items) = get_field(&v, "names") else {
        panic!("expected an array");
    };
    assert_eq!(items.len(), 2);
}

#[test]
fn missing_nested_type_in_registry_is_unknown_type_error() {
    use doe::schema::ir::{
        CompiledField, CompiledType, Endian, FieldKind, RepeatMode, TypeRegistry,
    };
    let mut reg = TypeRegistry::new();
    reg.insert(
        "t".into(),
        CompiledType {
            name: "t".into(),
            doc: None,
            endian: Endian::Little,
            bit_endian: Endian::Big,
            params: vec![],
            instances: vec![],
            fields: vec![CompiledField {
                id: "child".into(),
                doc: None,
                kind: FieldKind::TypeRef {
                    type_name: "missing".into(),
                    size: None,
                    args: vec![],
                },
                repeat: RepeatMode::Once,
                if_expr: None,
                pos: None,
                uses_instances: vec![],
                valid: None,
                enum_ref: None,
            }],
            enums: std::collections::HashMap::new(),
        },
    );
    let engine = Engine::new(&reg);
    let mut cursor = Cursor::new(&[]);
    let err = engine.parse_type("t", &mut cursor).unwrap_err();
    assert!(
        matches!(err, DoeError::UnknownType { ref type_name, .. } if type_name == "missing"),
        "{err:?}"
    );
}

#[test]
fn field_errors_name_the_field_and_type() {
    let yaml = indoc! {r"
        id: t
        seq:
          - id: n
            type: u1
          - id: items
            type: u1
            repeat: expr
            repeat-expr: no_such_field
    "};
    let err = parse_err(yaml, "t", &[1, 2]);
    assert!(
        matches!(&err, DoeError::FieldError { field, context, .. }
            if field == "items" && context == "t"),
        "{err:?}"
    );
}

// ── Resource limits (regressions for hangs and overflows found by fuzzing) ─

fn assert_field_error(err: &DoeError, needle: &str) {
    assert!(
        matches!(err, DoeError::FieldError { message, .. } if message.contains(needle)),
        "{err:?}"
    );
}

#[test]
fn self_referencing_type_is_an_error_not_a_stack_overflow() {
    let yaml = "id: t\nseq:\n  - id: x\n    type: t\n";
    assert_field_error(&parse_err(yaml, "t", &[1, 2, 3]), "nested more than");
}

#[test]
fn mutually_recursive_types_are_an_error_not_a_stack_overflow() {
    let yaml = indoc! {r"
        id: t
        seq:
          - id: a
            type: u
        types:
          u:
            seq:
              - id: b
                type: t
    "};
    assert_field_error(&parse_err(yaml, "t", &[1, 2, 3]), "nested more than");
}

#[test]
fn nesting_up_to_the_limit_still_parses() {
    use std::fmt::Write;

    // A chain of types, each holding the next, ending in a leaf byte.
    let depth = MAX_TYPE_DEPTH - 1;
    let mut yaml = String::from("id: t0\nseq:\n  - id: next\n    type: t1\ntypes:\n");
    for i in 1..depth {
        write!(
            yaml,
            "  t{i}:\n    seq:\n      - id: next\n        type: t{}\n",
            i + 1
        )
        .unwrap();
    }
    write!(
        yaml,
        "  t{depth}:\n    seq:\n      - id: leaf\n        type: u1\n"
    )
    .unwrap();
    let (engine, _) = engine_from_yaml(&yaml);
    let mut cursor = Cursor::new(&[9]);
    assert!(engine.parse_type("t0", &mut cursor).is_ok());
}

#[test]
fn repeat_eos_of_an_empty_type_is_an_error_not_an_endless_loop() {
    let yaml = indoc! {r"
        id: t
        seq:
          - id: x
            type: empty
            repeat: eos
        types:
          empty:
            seq: []
    "};
    assert_field_error(&parse_err(yaml, "t", &[1, 2, 3]), "consumed no bytes");
}

#[test]
fn repeat_until_of_an_empty_type_is_an_error() {
    let yaml = indoc! {r#"
        id: t
        seq:
          - id: x
            type: empty
            repeat: until
            repeat-until: "0"
        types:
          empty:
            seq: []
    "#};
    assert_field_error(&parse_err(yaml, "t", &[1, 2, 3]), "consumed no bytes");
}

#[test]
fn repeat_expr_of_zero_size_bytes_is_an_error() {
    let yaml = indoc! {r"
        id: t
        seq:
          - id: x
            type: bytes
            size: 0
            repeat: expr
            repeat-expr: 1000000000
    "};
    assert_field_error(&parse_err(yaml, "t", &[1]), "consumed no bytes");
}

#[test]
fn repeat_eos_over_one_byte_elements_still_works() {
    let yaml = "id: t\nseq:\n  - id: x\n    type: u1\n    repeat: eos\n";
    let v = parse(yaml, "t", &[1, 2, 3]);
    let Value::Array(items) = get_field(&v, "x") else {
        panic!("expected an array");
    };
    assert_eq!(items.len(), 3);
}

// ── Enum scope at parse time ──────────────────────────────────────────────

#[test]
fn sub_type_field_uses_a_top_level_enum() {
    // The README's sub-types example: the enum is declared on the schema and
    // used by a field of a sub-type.
    let yaml = indoc! {r#"
        id: t
        seq:
          - id: header
            type: header_t
        types:
          header_t:
            seq:
              - id: bitness
                type: u1
                enum: bits_t
        enums:
          bits_t:
            "1": b32
            "2": b64
    "#};
    let v = parse(yaml, "t", &[2]);
    let Value::Struct { fields, .. } = get_field(&v, "header") else {
        panic!("expected a struct");
    };
    assert_eq!(
        fields[0].1,
        Value::Enum {
            value: 2,
            name: Some("b64".into())
        }
    );
}

// ── References to fields of a parsed sub-type ─────────────────────────────

fn array_len(v: &Value, field: &str) -> usize {
    let Value::Array(items) = get_field(v, field) else {
        panic!("{field} is not an array");
    };
    items.len()
}

#[test]
fn repeat_count_can_come_from_a_field_of_a_sub_type() {
    // The README's sub-types example.
    let yaml = indoc! {r"
        id: t
        seq:
          - id: header
            type: header_t
          - id: sections
            type: section_t
            repeat: expr
            repeat-expr: header.section_count
        types:
          header_t:
            seq:
              - id: bitness
                type: u1
              - id: section_count
                type: u2le
          section_t:
            seq:
              - id: size
                type: u1
    "};
    let v = parse(yaml, "t", &[2, 3, 0, 10, 20, 30]);
    assert_eq!(array_len(&v, "sections"), 3);
}

#[test]
fn size_can_come_from_a_field_of_a_sub_type() {
    let yaml = indoc! {r"
        id: t
        seq:
          - id: header
            type: header_t
          - id: body
            type: bytes
            size: header.length - 1
        types:
          header_t:
            seq:
              - id: length
                type: u1
    "};
    let v = parse(yaml, "t", &[4, 0xaa, 0xbb, 0xcc]);
    assert_eq!(get_field(&v, "body"), &Value::Bytes(vec![0xaa, 0xbb, 0xcc]));
}

#[test]
fn references_can_go_through_several_levels_of_sub_types() {
    let yaml = indoc! {r"
        id: t
        seq:
          - id: outer
            type: outer_t
          - id: items
            type: u1
            repeat: expr
            repeat-expr: outer.inner.count
        types:
          outer_t:
            seq:
              - id: inner
                type: inner_t
          inner_t:
            seq:
              - id: count
                type: u1
    "};
    let v = parse(yaml, "t", &[2, 7, 8]);
    assert_eq!(array_len(&v, "items"), 2);
}

#[test]
fn enum_fields_of_a_sub_type_compare_by_value() {
    let yaml = indoc! {r#"
        id: t
        seq:
          - id: header
            type: header_t
          - id: extra
            type: u1
            if: header.kind == 2
        types:
          header_t:
            seq:
              - id: kind
                type: u1
                enum: kind_t
            enums:
              kind_t:
                "2": big
    "#};
    assert!(!get_field(&parse(yaml, "t", &[2, 9]), "extra").is_absent());
    assert!(get_field(&parse(yaml, "t", &[1, 9]), "extra").is_absent());
}

#[test]
fn a_sub_type_can_read_a_field_of_its_parents_sub_type() {
    let yaml = indoc! {r"
        id: t
        seq:
          - id: header
            type: header_t
          - id: body
            type: body_t
        types:
          header_t:
            seq:
              - id: count
                type: u1
          body_t:
            seq:
              - id: items
                type: u1
                repeat: expr
                repeat-expr: _parent.header.count
    "};
    let v = parse(yaml, "t", &[2, 5, 6]);
    let Value::Struct { fields, .. } = get_field(&v, "body") else {
        panic!("expected a struct");
    };
    assert!(matches!(&fields[0].1, Value::Array(items) if items.len() == 2));
}

#[test]
fn unknown_or_non_integer_sub_type_fields_are_errors() {
    for (reference, expected) in [
        ("header.nope", "no field 'nope'"),
        ("header", "expected an integer, found a struct"),
        ("header.name", "expected an integer, found a bytes"),
    ] {
        let yaml = format!(
            "id: t\nseq:\n  - id: header\n    type: header_t\n  - id: body\n    type: bytes\n    size: {reference}\ntypes:\n  header_t:\n    seq:\n      - id: name\n        type: bytes\n        size: 1\n"
        );
        let err = parse_err(&yaml, "t", &[1, 2, 3]);
        assert!(
            matches!(&err, DoeError::ExprEval { message, .. } if message.contains(expected)),
            "{reference}: {err:?}"
        );
    }
}

// ── pos ──────────────────────────────────────────────────────────────────

#[test]
fn pos_reads_at_the_offset_and_restores_the_position() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: offset
            type: u1
          - id: far
            type: u1
            pos: offset
          - id: next
            type: u1
    "};
    let v = parse(yaml, "t", &[4, 0xaa, 0xbb, 0xcc, 0xdd]);
    assert_eq!(get_field(&v, "far").as_int(), Some(0xdd));
    // `next` continues right after `offset`, not after `far`.
    assert_eq!(get_field(&v, "next").as_int(), Some(0xaa));
}

#[test]
fn pos_accepts_a_literal_and_applies_to_every_repeat() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: items
            type: u1
            pos: 2
            repeat: expr
            repeat-expr: 3
          - id: first
            type: u1
    "};
    let v = parse(yaml, "t", &[9, 8, 1, 2, 3]);
    let Value::Array(items) = get_field(&v, "items") else {
        panic!("expected array");
    };
    assert_eq!(items.len(), 3);
    assert_eq!(get_field(&v, "first").as_int(), Some(9));
}

#[test]
fn pos_is_skipped_with_a_false_if_guard() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: flag
            type: u1
          - id: far
            type: u1
            pos: 1000
            if: flag
    "};
    let v = parse(yaml, "t", &[0]);
    assert!(matches!(get_field(&v, "far"), Value::Absent));
}

#[test]
fn pos_inside_a_sub_type_is_relative_to_the_file() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: skip
            type: u1
          - id: inner
            type: rec
        types:
          rec:
            seq:
              - id: a
                type: u1
              - id: b
                type: u1
                pos: 0
    "};
    let v = parse(yaml, "t", &[7, 8]);
    let inner = get_field(&v, "inner");
    assert_eq!(get_field(inner, "a").as_int(), Some(8));
    assert_eq!(get_field(inner, "b").as_int(), Some(7));
}

#[test]
fn pos_past_the_end_is_a_field_error() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: far
            type: u1
            pos: 5
    "};
    for data in [&[1u8, 2][..], &[]] {
        let err = parse_err(yaml, "t", data);
        assert!(
            matches!(&err, DoeError::FieldError { field, .. } if field == "far"),
            "{err:?}"
        );
    }
}

#[test]
fn pos_to_exactly_the_end_fails_only_when_reading() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: far
            type: u1
            pos: 2
    "};
    let err = parse_err(yaml, "t", &[1, 2]);
    assert!(
        matches!(&err, DoeError::Schema(m) if m.contains("buffer overrun")),
        "{err:?}"
    );
}

#[test]
fn negative_or_huge_pos_is_a_field_error() {
    for pos in ["0 - 1", "0x7fffffffffffffff", "offset - 5"] {
        let yaml = format!(
            "id: t\nseq:\n  - id: offset\n    type: u1\n  - id: far\n    type: u1\n    pos: \"{pos}\"\n"
        );
        let err = parse_err(&yaml, "t", &[1, 2, 3]);
        assert!(
            matches!(&err, DoeError::FieldError { field, .. } if field == "far"),
            "{pos}: {err:?}"
        );
    }
}

#[test]
fn repeated_elements_that_only_reread_make_no_progress() {
    // The `pos:` read is undone, so an element made only of it consumes
    // nothing and the repeat guard still applies.
    let yaml = indoc! {"
        id: t
        seq:
          - id: items
            type: rec
            repeat: expr
            repeat-expr: 1000000
        types:
          rec:
            seq:
              - id: a
                type: u1
                pos: 0
    "};
    let err = parse_err(yaml, "t", &[1, 2, 3]);
    assert!(
        matches!(&err, DoeError::FieldError { message, .. } if message.contains("no bytes")),
        "{err:?}"
    );
}

#[test]
fn rereading_a_large_region_for_every_element_is_bounded() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: items
            type: rec
            repeat: eos
        types:
          rec:
            seq:
              - id: tick
                type: u1
              - id: whole
                type: bytes
                size: 2048
                pos: 0
    "};
    let err = parse_err(yaml, "t", &[0u8; 2048]);
    assert!(
        matches!(&err, DoeError::FieldError { message, .. } if message.contains("in total")),
        "{err:?}"
    );
}

// ── Large byte sequences ─────────────────────────────────────────────────

fn blob_schema() -> &'static str {
    indoc! {"
        id: t
        seq:
          - id: len
            type: u4le
          - id: body
            type: bytes
            size: len
          - id: after
            type: u1
    "}
}

fn blob_input(len: usize) -> Vec<u8> {
    let mut data = u32::try_from(len).unwrap().to_le_bytes().to_vec();
    data.extend((0..len).map(|i| u8::try_from(i % 251).unwrap()));
    data.push(0x5a);
    data
}

#[test]
fn bytes_up_to_the_limit_are_kept() {
    let v = parse(blob_schema(), "t", &blob_input(MAX_INLINE_BYTES));
    assert!(matches!(get_field(&v, "body"), Value::Bytes(b) if b.len() == MAX_INLINE_BYTES));
    assert_eq!(get_field(&v, "after").as_int(), Some(0x5a));
}

#[test]
fn longer_bytes_keep_a_position_a_length_and_a_preview() {
    let data = blob_input(MAX_INLINE_BYTES + 1);
    let v = parse(blob_schema(), "t", &data);
    let Value::Blob {
        offset,
        len,
        preview,
    } = get_field(&v, "body")
    else {
        panic!("expected a blob, got {:?}", get_field(&v, "body"));
    };
    assert_eq!(*offset, 4);
    assert_eq!(*len, u64::try_from(MAX_INLINE_BYTES + 1).unwrap());
    assert_eq!(preview.as_slice(), &data[4..4 + BLOB_PREVIEW_LEN]);
    // Parsing carries on after the whole sequence.
    assert_eq!(get_field(&v, "after").as_int(), Some(0x5a));
    assert_eq!(get_field(&v, "body").byte_len(), Some(MAX_INLINE_BYTES + 1));
}

#[test]
fn a_blob_prints_briefly() {
    let v = parse(blob_schema(), "t", &blob_input(1_000_000));
    assert_eq!(
        format!("{}", get_field(&v, "body")),
        "<1000000 bytes at 0x4>"
    );
    let json = doe::render::json::to_json(get_field(&v, "body"));
    assert_eq!(json["len"], 1_000_000);
    assert_eq!(json["offset"], 4);
    assert_eq!(json["truncated"], true);
    assert_eq!(json["hex"].as_str().unwrap().len(), BLOB_PREVIEW_LEN * 2);
}

#[test]
fn bytes_longer_than_the_input_are_an_error_without_allocating_them() {
    for size in [100_000u64, 1 << 33, 1 << 62] {
        let yaml = format!("id: t\nseq:\n  - id: b\n    type: bytes\n    size: {size}\n");
        for data in [vec![0u8; 10], vec![0u8; 5000]] {
            let err = parse_err(&yaml, "t", &data);
            assert!(
                matches!(&err, DoeError::Schema(m) if m.contains("overrun")),
                "{size}: {err:?}"
            );
        }
    }
}

#[test]
fn fixed_size_strings_are_bounded() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: s
            type: str
            size: 20000000
    "};
    let err = parse_err(yaml, "t", &[0u8; 10]);
    assert!(
        matches!(&err, DoeError::Schema(m) if m.contains("longer than the limit")),
        "{err:?}"
    );
}

#[test]
fn a_huge_repeat_count_does_not_reserve_memory_for_it() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: n
            type: u4le
          - id: items
            type: u1
            repeat: expr
            repeat-expr: n
    "};
    let mut data = 0x7fff_ffffu32.to_le_bytes().to_vec();
    data.extend(vec![1u8; 3_000_000]);
    let err = parse_err(yaml, "t", &data);
    assert!(
        matches!(&err, DoeError::Schema(m) if m.contains("overrun")),
        "{err:?}"
    );
}

// ── Typed expressions in schemas ─────────────────────────────────────────

#[test]
fn if_can_compare_a_parsed_string() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: tag
            type: str
            size: 3
            encoding: ascii
          - id: abc_only
            type: u1
            if: tag == 'abc'
          - id: long_tag
            type: u1
            if: tag.length > 3
    "};
    let v = parse(yaml, "t", b"abc\x07");
    assert_eq!(get_field(&v, "abc_only").as_int(), Some(7));
    assert!(get_field(&v, "long_tag").is_absent());
    let v = parse(yaml, "t", b"xyz\x07");
    assert!(get_field(&v, "abc_only").is_absent());
}

#[test]
fn a_size_can_come_from_the_length_of_a_parsed_value() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: raw
            type: bytes
            size: 2
          - id: again
            type: bytes
            size: raw.size * 2
    "};
    let v = parse(yaml, "t", &[1, 2, 3, 4, 5, 6]);
    assert_eq!(get_field(&v, "again").byte_len(), Some(4));
}

#[test]
fn a_repeat_count_can_come_from_the_size_of_an_array() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: first
            type: u1
            repeat: expr
            repeat-expr: 3
          - id: second
            type: u1
            repeat: expr
            repeat-expr: first.size
    "};
    let v = parse(yaml, "t", &[1, 2, 3, 9, 8, 7]);
    let Value::Array(items) = get_field(&v, "second") else {
        panic!("expected an array");
    };
    assert_eq!(items.len(), 3);
}

#[test]
fn u64_values_above_i64_max_compare_exactly() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: big
            type: u8le
          - id: marker
            type: u1
            if: big > 9223372036854775807
          - id: exact
            type: u1
            if: big == 0xffffffffffffffff
    "};
    let v = parse(yaml, "t", &[0xff; 10]);
    assert_eq!(get_field(&v, "marker").as_int(), Some(0xff));
    assert_eq!(get_field(&v, "exact").as_int(), Some(0xff));
}

#[test]
fn a_size_can_be_chosen_with_a_conditional() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: wide
            type: u1
          - id: body
            type: bytes
            size: 'wide != 0 ? 4 : 2'
    "};
    assert_eq!(
        get_field(&parse(yaml, "t", &[1, 1, 2, 3, 4]), "body").byte_len(),
        Some(4)
    );
    assert_eq!(
        get_field(&parse(yaml, "t", &[0, 1, 2]), "body").byte_len(),
        Some(2)
    );
}

#[test]
fn a_float_field_can_be_used_in_a_condition() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: ratio
            type: f4
          - id: tail
            type: u1
            if: ratio > 1
    "};
    let mut big = 1.5f32.to_le_bytes().to_vec();
    big.push(9);
    assert_eq!(get_field(&parse(yaml, "t", &big), "tail").as_int(), Some(9));
    let mut small = 0.5f32.to_le_bytes().to_vec();
    small.push(9);
    assert!(get_field(&parse(yaml, "t", &small), "tail").is_absent());
}

#[test]
fn repeat_until_can_test_a_field_of_a_struct_element() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: records
            type: rec
            repeat: until
            repeat-until: _.len == 0
          - id: after
            type: u1
        types:
          rec:
            seq:
              - id: len
                type: u1
              - id: pad
                type: u1
    "};
    let v = parse(yaml, "t", &[2, 0, 1, 0, 0, 0, 7]);
    let Value::Array(items) = get_field(&v, "records") else {
        panic!("expected an array");
    };
    assert_eq!(items.len(), 3);
    assert_eq!(get_field(&v, "after").as_int(), Some(7));
}

#[test]
fn repeat_until_can_test_a_string_element() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: names
            type: str
            size: 1
            encoding: ascii
            repeat: until
            repeat-until: _ == 'z'
    "};
    let v = parse(yaml, "t", b"abzc");
    let Value::Array(items) = get_field(&v, "names") else {
        panic!("expected an array");
    };
    assert_eq!(items.len(), 3);
}

#[test]
fn an_expression_of_the_wrong_type_is_an_error() {
    let cases = [
        ("size: \"'abc'\"", "bytes"),
        ("size: \"1.5\"", "bytes"),
        ("size: \"true\"", "bytes"),
        ("pos: \"[1]\"\n    size: 1", "bytes"),
        ("repeat: expr\n    repeat-expr: \"'a'\"", "u1"),
    ];
    for (attrs, ty) in cases {
        let yaml = format!("id: t\nseq:\n  - id: f\n    type: {ty}\n    {attrs}\n");
        let err = parse_err(&yaml, "t", &[1, 2, 3]);
        assert!(
            matches!(&err, DoeError::ExprEval { message, .. } if message.contains("expected an integer"))
                || matches!(&err, DoeError::FieldError { field, message, .. } if field == "f" && message.contains("expected an integer")),
            "{attrs}: {err:?}"
        );
    }
}

#[test]
fn an_if_expression_that_is_not_a_condition_is_an_error() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: f
            type: u1
            if: \"'text'\"
    "};
    let err = parse_err(yaml, "t", &[1]);
    assert!(
        matches!(&err, DoeError::FieldError { field, message, .. } if field == "f" && message.contains("expected a boolean")),
        "{err:?}"
    );
}

// ── Numeric type names ───────────────────────────────────────────────────

#[test]
fn floats_take_an_explicit_byte_order() {
    let be = 1.5f32.to_be_bytes();
    let le = 1.5f32.to_le_bytes();
    assert_eq!(one_field("f4be", &be), Value::Float(1.5));
    assert_eq!(one_field("f4le", &le), Value::Float(1.5));
    assert_eq!(
        one_field("f8be", &(-2.25f64).to_be_bytes()),
        Value::Float(-2.25)
    );
    assert_eq!(
        one_field("f8le", &(-2.25f64).to_le_bytes()),
        Value::Float(-2.25)
    );
}

#[test]
fn floats_without_a_suffix_use_the_schema_byte_order() {
    for (endian, bytes) in [("be", 0.75f64.to_be_bytes()), ("le", 0.75f64.to_le_bytes())] {
        let yaml = format!("id: t\nmeta:\n  endian: {endian}\nseq:\n  - id: x\n    type: f8\n");
        assert_eq!(
            get_field(&parse(&yaml, "t", &bytes), "x"),
            &Value::Float(0.75)
        );
    }
}

#[test]
fn u1_is_a_byte_and_u8_is_sixty_four_bits() {
    assert_eq!(one_field("u1", &[0xab]), Value::UInt(0xab));
    assert_eq!(
        one_field("u8le", &[1, 0, 0, 0, 0, 0, 0, 0x80]),
        Value::UInt(0x8000_0000_0000_0001)
    );
    assert_eq!(one_field("s1", &[0xff]), Value::SInt(-1));
    assert_eq!(one_field("s2be", &[0xff, 0xfe]), Value::SInt(-2));
}

// ── Bit fields ───────────────────────────────────────────────────────────

fn nibbles_schema(extra_meta: &str, kinds: [&str; 2]) -> String {
    format!(
        "id: t\n{extra_meta}seq:\n  - id: hi\n    type: {}\n  - id: lo\n    type: {}\n  - id: next\n    type: u1\n",
        kinds[0], kinds[1]
    )
}

#[test]
fn consecutive_bit_fields_share_a_byte() {
    let v = parse(&nibbles_schema("", ["b4", "b4"]), "t", &[0xab, 0x07]);
    assert_eq!(get_field(&v, "hi"), &Value::UInt(0xa));
    assert_eq!(get_field(&v, "lo"), &Value::UInt(0xb));
    assert_eq!(get_field(&v, "next"), &Value::UInt(7));
}

#[test]
fn bit_endian_le_takes_the_low_bits_first() {
    let schema = nibbles_schema("meta:\n  bit-endian: le\n", ["b4", "b4"]);
    let v = parse(&schema, "t", &[0xab, 0x07]);
    assert_eq!(get_field(&v, "hi"), &Value::UInt(0xb));
    assert_eq!(get_field(&v, "lo"), &Value::UInt(0xa));
}

#[test]
fn a_field_can_choose_its_own_bit_order() {
    let v = parse(&nibbles_schema("", ["b4le", "b4le"]), "t", &[0xab, 0x07]);
    assert_eq!(get_field(&v, "hi"), &Value::UInt(0xb));
    assert_eq!(get_field(&v, "lo"), &Value::UInt(0xa));
    let schema = nibbles_schema("meta:\n  bit-endian: le\n", ["b4be", "b4be"]);
    let v = parse(&schema, "t", &[0xab, 0x07]);
    assert_eq!(get_field(&v, "hi"), &Value::UInt(0xa));
}

#[test]
fn bit_order_is_independent_of_byte_order() {
    let schema = "id: t\nmeta:\n  endian: be\n  bit-endian: le\nseq:\n  - id: a\n    type: b4\n  - id: b\n    type: b4\n  - id: c\n    type: u2\n";
    let v = parse(schema, "t", &[0xab, 0x01, 0x02]);
    assert_eq!(get_field(&v, "a"), &Value::UInt(0xb));
    assert_eq!(get_field(&v, "c"), &Value::UInt(0x0102));
}

#[test]
fn a_one_bit_field_is_a_boolean() {
    let schema = "id: t\nseq:\n  - id: on\n    type: b1\n  - id: off\n    type: b1\n  - id: rest\n    type: b6\n";
    let v = parse(schema, "t", &[0b1000_0101]);
    assert_eq!(get_field(&v, "on"), &Value::Bool(true));
    assert_eq!(get_field(&v, "off"), &Value::Bool(false));
    assert_eq!(get_field(&v, "rest"), &Value::UInt(0b00_0101));
}

#[test]
fn a_boolean_bit_field_works_in_conditions() {
    let schema = "id: t\nseq:\n  - id: flag\n    type: b1\n  - id: pad\n    type: b7\n  - id: extra\n    type: u1\n    if: flag\n  - id: exact\n    type: u1\n    if: flag == true\n";
    let v = parse(schema, "t", &[0x80, 5, 6]);
    assert_eq!(get_field(&v, "extra").as_int(), Some(5));
    assert_eq!(get_field(&v, "exact").as_int(), Some(6));
    let v = parse(schema, "t", &[0x00]);
    assert!(get_field(&v, "extra").is_absent());
}

#[test]
fn fields_of_many_widths_pack_and_span_bytes() {
    let schema = "id: t\nseq:\n  - id: version\n    type: b4\n  - id: length\n    type: b12\n  - id: flags\n    type: b3\n  - id: offset\n    type: b13\n";
    let v = parse(schema, "t", &[0x45, 0x67, 0xe1, 0x23]);
    assert_eq!(get_field(&v, "version"), &Value::UInt(4));
    assert_eq!(get_field(&v, "length"), &Value::UInt(0x567));
    assert_eq!(get_field(&v, "flags"), &Value::UInt(0b111));
    assert_eq!(get_field(&v, "offset"), &Value::UInt(0x0123));
}

#[test]
fn the_bits_type_is_the_same_as_the_b_types() {
    let schema = "id: t\nseq:\n  - id: a\n    type: bits\n    bit_size: 4\n  - id: b\n    type: bits\n    bit_size: 4\n";
    let v = parse(schema, "t", &[0xab]);
    assert_eq!(get_field(&v, "a"), &Value::UInt(0xa));
    assert_eq!(get_field(&v, "b"), &Value::UInt(0xb));
}

#[test]
fn bit_fields_end_at_the_byte_and_other_fields_start_on_the_next() {
    let schema = "id: t\nseq:\n  - id: a\n    type: b3\n  - id: after\n    type: u1\n  - id: tail\n    type: u1\n";
    let v = parse(schema, "t", &[0b1110_0000, 0x11, 0x22]);
    assert_eq!(get_field(&v, "a"), &Value::UInt(0b111));
    assert_eq!(get_field(&v, "after").as_int(), Some(0x11));
    assert_eq!(get_field(&v, "tail").as_int(), Some(0x22));
}

#[test]
fn repeated_bit_fields_make_progress_within_a_byte() {
    let schema = "id: t\nseq:\n  - id: bits\n    type: b1\n    repeat: expr\n    repeat-expr: 8\n  - id: next\n    type: u1\n";
    let v = parse(schema, "t", &[0b1010_0000, 9]);
    let Value::Array(items) = get_field(&v, "bits") else {
        panic!("expected an array");
    };
    assert_eq!(items.len(), 8);
    assert_eq!(items[0], Value::Bool(true));
    assert_eq!(items[1], Value::Bool(false));
    assert_eq!(items[2], Value::Bool(true));
    assert_eq!(get_field(&v, "next").as_int(), Some(9));
}

#[test]
fn repeat_eos_reads_every_bit() {
    let schema = "id: t\nseq:\n  - id: bits\n    type: b1\n    repeat: eos\n";
    let v = parse(schema, "t", &[0xff, 0x00]);
    let Value::Array(items) = get_field(&v, "bits") else {
        panic!("expected an array");
    };
    assert_eq!(items.len(), 16);
}

#[test]
fn repeat_until_a_bit_field() {
    let schema = "id: t\nseq:\n  - id: bits\n    type: b1\n    repeat: until\n    repeat-until: _\n  - id: rest\n    type: b5\n";
    let v = parse(schema, "t", &[0b0010_1010]);
    let Value::Array(items) = get_field(&v, "bits") else {
        panic!("expected an array");
    };
    assert_eq!(items.len(), 3);
    assert_eq!(get_field(&v, "rest"), &Value::UInt(0b01010));
}

#[test]
fn an_element_that_consumes_nothing_is_still_an_error_for_bit_fields() {
    // A zero-length byte field repeats without moving; bits do not change that.
    let schema = "id: t\nseq:\n  - id: items\n    type: bytes\n    size: 0\n    repeat: expr\n    repeat-expr: 3\n";
    let err = parse_err(schema, "t", &[1, 2, 3]);
    assert!(matches!(&err, DoeError::FieldError { message, .. } if message.contains("no bytes")));
}

#[test]
fn pos_restores_the_bit_position() {
    let schema = "id: t\nseq:\n  - id: a\n    type: b4\n  - id: far\n    type: b4\n    pos: 2\n  - id: b\n    type: b4\n";
    let v = parse(schema, "t", &[0xab, 0xcd, 0xef]);
    assert_eq!(get_field(&v, "a"), &Value::UInt(0xa));
    assert_eq!(get_field(&v, "far"), &Value::UInt(0xe));
    // `b` carries on with the rest of the first byte.
    assert_eq!(get_field(&v, "b"), &Value::UInt(0xb));
}

#[test]
fn nested_types_inherit_the_bit_order_and_share_the_bits_of_the_byte() {
    // As in Kaitai Struct, entering a type does not align to a byte: the
    // unread half of the first byte goes to the first bit field inside.
    let schema = "id: t\nmeta:\n  bit-endian: le\nseq:\n  - id: a\n    type: b4\n  - id: inner\n    type: rec\ntypes:\n  rec:\n    seq:\n      - id: x\n        type: b4\n      - id: y\n        type: b4\n";
    let v = parse(schema, "t", &[0xab, 0xcd]);
    assert_eq!(get_field(&v, "a"), &Value::UInt(0xb));
    let inner = get_field(&v, "inner");
    assert_eq!(get_field(inner, "x"), &Value::UInt(0xa));
    assert_eq!(get_field(inner, "y"), &Value::UInt(0xd));
}

#[test]
fn bit_fields_past_the_end_are_errors() {
    let schema = "id: t\nseq:\n  - id: a\n    type: b12\n";
    let err = parse_err(schema, "t", &[0xab]);
    assert!(
        matches!(&err, DoeError::Schema(m) if m.contains("overrun")),
        "{err:?}"
    );
}

#[test]
fn a_bit_field_can_be_used_in_an_enum() {
    let schema = "id: t\nseq:\n  - id: kind\n    type: b3\n    enum: kind_t\nenums:\n  kind_t:\n    \"5\": five\n";
    let v = parse(schema, "t", &[0b1010_0000]);
    assert_eq!(
        get_field(&v, "kind"),
        &Value::Enum {
            value: 5,
            name: Some("five".into())
        }
    );
}

// ── Sized sub-streams ────────────────────────────────────────────────────

const WINDOWED: &str = indoc! {"
    id: t
    seq:
      - id: len
        type: u1
      - id: body
        type: rec
        size: len
      - id: after
        type: u1
    types:
      rec:
        seq:
          - id: a
            type: u1
          - id: b
            type: u1
"};

#[test]
fn a_sized_type_is_parsed_in_a_window_and_the_parent_skips_all_of_it() {
    let v = parse(WINDOWED, "t", &[4, 1, 2, 9, 9, 7]);
    let body = get_field(&v, "body");
    assert_eq!(get_field(body, "a").as_int(), Some(1));
    assert_eq!(get_field(body, "b").as_int(), Some(2));
    // The type used two of the four bytes; the parent continues after all four.
    assert_eq!(get_field(&v, "after").as_int(), Some(7));
}

#[test]
fn a_type_cannot_read_past_its_window() {
    // The window holds one byte, and the next one is there but not the type's.
    let err = parse_err(WINDOWED, "t", &[1, 1, 2, 7]);
    assert!(
        matches!(&err, DoeError::Schema(m) if m.contains("overrun")),
        "{err:?}"
    );
}

#[test]
fn a_window_larger_than_what_is_left_is_an_error() {
    for len in [10u8, 255] {
        let err = parse_err(WINDOWED, "t", &[len, 1, 2, 3]);
        assert!(
            matches!(&err, DoeError::Schema(m) if m.contains("overrun")),
            "{len}: {err:?}"
        );
    }
    let yaml = "id: t\nseq:\n  - id: body\n    type: rec\n    size: 0x7fffffffffffffff\ntypes:\n  rec:\n    seq: []\n";
    let err = parse_err(yaml, "t", &[1, 2, 3]);
    assert!(
        matches!(&err, DoeError::Schema(m) if m.contains("overrun")),
        "{err:?}"
    );
}

#[test]
fn a_zero_size_window_holds_a_type_with_no_fields() {
    let yaml = "id: t\nseq:\n  - id: body\n    type: empty\n    size: 0\n  - id: after\n    type: u1\ntypes:\n  empty:\n    seq: []\n";
    let v = parse(yaml, "t", &[5]);
    assert_eq!(get_field(&v, "after").as_int(), Some(5));
    assert!(
        parse_err(WINDOWED, "t", &[0, 1, 2])
            .to_string()
            .contains("overrun")
            || matches!(parse_err(WINDOWED, "t", &[0, 1, 2]), DoeError::Schema(_))
    );
}

#[test]
fn every_repeated_element_gets_its_own_window() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: items
            type: rec
            size: 3
            repeat: expr
            repeat-expr: 2
          - id: after
            type: u1
        types:
          rec:
            seq:
              - id: a
                type: u1
    "};
    let v = parse(yaml, "t", &[1, 0, 0, 2, 0, 0, 9]);
    let Value::Array(items) = get_field(&v, "items") else {
        panic!("expected an array");
    };
    assert_eq!(items.len(), 2);
    assert_eq!(get_field(&items[1], "a").as_int(), Some(2));
    assert_eq!(get_field(&v, "after").as_int(), Some(9));
}

#[test]
fn repeat_eos_inside_a_window_stops_at_its_end() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: len
            type: u1
          - id: body
            type: list
            size: len
          - id: after
            type: u1
        types:
          list:
            seq:
              - id: values
                type: u1
                repeat: eos
    "};
    let v = parse(yaml, "t", &[3, 10, 11, 12, 99]);
    let Value::Array(values) = get_field(get_field(&v, "body"), "values") else {
        panic!("expected an array");
    };
    assert_eq!(values.len(), 3);
    assert_eq!(get_field(&v, "after").as_int(), Some(99));
}

#[test]
fn size_eos_runs_to_the_end_of_the_input_or_the_window() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: head
            type: u1
          - id: rest
            type: list
            size-eos: true
        types:
          list:
            seq:
              - id: values
                type: u1
                repeat: eos
    "};
    let v = parse(yaml, "t", &[1, 2, 3, 4]);
    let Value::Array(values) = get_field(get_field(&v, "rest"), "values") else {
        panic!("expected an array");
    };
    assert_eq!(values.len(), 3);
    // Inside a window, "the rest" is the rest of that window.
    let nested = indoc! {"
        id: t
        seq:
          - id: outer
            type: outer_t
            size: 3
          - id: after
            type: u1
        types:
          outer_t:
            seq:
              - id: first
                type: u1
              - id: rest
                type: bytes
                size-eos: true
    "};
    let v = parse(nested, "t", &[1, 2, 3, 9]);
    assert_eq!(
        get_field(get_field(&v, "outer"), "rest").byte_len(),
        Some(2)
    );
    assert_eq!(get_field(&v, "after").as_int(), Some(9));
}

#[test]
fn size_eos_on_bytes_and_str_reads_the_rest() {
    let yaml = "id: t\nseq:\n  - id: head\n    type: u1\n  - id: text\n    type: str\n    size-eos: true\n    encoding: ascii\n";
    let v = parse(yaml, "t", b"\x00hello");
    assert_eq!(get_field(&v, "text"), &Value::Str("hello".into()));
    let yaml = "id: t\nseq:\n  - id: head\n    type: u1\n  - id: rest\n    type: bytes\n    size-eos: true\n";
    let v = parse(yaml, "t", &[1, 2, 3]);
    assert_eq!(get_field(&v, "rest"), &Value::Bytes(vec![2, 3]));
    let v = parse(yaml, "t", &[1]);
    assert_eq!(get_field(&v, "rest"), &Value::Bytes(vec![]));
}

#[test]
fn windows_nest_and_an_inner_one_cannot_exceed_the_outer() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: outer
            type: outer_t
            size: 4
          - id: after
            type: u1
        types:
          outer_t:
            seq:
              - id: inner
                type: inner_t
                size: 2
              - id: tail
                type: u1
          inner_t:
            seq:
              - id: x
                type: u1
    "};
    let v = parse(yaml, "t", &[1, 2, 3, 4, 9]);
    assert_eq!(get_field(&v, "after").as_int(), Some(9));
    let outer = get_field(&v, "outer");
    assert_eq!(get_field(outer, "tail").as_int(), Some(3));
    let too_big = yaml.replace("size: 2", "size: 6");
    let err = parse_err(&too_big, "t", &[1, 2, 3, 4, 9, 9, 9, 9]);
    assert!(
        matches!(&err, DoeError::Schema(m) if m.contains("overrun")),
        "{err:?}"
    );
}

#[test]
fn a_size_can_use_fields_of_the_enclosing_types() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: n
            type: u1
          - id: outer
            type: outer_t
        types:
          outer_t:
            seq:
              - id: inner
                type: inner_t
                size: _parent.n
              - id: after
                type: u1
          inner_t:
            seq:
              - id: x
                type: u1
    "};
    let v = parse(yaml, "t", &[3, 5, 0, 0, 8]);
    assert_eq!(get_field(get_field(&v, "outer"), "after").as_int(), Some(8));
}

#[test]
fn pos_inside_a_window_is_relative_to_the_window() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: skip
            type: u1
          - id: body
            type: rec
            size: 3
        types:
          rec:
            seq:
              - id: first
                type: u1
              - id: again
                type: u1
                pos: 0
    "};
    let v = parse(yaml, "t", &[0xaa, 7, 8, 9]);
    let body = get_field(&v, "body");
    assert_eq!(get_field(body, "first").as_int(), Some(7));
    assert_eq!(get_field(body, "again").as_int(), Some(7));
    // A position beyond the window is an error even if the input goes on.
    let beyond = yaml.replace("pos: 0", "pos: 4");
    assert!(
        parse_err(&beyond, "t", &[0xaa, 7, 8, 9, 10, 11])
            .to_string()
            .contains("again")
    );
}

#[test]
fn a_window_starts_on_a_byte_boundary_after_bit_fields() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: bits
            type: b4
          - id: body
            type: rec
            size: 1
        types:
          rec:
            seq:
              - id: x
                type: u1
    "};
    let v = parse(yaml, "t", &[0xab, 0xcd]);
    assert_eq!(get_field(get_field(&v, "body"), "x").as_int(), Some(0xcd));
}

#[test]
fn an_empty_window_per_element_makes_no_progress() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: items
            type: empty
            size: 0
            repeat: expr
            repeat-expr: 3
        types:
          empty:
            seq: []
    "};
    let err = parse_err(yaml, "t", &[1, 2, 3]);
    assert!(
        matches!(&err, DoeError::FieldError { message, .. } if message.contains("no bytes")),
        "{err:?}"
    );
}

#[test]
fn the_pos_budget_is_measured_against_the_whole_input() {
    // Many small windows each re-read a few bytes through `pos:`; the total is
    // well within the budget of a 4 KiB input, though a window is 8 bytes.
    let yaml = indoc! {"
        id: t
        seq:
          - id: items
            type: rec
            size: 8
            repeat: eos
        types:
          rec:
            seq:
              - id: a
                type: u1
              - id: again
                type: u4
                pos: 0
    "};
    let v = parse(yaml, "t", &[1; 4096]);
    let Value::Array(items) = get_field(&v, "items") else {
        panic!("expected an array");
    };
    assert_eq!(items.len(), 512);
}

// ── Enum members, _root, _io and arrays of sub-types ─────────────────────

#[test]
fn an_if_can_test_an_enum_field_against_a_member() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: kind
            type: u1
            enum: kind_t
          - id: payload
            type: u1
            if: kind == kind_t::data
          - id: control
            type: u1
            if: kind != kind_t::data
        enums:
          kind_t:
            1: data
            2: control
    "};
    let v = parse(yaml, "t", &[1, 9]);
    assert_eq!(get_field(&v, "payload").as_int(), Some(9));
    assert!(get_field(&v, "control").is_absent());
    let v = parse(yaml, "t", &[2, 9]);
    assert!(get_field(&v, "payload").is_absent());
    assert_eq!(get_field(&v, "control").as_int(), Some(9));
}

#[test]
fn _root_reaches_the_top_level_type_from_nested_ones() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: width
            type: u1
          - id: outer
            type: outer_t
        types:
          outer_t:
            seq:
              - id: inner
                type: inner_t
          inner_t:
            seq:
              - id: cells
                type: u1
                repeat: expr
                repeat-expr: _root.width
    "};
    let v = parse(yaml, "t", &[3, 1, 2, 3]);
    let inner = get_field(get_field(&v, "outer"), "inner");
    let Value::Array(cells) = get_field(inner, "cells") else {
        panic!("expected an array");
    };
    assert_eq!(cells.len(), 3);
}

#[test]
fn _io_describes_the_current_stream() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: head
            type: u1
          - id: at_start
            type: u1
            if: _io.pos == 1
          - id: framed
            type: frame
            size: 4
          - id: rest
            type: u1
            repeat: expr
            repeat-expr: _io.size - _io.pos
        types:
          frame:
            seq:
              - id: values
                type: u1
                repeat: until
                repeat-until: _io.eof
    "};
    let v = parse(yaml, "t", &[0, 0, 1, 2, 3, 4, 8, 9]);
    assert_eq!(get_field(&v, "at_start").as_int(), Some(0));
    let Value::Array(values) = get_field(get_field(&v, "framed"), "values") else {
        panic!("expected an array");
    };
    // `_io.eof` is true at the end of the four-byte window, not the input.
    assert_eq!(values.len(), 4);
    let Value::Array(rest) = get_field(&v, "rest") else {
        panic!("expected an array");
    };
    assert_eq!(rest.len(), 2);
}

#[test]
fn _io_size_is_the_size_of_a_window() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: body
            type: rec
            size: 3
        types:
          rec:
            seq:
              - id: copy
                type: bytes
                size: _io.size
    "};
    let v = parse(yaml, "t", &[1, 2, 3, 4, 5]);
    assert_eq!(get_field(get_field(&v, "body"), "copy").byte_len(), Some(3));
}

#[test]
fn entries_of_an_array_of_sub_types_can_be_referenced() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: count
            type: u1
          - id: entries
            type: entry
            repeat: expr
            repeat-expr: count
          - id: first_len
            type: bytes
            size: entries[0].len
          - id: last_len
            type: bytes
            size: entries.last.len
          - id: total
            type: u1
            if: entries.size == 2
        types:
          entry:
            seq:
              - id: len
                type: u1
    "};
    let v = parse(yaml, "t", &[2, 1, 2, 0xaa, 0xbb, 0xcc, 7]);
    assert_eq!(get_field(&v, "first_len").byte_len(), Some(1));
    assert_eq!(get_field(&v, "last_len").byte_len(), Some(2));
    assert_eq!(get_field(&v, "total").as_int(), Some(7));
}

#[test]
fn the_size_of_a_long_bytes_field_can_be_used() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: len
            type: u4
          - id: blob
            type: bytes
            size: len
          - id: tail
            type: u1
            if: blob.size > 1000
    "};
    let mut data = 70_000u32.to_le_bytes().to_vec();
    data.extend(vec![0; 70_000]);
    data.push(5);
    let v = parse(yaml, "t", &data);
    assert!(matches!(get_field(&v, "blob"), Value::Blob { .. }));
    assert_eq!(get_field(&v, "tail").as_int(), Some(5));
    // Anything but its size is not available.
    let bad = yaml.replace("blob.size > 1000", "blob[0] == 0");
    let err = parse_err(&bad, "t", &data);
    assert!(err.to_string().contains("too large"), "{err}");
}

#[test]
fn pos_can_come_from_an_entry_of_an_array() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: table
            type: entry
            repeat: expr
            repeat-expr: 2
          - id: second
            type: u1
            pos: table[1].offset
        types:
          entry:
            seq:
              - id: offset
                type: u1
    "};
    let v = parse(yaml, "t", &[6, 7, 0, 0, 0, 0, 0xee, 0xff]);
    assert_eq!(get_field(&v, "second").as_int(), Some(0xff));
}

// ── switch-on ────────────────────────────────────────────────────────────

const CHUNKS: &str = indoc! {"
    id: t
    seq:
      - id: kind
        type: u1
      - id: len
        type: u1
      - id: body
        type:
          switch-on: kind
          cases:
            1: text_t
            2: pair_t
            3: u2le
            _: bytes
        size: len
      - id: after
        type: u1
    types:
      text_t:
        seq:
          - id: text
            type: str
            size-eos: true
            encoding: ascii
      pair_t:
        seq:
          - id: a
            type: u1
          - id: b
            type: u1
"};

#[test]
fn a_switch_parses_the_case_that_matches_the_selector() {
    let v = parse(CHUNKS, "t", &[1, 3, b'a', b'b', b'c', 9]);
    assert_eq!(
        get_field(get_field(&v, "body"), "text"),
        &Value::Str("abc".into())
    );
    assert_eq!(get_field(&v, "after").as_int(), Some(9));
    let v = parse(CHUNKS, "t", &[2, 3, 7, 8, 0, 9]);
    let body = get_field(&v, "body");
    assert_eq!(get_field(body, "a").as_int(), Some(7));
    assert_eq!(get_field(body, "b").as_int(), Some(8));
    // The window is three bytes whatever the type used.
    assert_eq!(get_field(&v, "after").as_int(), Some(9));
}

#[test]
fn a_switch_can_choose_a_numeric_type() {
    let v = parse(CHUNKS, "t", &[3, 2, 0x34, 0x12, 9]);
    assert_eq!(get_field(&v, "body"), &Value::UInt(0x1234));
}

#[test]
fn the_default_case_catches_the_rest_and_fills_the_window() {
    let v = parse(CHUNKS, "t", &[9, 3, 1, 2, 3, 9]);
    assert_eq!(get_field(&v, "body"), &Value::Bytes(vec![1, 2, 3]));
    assert_eq!(get_field(&v, "after").as_int(), Some(9));
}

#[test]
fn without_a_default_a_sized_field_is_read_as_bytes() {
    let yaml = CHUNKS.replace("            _: bytes\n", "");
    let v = parse(&yaml, "t", &[9, 2, 5, 6, 9]);
    assert_eq!(get_field(&v, "body"), &Value::Bytes(vec![5, 6]));
    assert_eq!(get_field(&v, "after").as_int(), Some(9));
}

#[test]
fn without_a_default_or_a_size_an_unmatched_field_is_absent() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: kind
            type: u1
          - id: body
            type:
              switch-on: kind
              cases:
                1: u1
          - id: after
            type: u1
    "};
    let v = parse(yaml, "t", &[1, 5, 6]);
    assert_eq!(get_field(&v, "body"), &Value::UInt(5));
    assert_eq!(get_field(&v, "after").as_int(), Some(6));
    let v = parse(yaml, "t", &[2, 6]);
    assert!(get_field(&v, "body").is_absent());
    assert_eq!(get_field(&v, "after").as_int(), Some(6));
}

#[test]
fn the_chosen_type_cannot_read_past_the_window() {
    let err = parse_err(CHUNKS, "t", &[2, 1, 7, 8, 9]);
    assert!(
        matches!(&err, DoeError::Schema(m) if m.contains("overrun")),
        "{err:?}"
    );
}

#[test]
fn case_keys_can_be_enum_members_strings_and_booleans() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: kind
            type: u1
            enum: kind_t
          - id: by_enum
            type:
              switch-on: kind
              cases:
                kind_t::data: u1
                kind_t::control: u2le
          - id: tag
            type: str
            size: 3
            encoding: ascii
          - id: by_string
            type:
              switch-on: tag
              cases:
                '\"abc\"': u1
                '\"xyz\"': u2le
          - id: flag
            type: b1
          - id: pad
            type: b7
          - id: by_bool
            type:
              switch-on: flag
              cases:
                true: u1
                false: u2le
        enums:
          kind_t:
            1: data
            2: control
    "};
    let v = parse(
        yaml,
        "t",
        &[2, 0x01, 0x02, b'x', b'y', b'z', 0x07, 0x00, 0x80, 0x55],
    );
    assert_eq!(get_field(&v, "by_enum"), &Value::UInt(0x0201));
    assert_eq!(get_field(&v, "by_string"), &Value::UInt(7));
    assert_eq!(get_field(&v, "by_bool"), &Value::UInt(0x55));
}

#[test]
fn the_first_matching_case_wins() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: kind
            type: u1
          - id: body
            type:
              switch-on: kind
              cases:
                'kind > 0 ? 1 : 0': u1
                1: u2le
    "};
    // Both keys equal the selector 1; the first is tried first.
    let v = parse(yaml, "t", &[1, 5, 6]);
    assert_eq!(get_field(&v, "body"), &Value::UInt(5));
}

#[test]
fn a_switch_is_evaluated_for_every_repeated_element() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: records
            type: record
            repeat: eos
        types:
          record:
            seq:
              - id: kind
                type: u1
              - id: value
                type:
                  switch-on: kind
                  cases:
                    1: u1
                    2: u2le
    "};
    let v = parse(yaml, "t", &[1, 9, 2, 1, 2, 1, 3]);
    let Value::Array(records) = get_field(&v, "records") else {
        panic!("expected an array");
    };
    let values: Vec<Value> = records
        .iter()
        .map(|r| get_field(r, "value").clone())
        .collect();
    assert_eq!(
        values,
        [Value::UInt(9), Value::UInt(0x0201), Value::UInt(3)]
    );
}

#[test]
fn a_switch_field_can_have_an_enum_and_be_used_afterwards() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: kind
            type: u1
          - id: body
            type:
              switch-on: kind
              cases:
                1: u1
                2: u2le
            enum: body_t
          - id: next
            type: u1
            if: body == body_t::five
        enums:
          body_t:
            5: five
    "};
    let v = parse(yaml, "t", &[1, 5, 77]);
    assert_eq!(
        get_field(&v, "body"),
        &Value::Enum {
            value: 5,
            name: Some("five".into())
        }
    );
    assert_eq!(get_field(&v, "next").as_int(), Some(77));
}

#[test]
fn the_selector_may_use_parent_fields_and_sizes_expressions() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: kind
            type: u1
          - id: n
            type: u1
          - id: inner
            type: rec
        types:
          rec:
            seq:
              - id: body
                type:
                  switch-on: _parent.kind
                  cases:
                    1: bytes
                size: _parent.n
    "};
    let v = parse(yaml, "t", &[1, 2, 8, 9]);
    assert_eq!(
        get_field(get_field(&v, "inner"), "body"),
        &Value::Bytes(vec![8, 9])
    );
}

#[test]
fn a_selector_and_key_of_different_types_is_an_error() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: kind
            type: u1
          - id: body
            type:
              switch-on: kind
              cases:
                '\"one\"': u1
    "};
    let err = parse_err(yaml, "t", &[1, 2]);
    assert!(
        matches!(&err, DoeError::ExprEval { message, .. } if message.contains("cannot compare")),
        "{err:?}"
    );
}

#[test]
fn a_switch_with_a_size_eos_window_runs_to_the_end() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: kind
            type: u1
          - id: body
            type:
              switch-on: kind
              cases:
                1: bytes
            size-eos: true
    "};
    let v = parse(yaml, "t", &[1, 2, 3, 4]);
    assert_eq!(get_field(&v, "body"), &Value::Bytes(vec![2, 3, 4]));
}

#[test]
fn a_string_case_fills_the_window_or_stops_at_its_terminator() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: kind
            type: u1
          - id: body
            type:
              switch-on: kind
              cases:
                1: str
            size: 4
            encoding: ascii
          - id: after
            type: u1
    "};
    let v = parse(yaml, "t", &[1, b'a', b'b', b'c', b'd', 9]);
    assert_eq!(get_field(&v, "body"), &Value::Str("abcd".into()));
    assert_eq!(get_field(&v, "after").as_int(), Some(9));
}

// ── instances ────────────────────────────────────────────────────────────

fn instance_of<'a>(v: &'a Value, name: &str) -> (&'a Value, Option<u64>) {
    match get_field(v, name) {
        Value::Instance { value, offset } => (value.as_ref(), *offset),
        other => panic!("'{name}' is not an instance: {other:?}"),
    }
}

const TABLE: &str = indoc! {"
    id: t
    seq:
      - id: count
        type: u1
      - id: table_off
        type: u1
    instances:
      entries:
        pos: table_off
        type: entry
        repeat: expr
        repeat-expr: count
    types:
      entry:
        seq:
          - id: v
            type: u1
"};

#[test]
fn a_read_instance_is_read_at_its_pos_and_does_not_move_the_sequence() {
    let v = parse(TABLE, "t", &[2, 4, 0, 0, 7, 8]);
    let (entries, offset) = instance_of(&v, "entries");
    let Value::Array(items) = entries else {
        panic!("expected an array, got {entries:?}");
    };
    assert_eq!(items.len(), 2);
    assert_eq!(get_field(&items[1], "v").as_int(), Some(8));
    assert_eq!(offset, Some(4));
}

#[test]
fn instances_follow_the_fields_of_seq_in_schema_order() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: a
            type: u1
        instances:
          second:
            value: a + 2
          first:
            value: a + 1
    "};
    let v = parse(yaml, "t", &[10]);
    let Value::Struct { fields, .. } = &v else {
        panic!("expected a struct");
    };
    let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["a", "second", "first"]);
    assert_eq!(instance_of(&v, "second").0, &Value::UInt(12));
    assert_eq!(instance_of(&v, "first").0, &Value::UInt(11));
    assert_eq!(
        instance_of(&v, "first").1,
        None,
        "a computed value has no offset"
    );
}

#[test]
fn value_instances_can_be_literals() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: a
            type: u1
        instances:
          n:
            value: 4
          flag:
            value: true
          ratio:
            value: 1.5
          name:
            value: \"'abc'\"
          neg:
            value: a - 10
    "};
    let v = parse(yaml, "t", &[3]);
    assert_eq!(instance_of(&v, "n").0, &Value::UInt(4));
    assert_eq!(instance_of(&v, "flag").0, &Value::Bool(true));
    assert_eq!(instance_of(&v, "ratio").0, &Value::Float(1.5));
    assert_eq!(instance_of(&v, "name").0, &Value::Str("abc".into()));
    assert_eq!(instance_of(&v, "neg").0, &Value::SInt(-7));
}

#[test]
fn a_seq_field_can_use_an_instance_which_is_computed_first() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: words
            type: u1
          - id: body
            type: bytes
            size: bytes_len
          - id: tail
            type: u1
            if: bytes_len > 2
        instances:
          bytes_len:
            value: words * 2
    "};
    let v = parse(yaml, "t", &[2, 1, 2, 3, 4, 9]);
    assert_eq!(get_field(&v, "body"), &Value::Bytes(vec![1, 2, 3, 4]));
    assert_eq!(get_field(&v, "tail").as_int(), Some(9));
    assert_eq!(instance_of(&v, "bytes_len").0, &Value::UInt(4));
}

#[test]
fn instances_can_use_other_instances_in_any_order() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: a
            type: u1
        instances:
          third:
            value: second * 10
          second:
            value: first + 1
          first:
            value: a
    "};
    let v = parse(yaml, "t", &[4]);
    assert_eq!(instance_of(&v, "third").0, &Value::UInt(50));
    assert_eq!(instance_of(&v, "second").0, &Value::UInt(5));
}

#[test]
fn instances_that_depend_on_each_other_are_an_error() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: a
            type: u1
        instances:
          x:
            value: y + 1
          y:
            value: x + 1
    "};
    let err = parse_err(yaml, "t", &[1]);
    assert!(
        matches!(&err, DoeError::FieldError { message, .. } if message.contains("depends on itself")),
        "{err:?}"
    );
    let own = yaml.replace("value: y + 1", "value: x + 1");
    let err = parse_err(&own, "t", &[1]);
    assert!(
        err.to_string().contains('x') || err.to_string().contains("itself"),
        "{err}"
    );
}

#[test]
fn a_failing_instance_aborts_the_parse_and_names_itself() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: a
            type: u1
        instances:
          far:
            pos: 100
            type: u1
          bad:
            value: 1 / a
    "};
    let err = parse_err(yaml, "t", &[0]);
    assert!(
        matches!(&err, DoeError::FieldError { field, .. } if field == "far")
            || err.to_string().contains("far"),
        "{err:?}"
    );
    let err = parse_err(&yaml.replace("pos: 100", "pos: 0"), "t", &[0]);
    assert!(err.to_string().contains("bad"), "{err}");
}

#[test]
fn an_instance_without_pos_is_read_where_the_sequence_ended() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: a
            type: u1
        instances:
          next:
            type: u1
          again:
            type: u1
    "};
    let v = parse(yaml, "t", &[1, 2, 3]);
    // Each is read from the end of the sequence, not one after the other.
    assert_eq!(instance_of(&v, "next"), (&Value::UInt(2), Some(1)));
    assert_eq!(instance_of(&v, "again"), (&Value::UInt(2), Some(1)));
}

#[test]
fn instances_support_if_enum_repeat_size_and_contents() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: flag
            type: u1
        instances:
          maybe:
            pos: 1
            type: u1
            if: flag != 0
          kind:
            pos: 1
            type: u1
            enum: kind_t
          raw:
            pos: 1
            size: 2
          magic:
            pos: 1
            contents: [1, 2]
          items:
            pos: 1
            type: u1
            repeat: expr
            repeat-expr: 3
          computed_kind:
            value: flag + 1
            enum: kind_t
          skipped:
            value: 1
            if: flag == 0
        enums:
          kind_t:
            1: one
            2: two
    "};
    let v = parse(yaml, "t", &[1, 1, 2, 3]);
    assert_eq!(instance_of(&v, "maybe").0, &Value::UInt(1));
    assert!(matches!(instance_of(&v, "kind").0, Value::Enum { name: Some(n), .. } if n == "one"));
    assert_eq!(instance_of(&v, "raw").0, &Value::Bytes(vec![1, 2]));
    assert_eq!(instance_of(&v, "magic").0, &Value::Bytes(vec![1, 2]));
    let Value::Array(items) = instance_of(&v, "items").0 else {
        panic!("expected an array");
    };
    assert_eq!(items.len(), 3);
    assert!(
        matches!(instance_of(&v, "computed_kind").0, Value::Enum { name: Some(n), .. } if n == "two")
    );
    assert!(get_field(&v, "skipped").is_absent());
    let v = parse(yaml, "t", &[0, 1, 2, 3]);
    assert!(get_field(&v, "maybe").is_absent());
}

#[test]
fn instances_work_inside_sub_types_and_windows() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: body
            type: rec
            size: 4
          - id: after
            type: u1
        types:
          rec:
            seq:
              - id: n
                type: u1
            instances:
              doubled:
                value: n * 2
              last:
                pos: _io.size - 1
                type: u1
    "};
    let v = parse(yaml, "t", &[3, 0, 0, 9, 7]);
    let body = get_field(&v, "body");
    assert_eq!(instance_of(body, "doubled").0, &Value::UInt(6));
    // `pos` is relative to the window: its last byte, not the input's.
    assert_eq!(instance_of(body, "last"), (&Value::UInt(9), Some(3)));
    assert_eq!(get_field(&v, "after").as_int(), Some(7));
}

#[test]
fn every_element_of_a_repeated_type_has_its_own_instances() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: items
            type: item
            repeat: expr
            repeat-expr: 2
        types:
          item:
            seq:
              - id: n
                type: u1
            instances:
              twice:
                value: n * 2
    "};
    let v = parse(yaml, "t", &[3, 5]);
    let Value::Array(items) = get_field(&v, "items") else {
        panic!("expected an array");
    };
    assert_eq!(instance_of(&items[0], "twice").0, &Value::UInt(6));
    assert_eq!(instance_of(&items[1], "twice").0, &Value::UInt(10));
}

#[test]
fn bit_position_is_kept_when_an_instance_is_read() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: a
            type: b4
          - id: b
            type: b4
        instances:
          whole:
            pos: 0
            type: u1
    "};
    let v = parse(yaml, "t", &[0xab]);
    assert_eq!(get_field(&v, "a"), &Value::UInt(0xa));
    assert_eq!(get_field(&v, "b"), &Value::UInt(0xb));
    assert_eq!(instance_of(&v, "whole").0, &Value::UInt(0xab));
}

#[test]
fn an_instance_can_be_turned_off_in_the_output_but_seq_still_gets_it() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: words
            type: u1
          - id: body
            type: bytes
            size: n
        instances:
          n:
            value: words * 2
          unused:
            value: 5
    "};
    let (engine, reg) = engine_from_yaml(yaml);
    let _ = reg;
    let engine = engine.with_instances(false);
    let mut cursor = Cursor::new(&[1, 8, 9]);
    let v = engine.parse_type("t", &mut cursor).unwrap();
    let Value::Struct { fields, .. } = &v else {
        panic!("expected a struct");
    };
    let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["words", "body"]);
    assert_eq!(get_field(&v, "body"), &Value::Bytes(vec![8, 9]));
}

#[test]
fn an_instance_is_visible_to_expressions_after_it_is_computed() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: a
            type: u1
        instances:
          big:
            value: a > 5
          names:
            value: '[a.to_s, \"x\"]'
          second_len:
            value: names[0].length + names.size
    "};
    let v = parse(yaml, "t", &[7]);
    assert_eq!(instance_of(&v, "big").0, &Value::Bool(true));
    assert_eq!(instance_of(&v, "second_len").0, &Value::UInt(3));
}

// ── valid ────────────────────────────────────────────────────────────────

fn valid_schema(type_name: &str, valid: &str) -> String {
    format!(
        "id: t\nseq:\n  - id: limit\n    type: u1\n  - id: x\n    type: {type_name}\n    valid:{valid}\n  - id: after\n    type: u1\nenums:\n  kind_t:\n    1: one\n    2: two\n"
    )
}

fn check(type_name: &str, valid: &str, data: &[u8]) -> Result<Value, DoeError> {
    let schema = valid_schema(type_name, valid);
    let parsed = doe::schema::raw::RawSchema::from_yaml_str(&schema).unwrap();
    let reg = Box::leak(Box::new(compile_schema(&parsed).unwrap()));
    let engine = Engine::new(reg);
    engine.parse_type("t", &mut Cursor::new(data))
}

fn rejected(type_name: &str, valid: &str, data: &[u8]) -> String {
    match check(type_name, valid, data) {
        Err(DoeError::FieldError { field, message, .. }) => {
            assert_eq!(field, "x", "{message}");
            message
        }
        other => panic!("expected a validation failure, got {other:?}"),
    }
}

#[test]
fn a_bare_valid_value_must_be_equal() {
    assert!(check("u1", " 5", &[0, 5, 9]).is_ok());
    let message = rejected("u1", " 5", &[0, 6, 9]);
    assert!(
        message.contains("validation failed") && message.contains("6 is not equal to 5"),
        "{message}"
    );
}

#[test]
fn min_and_max_are_inclusive() {
    let rules = "\n      min: 2\n      max: 4\n";
    for ok in [2u8, 3, 4] {
        assert!(check("u1", rules, &[0, ok, 9]).is_ok(), "{ok}");
    }
    assert!(rejected("u1", rules, &[0, 1, 9]).contains("less than the minimum 2"));
    assert!(rejected("u1", rules, &[0, 5, 9]).contains("more than the maximum 4"));
}

#[test]
fn any_of_lists_the_allowed_values() {
    let rules = "\n      any-of: [1, 2, 4]\n";
    for ok in [1u8, 2, 4] {
        assert!(check("u1", rules, &[0, ok, 9]).is_ok(), "{ok}");
    }
    let message = rejected("u1", rules, &[0, 3, 9]);
    assert!(message.contains("3 is not one of 1, 2, 4"), "{message}");
}

#[test]
fn expr_sees_the_value_as_an_underscore() {
    let rules = "\n      expr: '_ % 2 == 0 && _ < limit'\n";
    assert!(check("u1", rules, &[10, 4, 9]).is_ok());
    assert!(rejected("u1", rules, &[10, 5, 9]).contains("does not satisfy"));
    assert!(rejected("u1", rules, &[10, 12, 9]).contains("does not satisfy"));
}

#[test]
fn in_enum_needs_a_member() {
    let schema = |data: &[u8]| {
        let yaml = "id: t\nseq:\n  - id: x\n    type: u1\n    enum: kind_t\n    valid:\n      in-enum: true\nenums:\n  kind_t:\n    1: one\n";
        let parsed = doe::schema::raw::RawSchema::from_yaml_str(yaml).unwrap();
        let reg = Box::leak(Box::new(compile_schema(&parsed).unwrap()));
        Engine::new(reg).parse_type("t", &mut Cursor::new(data))
    };
    assert!(schema(&[1]).is_ok());
    let err = schema(&[7]).unwrap_err();
    assert!(
        err.to_string()
            .contains("not a member of the enum 'kind_t'"),
        "{err}"
    );
}

#[test]
fn rules_can_use_earlier_fields_and_enum_members() {
    assert!(check("u1", " limit", &[3, 3, 9]).is_ok());
    assert!(rejected("u1", " limit", &[3, 4, 9]).contains("not equal to 3"));
    assert!(check("u1", " 'kind_t::two'", &[0, 2, 9]).is_ok());
    assert!(rejected("u1", " 'kind_t::two'", &[0, 1, 9]).contains("not equal to 2"));
}

#[test]
fn all_the_rules_given_must_hold() {
    let rules = "\n      min: 1\n      max: 9\n      expr: '_ != 5'\n";
    assert!(check("u1", rules, &[0, 4, 9]).is_ok());
    assert!(rejected("u1", rules, &[0, 5, 9]).contains("does not satisfy"));
    assert!(rejected("u1", rules, &[0, 0, 9]).contains("minimum"));
}

#[test]
fn valid_checks_floats_strings_bytes_and_booleans() {
    assert!(check("f4", " 1.5", &[0, 0, 0, 0xc0, 0x3f, 9]).is_ok());
    assert!(
        check(
            "f4",
            "\n      min: 1\n      max: 2\n",
            &[0, 0, 0, 0xc0, 0x3f, 9]
        )
        .is_ok()
    );
    assert!(rejected("f4", " 2.5", &[0, 0, 0, 0xc0, 0x3f, 9]).contains("not equal"));
    let yaml = "id: t\nseq:\n  - id: x\n    type: str\n    size: 3\n    encoding: ascii\n    valid: '\"abc\"'\n";
    let parsed = doe::schema::raw::RawSchema::from_yaml_str(yaml).unwrap();
    let reg = Box::leak(Box::new(compile_schema(&parsed).unwrap()));
    assert!(
        Engine::new(reg)
            .parse_type("t", &mut Cursor::new(b"abc"))
            .is_ok()
    );
    assert!(
        Engine::new(reg)
            .parse_type("t", &mut Cursor::new(b"abd"))
            .is_err()
    );
    let yaml =
        "id: t\nseq:\n  - id: x\n    size: 2\n    valid:\n      expr: '_[0] == 1 && _.size == 2'\n";
    let parsed = doe::schema::raw::RawSchema::from_yaml_str(yaml).unwrap();
    let reg = Box::leak(Box::new(compile_schema(&parsed).unwrap()));
    assert!(
        Engine::new(reg)
            .parse_type("t", &mut Cursor::new(&[1, 5]))
            .is_ok()
    );
    assert!(
        Engine::new(reg)
            .parse_type("t", &mut Cursor::new(&[2, 5]))
            .is_err()
    );
    let yaml = "id: t\nseq:\n  - id: x\n    type: b1\n    valid: true\n  - id: pad\n    type: b7\n";
    let parsed = doe::schema::raw::RawSchema::from_yaml_str(yaml).unwrap();
    let reg = Box::leak(Box::new(compile_schema(&parsed).unwrap()));
    assert!(
        Engine::new(reg)
            .parse_type("t", &mut Cursor::new(&[0x80]))
            .is_ok()
    );
    assert!(
        Engine::new(reg)
            .parse_type("t", &mut Cursor::new(&[0x00]))
            .is_err()
    );
}

#[test]
fn every_element_of_a_repeated_field_is_checked() {
    let yaml = "id: t\nseq:\n  - id: x\n    type: u1\n    repeat: expr\n    repeat-expr: 3\n    valid:\n      max: 5\n";
    let parsed = doe::schema::raw::RawSchema::from_yaml_str(yaml).unwrap();
    let reg = Box::leak(Box::new(compile_schema(&parsed).unwrap()));
    assert!(
        Engine::new(reg)
            .parse_type("t", &mut Cursor::new(&[1, 5, 3]))
            .is_ok()
    );
    let err = Engine::new(reg)
        .parse_type("t", &mut Cursor::new(&[1, 2, 9]))
        .unwrap_err();
    assert!(
        err.to_string().contains("9 is more than the maximum 5"),
        "{err}"
    );
}

#[test]
fn valid_applies_only_when_the_field_is_read() {
    let yaml = "id: t\nseq:\n  - id: flag\n    type: u1\n  - id: x\n    type: u1\n    if: flag != 0\n    valid: 7\n";
    let parsed = doe::schema::raw::RawSchema::from_yaml_str(yaml).unwrap();
    let reg = Box::leak(Box::new(compile_schema(&parsed).unwrap()));
    assert!(
        Engine::new(reg)
            .parse_type("t", &mut Cursor::new(&[0]))
            .is_ok()
    );
    assert!(
        Engine::new(reg)
            .parse_type("t", &mut Cursor::new(&[1, 8]))
            .is_err()
    );
}

#[test]
fn the_value_is_checked_before_it_is_named_after_an_enum_member() {
    let yaml = "id: t\nseq:\n  - id: x\n    type: u1\n    enum: kind_t\n    valid: 2\nenums:\n  kind_t:\n    2: two\n";
    let parsed = doe::schema::raw::RawSchema::from_yaml_str(yaml).unwrap();
    let reg = Box::leak(Box::new(compile_schema(&parsed).unwrap()));
    let v = Engine::new(reg)
        .parse_type("t", &mut Cursor::new(&[2]))
        .unwrap();
    assert!(matches!(get_field(&v, "x"), Value::Enum { value: 2, .. }));
}

#[test]
fn a_failing_instance_read_names_the_instance() {
    let yaml = "id: t\nseq:\n  - id: a\n    type: u1\ninstances:\n  far:\n    pos: 1\n    type: u1\n    valid:\n      max: 3\n";
    let parsed = doe::schema::raw::RawSchema::from_yaml_str(yaml).unwrap();
    let reg = Box::leak(Box::new(compile_schema(&parsed).unwrap()));
    assert!(
        Engine::new(reg)
            .parse_type("t", &mut Cursor::new(&[0, 3]))
            .is_ok()
    );
    let err = Engine::new(reg)
        .parse_type("t", &mut Cursor::new(&[0, 4]))
        .unwrap_err();
    assert!(
        matches!(&err, DoeError::FieldError { field, .. } if field == "far"),
        "{err:?}"
    );
}

#[test]
fn valid_can_use_an_instance_and_underscore_survives_repeat_until() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: n
            type: u1
            valid:
              max: limit
          - id: items
            type: u1
            repeat: until
            repeat-until: _ == 0
            valid:
              expr: _ < 100
        instances:
          limit:
            value: 9
    "};
    let parsed = doe::schema::raw::RawSchema::from_yaml_str(yaml).unwrap();
    let reg = Box::leak(Box::new(compile_schema(&parsed).unwrap()));
    let v = Engine::new(reg)
        .parse_type("t", &mut Cursor::new(&[5, 7, 3, 0]))
        .unwrap();
    let Value::Array(items) = get_field(&v, "items") else {
        panic!("expected an array");
    };
    assert_eq!(items.len(), 3);
    assert!(
        Engine::new(reg)
            .parse_type("t", &mut Cursor::new(&[10, 0]))
            .is_err()
    );
    assert!(
        Engine::new(reg)
            .parse_type("t", &mut Cursor::new(&[5, 200, 0]))
            .is_err()
    );
}

#[test]
fn a_sub_type_has_no_value_to_compare() {
    let yaml = "id: t\nseq:\n  - id: x\n    type: rec\n    valid: 1\ntypes:\n  rec:\n    seq: []\n";
    let parsed = doe::schema::raw::RawSchema::from_yaml_str(yaml).unwrap();
    // The value of a sub-type has fields but no single value to compare.
    let reg = Box::leak(Box::new(compile_schema(&parsed).unwrap()));
    let err = Engine::new(reg)
        .parse_type("t", &mut Cursor::new(&[]))
        .unwrap_err();
    assert!(err.to_string().contains('x'), "{err}");
}

// ── params ───────────────────────────────────────────────────────────────

const BLOCKS: &str = indoc! {"
    id: t
    seq:
      - id: n
        type: u1
      - id: first
        type: block(n, 'one')
      - id: second
        type: block(2, 'two')
      - id: after
        type: u1
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
          - id: tag
            type: str
            size: label.length
            encoding: ascii
            valid: label
"};

#[test]
fn a_type_gets_the_arguments_its_field_passes() {
    let v = parse(
        BLOCKS,
        "t",
        &[3, 1, 2, 3, b'o', b'n', b'e', 9, 9, b't', b'w', b'o', 7],
    );
    let first = get_field(&v, "first");
    assert_eq!(get_field(first, "data"), &Value::Bytes(vec![1, 2, 3]));
    assert_eq!(get_field(first, "tag"), &Value::Str("one".into()));
    let second = get_field(&v, "second");
    assert_eq!(get_field(second, "data"), &Value::Bytes(vec![9, 9]));
    assert_eq!(get_field(second, "tag"), &Value::Str("two".into()));
    assert_eq!(get_field(&v, "after").as_int(), Some(7));
}

#[test]
fn a_parameter_is_checked_like_any_name_in_the_type() {
    // `valid: label` fails when the tag read is not the label passed in.
    let err = parse_err(
        BLOCKS,
        "t",
        &[3, 1, 2, 3, b'o', b'n', b'x', 9, 9, b't', b'w', b'o', 7],
    );
    assert!(err.to_string().contains("tag"), "{err}");
}

#[test]
fn each_element_of_a_repeat_gets_its_own_arguments() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: sizes
            type: u1
            repeat: expr
            repeat-expr: 2
          - id: blocks
            type: block(sizes[0] + 1)
            repeat: expr
            repeat-expr: 2
        types:
          block:
            params:
              - id: len
            seq:
              - id: data
                type: bytes
                size: len
    "};
    let v = parse(yaml, "t", &[1, 5, 8, 8, 9, 9]);
    let Value::Array(blocks) = get_field(&v, "blocks") else {
        panic!("expected an array");
    };
    assert_eq!(get_field(&blocks[0], "data").byte_len(), Some(2));
    assert_eq!(get_field(&blocks[1], "data").byte_len(), Some(2));
}

#[test]
fn arguments_may_use_instances_and_enum_members() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: kind
            type: u1
            enum: kind_t
          - id: body
            type: rec(width, kind == kind_t::wide)
        instances:
          width:
            value: 3
        types:
          rec:
            params:
              - id: width
              - id: wide
            seq:
              - id: data
                type: bytes
                size: 'wide ? width * 2 : width'
        enums:
          kind_t:
            1: narrow
            2: wide
    "};
    let v = parse(yaml, "t", &[2, 1, 2, 3, 4, 5, 6]);
    assert_eq!(get_field(get_field(&v, "body"), "data").byte_len(), Some(6));
    let v = parse(yaml, "t", &[1, 1, 2, 3]);
    assert_eq!(get_field(get_field(&v, "body"), "data").byte_len(), Some(3));
}

#[test]
fn params_reach_nested_types_through_parent() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: outer
            type: outer_t(4)
        types:
          outer_t:
            params:
              - id: width
            seq:
              - id: inner
                type: inner_t
          inner_t:
            seq:
              - id: data
                type: bytes
                size: _parent.width
    "};
    let v = parse(yaml, "t", &[1, 2, 3, 4]);
    let inner = get_field(get_field(&v, "outer"), "inner");
    assert_eq!(get_field(inner, "data").byte_len(), Some(4));
}

#[test]
fn params_work_in_switch_cases_and_sized_windows() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: kind
            type: u1
          - id: len
            type: u1
          - id: body
            type:
              switch-on: kind
              cases:
                1: block(2)
                _: block(len)
            size: len
          - id: after
            type: u1
        types:
          block:
            params:
              - id: take
            seq:
              - id: data
                type: bytes
                size: take
    "};
    let v = parse(yaml, "t", &[1, 4, 7, 8, 0, 0, 9]);
    assert_eq!(
        get_field(get_field(&v, "body"), "data"),
        &Value::Bytes(vec![7, 8])
    );
    assert_eq!(get_field(&v, "after").as_int(), Some(9));
    let v = parse(yaml, "t", &[2, 3, 7, 8, 6, 9]);
    assert_eq!(
        get_field(get_field(&v, "body"), "data"),
        &Value::Bytes(vec![7, 8, 6])
    );
}

#[test]
fn a_param_is_visible_to_the_instances_and_the_guards_of_its_type() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: x
            type: rec(3, 'ab')
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
              - id: extra
                type: u1
                if: label == 'ab'
            instances:
              total:
                value: count * 10
    "};
    let v = parse(yaml, "t", &[1, 2, 3, 4]);
    let x = get_field(&v, "x");
    assert_eq!(get_field(x, "extra").as_int(), Some(4));
    let Value::Instance { value, .. } = get_field(x, "total") else {
        panic!("expected an instance");
    };
    assert_eq!(**value, Value::UInt(30));
}

#[test]
fn a_type_with_params_cannot_be_parsed_on_its_own() {
    let yaml = indoc! {"
        id: t
        params:
          - id: len
        seq:
          - id: data
            type: bytes
            size: len
    "};
    let err = parse_err(yaml, "t", &[1, 2]);
    assert!(
        matches!(&err, DoeError::FieldError { message, .. } if message.contains("cannot be parsed on its own")),
        "{err:?}"
    );
}

#[test]
fn a_failing_argument_names_the_type_and_the_field() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: n
            type: u1
          - id: body
            type: block(1 / n)
        types:
          block:
            params:
              - id: len
            seq: []
    "};
    let err = parse_err(yaml, "t", &[0]);
    assert!(err.to_string().contains("argument of t::block"), "{err}");
}

#[test]
fn the_same_type_with_different_arguments_does_not_leak_between_uses() {
    let yaml = indoc! {"
        id: t
        seq:
          - id: a
            type: block(1)
          - id: b
            type: block(3)
          - id: c
            type: block(2)
        types:
          block:
            params:
              - id: len
            seq:
              - id: data
                type: bytes
                size: len
    "};
    let v = parse(yaml, "t", &[1, 2, 3, 4, 5, 6]);
    let len = |name: &str| get_field(get_field(&v, name), "data").byte_len();
    assert_eq!((len("a"), len("b"), len("c")), (Some(1), Some(3), Some(2)));
}

// ── Terminated reads ─────────────────────────────────────────────────────

const TERMINATED: &str = indoc! {"
    meta:
      id: t
    seq:
      - id: a
        terminator: 0x2c
        __OPTS__
      - id: b
        size: 1
"};

fn terminated(opts: &str, data: &[u8]) -> Value {
    parse(&TERMINATED.replace("__OPTS__", opts), "t", data)
}

fn bytes_of(v: &Value) -> Vec<u8> {
    match v {
        Value::Bytes(b) => b.clone(),
        other => panic!("expected bytes, got {other:?}"),
    }
}

#[test]
fn terminator_without_a_type_is_bytes_and_consumes_the_terminator() {
    let v = terminated("", b"ab,c");
    assert_eq!(bytes_of(get_field(&v, "a")), b"ab");
    assert_eq!(bytes_of(get_field(&v, "b")), b"c");
}

#[test]
fn terminator_at_the_start_gives_an_empty_value() {
    let v = terminated("", b",c");
    assert_eq!(bytes_of(get_field(&v, "a")), b"");
    assert_eq!(bytes_of(get_field(&v, "b")), b"c");
}

#[test]
fn include_keeps_the_terminator() {
    let v = terminated("include: true", b"ab,c");
    assert_eq!(bytes_of(get_field(&v, "a")), b"ab,");
    assert_eq!(bytes_of(get_field(&v, "b")), b"c");
}

#[test]
fn consume_false_leaves_the_terminator_for_the_next_field() {
    let v = terminated("consume: false", b"ab,c");
    assert_eq!(bytes_of(get_field(&v, "a")), b"ab");
    assert_eq!(bytes_of(get_field(&v, "b")), b",");
}

#[test]
fn a_missing_terminator_is_an_error_unless_eos_error_is_false() {
    parse_err(&TERMINATED.replace("__OPTS__", ""), "t", b"abc");
    let (engine, _) = engine_from_yaml(
        &TERMINATED
            .replace("__OPTS__", "eos-error: false")
            .replace("size: 1", "size: 0"),
    );
    let mut cursor = Cursor::new(&b"abc"[..]);
    let v = engine.parse_type("t", &mut cursor).unwrap();
    assert_eq!(bytes_of(get_field(&v, "a")), b"abc");
}

#[test]
fn the_terminator_as_the_last_byte_leaves_nothing_after_it() {
    parse_err(&TERMINATED.replace("__OPTS__", ""), "t", b"ab,");
}

#[test]
fn size_with_terminator_reads_the_whole_size_and_cuts_the_value() {
    let yaml = indoc! {"
        meta:
          id: t
          encoding: ASCII
        seq:
          - id: s
            type: str
            size: 6
            terminator: 0
          - id: next
            size: 1
    "};
    let v = parse(yaml, "t", b"ab\0xyzQ");
    assert_eq!(*get_field(&v, "s"), Value::Str("ab".into()));
    assert_eq!(bytes_of(get_field(&v, "next")), b"Q");
}

#[test]
fn size_with_terminator_and_no_terminator_in_the_window_keeps_all() {
    let yaml = indoc! {"
        meta:
          id: t
        seq:
          - id: s
            size: 3
            terminator: 0
            include: true
    "};
    let v = parse(yaml, "t", b"abc");
    assert_eq!(bytes_of(get_field(&v, "s")), b"abc");
}

#[test]
fn a_window_larger_than_the_input_is_an_error() {
    let yaml = indoc! {"
        meta:
          id: t
        seq:
          - id: s
            size: 8
            terminator: 0
    "};
    parse_err(yaml, "t", b"ab\0");
}

#[test]
fn pad_right_strips_trailing_padding() {
    let yaml = indoc! {"
        meta:
          id: t
          encoding: ASCII
        seq:
          - id: s
            type: str
            size: 5
            pad-right: 0x20
          - id: e
            size: 3
            pad-right: 0x20
    "};
    let v = parse(yaml, "t", b"a b  \x20\x20\x20");
    assert_eq!(*get_field(&v, "s"), Value::Str("a b".into()));
    assert_eq!(bytes_of(get_field(&v, "e")), b"");
}
