//! The bytes of the file, read as they are shown: a window at a time, so that
//! a file of many gigabytes needs no more memory than a small one.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// How much is read at once. A screen shows a few kilobytes at most.
const WINDOW: u64 = 64 * 1024;

/// The file, and the last window that was read from it.
#[derive(Debug)]
pub struct FileBytes {
    file: File,
    size: u64,
    start: u64,
    cache: Vec<u8>,
    /// Why the last read failed, if it did (the file was moved or truncated).
    error: Option<String>,
}

impl FileBytes {
    /// Open `path` for reading windows of it.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be opened.
    pub fn open(path: &Path) -> io::Result<FileBytes> {
        let file = File::open(path)?;
        let size = file.metadata()?.len();
        Ok(FileBytes {
            file,
            size,
            start: 0,
            cache: Vec::new(),
            error: None,
        })
    }

    /// The size of the file in bytes.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Why the last read failed.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The bytes from `offset` up to `len` of them (fewer at the end of the
    /// file, none past it, none if the file cannot be read now).
    pub fn window(&mut self, offset: u64, len: usize) -> &[u8] {
        let end = offset.saturating_add(len as u64).min(self.size);
        if offset >= end {
            return &[];
        }
        let cached_end = self.start + self.cache.len() as u64;
        if offset < self.start || end > cached_end {
            // the window that holds the request, starting a little before it
            let start = offset.saturating_sub(WINDOW / 4);
            let want = (end - start).max(WINDOW).min(self.size - start);
            match self.read(start, want) {
                Ok(()) => self.error = None,
                Err(e) => {
                    self.error = Some(e.to_string());
                    self.cache.clear();
                    return &[];
                }
            }
        }
        let from = usize::try_from(offset - self.start).unwrap_or(0);
        let to = usize::try_from(end - self.start).unwrap_or(0);
        self.cache.get(from..to).unwrap_or(&[])
    }

    fn read(&mut self, start: u64, len: u64) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(start))?;
        let mut buffer = vec![0_u8; usize::try_from(len).unwrap_or(0)];
        let mut filled = 0;
        while filled < buffer.len() {
            let n = self
                .file
                .read(buffer.get_mut(filled..).unwrap_or_default())?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        buffer.truncate(filled);
        self.start = start;
        self.cache = buffer;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;

    fn file_of(bytes: &[u8]) -> (tempfile::TempDir, FileBytes) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.bin");
        std::fs::write(&path, bytes).unwrap();
        let file = FileBytes::open(&path).unwrap();
        (dir, file)
    }

    #[test]
    fn windows_are_the_bytes_of_the_file_with_the_ends_cut() {
        let data: Vec<u8> = (0..=255).collect();
        let (_dir, mut file) = file_of(&data);
        assert_eq!(file.size(), 256);
        assert_eq!(file.window(0, 4), [0, 1, 2, 3]);
        assert_eq!(file.window(250, 16), [250, 251, 252, 253, 254, 255]);
        assert_eq!(file.window(256, 4), [] as [u8; 0]);
        assert_eq!(file.window(1000, 4), [] as [u8; 0]);
        assert_eq!(file.window(10, 0), [] as [u8; 0]);
        assert_eq!(file.window(u64::MAX, 4), [] as [u8; 0]);
        // going back works as well
        assert_eq!(file.window(5, 2), [5, 6]);
        assert!(file.error().is_none());
    }

    #[test]
    fn a_file_that_is_gone_or_shorter_gives_no_bytes_and_no_panic() {
        let (dir, mut file) = file_of(&[1, 2, 3, 4]);
        std::fs::write(dir.path().join("f.bin"), [9]).unwrap();
        // the open handle sees the new, shorter content; asking for the old size must not panic
        let _ = file.window(0, 4);
        let _ = file.window(2, 2);
    }

    /// A file of several gigabytes that takes no disk (a sparse file): only
    /// the windows that are asked for are read.
    #[cfg(unix)]
    #[test]
    fn a_multi_gigabyte_sparse_file_is_scrolled_without_reading_it_all() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sparse.bin");
        let size: u64 = 6 * 1024 * 1024 * 1024;
        {
            use std::io::Write as _;
            let mut f = File::create(&path).unwrap();
            f.set_len(size).unwrap();
            f.seek(SeekFrom::Start(size - 4)).unwrap();
            f.write_all(&[0xde, 0xad, 0xbe, 0xef]).unwrap();
            f.seek(SeekFrom::Start(0)).unwrap();
            f.write_all(b"DOE").unwrap();
        }
        let mut file = FileBytes::open(&path).unwrap();
        assert_eq!(file.size(), size);
        assert_eq!(file.window(0, 3), b"DOE");
        // the middle, past 2^32, and the end
        assert_eq!(file.window(size / 2, 16), [0_u8; 16]);
        assert_eq!(file.window(size - 4, 16), [0xde, 0xad, 0xbe, 0xef]);
        // a window is held, not the file
        assert!(
            file.cache.len() as u64 <= 2 * WINDOW,
            "{}",
            file.cache.len()
        );
    }
}
