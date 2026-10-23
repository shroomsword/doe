//! The status bar: where the selected field is in the type, where its bytes are
//! in the file, and what the file is.

use crate::format;
use crate::tree::{Row, RowKind, Shown};

/// A size as a person reads it: `3 bytes`, `1.50 KiB (1536 bytes)`.
#[must_use]
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes < 1024 {
        return format!("{bytes} {}", if bytes == 1 { "byte" } else { "bytes" });
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "the size is shown to two decimals"
    )]
    let mut value = bytes as f64;
    let mut unit = "";
    for next in UNITS {
        value /= 1024.0;
        unit = next;
        if value < 1024.0 {
            break;
        }
    }
    format!("{value:.2} {unit} ({bytes} bytes)")
}

/// What kind a value is, for a field whose type is not a user type.
#[must_use]
pub fn kind_of(shown: &Shown) -> &'static str {
    match shown {
        Shown::UInt(_) => "unsigned integer",
        Shown::SInt(_) => "signed integer",
        Shown::Enum { .. } => "enum",
        Shown::Float(_) => "float",
        Shown::Bool(_) => "bit",
        Shown::Str { .. } => "string",
        Shown::Bytes { .. } => "bytes",
    }
}

/// `header/entries/[3]/name`: the names from the root to the field.
#[must_use]
pub fn field_path(names: &[String]) -> String {
    let joined: Vec<String> = names.iter().map(|n| format::safe(n)).collect();
    joined.join("/")
}

/// Where the field is: `@0x14 (20) · 4 bytes (32 bits)`, or why it is nowhere.
#[must_use]
pub fn place(row: &Row) -> String {
    if row.kind != RowKind::Node {
        return String::new();
    }
    if row.processed {
        return "not in the file (made by process:)".to_owned();
    }
    let Some(span) = row.span else {
        return "computed, not in the file".to_owned();
    };
    if span.is_empty() {
        return format!("@{:#x} · no bytes", span.start / 8);
    }
    let bits = span.end - span.start;
    if span.start % 8 == 0 && span.end % 8 == 0 {
        let bytes = bits / 8;
        let first = span.start / 8;
        format!(
            "@{first:#x} ({first}) · {bytes} {} ({bits} bits)",
            if bytes == 1 { "byte" } else { "bytes" }
        )
    } else {
        format!(
            "bit {} of byte {:#x} · {bits} bit{}",
            span.start % 8,
            span.start / 8,
            if bits == 1 { "" } else { "s" }
        )
    }
}

/// The type of the field: a user type, else the kind of its value.
#[must_use]
pub fn type_of(row: &Row) -> String {
    if let Some(name) = &row.type_name {
        return format::safe(name);
    }
    row.shown
        .as_ref()
        .map_or_else(|| "list".to_owned(), |s| kind_of(s).to_owned())
}

/// The two halves of the bar: the field (left) and the file (right).
#[must_use]
pub fn bar(row: &Row, names: &[String], file_name: &str, size: u64) -> (String, String) {
    if row.kind != RowKind::Node {
        return (
            row.name.clone(),
            format!("{} · {}", format::safe(file_name), human_size(size)),
        );
    }
    let left = format!("{} · {} · {}", field_path(names), type_of(row), place(row));
    let right = format!("{} · {}", format::safe(file_name), human_size(size));
    (left, right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(human_size(0), "0 bytes");
        assert_eq!(human_size(1), "1 byte");
        assert_eq!(human_size(1023), "1023 bytes");
        assert_eq!(human_size(1024), "1.00 KiB (1024 bytes)");
        assert_eq!(human_size(1536), "1.50 KiB (1536 bytes)");
        assert_eq!(
            human_size(5 * 1024 * 1024 * 1024),
            "5.00 GiB (5368709120 bytes)"
        );
        assert!(human_size(u64::MAX).contains("PiB"));
    }

    #[test]
    fn paths_join_the_names() {
        let names: Vec<String> = ["hdr", "entries", "[3]", "name"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert_eq!(field_path(&names), "hdr/entries/[3]/name");
        assert_eq!(field_path(&["a\u{1b}b".to_owned()]), "a\\u{1b}b");
        assert_eq!(field_path(&[]), "");
    }
}
