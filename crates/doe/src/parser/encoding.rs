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
