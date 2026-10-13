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
//! | Blob          | `{"$type":"bytes","hex":"de…","len":1048576,"offset":64,"truncated":true}` (`hex` is the first bytes only) |
//! | Str(s)        | string                                                  |
//! | Enum{v,name}  | `{"$type":"enum","value":1,"name":"data"}` or just the |
//! |               | value if name is null                                   |
//! | Struct{..}    | object with a `"$type"` key plus one key per field      |
//! | Array(items)  | JSON array                                              |
//! | Absent        | `null`                                                  |

use std::fmt::Write;

use serde_json::{Value as JValue, json};

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
            serde_json::Number::from_f64(*f).map_or(JValue::Null, JValue::Number)
        }
        Value::Bytes(b) => {
            json!({
                "$type": "bytes",
                "hex": hex_encode(b),
                "len": b.len(),
            })
        }
        Value::Blob {
            offset,
            len,
            preview,
        } => {
            json!({
                "$type": "bytes",
                "hex": hex_encode(preview),
                "len": len,
                "offset": offset,
                "truncated": true,
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
    b.iter()
        .fold(String::with_capacity(b.len() * 2), |mut out, byte| {
            // Writing to a `String` cannot fail.
            let _ = write!(out, "{byte:02x}");
            out
        })
}
