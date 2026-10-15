use doe::schema::raw::RawSchema;
use doe::schema::raw::*;
use indoc::indoc;

fn parse(yaml: &str) -> RawSchema {
    RawSchema::from_yaml_str(yaml).expect("YAML parse failed")
}

// ── Basic round-trip ─────────────────────────────────────────────────────

#[test]
fn minimal_schema_round_trip() {
    let yaml = indoc! {r"
        id: simple
        seq:
          - id: value
            type: u4
    "};
    let s = parse(yaml);
    assert_eq!(s.id, "simple");
    assert_eq!(s.seq.len(), 1);
    assert_eq!(s.seq[0].id, "value");
    assert_eq!(
        s.seq[0].type_ref.as_ref().and_then(RawType::as_name),
        Some("u4")
    );
}

#[test]
fn schema_with_doc_and_meta() {
    let yaml = indoc! {r#"
        id: be_file
        doc: "A big-endian file format"
        meta:
          endian: be
        seq: []
    "#};
    let s = parse(yaml);
    assert_eq!(s.doc.as_deref(), Some("A big-endian file format"));
    assert_eq!(s.meta.endian, Some(RawEndian::Be));
}

#[test]
fn meta_endian_little_endian() {
    let yaml = indoc! {r"
        id: le_file
        meta:
          endian: le
        seq: []
    "};
    let s = parse(yaml);
    assert_eq!(s.meta.endian, Some(RawEndian::Le));
}

#[test]
fn default_meta_has_no_endian() {
    let yaml = indoc! {r"
        id: no_meta
        seq: []
    "};
    let s = parse(yaml);
    assert!(s.meta.endian.is_none());
}

// ── Field attributes ─────────────────────────────────────────────────────

#[test]
fn field_with_size_integer() {
    let yaml = indoc! {r"
        id: buf
        seq:
          - id: data
            type: bytes
            size: 16
    "};
    let s = parse(yaml);
    let f = &s.seq[0];
    assert_eq!(
        f.type_ref.as_ref().and_then(RawType::as_name),
        Some("bytes")
    );
    assert!(matches!(&f.size, Some(StringOrInt::Int(16))));
}

#[test]
fn field_with_size_expression() {
    let yaml = indoc! {r"
        id: var_buf
        seq:
          - id: length
            type: u4
          - id: data
            type: bytes
            size: length
    "};
    let s = parse(yaml);
    let f = &s.seq[1];
    assert!(matches!(&f.size, Some(StringOrInt::Str(e)) if e == "length"));
}

#[test]
fn field_with_terminator_and_encoding() {
    let yaml = indoc! {r"
        id: strs
        seq:
          - id: name
            type: str
            terminator: 0
            encoding: ascii
    "};
    let s = parse(yaml);
    let f = &s.seq[0];
    assert_eq!(f.terminator, Some(0));
    assert_eq!(f.encoding, Some(RawEncoding::Ascii));
}

#[test]
fn field_with_repeat_eos() {
    let yaml = indoc! {r"
        id: list
        seq:
          - id: items
            type: u1
            repeat: eos
    "};
    let s = parse(yaml);
    assert_eq!(s.seq[0].repeat, Some(RawRepeat::Eos));
}

#[test]
fn field_with_repeat_expr() {
    let yaml = indoc! {r"
        id: counted
        seq:
          - id: count
            type: u2
          - id: items
            type: u1
            repeat: expr
            repeat-expr: count
    "};
    let s = parse(yaml);
    let f = &s.seq[1];
    assert_eq!(f.repeat, Some(RawRepeat::Expr));
    assert_eq!(f.repeat_expr.as_deref(), Some("count"));
}

#[test]
fn field_with_repeat_until() {
    let yaml = indoc! {r"
        id: terminated
        seq:
          - id: items
            type: u1
            repeat: until
            repeat-until: _ == 0
    "};
    let s = parse(yaml);
    let f = &s.seq[0];
    assert_eq!(f.repeat, Some(RawRepeat::Until));
    assert_eq!(f.repeat_until.as_deref(), Some("_ == 0"));
}

#[test]
fn field_with_if_expression() {
    let yaml = indoc! {r"
        id: conditional
        seq:
          - id: flags
            type: u1
          - id: extra
            type: u4
            if: flags > 0
    "};
    let s = parse(yaml);
    assert_eq!(s.seq[1].if_expr.as_deref(), Some("flags > 0"));
}

#[test]
fn field_with_enum_ref() {
    let yaml = indoc! {r#"
        id: tagged
        seq:
          - id: kind
            type: u1
            enum: record_type
        enums:
          record_type:
            "0": unknown
            "1": header
            "2": data
    "#};
    let s = parse(yaml);
    assert_eq!(s.seq[0].enum_ref.as_deref(), Some("record_type"));
    assert!(s.enums.contains_key("record_type"));
    let e = &s.enums["record_type"];
    assert_eq!(e.get("1").map(String::as_str), Some("header"));
}

#[test]
fn field_bit_size() {
    let yaml = indoc! {r"
        id: bitfield
        seq:
          - id: flags
            type: bits
            bit_size: 4
    "};
    let s = parse(yaml);
    assert_eq!(s.seq[0].bit_size, Some(4));
}

#[test]
fn field_with_doc() {
    let yaml = indoc! {r#"
        id: documented
        seq:
          - id: magic
            type: bytes
            size: 4
            doc: "File magic number"
    "#};
    let s = parse(yaml);
    assert_eq!(s.seq[0].doc.as_deref(), Some("File magic number"));
}

// ── Sub-types ────────────────────────────────────────────────────────────

#[test]
fn inline_type_declaration() {
    let yaml = indoc! {r"
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
    "};
    let s = parse(yaml);
    assert!(s.types.contains_key("header"));
    let hdr = &s.types["header"];
    assert_eq!(hdr.seq.len(), 2);
    assert_eq!(hdr.seq[0].id, "magic");
}

#[test]
fn nested_type_declaration() {
    let yaml = indoc! {r"
        id: deeply_nested
        seq:
          - id: top
            type: outer_type
        types:
          outer_type:
            seq:
              - id: inner
                type: inner_type
            types:
              inner_type:
                seq:
                  - id: value
                    type: u4
    "};
    let s = parse(yaml);
    let outer = &s.types["outer_type"];
    assert!(outer.types.contains_key("inner_type"));
    let inner = &outer.types["inner_type"];
    assert_eq!(inner.seq[0].id, "value");
}

#[test]
fn type_with_local_enum() {
    let yaml = indoc! {r#"
        id: typed_enum
        seq:
          - id: record
            type: record_t
        types:
          record_t:
            seq:
              - id: kind
                type: u1
                enum: kind_t
            enums:
              kind_t:
                "0": eof
                "1": data
    "#};
    let s = parse(yaml);
    let rt = &s.types["record_t"];
    assert!(rt.enums.contains_key("kind_t"));
}

// ── Imports ──────────────────────────────────────────────────────────────

#[test]
fn imports_parsed() {
    let yaml = indoc! {r"
        id: with_imports
        imports:
          - zlib_block
          - ./local_helper.yaml
        seq: []
    "};
    let s = parse(yaml);
    assert_eq!(s.imports.len(), 2);
    assert_eq!(s.imports[0], "zlib_block");
    assert_eq!(s.imports[1], "./local_helper.yaml");
}

// ── Contents field ───────────────────────────────────────────────────────

#[test]
fn contents_as_byte_array() {
    let yaml = indoc! {r"
        id: magic_file
        seq:
          - id: magic
            contents: [0x89, 0x50, 0x4e, 0x47]
    "};
    let s = parse(yaml);
    let f = &s.seq[0];
    assert!(matches!(&f.contents, Some(RawContents::Bytes(b)) if b == &[0x89, 0x50, 0x4e, 0x47]));
}

#[test]
fn contents_as_string() {
    let yaml = indoc! {r#"
        id: magic_str
        seq:
          - id: magic
            contents: "RIFF"
    "#};
    let s = parse(yaml);
    assert!(matches!(&s.seq[0].contents, Some(RawContents::Str(s)) if s == "RIFF"));
}

// ── Realistic example: PNG-like ───────────────────────────────────────────

#[test]
fn png_like_schema() {
    let yaml = indoc! {r#"
        id: png
        doc: "PNG image file"
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
    "#};
    let s = parse(yaml);
    assert_eq!(s.id, "png");
    assert_eq!(s.meta.endian, Some(RawEndian::Be));

    // root fields
    assert_eq!(s.seq.len(), 2);
    assert!(matches!(&s.seq[0].contents, Some(RawContents::Bytes(_))));
    assert_eq!(s.seq[1].repeat, Some(RawRepeat::Eos));

    // chunk sub-type
    let chunk = &s.types["chunk"];
    assert_eq!(chunk.seq.len(), 4);
    assert_eq!(
        chunk.seq[0].type_ref.as_ref().and_then(RawType::as_name),
        Some("u4be")
    );
    assert_eq!(
        chunk.seq[2].type_ref.as_ref().and_then(RawType::as_name),
        Some("bytes")
    );
    // body size references the length field
    assert!(matches!(&chunk.seq[2].size, Some(StringOrInt::Str(e)) if e == "length"));
}

// ── Realistic example: ELF-like ───────────────────────────────────────────

#[test]
fn elf_like_schema() {
    let yaml = indoc! {r#"
        id: elf
        doc: "ELF executable"
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
              - id: endian
                type: u1
                enum: endian_t
              - id: ei_version
                type: u1
              - id: abi
                type: u1
              - id: pad
                type: bytes
                size: 8
              - id: e_type
                type: u2le
                enum: obj_type_t
              - id: machine
                type: u2le
              - id: version
                type: u4le
            enums:
              bits_t:
                "1": b32
                "2": b64
              endian_t:
                "1": le
                "2": be
              obj_type_t:
                "0": no_file
                "1": rel
                "2": exec
                "3": dyn
                "4": core
    "#};
    let s = parse(yaml);
    let hdr = &s.types["elf_header"];
    assert_eq!(hdr.seq.len(), 9);
    assert!(hdr.enums.contains_key("bits_t"));
    assert!(hdr.enums.contains_key("obj_type_t"));
    assert_eq!(
        hdr.enums["obj_type_t"].get("2").map(String::as_str),
        Some("exec")
    );
}

// ── Error cases ───────────────────────────────────────────────────────────

#[test]
fn missing_id_fails() {
    let yaml = indoc! {r"
        seq:
          - id: value
            type: u4
    "};
    let result: Result<RawSchema, _> = RawSchema::from_yaml_str(yaml);
    assert!(result.is_err());
}

#[test]
fn unknown_field_fails() {
    let yaml = indoc! {r"
        id: bad
        unknown_key: true
        seq: []
    "};
    let result: Result<RawSchema, _> = RawSchema::from_yaml_str(yaml);
    assert!(
        result.is_err(),
        "deny_unknown_fields should reject unknown_key"
    );
}

#[test]
fn field_without_id_fails() {
    let yaml = indoc! {r"
        id: bad
        seq:
          - type: u4
    "};
    let result: Result<RawSchema, _> = RawSchema::from_yaml_str(yaml);
    assert!(result.is_err(), "field missing id should fail");
}

// ── StringOrInt helper ───────────────────────────────────────────────────

#[test]
fn string_or_int_as_str() {
    assert_eq!(StringOrInt::Int(42).as_str().as_ref(), "42");
    assert_eq!(StringOrInt::Str("foo".into()).as_str().as_ref(), "foo");
}

// ── YAML parser settings ──────────────────────────────────────────────────

#[test]
fn yaml_1_1_boolean_words_stay_strings_in_ids_and_expressions() {
    for word in ["y", "n", "yes", "no", "on", "off"] {
        let schema = RawSchema::from_yaml_str(&format!(
            "id: t\nseq:\n  - id: {word}\n    type: u1\n  - id: data\n    type: bytes\n    size: {word}\n"
        ))
        .unwrap();
        assert_eq!(schema.seq[0].id, word);
        assert_eq!(schema.seq[1].size.as_ref().unwrap().as_str(), word);
    }
}

#[test]
fn duplicate_mapping_keys_are_an_error() {
    let err = RawSchema::from_yaml_str("id: a\nid: b\nseq: []\n").unwrap_err();
    assert!(err.to_string().contains("duplicate"), "{err}");
}

#[test]
fn syntax_errors_name_the_line_and_column() {
    let err = RawSchema::from_yaml_str("id: t\nseq:\n  - id: [unterminated\n").unwrap_err();
    let message = err.to_string();
    assert!(message.contains("line 3"), "{message}");
}

#[test]
fn hexadecimal_integers_and_quoted_hex_keys_parse() {
    let schema = RawSchema::from_yaml_str(
        "id: t\nseq:\n  - id: m\n    contents: [0x89, 0x50]\nenums:\n  e:\n    \"0x10\": sixteen\n",
    )
    .unwrap();
    assert!(matches!(
        schema.seq[0].contents,
        Some(RawContents::Bytes(ref b)) if b == &[0x89, 0x50]
    ));
}

#[test]
fn field_with_pos_expression_or_literal() {
    let s = RawSchema::from_yaml_str(
        "id: t\nseq:\n  - id: a\n    type: u1\n    pos: header.off\n  - id: b\n    type: u1\n    pos: 16\n",
    )
    .unwrap();
    assert_eq!(s.seq[0].pos.as_ref().unwrap().as_str(), "header.off");
    assert_eq!(s.seq[1].pos.as_ref().unwrap().as_str(), "16");
}

#[test]
fn meta_bit_endian_is_read_separately_from_endian() {
    let s = RawSchema::from_yaml_str("id: t\nmeta:\n  endian: be\n  bit-endian: le\n").unwrap();
    assert_eq!(s.meta.endian, Some(RawEndian::Be));
    assert_eq!(s.meta.bit_endian, Some(RawEndian::Le));
    let s = RawSchema::from_yaml_str("id: t\nmeta:\n  endian: be\n").unwrap();
    assert_eq!(s.meta.bit_endian, None);
}

#[test]
fn a_type_can_be_a_switch() {
    let s = RawSchema::from_yaml_str(indoc! {"
        id: t
        seq:
          - id: body
            type:
              switch-on: kind
              cases:
                1: a_t
                kind_t::data: u4
                '\"RIFF\"': riff_t
                _: bytes
            size: 8
    "})
    .unwrap();
    let Some(RawType::Switch(switch)) = &s.seq[0].type_ref else {
        panic!("expected a switch, got {:?}", s.seq[0].type_ref);
    };
    assert_eq!(switch.switch_on, "kind");
    let cases: Vec<(&str, &str)> = switch
        .cases
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(
        cases,
        [
            ("1", "a_t"),
            ("kind_t::data", "u4"),
            ("\"RIFF\"", "riff_t"),
            ("_", "bytes")
        ]
    );
    assert!(s.seq[0].type_ref.as_ref().unwrap().as_name().is_none());
}

#[test]
fn a_switch_needs_switch_on_and_cases_and_nothing_else() {
    for body in [
        "cases:\n          1: a_t",
        "switch-on: kind",
        "switch-on: kind\n          cases:\n            1: a_t\n          extra: 1",
    ] {
        let yaml = format!(
            "id: t\nseq:\n  - id: body\n    type:\n      {}\n",
            body.replace("\n          ", "\n      ")
        );
        assert!(RawSchema::from_yaml_str(&yaml).is_err(), "{body}");
    }
}

#[test]
fn instances_are_read_in_order_with_their_attributes() {
    let s = RawSchema::from_yaml_str(indoc! {"
        id: t
        seq:
          - id: a
            type: u1
        instances:
          table:
            pos: a
            type: entry
            repeat: expr
            repeat-expr: 2
          total:
            value: a * 2
            doc: twice a
          literal:
            value: 4
          flag:
            value: true
          ratio:
            value: 1.5
        types:
          inner:
            seq: []
            instances:
              n:
                value: 1
    "})
    .unwrap();
    let names: Vec<&str> = s.instances.keys().map(String::as_str).collect();
    assert_eq!(names, ["table", "total", "literal", "flag", "ratio"]);
    assert!(s.instances["table"].reads());
    assert!(!s.instances["total"].reads());
    assert_eq!(s.instances["total"].value.as_ref().unwrap().0, "a * 2");
    assert_eq!(s.instances["literal"].value.as_ref().unwrap().0, "4");
    assert_eq!(s.instances["flag"].value.as_ref().unwrap().0, "true");
    assert_eq!(s.instances["ratio"].value.as_ref().unwrap().0, "1.5");
    assert_eq!(s.types["inner"].instances.len(), 1);
    let field = s.instances["table"].to_field("table");
    assert_eq!(field.id, "table");
    assert!(field.pos.is_some());
}

#[test]
fn instances_reject_unknown_keys_and_an_id() {
    for body in ["nope: 1", "id: x\n    value: 1"] {
        let yaml = format!("id: t\ninstances:\n  x:\n    {body}\n");
        assert!(RawSchema::from_yaml_str(&yaml).is_err(), "{body}");
    }
}

#[test]
fn valid_is_a_bare_value_or_a_mapping_of_rules() {
    let s = RawSchema::from_yaml_str(indoc! {"
        id: t
        seq:
          - id: magic
            type: u4
            valid: 0x7f454c46
          - id: tag
            type: str
            size: 3
            valid: '\"abc\"'
          - id: flag
            type: u1
            valid: true
          - id: ratio
            type: f4
            valid: 1.5
          - id: version
            type: u1
            valid:
              min: 1
              max: 3
              any-of: [1, 2, 3]
              expr: _ < 4
              in-enum: true
              eq: 2
    "})
    .unwrap();
    let eq = |i: usize| match s.seq[i].valid.as_ref().unwrap() {
        RawValid::Eq(text) => text.0.clone(),
        RawValid::Rules(_) => panic!("expected a bare value"),
    };
    assert_eq!(eq(0), "2135247942");
    assert_eq!(eq(1), "\"abc\"");
    assert_eq!(eq(2), "true");
    assert_eq!(eq(3), "1.5");
    let Some(RawValid::Rules(rules)) = &s.seq[4].valid else {
        panic!("expected rules");
    };
    assert_eq!(rules.min.as_ref().unwrap().0, "1");
    assert_eq!(rules.max.as_ref().unwrap().0, "3");
    assert_eq!(rules.any_of.as_ref().unwrap().len(), 3);
    assert_eq!(rules.expr.as_ref().unwrap().0, "_ < 4");
    assert_eq!(rules.in_enum, Some(true));
    assert_eq!(rules.eq.as_ref().unwrap().0, "2");
    assert!(!rules.is_empty());
}

#[test]
fn valid_rejects_unknown_rules() {
    let yaml = "id: t\nseq:\n  - id: a\n    type: u1\n    valid:\n      nope: 1\n";
    assert!(RawSchema::from_yaml_str(yaml).is_err());
}

#[test]
fn instances_that_read_can_have_valid() {
    let s = RawSchema::from_yaml_str(
        "id: t\ninstances:\n  x:\n    type: u1\n    pos: 0\n    valid: 1\n",
    )
    .unwrap();
    assert!(s.instances["x"].valid.is_some());
    assert!(s.instances["x"].reads());
    assert!(s.instances["x"].to_field("x").valid.is_some());
}

#[test]
fn params_are_listed_with_optional_type_enum_and_doc() {
    let s = RawSchema::from_yaml_str(indoc! {"
        id: t
        params:
          - id: size
            type: u4
            doc: how many
          - id: kind
            enum: kind_t
          - id: label
        seq: []
        types:
          inner:
            params:
              - id: n
            seq: []
    "})
    .unwrap();
    let ids: Vec<&str> = s.params.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["size", "kind", "label"]);
    assert_eq!(s.params[0].type_ref.as_deref(), Some("u4"));
    assert_eq!(s.params[0].doc.as_deref(), Some("how many"));
    assert_eq!(s.params[1].enum_ref.as_deref(), Some("kind_t"));
    assert_eq!(s.types["inner"].params.len(), 1);
}

#[test]
fn a_param_needs_an_id_and_takes_no_unknown_keys() {
    for body in ["type: u4", "id: x\n    nope: 1"] {
        let yaml = format!("id: t\nparams:\n  - {body}\nseq: []\n");
        assert!(RawSchema::from_yaml_str(&yaml).is_err(), "{body}");
    }
}
