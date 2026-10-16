//! A cursor over an immutable byte buffer.
//!
//! The parser holds one top-level `Cursor` and may create sub-cursors with
//! restricted windows for size-limited sub-structures (not used in the initial
//! implementation but scaffolded here for future use).

//! A cursor over the bytes being parsed.
//!
//! The parser holds one top-level `Cursor`, which reads either from a byte
//! slice already in memory ([`Cursor::new`]) or from a file on demand
//! ([`Cursor::from_file`]), so that parsing a few structures in a very large
//! file does not load the whole file. It may create sub-cursors with
//! restricted windows for size-limited sub-structures.

use std::borrow::Cow;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::error::{DoeError, Result};

/// The longest run of bytes [`Cursor::read_until`] returns.
///
/// A terminated string with no terminator would otherwise grow until the end
/// of the input, however large that is.
pub const MAX_TERMINATED_LEN: usize = 16 * 1024 * 1024;

/// How much of a file is read at a time when a small read misses the cache.
const FILE_CACHE_LEN: usize = 64 * 1024;

/// How much `read_until` examines per step on a file.
const SCAN_CHUNK_LEN: usize = 4096;

// ─────────────────────────────────────────────────────────────────────────────
// Cursor
// ─────────────────────────────────────────────────────────────────────────────

/// A position-tracking window over the bytes being parsed.
#[derive(Debug, Clone)]
pub struct Cursor<'buf> {
    source: Source<'buf>,
    /// Absolute offset of the window's first byte within the original input.
    /// Always zero for the root cursor; non-zero for sub-cursors.
    base: usize,
    /// Size of the window in bytes.
    len: usize,
    /// Current read position relative to the start of the window.
    pos: usize,
    /// The bits of the last byte read for a bit field that are still unread.
    bit_buf: u8,
    /// How many bits of `bit_buf` are unread; zero between byte-aligned reads.
    bits_left: u8,
    /// The order the unread bits are consumed in.
    bit_order: BitOrder,
}

/// The order in which bit fields take bits from a byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BitOrder {
    /// The first field takes the most significant bits (`b4`, `b4` over `0xAB`
    /// read `0xA`, then `0xB`).
    #[default]
    MostSignificantFirst,
    /// The first field takes the least significant bits (`b4le`, `b4le` over
    /// `0xAB` read `0xB`, then `0xA`).
    LeastSignificantFirst,
}

/// A saved position, including the unread bits of a partly read byte; see
/// [`Cursor::mark`].
#[derive(Debug, Clone, Copy)]
pub struct Mark {
    pos: usize,
    bit_buf: u8,
    bits_left: u8,
    bit_order: BitOrder,
}

#[derive(Debug, Clone)]
enum Source<'buf> {
    /// The window's bytes, in memory.
    Memory(&'buf [u8]),
    /// A file shared by the root cursor and its sub-cursors; `base` locates
    /// the window in it.
    File(Arc<Mutex<FileSource>>),
}

/// A file read on demand through a one-block cache.
#[derive(Debug)]
struct FileSource {
    file: File,
    path: PathBuf,
    cache: Vec<u8>,
    /// File offset of `cache[0]`.
    cache_start: usize,
}

impl FileSource {
    fn cached(&self, start: usize, end: usize) -> Option<&[u8]> {
        let from = start.checked_sub(self.cache_start)?;
        let to = end.checked_sub(self.cache_start)?;
        self.cache.get(from..to)
    }

    /// Fill `out` with the bytes at `offset`.
    fn fill(&mut self, offset: usize, out: &mut [u8]) -> io::Result<()> {
        let eof = || io::Error::from(io::ErrorKind::UnexpectedEof);
        let end = offset.checked_add(out.len()).ok_or_else(eof)?;
        if let Some(hit) = self.cached(offset, end) {
            out.copy_from_slice(hit);
            return Ok(());
        }
        let start = u64::try_from(offset).map_err(|_| eof())?;
        self.file.seek(SeekFrom::Start(start))?;
        if out.len() >= FILE_CACHE_LEN {
            // Too big to be worth caching.
            return self.file.read_exact(out);
        }
        self.cache.clear();
        self.cache_start = offset;
        let limit = u64::try_from(FILE_CACHE_LEN).map_err(|_| eof())?;
        (&mut self.file).take(limit).read_to_end(&mut self.cache)?;
        let hit = self.cached(offset, end).ok_or_else(eof)?;
        out.copy_from_slice(hit);
        Ok(())
    }
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
            source: Source::Memory(buf),
            base: 0,
            len: buf.len(),
            pos: 0,
            bit_buf: 0,
            bits_left: 0,
            bit_order: BitOrder::default(),
        }
    }

    /// Create a cursor over an open file, reading from it as needed instead of
    /// loading it. `path` is only used in error messages.
    ///
    /// The window is the file's size when this is called; if the file shrinks
    /// afterwards, reads past its new end fail with [`DoeError::Io`].
    ///
    /// # Errors
    ///
    /// Returns an error if the file's size cannot be read or does not fit in
    /// `usize`.
    pub fn from_file(file: File, path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let size = file.metadata().map_err(|source| DoeError::Io {
            path: path.clone(),
            source,
        })?;
        let len = usize::try_from(size.len()).map_err(|_| {
            DoeError::Schema(format!(
                "{} is too large to parse on this platform",
                path.display()
            ))
        })?;
        Ok(Cursor {
            source: Source::File(Arc::new(Mutex::new(FileSource {
                file,
                path,
                cache: Vec::new(),
                cache_start: 0,
            }))),
            base: 0,
            len,
            pos: 0,
            bit_buf: 0,
            bits_left: 0,
            bit_order: BitOrder::default(),
        })
    }

    /// Create a sub-cursor covering `buf[start..start+len]`.
    ///
    /// # Errors
    ///
    /// Returns an error if the window extends past the end of this cursor.
    pub fn sub(&self, start: usize, len: usize) -> Result<Cursor<'buf>> {
        let end = start
            .checked_add(len)
            .filter(|&end| end <= self.len)
            .ok_or_else(|| self.overrun_error(len))?;
        let source = match &self.source {
            Source::Memory(buf) => {
                let buf: &'buf [u8] = buf;
                Source::Memory(buf.get(start..end).ok_or_else(|| self.overrun_error(len))?)
            }
            Source::File(shared) => Source::File(Arc::clone(shared)),
        };
        Ok(Cursor {
            source,
            base: self.base.saturating_add(start),
            len,
            pos: 0,
            bit_buf: 0,
            bits_left: 0,
            bit_order: BitOrder::default(),
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

    /// Size of this cursor's window in bytes.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if this cursor's window is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Number of bytes remaining in this cursor's window.
    pub fn remaining(&self) -> usize {
        self.len.saturating_sub(self.pos)
    }

    /// Returns `true` if the cursor is at the end of its window.
    pub fn is_eof(&self) -> bool {
        self.bits_left == 0 && self.pos >= self.len
    }

    /// Seek to an absolute position within this cursor's window. The unread
    /// bits of a partly read byte are dropped.
    ///
    /// # Errors
    ///
    /// Returns an error if `pos` is past the end of the window.
    pub fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.len {
            return Err(DoeError::Schema(format!(
                "seek to {} is past end of window (size {})",
                pos, self.len
            )));
        }
        self.pos = pos;
        self.align();
        Ok(())
    }

    /// Save the position, including the unread bits of a partly read byte, to
    /// return to with [`Cursor::reset`].
    pub fn mark(&self) -> Mark {
        Mark {
            pos: self.pos,
            bit_buf: self.bit_buf,
            bits_left: self.bits_left,
            bit_order: self.bit_order,
        }
    }

    /// Return to a position saved with [`Cursor::mark`].
    pub fn reset(&mut self, mark: Mark) {
        self.pos = mark.pos;
        self.bit_buf = mark.bit_buf;
        self.bits_left = mark.bits_left;
        self.bit_order = mark.bit_order;
    }

    /// The position in bits from the start of the root buffer, which moves
    /// when a bit field is read even if no new byte is.
    pub fn bit_position(&self) -> u64 {
        u64::try_from(self.absolute_pos())
            .unwrap_or(u64::MAX)
            .saturating_mul(8)
            .saturating_sub(u64::from(self.bits_left))
    }

    /// Drop the unread bits of a partly read byte, so that the next read
    /// starts on a byte boundary.
    fn align(&mut self) {
        self.bit_buf = 0;
        self.bits_left = 0;
    }

    // ── Bit reads ─────────────────────────────────────────────────────────────

    /// Read a `bits`-bit unsigned integer, taking the bits in the given order.
    ///
    /// Consecutive bit fields share bytes; a field may span bytes. Every other
    /// read, and a bit field of the other order, starts on a byte boundary,
    /// dropping the unread bits.
    ///
    /// # Errors
    ///
    /// Returns an error if `bits` is not from 1 to 64, or if the bytes it
    /// needs are not there.
    pub fn read_bits(&mut self, bits: u8, order: BitOrder) -> Result<u64> {
        if !(1..=64).contains(&bits) {
            return Err(DoeError::Schema(format!(
                "a bit field has 1 to 64 bits, not {bits}"
            )));
        }
        if self.bits_left > 0 && self.bit_order != order {
            self.align();
        }
        let wanted = u32::from(bits);
        let mut total = u32::from(self.bits_left);
        let fresh_bytes = usize::try_from(wanted.saturating_sub(total).div_ceil(8)).unwrap_or(0);
        let mut fresh = [0u8; 9];
        let fresh = fresh.get_mut(..fresh_bytes).unwrap_or_default();
        let end = self.end_of(fresh_bytes)?;
        self.fill(self.pos, fresh)?;

        let mut acc = u128::from(self.bit_buf);
        let value;
        match order {
            BitOrder::MostSignificantFirst => {
                for &byte in &*fresh {
                    acc = (acc << 8) | u128::from(byte);
                    total += 8;
                }
                let left = total - wanted;
                value = acc >> left;
                acc &= (1u128 << left) - 1;
            }
            BitOrder::LeastSignificantFirst => {
                for &byte in &*fresh {
                    acc |= u128::from(byte) << total;
                    total += 8;
                }
                value = acc;
                acc >>= wanted;
            }
        }
        self.pos = end;
        self.bits_left = u8::try_from(total - wanted).unwrap_or(0);
        self.bit_buf = u8::try_from(acc & 0xff).unwrap_or(0);
        self.bit_order = order;
        Ok(u64::try_from(value & ((1u128 << wanted) - 1)).unwrap_or(0))
    }

    // ── Raw reads ─────────────────────────────────────────────────────────────

    /// Read exactly `n` bytes, advancing the cursor.
    ///
    /// The bytes are borrowed from the buffer for a cursor over memory and
    /// copied for a cursor over a file, so `n` should be checked against what
    /// the caller is prepared to hold before it is called.
    ///
    /// # Errors
    ///
    /// Returns an error if fewer than `n` bytes remain, or if reading the file
    /// fails.
    pub fn read_bytes(&mut self, n: usize) -> Result<Cow<'buf, [u8]>> {
        self.align();
        let bytes = self.peek_bytes(n)?;
        self.pos = self.pos.saturating_add(n);
        Ok(bytes)
    }

    /// Peek at the next `n` bytes without advancing. See [`Cursor::read_bytes`]
    /// for the cost of a large `n`.
    ///
    /// # Errors
    ///
    /// Returns an error if fewer than `n` bytes remain, or if reading the file
    /// fails.
    pub fn peek_bytes(&self, n: usize) -> Result<Cow<'buf, [u8]>> {
        let end = self.end_of(n)?;
        match &self.source {
            Source::Memory(buf) => {
                let buf: &'buf [u8] = buf;
                buf.get(self.pos..end)
                    .map(Cow::Borrowed)
                    .ok_or_else(|| self.overrun_error(n))
            }
            Source::File(_) => {
                let mut bytes = vec![0; n];
                self.fill(self.pos, &mut bytes)?;
                Ok(Cow::Owned(bytes))
            }
        }
    }

    /// Advance past the next `n` bytes without reading them.
    ///
    /// # Errors
    ///
    /// Returns an error if fewer than `n` bytes remain.
    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.align();
        self.pos = self.end_of(n)?;
        Ok(())
    }

    /// Read bytes up to and including the first occurrence of `terminator`.
    /// Returns the bytes *without* the terminator byte.
    ///
    /// If the terminator is not found, the rest of the window is returned.
    ///
    /// # Errors
    ///
    /// Returns an error if the result would be longer than
    /// [`MAX_TERMINATED_LEN`] bytes, or if reading the file fails.
    pub fn read_until(&mut self, terminator: u8) -> Result<Cow<'buf, [u8]>> {
        self.read_terminated(terminator, false, true, false)
    }

    /// Read bytes up to the first occurrence of `terminator`.
    ///
    /// With `include` the terminator is part of the result; with `consume` the
    /// position ends after it, and otherwise at it. If there is no terminator
    /// the rest of the window is returned, or with `eos_error` that is an
    /// error.
    ///
    /// # Errors
    ///
    /// Returns an error if there is no terminator and `eos_error` is set, if the
    /// result would be longer than [`MAX_TERMINATED_LEN`] bytes, or if reading
    /// the file fails.
    pub fn read_terminated(
        &mut self,
        terminator: u8,
        include: bool,
        consume: bool,
        eos_error: bool,
    ) -> Result<Cow<'buf, [u8]>> {
        self.align();
        let missing = |offset: usize| {
            DoeError::Schema(format!(
                "end of the input reached at offset {offset} without the terminator {terminator:#04x}"
            ))
        };
        match &self.source {
            Source::Memory(buf) => {
                let buf: &'buf [u8] = buf;
                let rest = buf.get(self.pos..).unwrap_or_default();
                let found = rest.iter().position(|&b| b == terminator);
                if found.is_none() && eos_error {
                    return Err(missing(self.absolute_pos()));
                }
                let at = found.unwrap_or(rest.len());
                let len = at.saturating_add(usize::from(found.is_some() && include));
                self.check_terminated_len(len)?;
                self.pos = self
                    .pos
                    .saturating_add(at)
                    .saturating_add(usize::from(found.is_some() && consume));
                Ok(Cow::Borrowed(rest.get(..len).unwrap_or_default()))
            }
            Source::File(_) => {
                let mut out = Vec::new();
                let mut chunk = [0u8; SCAN_CHUNK_LEN];
                while !self.is_eof() {
                    let n = self.remaining().min(SCAN_CHUNK_LEN);
                    let window = chunk.get_mut(..n).unwrap_or_default();
                    self.fill(self.pos, window)?;
                    if let Some(at) = window.iter().position(|&b| b == terminator) {
                        let kept = at.saturating_add(usize::from(include));
                        out.extend_from_slice(window.get(..kept).unwrap_or_default());
                        self.check_terminated_len(out.len())?;
                        self.pos = self
                            .pos
                            .saturating_add(at)
                            .saturating_add(usize::from(consume));
                        return Ok(Cow::Owned(out));
                    }
                    out.extend_from_slice(window);
                    self.check_terminated_len(out.len())?;
                    self.pos = self.pos.saturating_add(n);
                }
                if eos_error {
                    return Err(missing(self.absolute_pos()));
                }
                Ok(Cow::Owned(out))
            }
        }
    }

    /// Read exactly `N` bytes as an array, advancing the cursor.
    fn read_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.align();
        let end = self.end_of(N)?;
        let mut array = [0; N];
        self.fill(self.pos, &mut array)?;
        self.pos = end;
        Ok(array)
    }

    /// The window offset `n` bytes past the position, if that is inside the
    /// window.
    fn end_of(&self, n: usize) -> Result<usize> {
        self.pos
            .checked_add(n)
            .filter(|&end| end <= self.len)
            .ok_or_else(|| self.overrun_error(n))
    }

    /// Copy `out.len()` bytes at window offset `offset` into `out`. The caller
    /// has checked that they are inside the window.
    fn fill(&self, offset: usize, out: &mut [u8]) -> Result<()> {
        match &self.source {
            Source::Memory(buf) => {
                let bytes = offset
                    .checked_add(out.len())
                    .and_then(|end| buf.get(offset..end))
                    .ok_or_else(|| self.overrun_error(out.len()))?;
                out.copy_from_slice(bytes);
                Ok(())
            }
            Source::File(shared) => {
                let mut file = lock(shared);
                let absolute = self.base.saturating_add(offset);
                file.fill(absolute, out).map_err(|source| DoeError::Io {
                    path: file.path.clone(),
                    source,
                })
            }
        }
    }

    fn check_terminated_len(&self, len: usize) -> Result<()> {
        if len > MAX_TERMINATED_LEN {
            return Err(DoeError::Schema(format!(
                "no terminator within {MAX_TERMINATED_LEN} bytes of offset {}",
                self.absolute_pos()
            )));
        }
        Ok(())
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

fn lock(shared: &Mutex<FileSource>) -> MutexGuard<'_, FileSource> {
    // A panic while the lock was held cannot leave the cache half-updated in a
    // way that matters: every read starts by checking what the cache covers.
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}
