//! What is asked when a field is added to the schema that is being built (`a`):
//! the kind, the length of bytes and text, the encoding of text, and the name.
//! This module only has the rules for what is proposed and what is accepted; the
//! questions are asked by the prompt of the viewer.

use crate::draft::{Draft, Kind};

/// The kinds that can be typed.
pub const KINDS: [&str; 12] = [
    "u1", "u2", "u4", "u8", "s1", "s2", "s4", "s8", "f4", "f8", "bytes", "str",
];

/// The encoding that text is proposed in.
pub const DEFAULT_ENCODING: &str = "ASCII";

/// The kind that is proposed: the integer of the size of a selection of 1, 2, 4 or 8
/// bytes, `bytes` for another selection, and a byte with nothing selected.
#[must_use]
pub fn propose_kind(selection: Option<u64>) -> String {
    match selection {
        Some(n @ (1 | 2 | 4 | 8)) => format!("u{n}"),
        Some(_) => "bytes".to_owned(),
        None => "u1".to_owned(),
    }
}

/// Whether a kind has a length that has to be asked.
#[must_use]
pub fn needs_length(kind: &str) -> bool {
    matches!(kind, "bytes" | "str")
}

/// A kind as typed, in lower case; none if it is not one of [`KINDS`].
#[must_use]
pub fn parse_kind(text: &str) -> Option<&'static str> {
    let text = text.trim().to_ascii_lowercase();
    KINDS.into_iter().find(|k| *k == text)
}

/// A length as typed: a whole number of bytes, from 1.
///
/// # Errors
///
/// What is wrong with it.
pub fn parse_length(text: &str) -> Result<u64, String> {
    let text = text.trim();
    let n: u64 = if let Some(hex) = text.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)
    } else {
        text.parse()
    }
    .map_err(|_| format!("{text:?} is not a number of bytes"))?;
    if n == 0 {
        return Err("a field has at least one byte".to_owned());
    }
    Ok(n)
}

/// The kind of a field from what was asked.
///
/// # Errors
///
/// The kind is not one of [`KINDS`], or a length is needed and missing.
pub fn kind_of(kind: &str, length: Option<u64>, encoding: &str) -> Result<Kind, String> {
    // `u4` is a letter and a size
    let (letter, digits) = match kind.char_indices().nth(1) {
        Some((at, _)) => kind.split_at(at),
        None => (kind, ""),
    };
    let size = digits.parse::<u8>().unwrap_or(0);
    Ok(match (kind, letter) {
        ("bytes", _) => Kind::Bytes(length.ok_or("bytes needs a length")?),
        ("str", _) => Kind::Text {
            len: length.ok_or("str needs a length")?,
            encoding: encoding.to_owned(),
        },
        (_, "u") => Kind::Unsigned(size),
        (_, "s") => Kind::Signed(size),
        (_, "f") => Kind::Float(size),
        (other, _) => return Err(format!("{other:?} is not a kind of field")),
    })
}

/// A name for a field that nothing in the draft has yet: `u2_1`, `bytes_2`...
#[must_use]
pub fn propose_name(draft: &Draft, base: &str) -> String {
    (1..=draft.fields().len() + 1)
        .map(|n| format!("{base}_{n}"))
        .find(|name| draft.fields().iter().all(|f| &f.id != name))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;
    use crate::draft::Field;

    #[test]
    fn the_kind_that_is_proposed_is_the_integer_of_the_size_of_the_selection() {
        assert_eq!(propose_kind(None), "u1");
        for (n, kind) in [(1, "u1"), (2, "u2"), (4, "u4"), (8, "u8")] {
            assert_eq!(propose_kind(Some(n)), kind);
        }
        for n in [3, 5, 16, 100] {
            assert_eq!(propose_kind(Some(n)), "bytes");
        }
    }

    #[test]
    fn kinds_and_lengths_are_read_as_typed() {
        assert_eq!(parse_kind(" U4 "), Some("u4"));
        assert_eq!(parse_kind("str"), Some("str"));
        assert_eq!(parse_kind("u3"), None);
        assert_eq!(parse_kind(""), None);
        assert_eq!(parse_length("16"), Ok(16));
        assert_eq!(parse_length("0x10"), Ok(16));
        assert!(parse_length("0").is_err());
        assert!(parse_length("-1").is_err());
        assert!(parse_length("many").is_err());
        assert!(needs_length("bytes") && needs_length("str") && !needs_length("u4"));
    }

    #[test]
    fn a_kind_and_what_was_asked_make_the_kind_of_the_field() {
        assert_eq!(kind_of("u4", None, "ASCII"), Ok(Kind::Unsigned(4)));
        assert_eq!(kind_of("s8", None, "ASCII"), Ok(Kind::Signed(8)));
        assert_eq!(kind_of("f4", None, "ASCII"), Ok(Kind::Float(4)));
        assert_eq!(kind_of("bytes", Some(3), "ASCII"), Ok(Kind::Bytes(3)));
        assert_eq!(
            kind_of("str", Some(2), "UTF-8"),
            Ok(Kind::Text {
                len: 2,
                encoding: "UTF-8".to_owned()
            })
        );
        assert!(kind_of("bytes", None, "ASCII").is_err());
        assert!(kind_of("zzz", None, "ASCII").is_err());
        for kind in KINDS {
            let length = needs_length(kind).then_some(2);
            assert!(
                kind_of(kind, length, DEFAULT_ENCODING)
                    .unwrap()
                    .check()
                    .is_ok(),
                "{kind}"
            );
        }
    }

    #[test]
    fn the_name_that_is_proposed_is_not_taken() {
        let mut draft = Draft::new("t").unwrap();
        assert_eq!(propose_name(&draft, "u2"), "u2_1");
        draft
            .push(Field {
                id: "u2_1".to_owned(),
                kind: Kind::Unsigned(2),
            })
            .unwrap();
        assert_eq!(propose_name(&draft, "u2"), "u2_2");
        assert_eq!(propose_name(&draft, "bytes"), "bytes_1");
    }
}
