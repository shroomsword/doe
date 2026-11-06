//! The checksums of the files of an archive, for the column of the Files view (`m`).
//!
//! They are not made at parse time and not on the thread that draws the screen: a worker thread reads the ranges of the
//! file that the entries say, a megabyte at a time, and sends each checksum back as it is done. The screen shows `…`
//! until it comes. Leaving the column off, or reading the file again, stops the worker between two chunks.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread::JoinHandle;

use doe::filesystem::{Algorithm, Hasher};

/// The algorithm of the column. Only MD5 for now; the follow-ups add to this.
pub const ALGORITHM: Algorithm = Algorithm::Md5;

/// How much of a file is read at a time.
const CHUNK: usize = 1024 * 1024;

/// A range of the file: the first byte, and the byte after the last.
pub type Range = (u64, u64);

/// What is known of the checksum of a range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Digest {
    /// The digits.
    Done(String),
    /// The range could not be read (the file changed or is shorter than the entry says).
    Failed,
}

/// The checksums of the ranges of one file, and the worker that makes the missing ones.
#[derive(Debug, Default)]
pub struct Checksums {
    digests: HashMap<Range, Digest>,
    job: Option<Job>,
}

#[derive(Debug)]
struct Job {
    receiver: Receiver<(Range, Option<String>)>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // the worker stops between two chunks; it is not waited for (a read of a slow disk can take a while)
        drop(self.thread.take());
    }
}

impl Checksums {
    /// What is known about a range.
    #[must_use]
    pub fn get(&self, range: Range) -> Option<&Digest> {
        self.digests.get(&range)
    }

    /// Whether the worker still has ranges to do.
    #[must_use]
    pub fn running(&self) -> bool {
        self.job.is_some()
    }

    /// Forget everything and stop the worker: the file is another one, or was read again.
    pub fn clear(&mut self) {
        self.job = None;
        self.digests.clear();
    }

    /// Stop the worker; what is done stays.
    pub fn stop(&mut self) {
        self.job = None;
    }

    /// Make the checksums of the ranges that are not done (in the order given) from the file at `path`, on a thread.
    pub fn start(&mut self, path: &Path, ranges: &[Range]) {
        self.job = None;
        let mut todo: Vec<Range> = Vec::new();
        for range in ranges {
            if !self.digests.contains_key(range) && !todo.contains(range) {
                todo.push(*range);
            }
        }
        if todo.is_empty() {
            return;
        }
        let (sender, receiver) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let path = path.to_path_buf();
        let thread = std::thread::spawn(move || {
            let mut file = File::open(&path).ok();
            for range in todo {
                if flag.load(Ordering::Relaxed) {
                    return;
                }
                let digest = file.as_mut().and_then(|file| digest_of(file, range, &flag));
                if flag.load(Ordering::Relaxed) || sender.send((range, digest)).is_err() {
                    return;
                }
            }
        });
        self.job = Some(Job {
            receiver,
            stop,
            thread: Some(thread),
        });
    }

    /// Take what the worker has finished. True when something arrived (the screen should be drawn again).
    pub fn poll(&mut self) -> bool {
        let Some(job) = self.job.as_ref() else {
            return false;
        };
        let mut changed = false;
        loop {
            match job.receiver.try_recv() {
                Ok((range, digest)) => {
                    self.digests
                        .insert(range, digest.map_or(Digest::Failed, Digest::Done));
                    changed = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // the worker is done (or stopped)
                    self.job = None;
                    changed = true;
                    break;
                }
            }
        }
        changed
    }
}

/// The checksum of a range of `file`, or None when it cannot be read all (or `stop` was set).
fn digest_of(file: &mut File, range: Range, stop: &AtomicBool) -> Option<String> {
    file.seek(SeekFrom::Start(range.0)).ok()?;
    let mut hasher = Hasher::new(ALGORITHM);
    let mut left = range.1.checked_sub(range.0)?;
    let mut buffer = vec![0_u8; CHUNK];
    while left > 0 {
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        let want = usize::try_from(left).map_or(CHUNK, |n| n.min(CHUNK));
        let got = file.read(buffer.get_mut(..want)?).ok()?;
        if got == 0 {
            // shorter than the entry says
            return None;
        }
        hasher.update(buffer.get(..got)?);
        left -= got as u64;
    }
    Some(hasher.finish())
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;

    fn wait(checks: &mut Checksums) {
        let start = std::time::Instant::now();
        while checks.running() && start.elapsed() < std::time::Duration::from_secs(20) {
            checks.poll();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        checks.poll();
    }

    #[test]
    fn ranges_of_a_file_are_hashed_on_a_thread_and_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.bin");
        let mut bytes = b"xxabcyy".to_vec();
        // more than one chunk
        bytes.extend(std::iter::repeat_n(7_u8, 3 * CHUNK + 5));
        std::fs::write(&path, &bytes).unwrap();
        let mut checks = Checksums::default();
        let whole = (0, bytes.len() as u64);
        checks.start(
            &path,
            &[(2, 5), (0, 0), whole, (2, 5), (0, bytes.len() as u64 + 1)],
        );
        assert!(checks.running());
        wait(&mut checks);
        assert_eq!(
            checks.get((2, 5)),
            Some(&Digest::Done("900150983cd24fb0d6963f7d28e17f72".to_owned()))
        );
        assert_eq!(
            checks.get((0, 0)),
            Some(&Digest::Done("d41d8cd98f00b204e9800998ecf8427e".to_owned()))
        );
        let expected = doe::filesystem::md5_of(&bytes, whole).unwrap();
        assert_eq!(checks.get(whole), Some(&Digest::Done(expected)));
        // past the end of the file: failed, not a wrong digest
        assert_eq!(
            checks.get((0, bytes.len() as u64 + 1)),
            Some(&Digest::Failed)
        );
        assert!(!checks.running());
    }

    #[test]
    fn a_file_that_cannot_be_opened_fails_each_range_and_clear_forgets() {
        let mut checks = Checksums::default();
        checks.start(Path::new("/nonexistent/file"), &[(0, 3)]);
        wait(&mut checks);
        assert_eq!(checks.get((0, 3)), Some(&Digest::Failed));
        checks.clear();
        assert_eq!(checks.get((0, 3)), None);
    }

    #[test]
    fn stopping_ends_the_worker_and_keeps_what_is_done() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.bin");
        std::fs::write(&path, b"abc").unwrap();
        let mut checks = Checksums::default();
        checks.start(&path, &[(0, 3)]);
        wait(&mut checks);
        checks.stop();
        assert!(!checks.running());
        assert!(matches!(checks.get((0, 3)), Some(Digest::Done(_))));
        // nothing is started again for what is done
        checks.start(&path, &[(0, 3)]);
        assert!(!checks.running());
    }
}
