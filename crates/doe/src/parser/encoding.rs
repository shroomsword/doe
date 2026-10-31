//! Decoding the bytes of a string.

use encoding_rs::DecoderResult;

use crate::error::{DoeError, Result};
use crate::schema::ir::Encoding;

/// IBM code page 437 above ASCII: the characters of the bytes 0x80 to 0xff.
const CP437_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', 'É', 'æ', 'Æ',
    'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', 'á', 'í', 'ó', 'ú', 'ñ', 'Ñ',
    'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕',
    '╣', '║', '╗', '╝', '╜', '╛', '┐', '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦',
    '╠', '═', '╬', '╧', '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐',
    '▀', 'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', '≡', '±',
    '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

/// Decode `bytes` as `encoding`.
///
/// # Errors
///
/// Returns an error that names the encoding and the offset in `bytes` if they
/// are not valid in the encoding.
pub fn decode(bytes: &[u8], encoding: Encoding) -> Result<String> {
    match encoding {
        Encoding::Utf8 => std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|e| DoeError::Schema(format!("UTF-8 decode error: {e}"))),
        Encoding::Ascii => match bytes.iter().position(|&b| b > 0x7f) {
            Some(at) => Err(invalid("ASCII", at)),
            None => Ok(String::from_utf8_lossy(bytes).into_owned()),
        },
        // Latin-1 maps directly to the first 256 Unicode code points.
        Encoding::Latin1 => Ok(bytes.iter().map(|&b| char::from(b)).collect()),
        Encoding::Utf16Le => utf16(bytes, u16::from_le_bytes, "UTF-16LE"),
        Encoding::Utf16Be => utf16(bytes, u16::from_be_bytes, "UTF-16BE"),
        Encoding::Utf32Le => utf32(bytes, u32::from_le_bytes, "UTF-32LE"),
        Encoding::Utf32Be => utf32(bytes, u32::from_be_bytes, "UTF-32BE"),
        Encoding::Cp437 => Ok(bytes
            .iter()
            .map(|&b| match b.checked_sub(0x80) {
                Some(high) => CP437_HIGH
                    .get(usize::from(high))
                    .copied()
                    .unwrap_or(char::REPLACEMENT_CHARACTER),
                None => char::from(b),
            })
            .collect()),
        Encoding::Legacy(name) => legacy(bytes, name),
    }
}

/// A string decoded as far as it could be: the text, with U+FFFD (or, for ASCII, the character of Latin-1) where the
/// bytes are not valid, and where the first of them is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// The text.
    pub text: String,
    /// The byte of the string where the first invalid data starts; `None` when all of it is valid.
    pub invalid_at: Option<usize>,
}

/// The name of the encoding, as a message shows it.
pub fn name_of(encoding: Encoding) -> String {
    match encoding {
        Encoding::Utf8 => "UTF-8".to_owned(),
        Encoding::Ascii => "ASCII".to_owned(),
        Encoding::Latin1 => "ISO-8859-1".to_owned(),
        Encoding::Utf16Le => "UTF-16LE".to_owned(),
        Encoding::Utf16Be => "UTF-16BE".to_owned(),
        Encoding::Utf32Le => "UTF-32LE".to_owned(),
        Encoding::Utf32Be => "UTF-32BE".to_owned(),
        Encoding::Cp437 => "CP437".to_owned(),
        Encoding::Legacy(name) => name.to_owned(),
    }
}

/// Decode `bytes` as `encoding` and go on where they are not valid, the way the reference runtime does: UTF-8, UTF-16
/// and UTF-32 give U+FFFD for the data that is not valid, and ASCII gives the character of Latin-1 for a byte above 0x7f.
///
/// # Errors
///
/// Only for an encoding that is not known, or a string too long to decode.
pub fn decode_lossy(bytes: &[u8], encoding: Encoding) -> Result<Decoded> {
    Ok(match encoding {
        Encoding::Utf8 => match std::str::from_utf8(bytes) {
            Ok(text) => Decoded {
                text: text.to_owned(),
                invalid_at: None,
            },
            Err(e) => Decoded {
                text: String::from_utf8_lossy(bytes).into_owned(),
                invalid_at: Some(e.valid_up_to()),
            },
        },
        Encoding::Ascii => Decoded {
            text: bytes.iter().map(|&b| char::from(b)).collect(),
            invalid_at: bytes.iter().position(|&b| b > 0x7f),
        },
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let unit: fn([u8; 2]) -> u16 = if matches!(encoding, Encoding::Utf16Le) {
                u16::from_le_bytes
            } else {
                u16::from_be_bytes
            };
            let (pairs, rest) = bytes.as_chunks::<2>();
            let mut invalid_at = None;
            let mut offset = 0;
            let mut text = String::with_capacity(bytes.len());
            for decoded in char::decode_utf16(pairs.iter().map(|&pair| unit(pair))) {
                match decoded {
                    Ok(c) => {
                        offset += 2 * c.len_utf16();
                        text.push(c);
                    }
                    Err(_) => {
                        invalid_at.get_or_insert(offset);
                        offset += 2;
                        text.push(char::REPLACEMENT_CHARACTER);
                    }
                }
            }
            if !rest.is_empty() {
                invalid_at.get_or_insert(pairs.len() * 2);
                text.push(char::REPLACEMENT_CHARACTER);
            }
            Decoded { text, invalid_at }
        }
        Encoding::Utf32Le | Encoding::Utf32Be => {
            let unit: fn([u8; 4]) -> u32 = if matches!(encoding, Encoding::Utf32Le) {
                u32::from_le_bytes
            } else {
                u32::from_be_bytes
            };
            let (quads, rest) = bytes.as_chunks::<4>();
            let mut invalid_at = None;
            let mut text = String::with_capacity(quads.len());
            for (index, &quad) in quads.iter().enumerate() {
                match char::from_u32(unit(quad)) {
                    Some(c) => text.push(c),
                    None => {
                        invalid_at.get_or_insert(index * 4);
                        text.push(char::REPLACEMENT_CHARACTER);
                    }
                }
            }
            if !rest.is_empty() {
                invalid_at.get_or_insert(quads.len() * 4);
                text.push(char::REPLACEMENT_CHARACTER);
            }
            Decoded { text, invalid_at }
        }
        Encoding::Legacy(name) => {
            let label = encoding_rs::Encoding::for_label(name.as_bytes())
                .ok_or_else(|| DoeError::Schema(format!("unknown encoding '{name}'")))?;
            let (text, _, had_errors) = label.decode(bytes);
            let invalid_at = if had_errors {
                // where the strict decoder stops
                match legacy(bytes, name) {
                    Err(DoeError::Schema(message)) if message.starts_with("string too long") => {
                        return Err(DoeError::Schema(message));
                    }
                    Err(DoeError::Schema(message)) => message
                        .split(" at byte ")
                        .nth(1)
                        .and_then(|rest| rest.split(' ').next())
                        .and_then(|number| number.parse().ok())
                        .or(Some(0)),
                    _ => Some(0),
                }
            } else {
                None
            };
            Decoded {
                text: text.into_owned(),
                invalid_at,
            }
        }
        // Latin-1 and CP437 have a character for every byte.
        Encoding::Latin1 | Encoding::Cp437 => Decoded {
            text: decode(bytes, encoding)?,
            invalid_at: None,
        },
    })
}

fn invalid(name: &str, offset: usize) -> DoeError {
    DoeError::Schema(format!(
        "invalid {name} data at byte {offset} of the string"
    ))
}

fn utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16, name: &str) -> Result<String> {
    let (pairs, rest) = bytes.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(invalid(name, pairs.len() * 2));
    }
    let units = pairs.iter().map(|&pair| unit(pair));
    let mut out = String::with_capacity(bytes.len());
    let mut offset = 0;
    for decoded in char::decode_utf16(units) {
        let c = decoded.map_err(|_| invalid(name, offset))?;
        offset += 2 * c.len_utf16();
        out.push(c);
    }
    Ok(out)
}

fn utf32(bytes: &[u8], unit: fn([u8; 4]) -> u32, name: &str) -> Result<String> {
    let (quads, rest) = bytes.as_chunks::<4>();
    if !rest.is_empty() {
        return Err(invalid(name, quads.len() * 4));
    }
    let mut out = String::with_capacity(quads.len());
    for (index, &quad) in quads.iter().enumerate() {
        let c = char::from_u32(unit(quad)).ok_or_else(|| invalid(name, index * 4))?;
        out.push(c);
    }
    Ok(out)
}

fn legacy(bytes: &[u8], name: &str) -> Result<String> {
    let encoding = encoding_rs::Encoding::for_label(name.as_bytes())
        .ok_or_else(|| DoeError::Schema(format!("unknown encoding '{name}'")))?;
    let mut decoder = encoding.new_decoder_without_bom_handling();
    let mut out = String::new();
    if let Some(room) = decoder.max_utf8_buffer_length_without_replacement(bytes.len()) {
        out.reserve(room);
    }
    let (result, read) = decoder.decode_to_string_without_replacement(bytes, &mut out, true);
    match result {
        DecoderResult::InputEmpty => Ok(out),
        DecoderResult::Malformed(bad, extra) => Err(invalid(
            name,
            read.saturating_sub(usize::from(bad) + usize::from(extra)),
        )),
        // `reserve` made room for everything.
        DecoderResult::OutputFull => Err(DoeError::Schema(format!(
            "string too long to decode as {name}"
        ))),
    }
}
