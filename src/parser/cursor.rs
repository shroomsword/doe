//! A cursor over an immutable byte buffer.
//!
//! The parser holds one top-level `Cursor` and may create sub-cursors with
//! restricted windows for size-limited sub-structures (not used in the initial
//! implementation but scaffolded here for future use).

use crate::error::{DoeError, Result};

// ─────────────────────────────────────────────────────────────────────────────
// Cursor
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Cursor<'buf> {
    buf: &'buf [u8],
    /// Absolute offset of `buf[0]` within the original file buffer.
    /// Always zero for the root cursor; non-zero for sub-cursors.
    base: usize,
    /// Current read position relative to `buf[0]`.
    pos: usize,
}

impl<'buf> Cursor<'buf> {
    /// Create a cursor over an entire buffer.
    pub fn new(buf: &'buf [u8]) -> Self {
        Cursor { buf, base: 0, pos: 0 }
    }

    /// Create a sub-cursor covering `buf[start..start+len]`.
    pub fn sub(&self, start: usize, len: usize) -> Result<Cursor<'buf>> {
        let end = start + len;
        if end > self.buf.len() {
            return Err(self.overrun_error(len));
        }
        Ok(Cursor {
            buf:  &self.buf[start..end],
            base: self.base + start,
            pos:  0,
        })
    }

    // ── Position ─────────────────────────────────────────────────────────────

    /// Current byte offset from the start of the *root* buffer.
    pub fn absolute_pos(&self) -> usize { self.base + self.pos }

    /// Current byte offset within this cursor's window.
    pub fn pos(&self) -> usize { self.pos }

    /// Number of bytes remaining in this cursor's window.
    pub fn remaining(&self) -> usize { self.buf.len() - self.pos }

    /// Returns `true` if the cursor is at the end of its window.
    pub fn is_eof(&self) -> bool { self.pos >= self.buf.len() }

    /// Seek to an absolute position within this cursor's window.
    pub fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.buf.len() {
            return Err(DoeError::Schema(format!(
                "seek to {} is past end of window (size {})",
                pos, self.buf.len()
            )));
        }
        self.pos = pos;
        Ok(())
    }

    // ── Raw reads ─────────────────────────────────────────────────────────────

    /// Read exactly `n` bytes, advancing the cursor.
    pub fn read_bytes(&mut self, n: usize) -> Result<&'buf [u8]> {
        if self.pos + n > self.buf.len() {
            return Err(self.overrun_error(n));
        }
        let slice = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    /// Peek at the next `n` bytes without advancing.
    pub fn peek_bytes(&self, n: usize) -> Result<&'buf [u8]> {
        if self.pos + n > self.buf.len() {
            return Err(self.overrun_error(n));
        }
        Ok(&self.buf[self.pos..self.pos + n])
    }

    /// Read bytes up to and including the first occurrence of `terminator`.
    /// Returns the slice *without* the terminator byte.
    pub fn read_until(&mut self, terminator: u8) -> Result<&'buf [u8]> {
        let start = self.pos;
        while self.pos < self.buf.len() {
            if self.buf[self.pos] == terminator {
                let s = &self.buf[start..self.pos];
                self.pos += 1; // consume the terminator
                return Ok(s);
            }
            self.pos += 1;
        }
        // Hit EOF without finding terminator — return what we have.
        Ok(&self.buf[start..self.pos])
    }

    // ── Typed integer reads ───────────────────────────────────────────────────

    pub fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_bytes(1)?[0])
    }

    pub fn read_u16_le(&mut self) -> Result<u16> {
        let b = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn read_u16_be(&mut self) -> Result<u16> {
        let b = self.read_bytes(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    pub fn read_u32_le(&mut self) -> Result<u32> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn read_u32_be(&mut self) -> Result<u32> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn read_u64_le(&mut self) -> Result<u64> {
        let b = self.read_bytes(8)?;
        Ok(u64::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn read_u64_be(&mut self) -> Result<u64> {
        let b = self.read_bytes(8)?;
        Ok(u64::from_be_bytes(b.try_into().unwrap()))
    }

    pub fn read_i8(&mut self) -> Result<i8> {
        Ok(self.read_u8()? as i8)
    }

    pub fn read_i16_le(&mut self) -> Result<i16> { Ok(self.read_u16_le()? as i16) }
    pub fn read_i16_be(&mut self) -> Result<i16> { Ok(self.read_u16_be()? as i16) }
    pub fn read_i32_le(&mut self) -> Result<i32> { Ok(self.read_u32_le()? as i32) }
    pub fn read_i32_be(&mut self) -> Result<i32> { Ok(self.read_u32_be()? as i32) }
    pub fn read_i64_le(&mut self) -> Result<i64> { Ok(self.read_u64_le()? as i64) }
    pub fn read_i64_be(&mut self) -> Result<i64> { Ok(self.read_u64_be()? as i64) }

    pub fn read_f32_le(&mut self) -> Result<f32> {
        let b = self.read_bytes(4)?;
        Ok(f32::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn read_f32_be(&mut self) -> Result<f32> {
        let b = self.read_bytes(4)?;
        Ok(f32::from_be_bytes(b.try_into().unwrap()))
    }

    pub fn read_f64_le(&mut self) -> Result<f64> {
        let b = self.read_bytes(8)?;
        Ok(f64::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn read_f64_be(&mut self) -> Result<f64> {
        let b = self.read_bytes(8)?;
        Ok(f64::from_be_bytes(b.try_into().unwrap()))
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn overrun_error(&self, requested: usize) -> DoeError {
        DoeError::Schema(format!(
            "buffer overrun at offset {}: requested {} bytes, {} remaining",
            self.absolute_pos(),
            requested,
            self.remaining(),
        ))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cur(data: &[u8]) -> Cursor { Cursor::new(data) }

    // ── Position tracking ─────────────────────────────────────────────────────

    #[test]
    fn new_cursor_at_zero() {
        let c = cur(&[1, 2, 3]);
        assert_eq!(c.pos(), 0);
        assert_eq!(c.remaining(), 3);
        assert!(!c.is_eof());
    }

    #[test]
    fn eof_on_empty() {
        let c = cur(&[]);
        assert!(c.is_eof());
        assert_eq!(c.remaining(), 0);
    }

    #[test]
    fn pos_advances_after_read() {
        let mut c = cur(&[1, 2, 3, 4]);
        c.read_u8().unwrap();
        assert_eq!(c.pos(), 1);
        assert_eq!(c.remaining(), 3);
    }

    #[test]
    fn absolute_pos_on_root() {
        let mut c = cur(&[1, 2, 3]);
        c.read_u8().unwrap();
        assert_eq!(c.absolute_pos(), 1);
    }

    #[test]
    fn absolute_pos_on_sub_cursor() {
        let buf = [0u8; 10];
        let c = cur(&buf);
        let mut sub = c.sub(4, 3).unwrap();
        sub.read_u8().unwrap();
        assert_eq!(sub.absolute_pos(), 5);
    }

    // ── Seek ─────────────────────────────────────────────────────────────────

    #[test]
    fn seek_forward() {
        let mut c = cur(&[1, 2, 3, 4, 5]);
        c.seek(3).unwrap();
        assert_eq!(c.pos(), 3);
        assert_eq!(c.read_u8().unwrap(), 4);
    }

    #[test]
    fn seek_to_end() {
        let mut c = cur(&[1, 2, 3]);
        c.seek(3).unwrap();
        assert!(c.is_eof());
    }

    #[test]
    fn seek_past_end_is_error() {
        let mut c = cur(&[1, 2, 3]);
        assert!(c.seek(4).is_err());
    }

    // ── read_bytes ────────────────────────────────────────────────────────────

    #[test]
    fn read_bytes_exact() {
        let mut c = cur(&[0xde, 0xad, 0xbe, 0xef]);
        let b = c.read_bytes(4).unwrap();
        assert_eq!(b, &[0xde, 0xad, 0xbe, 0xef]);
        assert!(c.is_eof());
    }

    #[test]
    fn read_bytes_partial_then_rest() {
        let mut c = cur(&[1, 2, 3, 4, 5]);
        assert_eq!(c.read_bytes(2).unwrap(), &[1, 2]);
        assert_eq!(c.read_bytes(3).unwrap(), &[3, 4, 5]);
    }

    #[test]
    fn read_bytes_overrun_is_error() {
        let mut c = cur(&[1, 2]);
        assert!(c.read_bytes(3).is_err());
    }

    #[test]
    fn read_zero_bytes() {
        let mut c = cur(&[1, 2]);
        assert_eq!(c.read_bytes(0).unwrap(), &[] as &[u8]);
        assert_eq!(c.pos(), 0); // position unchanged
    }

    // ── peek_bytes ────────────────────────────────────────────────────────────

    #[test]
    fn peek_does_not_advance() {
        let c = cur(&[1, 2, 3]);
        let p = c.peek_bytes(2).unwrap();
        assert_eq!(p, &[1, 2]);
        assert_eq!(c.pos(), 0);
    }

    #[test]
    fn peek_overrun_is_error() {
        let c = cur(&[1]);
        assert!(c.peek_bytes(2).is_err());
    }

    // ── read_until ────────────────────────────────────────────────────────────

    #[test]
    fn read_until_terminator() {
        let mut c = cur(b"hello\x00world");
        let s = c.read_until(0x00).unwrap();
        assert_eq!(s, b"hello");
        assert_eq!(c.pos(), 6); // "hello" + null
    }

    #[test]
    fn read_until_at_start() {
        let mut c = cur(&[0x00, 1, 2]);
        let s = c.read_until(0x00).unwrap();
        assert_eq!(s, &[] as &[u8]);
        assert_eq!(c.pos(), 1);
    }

    #[test]
    fn read_until_no_terminator_returns_all() {
        let mut c = cur(b"hello");
        let s = c.read_until(0x00).unwrap();
        assert_eq!(s, b"hello");
        assert!(c.is_eof());
    }

    #[test]
    fn read_until_newline() {
        let mut c = cur(b"line1\nline2\n");
        assert_eq!(c.read_until(b'\n').unwrap(), b"line1");
        assert_eq!(c.read_until(b'\n').unwrap(), b"line2");
        assert!(c.is_eof());
    }

    // ── Unsigned integer reads ────────────────────────────────────────────────

    #[test]
    fn read_u8_values() {
        let mut c = cur(&[0x00, 0x7f, 0x80, 0xff]);
        assert_eq!(c.read_u8().unwrap(), 0x00);
        assert_eq!(c.read_u8().unwrap(), 0x7f);
        assert_eq!(c.read_u8().unwrap(), 0x80);
        assert_eq!(c.read_u8().unwrap(), 0xff);
    }

    #[test]
    fn read_u16_le() {
        let mut c = cur(&[0x34, 0x12]);
        assert_eq!(c.read_u16_le().unwrap(), 0x1234);
    }

    #[test]
    fn read_u16_be() {
        let mut c = cur(&[0x12, 0x34]);
        assert_eq!(c.read_u16_be().unwrap(), 0x1234);
    }

    #[test]
    fn read_u32_le() {
        let mut c = cur(&[0x78, 0x56, 0x34, 0x12]);
        assert_eq!(c.read_u32_le().unwrap(), 0x12345678);
    }

    #[test]
    fn read_u32_be() {
        let mut c = cur(&[0x12, 0x34, 0x56, 0x78]);
        assert_eq!(c.read_u32_be().unwrap(), 0x12345678);
    }

    #[test]
    fn read_u64_le() {
        let data = 0x0102030405060708u64.to_le_bytes();
        let mut c = cur(&data);
        assert_eq!(c.read_u64_le().unwrap(), 0x0102030405060708);
    }

    #[test]
    fn read_u64_be() {
        let data = 0x0102030405060708u64.to_be_bytes();
        let mut c = cur(&data);
        assert_eq!(c.read_u64_be().unwrap(), 0x0102030405060708);
    }

    // ── Signed integer reads ──────────────────────────────────────────────────

    #[test]
    fn read_i8_positive() {
        let mut c = cur(&[0x7f]);
        assert_eq!(c.read_i8().unwrap(), 127);
    }

    #[test]
    fn read_i8_negative() {
        let mut c = cur(&[0xff]);
        assert_eq!(c.read_i8().unwrap(), -1);
    }

    #[test]
    fn read_i16_le_negative() {
        let mut c = cur(&[0xff, 0xff]);
        assert_eq!(c.read_i16_le().unwrap(), -1);
    }

    #[test]
    fn read_i32_be_negative() {
        let mut c = cur(&[0xff, 0xff, 0xff, 0xff]);
        assert_eq!(c.read_i32_be().unwrap(), -1);
    }

    #[test]
    fn read_i64_le_min() {
        let data = i64::MIN.to_le_bytes();
        let mut c = cur(&data);
        assert_eq!(c.read_i64_le().unwrap(), i64::MIN);
    }

    // ── Float reads ───────────────────────────────────────────────────────────

    #[test]
    fn read_f32_le() {
        let v: f32 = 1.5;
        let data = v.to_le_bytes();
        let mut c = cur(&data);
        assert_eq!(c.read_f32_le().unwrap(), 1.5f32);
    }

    #[test]
    fn read_f32_be() {
        let v: f32 = -2.0;
        let data = v.to_be_bytes();
        let mut c = cur(&data);
        assert_eq!(c.read_f32_be().unwrap(), -2.0f32);
    }

    #[test]
    fn read_f64_le() {
        let v: f64 = std::f64::consts::PI;
        let data = v.to_le_bytes();
        let mut c = cur(&data);
        assert!((c.read_f64_le().unwrap() - std::f64::consts::PI).abs() < 1e-15);
    }

    #[test]
    fn read_f64_be() {
        let v: f64 = -1.0;
        let data = v.to_be_bytes();
        let mut c = cur(&data);
        assert_eq!(c.read_f64_be().unwrap(), -1.0f64);
    }

    // ── Sub-cursor ────────────────────────────────────────────────────────────

    #[test]
    fn sub_cursor_reads_slice() {
        let buf = [0x00, 0x01, 0x02, 0x03, 0x04];
        let c = cur(&buf);
        let mut sub = c.sub(1, 3).unwrap();
        assert_eq!(sub.read_bytes(3).unwrap(), &[0x01, 0x02, 0x03]);
        assert!(sub.is_eof());
    }

    #[test]
    fn sub_cursor_out_of_bounds_is_error() {
        let buf = [0u8; 4];
        let c = cur(&buf);
        assert!(c.sub(3, 3).is_err());
    }

    #[test]
    fn sub_cursor_at_end() {
        let buf = [1u8, 2, 3];
        let c = cur(&buf);
        let sub = c.sub(3, 0).unwrap();
        assert!(sub.is_eof());
    }

    // ── Overrun on typed reads ────────────────────────────────────────────────

    #[test]
    fn u16_overrun() { let mut c = cur(&[0x01]); assert!(c.read_u16_le().is_err()); }
    #[test]
    fn u32_overrun() { let mut c = cur(&[0x01, 0x02, 0x03]); assert!(c.read_u32_le().is_err()); }
    #[test]
    fn u64_overrun() { let mut c = cur(&[0u8; 7]); assert!(c.read_u64_be().is_err()); }
    #[test]
    fn f32_overrun() { let mut c = cur(&[0u8; 3]); assert!(c.read_f32_le().is_err()); }
    #[test]
    fn f64_overrun() { let mut c = cur(&[0u8; 7]); assert!(c.read_f64_be().is_err()); }
}
