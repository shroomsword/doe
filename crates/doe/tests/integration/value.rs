use doe::Value;

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
    let v = Value::Enum {
        value: 99,
        name: None,
    };
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
    assert!(
        Value::Enum {
            value: 1,
            name: None
        }
        .is_truthy()
    );
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
    assert_eq!(Value::Str(String::new()).type_name(), "str");
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
    assert_eq!(format!("{}", Value::Float(2.75)), "2.75");
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
    assert_eq!(format!("{v}"), "header (1)");
}
#[test]
fn display_enum_unnamed() {
    let v = Value::Enum {
        value: 42,
        name: None,
    };
    assert_eq!(format!("{v}"), "42");
}
#[test]
fn display_struct() {
    let v = Value::Struct {
        type_name: "png::chunk".into(),
        fields: vec![],
    };
    assert_eq!(format!("{v}"), "<png::chunk>");
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

#[test]
fn bool_values() {
    assert_eq!(Value::Bool(true).as_int(), None);
    assert!(Value::Bool(true).is_truthy());
    assert!(!Value::Bool(false).is_truthy());
    assert_eq!(Value::Bool(true).type_name(), "bool");
    assert_eq!(format!("{}", Value::Bool(true)), "true");
    assert_eq!(format!("{}", Value::Bool(false)), "false");
}

#[test]
fn an_instance_acts_like_the_value_it_holds() {
    let wrap = |value: Value| Value::Instance {
        value: Box::new(value),
        offset: Some(4),
    };
    assert_eq!(wrap(Value::UInt(7)).as_int(), Some(7));
    assert_eq!(wrap(Value::Bytes(vec![1, 2])).byte_len(), Some(2));
    assert!(wrap(Value::Bool(true)).is_truthy());
    assert!(wrap(Value::Absent).is_absent());
    assert!(!wrap(Value::UInt(1)).is_absent());
    assert_eq!(wrap(Value::UInt(1)).type_name(), "uint");
    assert_eq!(format!("{}", wrap(Value::UInt(7))), "7");
}
