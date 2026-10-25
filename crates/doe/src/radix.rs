//! How an integer is written for a reader: in decimal, hexadecimal, binary or
//! octal. The viewers (the terminal viewer and the web IDE) show the string
//! made here, so that they show the same one; the web IDE has the same
//! algorithm in TypeScript, and both are tested with the same table of values
//! (`tests/radix.json`).

use std::fmt;
use std::str::FromStr;

/// The base in which integers are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum Radix {
    /// `31`
    #[default]
    Decimal,
    /// `0x1f`
    Hex,
    /// `0b00011111`
    Binary,
    /// `0o37`
    Octal,
}

impl Radix {
    /// All of them, in the order that `next` goes through them.
    pub const ALL: [Radix; 4] = [Radix::Decimal, Radix::Hex, Radix::Binary, Radix::Octal];

    /// The name that the saved settings and the web calls use: `dec`, `hex`,
    /// `bin` or `oct`.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Radix::Decimal => "dec",
            Radix::Hex => "hex",
            Radix::Binary => "bin",
            Radix::Octal => "oct",
        }
    }

    /// The name in words.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Radix::Decimal => "decimal",
            Radix::Hex => "hexadecimal",
            Radix::Binary => "binary",
            Radix::Octal => "octal",
        }
    }

    /// The one after this, going round: decimal, hex, binary, octal, decimal.
    #[must_use]
    pub fn next(self) -> Radix {
        match self {
            Radix::Decimal => Radix::Hex,
            Radix::Hex => Radix::Binary,
            Radix::Binary => Radix::Octal,
            Radix::Octal => Radix::Decimal,
        }
    }
}

impl fmt::Display for Radix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// The text is not the name of a radix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownRadix(pub String);

impl fmt::Display for UnknownRadix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown radix `{}`; use dec, hex, bin or oct", self.0)
    }
}

impl std::error::Error for UnknownRadix {}

impl FromStr for Radix {
    type Err = UnknownRadix;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.to_ascii_lowercase().as_str() {
            "dec" | "decimal" => Ok(Radix::Decimal),
            "hex" | "hexadecimal" => Ok(Radix::Hex),
            "bin" | "binary" => Ok(Radix::Binary),
            "oct" | "octal" => Ok(Radix::Octal),
            _ => Err(UnknownRadix(text.to_owned())),
        }
    }
}

/// An integer in a radix.
///
/// `bits` is the width of the type it was read as (16 for a `u2`, 3 for a
/// `b3`): the digits are padded with zeros to what that width needs, because
/// the width is part of what the reader wants to see. `None` is for a value
/// that was not read from the input (an instance), which has no width.
///
/// The form is `0x1f`: a prefix and the digits in lower case. A negative
/// number is the sign and the magnitude, `-0x1f`, not two's complement.
/// Decimal has no prefix and no padding.
///
/// ```
/// use doe::radix::{Radix, format_integer};
/// assert_eq!(format_integer(31, Some(16), Radix::Hex), "0x001f");
/// assert_eq!(format_integer(31, Some(8), Radix::Binary), "0b00011111");
/// assert_eq!(format_integer(-31, Some(8), Radix::Octal), "-0o037");
/// ```
#[must_use]
pub fn format_integer(value: i128, bits: Option<u32>, radix: Radix) -> String {
    let (prefix, per_digit) = match radix {
        Radix::Decimal => return value.to_string(),
        Radix::Hex => ("0x", 4),
        Radix::Binary => ("0b", 1),
        Radix::Octal => ("0o", 3),
    };
    let magnitude = value.unsigned_abs();
    let digits = match radix {
        Radix::Hex => format!("{magnitude:x}"),
        Radix::Binary => format!("{magnitude:b}"),
        _ => format!("{magnitude:o}"),
    };
    let width = bits.map_or(0, |bits| bits.div_ceil(per_digit) as usize);
    let sign = if value < 0 { "-" } else { "" };
    format!("{sign}{prefix}{digits:0>width$}")
}

/// The two's complement of a negative number of `bits` bits, as the bytes hold
/// it: `0xff` for -1 in 8 bits. `None` for a number that is not negative, or
/// for a width that does not hold it.
#[must_use]
pub fn twos_complement(value: i128, bits: u32) -> Option<String> {
    if value >= 0 || bits == 0 || bits > 64 {
        return None;
    }
    let low = i128::from(bits - 1);
    if value < -(1_i128 << low) {
        return None;
    }
    let pattern = (1_i128 << bits) + value;
    Some(format_integer(pattern, Some(bits), Radix::Hex))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_of_values_is_what_the_web_page_checks_too() {
        let table: serde_json::Value =
            serde_json::from_str(include_str!("../tests/radix.json")).expect("table is JSON");
        for row in table.as_array().expect("table is a list") {
            let value: i128 = row["value"].as_str().unwrap().parse().unwrap();
            let bits = row["bits"]
                .as_u64()
                .map(|bits| u32::try_from(bits).unwrap());
            for radix in Radix::ALL {
                let want = row[radix.id()].as_str().unwrap();
                assert_eq!(
                    format_integer(value, bits, radix),
                    want,
                    "{value} in {} bits as {radix}",
                    bits.map_or_else(|| "no".to_owned(), |b| b.to_string())
                );
            }
        }
    }

    #[test]
    fn the_radixes_go_round_and_are_known_by_name() {
        let mut radix = Radix::Decimal;
        for expect in [Radix::Hex, Radix::Binary, Radix::Octal, Radix::Decimal] {
            radix = radix.next();
            assert_eq!(radix, expect);
        }
        for radix in Radix::ALL {
            assert_eq!(radix.id().parse::<Radix>(), Ok(radix));
            assert_eq!(radix.name().parse::<Radix>(), Ok(radix));
        }
        assert_eq!("HEX".parse::<Radix>(), Ok(Radix::Hex));
        assert!("base16".parse::<Radix>().is_err());
    }

    #[test]
    fn the_twos_complement_is_what_the_bytes_hold() {
        assert_eq!(twos_complement(-1, 8).as_deref(), Some("0xff"));
        assert_eq!(twos_complement(-2, 16).as_deref(), Some("0xfffe"));
        assert_eq!(
            twos_complement(i128::from(i64::MIN), 64).as_deref(),
            Some("0x8000000000000000")
        );
        assert_eq!(twos_complement(5, 8), None);
        assert_eq!(twos_complement(-129, 8), None);
    }
}
