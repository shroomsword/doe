//! What is asked when a field is added to the schema that is being built (`a`):
//! the kind, then what the kind needs (a length, an encoding, the bytes of a
//! signature, a terminator, how often it is repeated), and the name. This module
//! only has the rules for what is proposed, accepted and asked next; the questions
//! are asked by the prompt of the viewer.

use crate::draft::{Count, Draft, Kind};
use crate::prompt::Kind as Step;

/// The kinds that can be typed, besides `b1` to `b64` (a field of that many bits).
pub const KINDS: [&str; 14] = [
    "u1", "u2", "u4", "u8", "s1", "s2", "s4", "s8", "f4", "f8", "bytes", "str", "magic", "strz",
];

/// The encoding that text is proposed in.
pub const DEFAULT_ENCODING: &str = "ASCII";

/// The most bytes of a selection that are proposed as a signature.
pub const MAX_MAGIC: usize = 16;

/// What was answered, besides the kind, to make a [`Kind`].
#[derive(Debug, Clone, Default)]
pub struct Answers {
    /// The length of `bytes` and `str`.
    pub length: Option<u64>,
    /// The encoding of `str` and `strz`.
    pub encoding: String,
    /// The bytes of a signature.
    pub magic: Vec<u8>,
    /// The byte that ends a `strz`.
    pub terminator: u8,
    /// How many bytes a `strz` and its terminator take in the file.
    pub text_sample: u64,
}

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

/// The number of bits of a kind like `b3`.
#[must_use]
pub fn bits_of(kind: &str) -> Option<u8> {
    let digits = kind.strip_prefix('b')?;
    let n: u8 = digits.parse().ok()?;
    (1..=64).contains(&n).then_some(n)
}

/// A kind as typed, in lower case; none if it is not one of [`KINDS`] or `b1` to `b64`.
#[must_use]
pub fn parse_kind(text: &str) -> Option<String> {
    let text = text.trim().to_ascii_lowercase();
    (KINDS.contains(&text.as_str()) || bits_of(&text).is_some()).then_some(text)
}

/// Whether a kind has a length that has to be asked.
#[must_use]
pub fn needs_length(kind: &str) -> bool {
    matches!(kind, "bytes" | "str")
}

/// What is asked after `after` (a question about the kind), or none when only the name is
/// left.
#[must_use]
pub fn next(kind: &str, after: Step) -> Option<Step> {
    match after {
        Step::FieldKind => match kind {
            "bytes" | "str" => Some(Step::FieldLength),
            "strz" => Some(Step::FieldTerminator),
            "magic" => Some(Step::FieldMagic),
            k if bits_of(k).is_some() => None,
            _ => Some(Step::FieldRepeat),
        },
        Step::FieldLength => Some(if kind == "str" {
            Step::FieldEncoding
        } else {
            Step::FieldRepeat
        }),
        Step::FieldTerminator => Some(Step::FieldEncoding),
        Step::FieldEncoding if kind == "str" => Some(Step::FieldRepeat),
        _ => None,
    }
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

/// One byte as typed: decimal, or hex with `0x`.
///
/// # Errors
///
/// What is wrong with it.
pub fn parse_byte(text: &str) -> Result<u8, String> {
    let text = text.trim();
    let n = if let Some(hex) = text.strip_prefix("0x") {
        u8::from_str_radix(hex, 16)
    } else {
        text.parse()
    };
    n.map_err(|_| format!("{text:?} is not a byte (0 to 255, or 0x00 to 0xff)"))
}

/// Bytes as typed in hex: `7f 45 4c 46`, `0x7f,0x45`, or `7f454c46`.
///
/// # Errors
///
/// What is wrong with it.
pub fn parse_hex_bytes(text: &str) -> Result<Vec<u8>, String> {
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| !w.is_empty())
        .collect();
    let mut bytes = Vec::new();
    for word in &words {
        let digits = word.strip_prefix("0x").unwrap_or(word);
        // a word of two digits is a byte; a longer one of an even length is several
        if digits.is_empty()
            || digits.len() % 2 != 0
            || !digits.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err(format!(
                "{word:?} is not hex bytes (two digits each: 7f 45 4c 46)"
            ));
        }
        for pair in digits.as_bytes().chunks(2) {
            let pair = std::str::from_utf8(pair).unwrap_or("00");
            bytes.push(u8::from_str_radix(pair, 16).map_err(|e| e.to_string())?);
        }
    }
    if bytes.is_empty() {
        return Err("type the bytes of the signature in hex: 7f 45 4c 46".to_owned());
    }
    Ok(bytes)
}

/// How often a field is repeated, as typed: nothing for once, a number, the name of an
/// earlier integer field, or `eos` for until the end of the file.
///
/// # Errors
///
/// What is wrong with it.
pub fn parse_repeat(text: &str) -> Result<Option<Count>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    if text.eq_ignore_ascii_case("eos") {
        return Ok(Some(Count::ToEnd));
    }
    if text.chars().all(|c| c.is_ascii_digit()) {
        let n: u64 = text.parse().map_err(|_| format!("{text} is too many"))?;
        if n == 0 {
            return Err("a field is repeated at least once".to_owned());
        }
        return Ok(Some(Count::Fixed(n)));
    }
    crate::draft::check_name(text).map_err(|e| e.to_string())?;
    Ok(Some(Count::Field(text.to_owned())))
}

/// The kind of a field from what was asked (`kind` is one of [`parse_kind`]'s).
///
/// # Errors
///
/// The kind is not one that is known, or a length is needed and missing.
pub fn kind_of(kind: &str, answers: &Answers) -> Result<Kind, String> {
    if let Some(n) = bits_of(kind) {
        return Ok(Kind::Bits(n));
    }
    // `u4` is a letter and a size
    let (letter, digits) = match kind.char_indices().nth(1) {
        Some((at, _)) => kind.split_at(at),
        None => (kind, ""),
    };
    let size = digits.parse::<u8>().unwrap_or(0);
    Ok(match (kind, letter) {
        ("bytes", _) => Kind::Bytes(answers.length.ok_or("bytes needs a length")?),
        ("str", _) => Kind::Text {
            len: answers.length.ok_or("str needs a length")?,
            encoding: answers.encoding.clone(),
        },
        ("magic", _) => Kind::Contents(answers.magic.clone()),
        ("strz", _) => Kind::Terminated {
            terminator: answers.terminator,
            encoding: answers.encoding.clone(),
            sample: answers.text_sample,
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

    fn answers() -> Answers {
        Answers {
            length: Some(2),
            encoding: DEFAULT_ENCODING.to_owned(),
            magic: vec![1, 2],
            terminator: 0,
            text_sample: 4,
        }
    }

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
    fn kinds_lengths_bytes_and_repeats_are_read_as_typed() {
        assert_eq!(parse_kind(" U4 ").as_deref(), Some("u4"));
        assert_eq!(parse_kind("str").as_deref(), Some("str"));
        assert_eq!(parse_kind("B12").as_deref(), Some("b12"));
        assert_eq!(parse_kind("magic").as_deref(), Some("magic"));
        for bad in ["u3", "", "b0", "b65", "b", "bb", "float"] {
            assert_eq!(parse_kind(bad), None, "{bad}");
        }
        assert_eq!(parse_length("16"), Ok(16));
        assert_eq!(parse_length("0x10"), Ok(16));
        assert!(
            parse_length("0").is_err()
                && parse_length("-1").is_err()
                && parse_length("many").is_err()
        );
        assert_eq!(parse_byte("0"), Ok(0));
        assert_eq!(parse_byte("0x0a"), Ok(10));
        assert!(
            parse_byte("256").is_err() && parse_byte("-1").is_err() && parse_byte("x").is_err()
        );
        assert_eq!(
            parse_hex_bytes("7f 45 4c 46"),
            Ok(vec![0x7f, 0x45, 0x4c, 0x46])
        );
        assert_eq!(parse_hex_bytes("0x7f,0x45"), Ok(vec![0x7f, 0x45]));
        assert_eq!(
            parse_hex_bytes("7f454c46"),
            Ok(vec![0x7f, 0x45, 0x4c, 0x46])
        );
        for bad in ["", "7", "zz", "7f 4"] {
            assert!(parse_hex_bytes(bad).is_err(), "{bad:?}");
        }
        assert_eq!(parse_repeat(""), Ok(None));
        assert_eq!(parse_repeat("3"), Ok(Some(Count::Fixed(3))));
        assert_eq!(parse_repeat("EOS"), Ok(Some(Count::ToEnd)));
        assert_eq!(
            parse_repeat("count"),
            Ok(Some(Count::Field("count".to_owned())))
        );
        assert!(parse_repeat("0").is_err() && parse_repeat("Bad Name").is_err());
        assert!(needs_length("bytes") && needs_length("str") && !needs_length("u4"));
    }

    #[test]
    fn what_is_asked_depends_on_the_kind() {
        use Step::{
            FieldEncoding, FieldKind, FieldLength, FieldMagic, FieldRepeat, FieldTerminator,
        };
        let path = |kind: &str| {
            let mut asked = Vec::new();
            let mut at = FieldKind;
            while let Some(step) = next(kind, at) {
                asked.push(step);
                at = step;
            }
            asked
        };
        assert_eq!(path("u4"), [FieldRepeat]);
        assert_eq!(path("f8"), [FieldRepeat]);
        assert_eq!(path("bytes"), [FieldLength, FieldRepeat]);
        assert_eq!(path("str"), [FieldLength, FieldEncoding, FieldRepeat]);
        assert_eq!(path("strz"), [FieldTerminator, FieldEncoding]);
        assert_eq!(path("magic"), [FieldMagic]);
        assert_eq!(path("b3"), []);
    }

    #[test]
    fn a_kind_and_what_was_asked_make_the_kind_of_the_field() {
        let a = answers();
        assert_eq!(kind_of("u4", &a), Ok(Kind::Unsigned(4)));
        assert_eq!(kind_of("s8", &a), Ok(Kind::Signed(8)));
        assert_eq!(kind_of("f4", &a), Ok(Kind::Float(4)));
        assert_eq!(kind_of("bytes", &a), Ok(Kind::Bytes(2)));
        assert_eq!(kind_of("b13", &a), Ok(Kind::Bits(13)));
        assert_eq!(kind_of("magic", &a), Ok(Kind::Contents(vec![1, 2])));
        assert_eq!(
            kind_of("str", &a),
            Ok(Kind::Text {
                len: 2,
                encoding: "ASCII".to_owned()
            })
        );
        assert_eq!(
            kind_of("strz", &a),
            Ok(Kind::Terminated {
                terminator: 0,
                encoding: "ASCII".to_owned(),
                sample: 4
            })
        );
        let none = Answers::default();
        assert!(kind_of("bytes", &none).is_err() && kind_of("zzz", &a).is_err());
        for kind in KINDS {
            assert!(kind_of(kind, &a).unwrap().check().is_ok(), "{kind}");
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
