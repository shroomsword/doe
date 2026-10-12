use doe::Value;
use doe::render::text::*;

fn struct_val(name: &str, fields: Vec<(&str, Value)>) -> Value {
    Value::Struct {
        type_name: name.into(),
        fields: fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    }
}

// ── Leaf values ───────────────────────────────────────────────────────────

#[test]
fn render_single_uint() {
    let v = struct_val("t", vec![("x", Value::UInt(42))]);
    let out = render(&v);
    assert_eq!(out, "t\n  x  42\n");
}

#[test]
fn render_sint_negative() {
    let v = struct_val("t", vec![("n", Value::SInt(-7))]);
    let out = render(&v);
    assert!(out.contains("n  -7"));
}

#[test]
fn render_float() {
    let v = struct_val("t", vec![("f", Value::Float(2.75))]);
    let out = render(&v);
    assert!(out.contains("f  2.75"));
}

#[test]
fn render_bytes() {
    let v = struct_val("t", vec![("b", Value::Bytes(vec![0; 8]))]);
    let out = render(&v);
    assert!(out.contains("b  <8 bytes>"));
}

#[test]
fn render_str() {
    let v = struct_val("t", vec![("s", Value::Str("hello".into()))]);
    let out = render(&v);
    assert!(out.contains(r#"s  "hello""#));
}

#[test]
fn render_enum_named() {
    let v = struct_val(
        "t",
        vec![(
            "kind",
            Value::Enum {
                value: 1,
                name: Some("data".into()),
            },
        )],
    );
    let out = render(&v);
    assert!(out.contains("kind  data (1)"));
}

#[test]
fn render_enum_unnamed() {
    let v = struct_val(
        "t",
        vec![(
            "kind",
            Value::Enum {
                value: 99,
                name: None,
            },
        )],
    );
    let out = render(&v);
    assert!(out.contains("kind  99"));
}

// ── Absent fields are omitted ─────────────────────────────────────────────

#[test]
fn absent_field_not_rendered() {
    let v = struct_val(
        "t",
        vec![
            ("a", Value::UInt(1)),
            ("b", Value::Absent),
            ("c", Value::UInt(3)),
        ],
    );
    let out = render(&v);
    assert!(out.contains("a  1"));
    assert!(!out.contains('b'));
    assert!(out.contains("c  3"));
}

// ── Arrays ────────────────────────────────────────────────────────────────

#[test]
fn render_array_of_uints() {
    let v = struct_val(
        "t",
        vec![(
            "items",
            Value::Array(vec![Value::UInt(10), Value::UInt(20), Value::UInt(30)]),
        )],
    );
    let out = render(&v);
    assert!(out.contains("items  [3 items]"));
    assert!(out.contains("[0]  10"));
    assert!(out.contains("[1]  20"));
    assert!(out.contains("[2]  30"));
}

#[test]
fn render_empty_array() {
    let v = struct_val("t", vec![("items", Value::Array(vec![]))]);
    let out = render(&v);
    assert!(out.contains("items  [0 items]"));
}

#[test]
fn render_array_singular_label() {
    let v = struct_val("t", vec![("items", Value::Array(vec![Value::UInt(1)]))]);
    let out = render(&v);
    assert!(out.contains("[1 item]")); // singular
}

// ── Nested structs ────────────────────────────────────────────────────────

#[test]
fn render_nested_struct() {
    let inner = Value::Struct {
        type_name: "header".into(),
        fields: vec![
            ("magic".into(), Value::UInt(0xdead_beef)),
            ("version".into(), Value::UInt(2)),
        ],
    };
    let v = struct_val("outer", vec![("hdr", inner)]);
    let out = render(&v);

    // outer type name at top
    assert!(out.starts_with("outer\n"));
    // hdr is at depth 1
    assert!(out.contains("  hdr  header"));
    // inner fields at depth 2
    assert!(out.contains("    magic  3735928559"));
    assert!(out.contains("    version  2"));
}

#[test]
fn render_deeply_nested() {
    let innermost = Value::Struct {
        type_name: "leaf".into(),
        fields: vec![("val".into(), Value::UInt(99))],
    };
    let mid = Value::Struct {
        type_name: "mid".into(),
        fields: vec![("data".into(), innermost)],
    };
    let v = struct_val("root", vec![("child", mid)]);
    let out = render(&v);

    assert!(out.contains("root\n"));
    assert!(out.contains("  child  mid\n"));
    assert!(out.contains("    data  leaf\n"));
    assert!(out.contains("      val  99\n"));
}

// ── Indentation depth ─────────────────────────────────────────────────────

#[test]
fn custom_indent_four_spaces() {
    let v = struct_val("t", vec![("x", Value::UInt(1))]);
    let out = render_with_indent(&v, "    ");
    assert!(out.contains("    x  1"));
}

#[test]
fn custom_indent_tab() {
    let v = struct_val("t", vec![("x", Value::UInt(1))]);
    let out = render_with_indent(&v, "\t");
    assert!(out.contains("\tx  1"));
}

// ── Top-level struct header ───────────────────────────────────────────────

#[test]
fn top_level_type_name_is_first_line() {
    let v = struct_val("my_format", vec![("a", Value::UInt(1))]);
    let out = render(&v);
    assert!(out.starts_with("my_format\n"));
}

// ── Array of structs ──────────────────────────────────────────────────────

#[test]
fn array_of_structs() {
    let make_point = |x: u64, y: u64| Value::Struct {
        type_name: "point".into(),
        fields: vec![("x".into(), Value::UInt(x)), ("y".into(), Value::UInt(y))],
    };
    let v = struct_val(
        "t",
        vec![(
            "points",
            Value::Array(vec![make_point(1, 2), make_point(3, 4)]),
        )],
    );
    let out = render(&v);
    assert!(out.contains("points  [2 items]"));
    assert!(out.contains("[0]  point"));
    assert!(out.contains("[1]  point"));
    // x=1 and x=3 should appear at depth 3
    assert!(out.contains("      x  1"));
    assert!(out.contains("      x  3"));
}

// ── Realistic PNG-like output ─────────────────────────────────────────────

#[test]
fn realistic_output_structure() {
    let chunk = |tag: &str, len: u64| Value::Struct {
        type_name: "png::chunk".into(),
        fields: vec![
            ("length".into(), Value::UInt(len)),
            ("type".into(), Value::Str(tag.into())),
            (
                "body".into(),
                Value::Bytes(vec![0u8; usize::try_from(len).unwrap()]),
            ),
            ("crc".into(), Value::UInt(0xdead_beef)),
        ],
    };
    let v = Value::Struct {
        type_name: "png".into(),
        fields: vec![
            ("signature".into(), Value::Bytes(vec![0u8; 8])),
            (
                "chunks".into(),
                Value::Array(vec![chunk("IHDR", 13), chunk("IEND", 0)]),
            ),
        ],
    };
    let out = render(&v);
    assert!(out.starts_with("png\n"));
    assert!(out.contains("signature  <8 bytes>"));
    assert!(out.contains("chunks  [2 items]"));
    assert!(out.contains("[0]  png::chunk"));
    assert!(out.contains(r#"type  "IHDR""#));
    assert!(out.contains("[1]  png::chunk"));
}

#[test]
fn render_uses_default_indent() {
    let v = struct_val("t", vec![("x", Value::UInt(1))]);
    assert_eq!(DEFAULT_INDENT, "  ");
    assert_eq!(render(&v), render_with_indent(&v, DEFAULT_INDENT));
}
