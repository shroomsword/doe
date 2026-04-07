//! JSON renderer.
//!
//! Converts a `Value` tree into a `serde_json::Value`, then serializes it.
//! The mapping is straightforward:
//!
//! | `Value`       | JSON                                                    |
//! |---------------|---------------------------------------------------------|
//! | UInt(n)       | number                                                  |
//! | SInt(n)       | number                                                  |
//! | Float(f)      | number (NaN/Inf become null)                            |
//! | Bytes(b)      | `{"$type":"bytes","hex":"deadbeef","len":4}`            |
//! | Str(s)        | string                                                  |
//! | Enum{v,name}  | `{"$type":"enum","value":1,"name":"data"}` or just the |
//! |               | value if name is null                                   |
//! | Struct{..}    | object with a `"$type"` key plus one key per field      |
//! | Array(items)  | JSON array                                              |
//! | Absent        | `null`                                                  |

use serde_json::{json, Value as JValue};

use crate::value::Value;

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Render a `Value` tree to a compact JSON string.
pub fn render(value: &Value) -> String {
    to_json(value).to_string()
}

/// Render a `Value` tree to a pretty-printed JSON string.
pub fn render_pretty(value: &Value) -> String {
    serde_json::to_string_pretty(&to_json(value)).unwrap_or_else(|_| "null".into())
}

/// Convert a `Value` to a `serde_json::Value`.
pub fn to_json(value: &Value) -> JValue {
    match value {
        Value::UInt(n) => {
            // serde_json's `u64` number type handles the full u64 range
            JValue::Number((*n).into())
        }
        Value::SInt(n) => JValue::Number((*n).into()),
        Value::Float(f) => {
            // JSON does not support NaN or Infinity; map them to null
            serde_json::Number::from_f64(*f)
                .map(JValue::Number)
                .unwrap_or(JValue::Null)
        }
        Value::Bytes(b) => {
            json!({
                "$type": "bytes",
                "hex": hex_encode(b),
                "len": b.len(),
            })
        }
        Value::Str(s) => JValue::String(s.clone()),
        Value::Enum { value, name } => match name {
            Some(n) => json!({
                "$type": "enum",
                "value": value,
                "name": n,
            }),
            None => json!({
                "$type": "enum",
                "value": value,
                "name": JValue::Null,
            }),
        },
        Value::Struct { type_name, fields } => {
            let mut map = serde_json::Map::new();
            map.insert("$type".into(), JValue::String(type_name.clone()));
            for (k, v) in fields {
                map.insert(k.clone(), to_json(v));
            }
            JValue::Object(map)
        }
        Value::Array(items) => JValue::Array(items.iter().map(to_json).collect()),
        Value::Absent => JValue::Null,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn hex_encode(b: &[u8]) -> String {
    b.iter().map(|byte| format!("{:02x}", byte)).collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn j(v: &Value) -> JValue {
        to_json(v)
    }

    fn struct_val(name: &str, fields: Vec<(&str, Value)>) -> Value {
        Value::Struct {
            type_name: name.into(),
            fields: fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        }
    }

    // ── Scalars ───────────────────────────────────────────────────────────────

    #[test]
    fn uint_to_json() {
        assert_eq!(j(&Value::UInt(42)), json!(42u64));
    }

    #[test]
    fn uint_max_to_json() {
        let v = j(&Value::UInt(u64::MAX));
        assert_eq!(v, json!(u64::MAX));
    }

    #[test]
    fn sint_negative_to_json() {
        assert_eq!(j(&Value::SInt(-7)), json!(-7i64));
    }

    #[test]
    fn sint_min_to_json() {
        assert_eq!(j(&Value::SInt(i64::MIN)), json!(i64::MIN));
    }

    #[test]
    fn float_to_json() {
        let v = j(&Value::Float(3.14));
        assert!(v.is_number());
        assert!((v.as_f64().unwrap() - 3.14).abs() < 1e-10);
    }

    #[test]
    fn float_nan_becomes_null() {
        assert_eq!(j(&Value::Float(f64::NAN)), JValue::Null);
    }

    #[test]
    fn float_inf_becomes_null() {
        assert_eq!(j(&Value::Float(f64::INFINITY)), JValue::Null);
    }

    #[test]
    fn float_neg_inf_becomes_null() {
        assert_eq!(j(&Value::Float(f64::NEG_INFINITY)), JValue::Null);
    }

    // ── Bytes ─────────────────────────────────────────────────────────────────

    #[test]
    fn bytes_has_hex_and_len() {
        let v = j(&Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef]));
        assert_eq!(v["$type"], "bytes");
        assert_eq!(v["hex"], "deadbeef");
        assert_eq!(v["len"], 4);
    }

    #[test]
    fn empty_bytes() {
        let v = j(&Value::Bytes(vec![]));
        assert_eq!(v["hex"], "");
        assert_eq!(v["len"], 0);
    }

    #[test]
    fn bytes_hex_zero_padded() {
        let v = j(&Value::Bytes(vec![0x01, 0x0f]));
        assert_eq!(v["hex"], "010f");
    }

    // ── Strings ───────────────────────────────────────────────────────────────

    #[test]
    fn str_to_json_string() {
        assert_eq!(j(&Value::Str("hello".into())), json!("hello"));
    }

    #[test]
    fn empty_str() {
        assert_eq!(j(&Value::Str("".into())), json!(""));
    }

    #[test]
    fn str_with_unicode() {
        assert_eq!(j(&Value::Str("héllo".into())), json!("héllo"));
    }

    // ── Enums ─────────────────────────────────────────────────────────────────

    #[test]
    fn enum_with_name() {
        let v = j(&Value::Enum {
            value: 2,
            name: Some("data".into()),
        });
        assert_eq!(v["$type"], "enum");
        assert_eq!(v["value"], 2u64);
        assert_eq!(v["name"], "data");
    }

    #[test]
    fn enum_without_name() {
        let v = j(&Value::Enum {
            value: 99,
            name: None,
        });
        assert_eq!(v["$type"], "enum");
        assert_eq!(v["value"], 99u64);
        assert!(v["name"].is_null());
    }

    // ── Structs ───────────────────────────────────────────────────────────────

    #[test]
    fn struct_has_type_key() {
        let v = j(&struct_val("png", vec![]));
        assert_eq!(v["$type"], "png");
    }

    #[test]
    fn struct_fields_present() {
        let v = j(&struct_val(
            "t",
            vec![("x", Value::UInt(1)), ("y", Value::UInt(2))],
        ));
        assert_eq!(v["x"], 1u64);
        assert_eq!(v["y"], 2u64);
    }

    #[test]
    fn nested_struct() {
        let inner = Value::Struct {
            type_name: "inner_t".into(),
            fields: vec![("val".into(), Value::UInt(7))],
        };
        let v = j(&struct_val("outer_t", vec![("child", inner)]));
        assert_eq!(v["$type"], "outer_t");
        assert_eq!(v["child"]["$type"], "inner_t");
        assert_eq!(v["child"]["val"], 7u64);
    }

    // ── Arrays ────────────────────────────────────────────────────────────────

    #[test]
    fn array_of_uints() {
        let v = j(&Value::Array(vec![
            Value::UInt(1),
            Value::UInt(2),
            Value::UInt(3),
        ]));
        assert_eq!(v, json!([1u64, 2u64, 3u64]));
    }

    #[test]
    fn empty_array() {
        let v = j(&Value::Array(vec![]));
        assert_eq!(v, json!([]));
    }

    #[test]
    fn array_of_structs() {
        let v = j(&Value::Array(vec![
            struct_val("pt", vec![("x", Value::UInt(1)), ("y", Value::UInt(2))]),
            struct_val("pt", vec![("x", Value::UInt(3)), ("y", Value::UInt(4))]),
        ]));
        assert!(v.is_array());
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["x"], 1u64);
        assert_eq!(arr[1]["y"], 4u64);
    }

    // ── Absent ────────────────────────────────────────────────────────────────

    #[test]
    fn absent_is_null() {
        assert_eq!(j(&Value::Absent), JValue::Null);
    }

    // ── render / render_pretty ────────────────────────────────────────────────

    #[test]
    fn render_produces_valid_json() {
        let v = struct_val("t", vec![("x", Value::UInt(1))]);
        let s = render(&v);
        let parsed: JValue = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed["x"], 1u64);
    }

    #[test]
    fn render_pretty_is_parseable() {
        let v = struct_val("t", vec![("x", Value::UInt(1))]);
        let s = render_pretty(&v);
        let parsed: JValue = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed["$type"], "t");
    }

    #[test]
    fn render_pretty_has_newlines() {
        let v = struct_val("t", vec![("x", Value::UInt(1))]);
        let s = render_pretty(&v);
        assert!(s.contains('\n'));
    }

    // ── hex_encode ────────────────────────────────────────────────────────────

    #[test]
    fn hex_encode_empty() {
        assert_eq!(hex_encode(&[]), "");
    }
    #[test]
    fn hex_encode_single_byte() {
        assert_eq!(hex_encode(&[0xff]), "ff");
    }
    #[test]
    fn hex_encode_zero_padded() {
        assert_eq!(hex_encode(&[0x0f]), "0f");
    }
    #[test]
    fn hex_encode_multi() {
        assert_eq!(hex_encode(&[0xca, 0xfe]), "cafe");
    }

    // ── Realistic: PNG-like ───────────────────────────────────────────────────

    #[test]
    fn realistic_json_structure() {
        let v = Value::Struct {
            type_name: "png".into(),
            fields: vec![
                (
                    "signature".into(),
                    Value::Bytes(vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
                ),
                (
                    "chunks".into(),
                    Value::Array(vec![Value::Struct {
                        type_name: "png::chunk".into(),
                        fields: vec![
                            ("length".into(), Value::UInt(13)),
                            ("type".into(), Value::Str("IHDR".into())),
                            ("body".into(), Value::Bytes(vec![0u8; 13])),
                            ("crc".into(), Value::UInt(0xae426082)),
                        ],
                    }]),
                ),
            ],
        };
        let json_str = render_pretty(&v);
        let parsed: JValue = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed["$type"], "png");
        assert_eq!(parsed["signature"]["$type"], "bytes");
        assert_eq!(parsed["signature"]["len"], 8u64);
        assert_eq!(parsed["chunks"][0]["type"], "IHDR");
        assert_eq!(parsed["chunks"][0]["length"], 13u64);
    }
}
