//! The bytes of the selected field in the forms a person looks for: as numbers
//! of both byte orders and signs, as a float, as text in the common encodings.
//! It reads the bytes of the file, not the parsed value, so it also says
//! something about a field that was not kept in memory (a long byte sequence).

use std::fmt::Write as _;

use doe::radix::{Radix, format_integer, twos_complement};

use crate::format;
use crate::hex_view::Selection;
use crate::tree::Row;

/// The most bytes that are read and decoded as text.
pub const TEXT_BYTES: usize = 64;
/// The longest field that is read as a number.
const NUMBER_BYTES: usize = 8;

/// Why a field has no bytes to look at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoBytes {
    /// It is computed by an expression (`value:`), not read.
    Computed,
    /// It is read from the result of a `process:`, not from the file.
    Processed,
    /// It has no bits.
    Empty,
}

impl NoBytes {
    /// What the inspector says.
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            NoBytes::Computed => "No bytes of the file: this value is computed by an expression.",
            NoBytes::Processed => {
                "No bytes of the file: this value is read from the result of a process:."
            }
            NoBytes::Empty => "No bytes: this field is empty.",
        }
    }
}

/// What the inspector looks at for a row: the bits, or why there are none.
///
/// # Errors
///
/// The reason there are no bytes of the file for the row.
pub fn target(row: &Row) -> Result<Selection, NoBytes> {
    if row.processed {
        return Err(NoBytes::Processed);
    }
    match row.span {
        None => Err(NoBytes::Computed),
        Some(span) if span.is_empty() => Err(NoBytes::Empty),
        Some(span) => Ok(Selection {
            start: span.start,
            end: span.end,
        }),
    }
}

/// The first byte and the number of bytes (rounded outwards) that `selection` touches.
#[must_use]
pub fn byte_range(selection: &Selection) -> (u64, u64) {
    let first = selection.start / 8;
    let end = selection.end.div_ceil(8);
    (first, end - first)
}

/// The value of the bits `start..end` (counted from the start of `data`, most
/// significant bit first, which is how `doe` reads bit fields by default) if
/// there are at most 64 of them.
#[must_use]
pub fn bits_value(data: &[u8], start: u64, end: u64) -> Option<u64> {
    if end <= start || end - start > 64 {
        return None;
    }
    let mut value = 0_u64;
    for bit in start..end {
        let byte = *data.get(usize::try_from(bit / 8).ok()?)?;
        value = (value << 1) | u64::from((byte >> (7 - bit % 8)) & 1);
    }
    Some(value)
}

fn unsigned(data: &[u8], big: bool) -> u64 {
    let mut value = 0_u64;
    if big {
        for &b in data {
            value = (value << 8) | u64::from(b);
        }
    } else {
        for &b in data.iter().rev() {
            value = (value << 8) | u64::from(b);
        }
    }
    value
}

fn signed(value: u64, bytes: usize) -> i64 {
    let bits = u32::try_from(bytes * 8).unwrap_or(64);
    if bits >= 64 {
        return i64::from_ne_bytes(value.to_ne_bytes());
    }
    let sign = 1_u64 << (bits - 1);
    if value & sign == 0 {
        i64::try_from(value).unwrap_or(0)
    } else {
        i64::try_from(value).unwrap_or(0) - (1_i64 << bits)
    }
}

fn float(data: &[u8], big: bool) -> Option<String> {
    let mut bytes = data.to_vec();
    if big {
        bytes.reverse();
    }
    // `bytes` is little-endian now
    match bytes.len() {
        4 => Some(format!("{}", f32::from_le_bytes(bytes.try_into().ok()?))),
        8 => Some(format!("{}", f64::from_le_bytes(bytes.try_into().ok()?))),
        _ => None,
    }
}

fn binary(value: u64, bytes: usize) -> String {
    let width = bytes * 8;
    format!("{value:0width$b}")
}

fn hex_of(data: &[u8], limit: usize) -> String {
    let mut out = String::new();
    for (i, b) in data.iter().take(limit).enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let _ = write!(out, "{b:02x}"); // writing to a String cannot fail
    }
    if data.len() > limit {
        out.push_str(" …");
    }
    out
}

/// Text in an encoding, safe to print, and cut to `max` characters.
fn text(decoded: &str, max: usize) -> String {
    let mut shown: String = decoded.chars().take(max).collect();
    if decoded.chars().count() > max {
        shown.push('…');
    }
    format!("\"{}\"", format::safe(&shown).replace('"', "\\\""))
}

/// One line of the inspector: a label and what it says.
pub type InspectLine = (&'static str, String);

/// The lines for `data` (the bytes of the selected field, at most
/// [`TEXT_BYTES`] of them), when the field has `total` bytes and is
/// `bit_start..bit_end` bits of them (bits counted from the first byte of
/// `data`). The numbers are in decimal with their hex.
#[must_use]
pub fn lines(data: &[u8], total: u64, bit_start: u64, bit_end: u64) -> Vec<InspectLine> {
    lines_in(data, total, bit_start, bit_end, Radix::Decimal)
}

/// As [`lines`], with the numbers in `radix` when it is not decimal; a
/// negative one then has its two's complement next to it.
#[must_use]
pub fn lines_in(
    data: &[u8],
    total: u64,
    bit_start: u64,
    bit_end: u64,
    radix: Radix,
) -> Vec<InspectLine> {
    let mut out: Vec<InspectLine> = Vec::new();
    let bits = bit_end - bit_start;
    let whole = bit_start.is_multiple_of(8) && bit_end.is_multiple_of(8);
    out.push((
        "bytes",
        format!(
            "{} ({total} {})",
            hex_of(data, 8),
            if total == 1 { "byte" } else { "bytes" }
        ),
    ));
    if !whole {
        // a bit field: the value of its bits, and the bytes that hold them
        if let Some(value) = bits_value(data, bit_start, bit_end) {
            let plural = if bits == 1 { "" } else { "s" };
            let text = if radix == Radix::Decimal {
                format!(
                    "{bits} bit{plural} = {value} ({value:#x}, {value:#0w$b})",
                    w = usize::try_from(bits).unwrap_or(64) + 2
                )
            } else {
                let number = format_integer(i128::from(value), u32::try_from(bits).ok(), radix);
                format!("{bits} bit{plural} = {number}")
            };
            out.push(("bits", text));
        }
        return out;
    }

    let len = data.len();
    if total <= NUMBER_BYTES as u64
        && len == usize::try_from(total).unwrap_or(0)
        && (1..=NUMBER_BYTES).contains(&len)
    {
        for big in [false, true] {
            let value = unsigned(data, big);
            let mut line = if radix == Radix::Decimal {
                format!("{value} ({value:#x})")
            } else {
                format_integer(i128::from(value), u32::try_from(len * 8).ok(), radix)
            };
            if len <= 4 && radix == Radix::Decimal {
                let _ = write!(line, " {}", binary(value, len)); // writing to a String cannot fail
            }
            out.push((if big { "unsigned BE" } else { "unsigned LE" }, line));
        }
        for big in [false, true] {
            let value = signed(unsigned(data, big), len);
            let mut line = if radix == Radix::Decimal {
                format!("{value}")
            } else {
                format_integer(i128::from(value), u32::try_from(len * 8).ok(), radix)
            };
            if radix != Radix::Decimal
                && let Some(pattern) = u32::try_from(len * 8)
                    .ok()
                    .and_then(|bits| twos_complement(i128::from(value), bits))
            {
                let _ = write!(line, " (two's complement {pattern})"); // writing to a String cannot fail
            }
            out.push((if big { "signed BE" } else { "signed LE" }, line));
        }
        for big in [false, true] {
            if let Some(f) = float(data, big) {
                out.push((if big { "float BE" } else { "float LE" }, f));
            }
        }
    }

    // text, in the encodings that are likely, of the first bytes
    let max_chars = 40;
    out.push(("UTF-8", text(&String::from_utf8_lossy(data), max_chars)));
    let ascii: String = data
        .iter()
        .map(|&b| {
            if (0x20..0x7f).contains(&b) {
                char::from(b)
            } else {
                '.'
            }
        })
        .collect();
    out.push(("ASCII", text(&ascii, max_chars)));
    let latin1: String = data.iter().map(|&b| char::from(b)).collect();
    out.push(("Latin-1", text(&latin1, max_chars)));
    if len >= 2 {
        for (label, big) in [("UTF-16 LE", false), ("UTF-16 BE", true)] {
            let (pairs, _) = data.as_chunks::<2>();
            let units = pairs.iter().map(|&pair| {
                if big {
                    u16::from_be_bytes(pair)
                } else {
                    u16::from_le_bytes(pair)
                }
            });
            let decoded: String = char::decode_utf16(units)
                .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
                .collect();
            out.push((label, text(&decoded, max_chars)));
        }
    }
    if total > len as u64 {
        out.push((
            "more",
            format!("{} more bytes not shown", total - len as u64),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labelled(lines: &[InspectLine]) -> Vec<String> {
        lines.iter().map(|(l, t)| format!("{l}: {t}")).collect()
    }

    fn find<'a>(lines: &'a [InspectLine], label: &str) -> &'a str {
        lines
            .iter()
            .find(|(l, _)| *l == label)
            .map_or("<none>", |(_, t)| t.as_str())
    }

    #[test]
    fn the_numbers_are_in_the_radix_and_a_negative_one_has_its_twos_complement() {
        let l = lines_in(&[0xfe], 1, 0, 8, Radix::Hex);
        assert_eq!(find(&l, "unsigned LE"), "0xfe");
        assert_eq!(find(&l, "signed LE"), "-0x02 (two's complement 0xfe)");
        let l = lines_in(&[0x34, 0x12], 2, 0, 16, Radix::Binary);
        assert_eq!(find(&l, "unsigned LE"), "0b0001001000110100");
        assert_eq!(find(&l, "unsigned BE"), "0b0011010000010010");
        // a positive signed number has no complement to tell
        assert_eq!(find(&l, "signed LE"), "0b0001001000110100");
        // a bit field
        let l = lines_in(&[0b1010_0000], 1, 0, 3, Radix::Octal);
        assert_eq!(find(&l, "bits"), "3 bits = 0o5");
        // decimal is as it was
        assert_eq!(
            lines_in(&[0xfe], 1, 0, 8, Radix::Decimal),
            lines(&[0xfe], 1, 0, 8)
        );
    }

    #[test]
    fn one_byte() {
        let l = lines(&[0xfe], 1, 0, 8);
        assert_eq!(find(&l, "bytes"), "fe (1 byte)");
        assert_eq!(find(&l, "unsigned LE"), "254 (0xfe) 11111110");
        assert_eq!(find(&l, "unsigned BE"), "254 (0xfe) 11111110");
        assert_eq!(find(&l, "signed LE"), "-2");
        assert_eq!(find(&l, "float LE"), "<none>");
        assert_eq!(find(&l, "ASCII"), "\".\"");
        assert_eq!(find(&l, "Latin-1"), "\"þ\"");
        assert_eq!(find(&l, "UTF-16 LE"), "<none>");
    }

    #[test]
    fn two_bytes_in_both_orders() {
        let l = lines(&[0x34, 0x12], 2, 0, 16);
        assert_eq!(find(&l, "unsigned LE"), "4660 (0x1234) 0001001000110100");
        assert_eq!(find(&l, "unsigned BE"), "13330 (0x3412) 0011010000010010");
        assert_eq!(find(&l, "signed LE"), "4660");
        let negative = lines(&[0x00, 0x80], 2, 0, 16);
        assert_eq!(find(&negative, "signed LE"), "-32768");
        assert_eq!(find(&negative, "signed BE"), "128");
        assert_eq!(find(&l, "ASCII"), "\"4.\"");
        assert_eq!(find(&l, "UTF-16 LE"), "\"\u{1234}\"");
    }

    #[test]
    fn four_bytes_as_a_float() {
        let l = lines(&[0x00, 0x00, 0xc0, 0x3f], 4, 0, 32);
        assert_eq!(find(&l, "float LE"), "1.5");
        assert_eq!(
            find(&l, "unsigned LE"),
            "1069547520 (0x3fc00000) 00111111110000000000000000000000"
        );
        assert!(!find(&l, "float BE").is_empty() && find(&l, "float BE") != "1.5");
    }

    #[test]
    fn eight_bytes_as_a_double_without_the_binary() {
        let l = lines(&1.25_f64.to_le_bytes(), 8, 0, 64);
        assert_eq!(find(&l, "float LE"), "1.25");
        assert_eq!(
            find(&l, "unsigned LE"),
            format!("{0} ({0:#x})", 1.25_f64.to_bits())
        );
        let max = lines(&[0xff; 8], 8, 0, 64);
        assert_eq!(
            find(&max, "unsigned LE"),
            "18446744073709551615 (0xffffffffffffffff)"
        );
        assert_eq!(find(&max, "signed LE"), "-1");
        assert_eq!(find(&max, "float LE"), "NaN");
    }

    #[test]
    fn a_longer_field_is_text_and_says_how_much_is_not_shown() {
        let text_bytes =
            b"hello, binary world! with more after it......................................";
        let shown = &text_bytes[..TEXT_BYTES.min(text_bytes.len())];
        let l = lines(shown, 300, 0, 300 * 8);
        assert_eq!(
            find(&l, "unsigned LE"),
            "<none>",
            "no number for a long field"
        );
        assert!(find(&l, "bytes").ends_with("(300 bytes)"));
        assert!(find(&l, "UTF-8").starts_with("\"hello, binary world! with more after it."));
        assert!(find(&l, "UTF-8").ends_with("…\""), "{}", find(&l, "UTF-8"));
        assert_eq!(
            find(&l, "more"),
            format!("{} more bytes not shown", 300 - shown.len())
        );
    }

    #[test]
    fn text_is_made_safe() {
        let l = lines(b"a\x1b[2Jb\x00\xff\xfe", 7, 0, 56);
        for (label, text) in &l {
            assert!(!text.chars().any(char::is_control), "{label}: {text:?}");
        }
        assert_eq!(find(&l, "UTF-8"), "\"a\\u{1b}[2Jb\\u{0}\u{fffd}\u{fffd}\"");
        assert_eq!(find(&l, "ASCII"), "\"a.[2Jb...\"");
    }

    #[test]
    fn a_bit_field_shows_its_bits_and_the_bytes_that_hold_them() {
        // bits 4..8 of 0xab are 0xb
        let l = lines(&[0xab], 1, 4, 8);
        assert_eq!(find(&l, "bytes"), "ab (1 byte)");
        assert_eq!(find(&l, "bits"), "4 bits = 11 (0xb, 0b1011)");
        assert_eq!(find(&l, "unsigned LE"), "<none>");
        assert_eq!(bits_value(&[0xab, 0xcd], 4, 12), Some(0xbc));
        assert_eq!(bits_value(&[0xab], 0, 1), Some(1));
        assert_eq!(bits_value(&[0xab], 3, 3), None);
        assert_eq!(bits_value(&[0xab], 0, 16), None, "past the data");
        assert_eq!(bits_value(&[0; 16], 0, 65), None, "more than 64 bits");
    }

    #[test]
    fn the_range_of_bytes_is_rounded_outwards() {
        assert_eq!(byte_range(&Selection { start: 0, end: 8 }), (0, 1));
        assert_eq!(byte_range(&Selection { start: 4, end: 12 }), (0, 2));
        assert_eq!(byte_range(&Selection { start: 8, end: 9 }), (1, 1));
    }

    #[test]
    fn nothing_is_printed_raw() {
        // the whole output for odd lengths, at the edges
        for len in 0..=9 {
            let data: Vec<u8> = (0_u8..len).map(|i| i.wrapping_mul(29)).collect();
            if data.is_empty() {
                continue;
            }
            let l = lines(&data, data.len() as u64, 0, data.len() as u64 * 8);
            assert!(
                !labelled(&l)
                    .join("\n")
                    .chars()
                    .any(|c| c.is_control() && c != '\n')
            );
        }
    }
}
