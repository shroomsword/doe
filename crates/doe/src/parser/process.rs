//! The byte transforms of `process:`: `xor`, `rol`, `ror` and `zlib`.

use std::io::Read;

use flate2::read::ZlibDecoder;

use crate::error::{DoeError, Result};

/// The most bytes a `process:` reads or produces: the compressed data it takes
/// from the input, and what `zlib` decompresses to.
pub const MAX_PROCESSED_LEN: usize = 256 * 1024 * 1024;

/// Exclusive or of `data` with `key`, repeated.
///
/// # Errors
///
/// Returns an error if the key is empty.
pub fn xor(data: &mut [u8], key: &[u8]) -> Result<()> {
    if key.is_empty() {
        return Err(DoeError::Schema("xor key is empty".to_owned()));
    }
    for (byte, k) in data.iter_mut().zip(key.iter().cycle()) {
        *byte ^= k;
    }
    Ok(())
}

/// Rotate every byte of `data` left by `amount` bits; a negative amount rotates
/// right.
pub fn rotate_left(data: &mut [u8], amount: i128) {
    let bits = u32::try_from(amount.rem_euclid(8)).unwrap_or(0);
    for byte in data {
        *byte = byte.rotate_left(bits);
    }
}

/// Decompress zlib data to at most `limit` bytes.
///
/// # Errors
///
/// Returns an error if the data is not valid zlib data, or the result is
/// longer than `limit` bytes.
pub fn inflate(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    // One more byte than the limit tells whether the data goes past it, and
    // nothing is allocated beyond that.
    ZlibDecoder::new(data)
        .take(u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut out)
        .map_err(|e| DoeError::Schema(format!("zlib decompression failed: {e}")))?;
    if out.len() > limit {
        return Err(DoeError::Schema(format!(
            "zlib data decompresses to more than the limit of {limit} bytes"
        )));
    }
    Ok(out)
}
