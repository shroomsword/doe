//! The byte transforms of `process:`: `xor`, `rol`, `ror`, `zlib`, `deflate`, `gzip`, `brotli`, `bzip2`, `xz`, `lzma`, `lzip` and `zstd`.

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

/// Read a decoder to its end onto `out`, so that `out` is at most `limit` bytes long.
fn read_onto(decoder: impl Read, out: &mut Vec<u8>, what: &str, limit: usize) -> Result<()> {
    // One more byte than the limit tells whether the data goes past it, and
    // nothing is allocated beyond that.
    let room = limit.saturating_sub(out.len());
    decoder
        .take(u64::try_from(room).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(out)
        .map_err(|e| DoeError::Schema(format!("{what} decompression failed: {e}")))?;
    if out.len() > limit {
        return Err(DoeError::Schema(format!(
            "{what} data decompresses to more than the limit of {limit} bytes"
        )));
    }
    Ok(())
}

/// Read a decoder to its end, up to `limit` bytes.
fn read_limited(decoder: impl Read, what: &str, limit: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    read_onto(decoder, &mut out, what, limit)?;
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

/// Decompress a bzip2 file (`BZh`, blocks, the CRCs checked) to at most `limit` bytes. The output is limited while it is
/// decoded.
///
/// # Errors
///
/// Returns an error if the data is not valid bzip2 data (a bad header, a cut or corrupt stream, a CRC that does not
/// match), or the result is longer than `limit` bytes.
pub fn bzip2_decode(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    read_limited(bzip2::read::MultiBzDecoder::new(data), "bzip2", limit)
}

/// The memory a decoder may ask for, in KiB: what the output may be, as a window larger than that is no use for output
/// that size. At least a few megabytes, so that a small limit does not refuse an ordinary stream.
fn memory_kib(limit: usize) -> u32 {
    u32::try_from(limit / 1024)
        .unwrap_or(u32::MAX)
        .max(16 * 1024)
}

/// Decompress an xz file (the stream or streams, with their padding; the blocks' LZMA2 data and the delta and branch
/// filters; the CRC32, CRC64 or SHA-256 checks verified) to at most `limit` bytes.
///
/// # Errors
///
/// Returns an error if the data is not valid xz data (a bad header, a cut or corrupt stream, a check that does not
/// match), or the result is longer than `limit` bytes.
pub fn xz_decode(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    read_limited(
        lzma_rust2::XzReader::new_mem_limit(data, true, memory_kib(limit)),
        "xz",
        limit,
    )
}

/// Decompress a `.lzma` file (the 13-byte header and the LZMA stream) to at most `limit` bytes. The dictionary the header
/// asks for is held to a memory limit that follows `limit`.
///
/// # Errors
///
/// Returns an error if the data is not a valid LZMA stream, or the result is longer than `limit` bytes.
pub fn lzma_decode(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    let reader = lzma_rust2::LzmaReader::new_mem_limit(data, memory_kib(limit), None)
        .map_err(|e| DoeError::Schema(format!("lzma decompression failed: {e}")))?;
    read_limited(reader, "lzma", limit)
}

/// Decompress a lzip file (every member, the CRC-32 and the sizes checked) to at most `limit` bytes.
///
/// # Errors
///
/// Returns an error if the data is not valid lzip data, or the result is longer than `limit` bytes.
pub fn lzip_decode(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    read_limited(lzma_rust2::LzipReader::new(data), "lzip", limit)
}

/// Decompress Zstandard data (RFC 8878): every frame in turn, to at most `limit` bytes.
///
/// # Errors
///
/// Returns an error if the data is not valid Zstandard data (a bad frame, a cut or corrupt stream, a checksum that does
/// not match), or the result is longer than `limit` bytes.
pub fn zstd_decode(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    let mut input = data;
    let mut out = Vec::new();
    loop {
        match ruzstd::decoding::StreamingDecoder::new(&mut input) {
            Ok(mut decoder) => {
                read_onto(&mut decoder, &mut out, "zstd", limit)?;
                // the decoder does not compare the checksum of the content (the last four bytes of a frame that has
                // one) with the one it calculated
                let (_, frame) = decoder.into_parts();
                if let (Some(written), Some(calculated)) = (
                    frame.get_checksum_from_data(),
                    frame.get_calculated_checksum(),
                ) && written != calculated
                {
                    return Err(DoeError::Schema(format!(
                        "zstd decompression failed: the content checksum is {written:08x} but the data has {calculated:08x}"
                    )));
                }
            }
            // a skippable frame holds data for the application: the decoder has read its header, the data is skipped
            Err(ruzstd::decoding::errors::FrameDecoderError::ReadFrameHeaderError(
                ruzstd::decoding::errors::ReadFrameHeaderError::SkipFrame { length, .. },
            )) => {
                let length = usize::try_from(length).unwrap_or(usize::MAX);
                input = input.get(length..).ok_or_else(|| {
                    DoeError::Schema(
                        "zstd decompression failed: a skippable frame is cut".to_owned(),
                    )
                })?;
            }
            Err(e) => return Err(DoeError::Schema(format!("zstd decompression failed: {e}"))),
        }
        if input.is_empty() {
            return Ok(out);
        }
    }
}
