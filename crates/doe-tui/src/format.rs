//! Text for the tree pane: values as they are read, made safe to print, and cut
//! to the width of a cell. Pure functions, tested without a terminal.
//!
//! Everything that comes from the file (strings, names from a schema that
//! someone else wrote) goes through [`safe`] before it is drawn: a control
//! character is shown as an escape (`\u{1b}`), never sent to the terminal.

use std::fmt::Write as _;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::tree::Shown;

/// The text with its control characters escaped, so that none reaches the
/// terminal: `\n`, `\r`, `\t`, `\\` and the rest as `\u{..}`. Printable text
/// (also wide, and combining marks) is kept as it is.
#[must_use]
pub fn safe(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // not only C0 and DEL: the C1 controls (0x80..0x9f) act as escapes
            // on some terminals, and bidirectional controls would reorder a line
            c if c.is_control() || is_format_control(c) => {
                // writing to a String cannot fail
                let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out
}

/// The invisible characters that change how text is laid out.
fn is_format_control(c: char) -> bool {
    matches!(
        c,
        '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{206f}' | '\u{feff}'
    )
}

/// A string as a quoted, escaped literal: `"a\"b"`.
#[must_use]
pub fn quote(text: &str) -> String {
    let mut inner = String::with_capacity(text.len() + 2);
    for c in text.chars() {
        match c {
            '"' => inner.push_str("\\\""),
            '\\' => inner.push_str("\\\\"),
            c => inner.push(c),
        }
    }
    format!("\"{}\"", safe(&inner))
}

/// The width of text in terminal cells.
#[must_use]
pub fn width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// `text` cut to at most `max` cells, ending in `…` when something was cut.
/// It cuts between grapheme clusters, so a combining mark is never left
/// without its letter and a wide character is never half drawn.
#[must_use]
pub fn truncate(text: &str, max: usize) -> String {
    if width(text) <= max {
        return text.to_owned();
    }
    if max == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for grapheme in text.graphemes(true) {
        let w = width(grapheme);
        if used + w + 1 > max {
            break;
        }
        out.push_str(grapheme);
        used += w;
    }
    out.push('…');
    out
}

/// An integer in decimal, with the hex next to it when that tells something
/// more: from 16 up (below that the two are the same digit).
#[must_use]
pub fn integer(value: i128) -> String {
    if (0..16).contains(&value) {
        value.to_string()
    } else if value < 0 {
        format!("{value} (-{:#x})", value.unsigned_abs())
    } else {
        format!("{value} ({value:#x})")
    }
}

/// `n` bytes of hex, `48 65 6c`, with `…` when `total` is more.
#[must_use]
pub fn hex_preview(bytes: &[u8], total: u64) -> String {
    let mut out = String::new();
    for (i, byte) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let _ = write!(out, "{byte:02x}"); // writing to a String cannot fail
    }
    if total > bytes.len() as u64 {
        out.push_str(" …");
    }
    out
}

/// A value as the tree shows it. Not cut: the caller cuts it to the cells it has.
#[must_use]
pub fn shown(value: &Shown) -> String {
    match value {
        Shown::UInt(n) => integer(i128::from(*n)),
        Shown::SInt(n) => integer(i128::from(*n)),
        Shown::Enum {
            value,
            name: Some(name),
        } => format!("{} ({value})", safe(name)),
        Shown::Enum { value, name: None } => format!("{value} (no name)"),
        Shown::Float(f) => format!("{f}"),
        Shown::Bool(b) => b.to_string(),
        Shown::Str { text, cut } => {
            let quoted = quote(text);
            if *cut {
                // drop the closing quote of the part and say that there is more
                format!("{}…\"", quoted.trim_end_matches('"'))
            } else {
                quoted
            }
        }
        Shown::Bytes { len, preview } => {
            let n = if *len == 1 { "byte" } else { "bytes" };
            if preview.is_empty() {
                format!("<{len} {n}>")
            } else {
                format!("<{len} {n}> {}", hex_preview(preview, *len))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_characters_never_reach_the_terminal() {
        let nasty = "a\x1b[31mred\x07\x00\r\n\t\u{7f}\u{85}\u{202e}z";
        let s = safe(nasty);
        assert!(!s.chars().any(char::is_control), "{s:?}");
        assert!(!s.contains('\u{202e}'));
        assert_eq!(
            s,
            "a\\u{1b}[31mred\\u{7}\\u{0}\\r\\n\\t\\u{7f}\\u{85}\\u{202e}z"
        );
        // printable text is as it was: wide, accented, combining, emoji
        for text in ["日本語", "é", "e\u{301}", "🦀", "plain"] {
            assert_eq!(safe(text), text);
        }
    }

    #[test]
    fn a_string_is_quoted_and_a_backslash_is_not_mistaken_for_an_escape() {
        assert_eq!(quote("hi"), "\"hi\"");
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote("a\\b"), "\"a\\\\b\"");
        assert_eq!(quote("x\ny"), "\"x\\ny\"");
        // a backslash followed by what looks like an escape stays distinct from a real one
        assert_ne!(quote("\\u{1b}"), quote("\u{1b}"));
        assert_eq!(quote("\u{1b}"), "\"\\u{1b}\"");
    }

    #[test]
    fn truncation_is_by_cells_and_by_grapheme() {
        assert_eq!(truncate("hello", 5), "hello");
        assert_eq!(truncate("hello", 4), "hel…");
        assert_eq!(truncate("hello", 1), "…");
        assert_eq!(truncate("hello", 0), "");
        // wide characters take two cells and are not cut in half
        assert_eq!(truncate("日本語", 6), "日本語");
        assert_eq!(truncate("日本語", 5), "日本…");
        assert_eq!(truncate("日本語", 4), "日…");
        // a letter and its combining mark go together or not at all
        assert_eq!(
            truncate("e\u{301}e\u{301}e\u{301}", 3),
            "e\u{301}e\u{301}e\u{301}"
        );
        assert_eq!(truncate("e\u{301}e\u{301}e\u{301}", 2), "e\u{301}…");
        assert_eq!(width(&truncate("e\u{301}e\u{301}e\u{301}", 2)), 2);
        for max in 0..8 {
            assert!(width(&truncate("日本e\u{301}語🦀x", max)) <= max);
        }
    }

    #[test]
    fn integers_have_their_hex_when_it_helps() {
        assert_eq!(integer(0), "0");
        assert_eq!(integer(15), "15");
        assert_eq!(integer(16), "16 (0x10)");
        assert_eq!(integer(255), "255 (0xff)");
        assert_eq!(integer(-1), "-1 (-0x1)");
        assert_eq!(
            integer(i128::from(u64::MAX)),
            "18446744073709551615 (0xffffffffffffffff)"
        );
        assert_eq!(
            integer(i128::from(i64::MIN)),
            "-9223372036854775808 (-0x8000000000000000)"
        );
    }

    #[test]
    fn every_kind_of_value() {
        assert_eq!(shown(&Shown::UInt(300)), "300 (0x12c)");
        assert_eq!(shown(&Shown::SInt(-5)), "-5 (-0x5)");
        assert_eq!(shown(&Shown::Bool(true)), "true");
        assert_eq!(shown(&Shown::Float(1.5)), "1.5");
        assert_eq!(shown(&Shown::Float(f64::NAN)), "NaN");
        assert_eq!(
            shown(&Shown::Enum {
                value: 2,
                name: Some("png".into())
            }),
            "png (2)"
        );
        assert_eq!(
            shown(&Shown::Enum {
                value: 99,
                name: None
            }),
            "99 (no name)"
        );
        assert_eq!(
            shown(&Shown::Str {
                text: "hi".into(),
                cut: false
            }),
            "\"hi\""
        );
        assert_eq!(
            shown(&Shown::Str {
                text: "hi".into(),
                cut: true
            }),
            "\"hi…\""
        );
        assert_eq!(
            shown(&Shown::Bytes {
                len: 3,
                preview: vec![0x48, 0x65, 0x6c]
            }),
            "<3 bytes> 48 65 6c"
        );
        assert_eq!(
            shown(&Shown::Bytes {
                len: 1,
                preview: vec![0]
            }),
            "<1 byte> 00"
        );
        assert_eq!(
            shown(&Shown::Bytes {
                len: 0,
                preview: vec![]
            }),
            "<0 bytes>"
        );
        assert_eq!(
            shown(&Shown::Bytes {
                len: 300,
                preview: vec![1, 2]
            }),
            "<300 bytes> 01 02 …"
        );
    }
}
