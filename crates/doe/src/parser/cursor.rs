//! A cursor over an immutable byte buffer.
//!
//! The parser holds one top-level `Cursor` and may create sub-cursors with
//! restricted windows for size-limited sub-structures (not used in the initial
//! implementation but scaffolded here for future use).

use crate::error::{DoeError, Result};

// ─────────────────────────────────────────────────────────────────────────────
// Cursor
// ─────────────────────────────────────────────────────────────────────────────

/// A position-tracking window over an immutable byte buffer.
#[derive(Debug, Clone)]
pub struct Cursor<'buf> {
    buf: &'buf [u8],
    /// Absolute offset of `buf[0]` within the original file buffer.
    /// Always zero for the root cursor; non-zero for sub-cursors.
    base: usize,
    /// Current read position relative to `buf[0]`.
    pos: usize,
}

macro_rules! read_int {
    ($($name:ident, $ty:ty, $from:ident, $width:literal;)*) => {$(
        #[doc = concat!("Read a `", stringify!($ty), "`, advancing the cursor.")]
        ///
        /// # Errors
        ///
        /// Returns an error if fewer bytes remain than the type's width.
        pub fn $name(&mut self) -> Result<$ty> {
            Ok(<$ty>::$from(self.read_array::<$width>()?))
        }
    )*};
}

impl<'buf> Cursor<'buf> {
    /// Create a cursor over an entire buffer.
    pub fn new(buf: &'buf [u8]) -> Self {
        Cursor {
            buf,
            base: 0,
            pos: 0,
        }
    }

    /// Create a sub-cursor covering `buf[start..start+len]`.
    ///
    /// # Errors
    ///
    /// Returns an error if the window extends past the end of this cursor.
    pub fn sub(&self, start: usize, len: usize) -> Result<Cursor<'buf>> {
        let window = start
            .checked_add(len)
            .and_then(|end| self.buf.get(start..end))
            .ok_or_else(|| self.overrun_error(len))?;
        Ok(Cursor {
            buf: window,
            base: self.base.saturating_add(start),
            pos: 0,
        })
    }

    // ── Position ─────────────────────────────────────────────────────────────

    /// Current byte offset from the start of the *root* buffer.
    pub fn absolute_pos(&self) -> usize {
        self.base.saturating_add(self.pos)
    }

    /// Current byte offset within this cursor's window.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Number of bytes remaining in this cursor's window.
    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    /// Returns `true` if the cursor is at the end of its window.
    pub fn is_eof(&self) -> bool {
        self.pos >= self.buf.len()
    }

    /// Seek to an absolute position within this cursor's window.
    ///
    /// # Errors
    ///
    /// Returns an error if `pos` is past the end of the window.
    pub fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.buf.len() {
            return Err(DoeError::Schema(format!(
                "seek to {} is past end of window (size {})",
                pos,
                self.buf.len()
            )));
        }
        self.pos = pos;
        Ok(())
    }

    // ── Raw reads ─────────────────────────────────────────────────────────────

    /// Read exactly `n` bytes, advancing the cursor.
    ///
    /// # Errors
    ///
    /// Returns an error if fewer than `n` bytes remain.
    pub fn read_bytes(&mut self, n: usize) -> Result<&'buf [u8]> {
        let slice = self.peek_bytes(n)?;
        self.pos = self.pos.saturating_add(n);
        Ok(slice)
    }

    /// Peek at the next `n` bytes without advancing.
    ///
    /// # Errors
    ///
    /// Returns an error if fewer than `n` bytes remain.
    pub fn peek_bytes(&self, n: usize) -> Result<&'buf [u8]> {
        self.pos
            .checked_add(n)
            .and_then(|end| self.buf.get(self.pos..end))
            .ok_or_else(|| self.overrun_error(n))
    }

    /// Read bytes up to and including the first occurrence of `terminator`.
    /// Returns the slice *without* the terminator byte.
    ///
    /// If the terminator is not found, the rest of the window is returned.
    ///
    /// # Errors
    ///
    /// Currently infallible; the `Result` is kept for API stability.
    pub fn read_until(&mut self, terminator: u8) -> Result<&'buf [u8]> {
        let rest = self.buf.get(self.pos..).unwrap_or_default();
        if let Some(len) = rest.iter().position(|&b| b == terminator) {
            let s = rest.get(..len).unwrap_or_default();
            self.pos = self.pos.saturating_add(len).saturating_add(1); // consume the terminator
            Ok(s)
        } else {
            // Hit EOF without finding terminator — return what we have.
            self.pos = self.buf.len();
            Ok(rest)
        }
    }

    /// Read exactly `N` bytes as an array, advancing the cursor.
    fn read_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let bytes = self.peek_bytes(N)?;
        let array = <[u8; N]>::try_from(bytes).map_err(|_| self.overrun_error(N))?;
        self.pos = self.pos.saturating_add(N);
        Ok(array)
    }

    // ── Typed integer reads ───────────────────────────────────────────────────

    /// Read a `u8`, advancing the cursor.
    ///
    /// # Errors
    ///
    /// Returns an error if no bytes remain.
    pub fn read_u8(&mut self) -> Result<u8> {
        Ok(u8::from_ne_bytes(self.read_array::<1>()?))
    }

    /// Read an `i8`, advancing the cursor.
    ///
    /// # Errors
    ///
    /// Returns an error if no bytes remain.
    pub fn read_i8(&mut self) -> Result<i8> {
        Ok(i8::from_ne_bytes(self.read_array::<1>()?))
    }

    read_int! {
        read_u16_le, u16, from_le_bytes, 2;
        read_u16_be, u16, from_be_bytes, 2;
        read_u32_le, u32, from_le_bytes, 4;
        read_u32_be, u32, from_be_bytes, 4;
        read_u64_le, u64, from_le_bytes, 8;
        read_u64_be, u64, from_be_bytes, 8;
        read_i16_le, i16, from_le_bytes, 2;
        read_i16_be, i16, from_be_bytes, 2;
        read_i32_le, i32, from_le_bytes, 4;
        read_i32_be, i32, from_be_bytes, 4;
        read_i64_le, i64, from_le_bytes, 8;
        read_i64_be, i64, from_be_bytes, 8;
        read_f32_le, f32, from_le_bytes, 4;
        read_f32_be, f32, from_be_bytes, 4;
        read_f64_le, f64, from_le_bytes, 8;
        read_f64_be, f64, from_be_bytes, 8;
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
