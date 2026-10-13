//! Indented text renderer.
//!
//! Each level of nesting adds one indentation level.  The default indent
//! unit is two spaces.  Arrays render their items with a leading index.
//!
//! Example output:
//!
//! ```text
//! png
//!   signature  <8 bytes>
//!   chunks  [3 items]
//!     [0]  png::chunk
//!       length  13
//!       type  "IHDR"
//!       body  <13 bytes>
//!       crc  3731974793
//!     [1]  png::chunk
//!       ...
//! ```

use crate::value::Value;

/// The default indentation unit: two spaces.
pub const DEFAULT_INDENT: &str = "  ";

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Render a `Value` tree to a `String` using the default indent (`"  "`).
pub fn render(value: &Value) -> String {
    render_with_indent(value, DEFAULT_INDENT)
}

/// Render a `Value` tree with a custom indent string.
pub fn render_with_indent(value: &Value, indent: &str) -> String {
    let mut out = String::new();
    write_value(&mut out, value, 0, indent, None);
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal recursion
// ─────────────────────────────────────────────────────────────────────────────

/// Write one value at the given depth.
///
/// `label` is the field name or array index prefix (e.g. `"length"` or
/// `"[2]"`); `None` at the top level.
fn write_value(out: &mut String, value: &Value, depth: usize, indent: &str, label: Option<&str>) {
    match value {
        // ── Leaf types ────────────────────────────────────────────────────
        Value::UInt(_)
        | Value::SInt(_)
        | Value::Float(_)
        | Value::Bytes(_)
        | Value::Blob { .. }
        | Value::Str(_)
        | Value::Enum { .. }
        | Value::Absent => {
            write_line(out, depth, indent, label, &format!("{value}"));
        }

        // ── Struct ────────────────────────────────────────────────────────
        Value::Struct { type_name, fields } => {
            if let Some(lbl) = label {
                write_line(out, depth, indent, Some(lbl), type_name);
            } else {
                // Top-level: just print the type name as the header
                out.push_str(type_name);
                out.push('\n');
            }
            for (field_id, field_val) in fields {
                // Skip absent fields entirely — no output for them
                if field_val.is_absent() {
                    continue;
                }
                write_value(out, field_val, depth + 1, indent, Some(field_id));
            }
        }

        // ── Array ─────────────────────────────────────────────────────────
        Value::Array(items) => {
            write_line(
                out,
                depth,
                indent,
                label,
                &format!(
                    "[{} item{}]",
                    items.len(),
                    if items.len() == 1 { "" } else { "s" }
                ),
            );
            for (i, item) in items.iter().enumerate() {
                let index_label = format!("[{i}]");
                write_value(out, item, depth + 1, indent, Some(&index_label));
            }
        }
    }
}

fn write_line(out: &mut String, depth: usize, indent: &str, label: Option<&str>, value: &str) {
    for _ in 0..depth {
        out.push_str(indent);
    }
    if let Some(lbl) = label {
        out.push_str(lbl);
        out.push_str("  ");
    }
    out.push_str(value);
    out.push('\n');
}
