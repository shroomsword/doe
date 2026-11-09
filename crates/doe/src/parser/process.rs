//! The byte transforms of `process:`: `xor`, `rol`, `ror`, `zlib`, `deflate`, `gzip` and `brotli`.

use std::io::Read;

use flate2::read::{DeflateDecoder, MultiGzDecoder, ZlibDecoder};

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

/// Read a decoder to its end, up to `limit` bytes.
fn read_limited(decoder: impl Read, what: &str, limit: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    // One more byte than the limit tells whether the data goes past it, and
    // nothing is allocated beyond that.
    decoder
        .take(u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut out)
        .map_err(|e| DoeError::Schema(format!("{what} decompression failed: {e}")))?;
    if out.len() > limit {
        return Err(DoeError::Schema(format!(
            "{what} data decompresses to more than the limit of {limit} bytes"
        )));
    }
    Ok(out)
}

/// Decompress zlib data (RFC 1950) to at most `limit` bytes.
///
/// # Errors
///
/// Returns an error if the data is not valid zlib data, or the result is
/// longer than `limit` bytes.
pub fn inflate(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    read_limited(ZlibDecoder::new(data), "zlib", limit)
}

/// Decompress a raw deflate stream (RFC 1951: no header, no checksum) to at most `limit` bytes.
///
/// # Errors
///
/// Returns an error if the data is not a valid deflate stream, or the result is longer than `limit` bytes.
pub fn inflate_raw(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    read_limited(DeflateDecoder::new(data), "deflate", limit)
}

/// Decompress gzip data (RFC 1952) to at most `limit` bytes: every member in turn, each with its header read and its
/// CRC-32 and size checked, as `gunzip` does. Bytes after the last member are an error.
///
/// # Errors
///
/// Returns an error if the data is not valid gzip data (a bad header, a stream that is cut or corrupt, a CRC or size
/// that does not match), or the result is longer than `limit` bytes.
pub fn gunzip(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    read_limited(MultiGzDecoder::new(data), "gzip", limit)
}

/// Decompress a Brotli stream (RFC 7932) to at most `limit` bytes. The output is limited while it is decoded (a stream can
/// expand a lot), and a stream that is cut is an error. Bytes after the end of the stream are not looked at (the decoder
/// reads ahead, so it cannot say where the stream ended in the input), unlike `gzip`, whose members are counted.
///
/// # Errors
///
/// Returns an error if the data is not a valid Brotli stream (a bad header, a cut or corrupt stream), or the result is
/// longer than `limit` bytes.
pub fn brotli_decode(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    let mut input = data;
    let out = read_limited(
        brotli_decompressor::Decompressor::new(&mut input, 4096),
        "brotli",
        limit,
    )?;
    Ok(out)
}
