//! Noticing that the schema or the file changed on disk (`--watch`).
//!
//! The files are looked at by their modification time and size, by polling:
//! no thread, no new dependency. [`Watch::changed`] looks at most once an
//! interval, so a loop that calls it every tick does not busy-loop, and it
//! says `true` once for a change, after the files have been the same for one
//! more interval, so that a file an editor is still writing is not read half
//! way and a change that is saved twice reloads once.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// How often the files are looked at.
pub const INTERVAL: Duration = Duration::from_millis(500);

/// What is known of a file: nothing if it is not there.
type Stamp = Option<(SystemTime, u64)>;

/// What is known of a file on disk.
#[must_use]
pub fn stamp_of(path: &Path) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// The files that are watched and what they were like when last looked at.
pub struct Watch<F = fn(&Path) -> Stamp> {
    files: Vec<(PathBuf, Stamp)>,
    interval: Duration,
    /// When the files were last looked at.
    last: Option<Instant>,
    /// A change was seen and is waiting to be confirmed by a second look.
    pending: bool,
    stat: F,
}

impl<F> std::fmt::Debug for Watch<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Watch")
            .field("files", &self.files.len())
            .field("pending", &self.pending)
            .finish_non_exhaustive()
    }
}

impl Watch {
    /// Watch `paths`, looking at them every [`INTERVAL`].
    #[must_use]
    pub fn new(paths: Vec<PathBuf>) -> Watch {
        Watch::with(paths, INTERVAL, stamp_of)
    }
}

impl<F: Fn(&Path) -> Stamp> Watch<F> {
    /// Watch `paths`, looking at them every `interval` with `stat` (a function
    /// so that a test can count the looks).
    pub fn with(paths: Vec<PathBuf>, interval: Duration, stat: F) -> Self {
        let files = paths
            .into_iter()
            .map(|path| {
                let stamp = stat(&path);
                (path, stamp)
            })
            .collect();
        Watch {
            files,
            interval,
            last: None,
            pending: false,
            stat,
        }
    }

    /// Watch these files instead, as they are now (a schema can import other
    /// files after it was changed).
    pub fn set_paths(&mut self, paths: Vec<PathBuf>) {
        self.files = paths
            .into_iter()
            .map(|path| {
                let stamp = (self.stat)(&path);
                (path, stamp)
            })
            .collect();
        self.pending = false;
    }

    /// Whether a change is to be acted on now. It does not look at the files
    /// more than once an interval.
    pub fn changed(&mut self, now: Instant) -> bool {
        if self
            .last
            .is_some_and(|last| now.duration_since(last) < self.interval)
        {
            return false;
        }
        self.last = Some(now);
        let mut differs = false;
        for (path, stamp) in &mut self.files {
            let current = (self.stat)(path);
            if current != *stamp {
                *stamp = current;
                differs = true;
            }
        }
        if differs {
            // wait for the files to be still before reading them
            self.pending = true;
            false
        } else if self.pending {
            self.pending = false;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    use super::*;

    /// Files that are only numbers, and a count of the looks at them.
    struct Fake {
        stamps: RefCell<HashMap<PathBuf, Stamp>>,
        looks: Cell<usize>,
    }

    impl Fake {
        fn new() -> Self {
            Fake {
                stamps: RefCell::new(HashMap::new()),
                looks: Cell::new(0),
            }
        }

        fn touch(&self, name: &str, version: u64) {
            let time = SystemTime::UNIX_EPOCH + Duration::from_secs(version);
            self.stamps
                .borrow_mut()
                .insert(PathBuf::from(name), Some((time, version)));
        }

        fn remove(&self, name: &str) {
            self.stamps.borrow_mut().insert(PathBuf::from(name), None);
        }

        fn stat(&self, path: &Path) -> Stamp {
            self.looks.set(self.looks.get() + 1);
            self.stamps.borrow().get(path).copied().flatten()
        }
    }

    fn watch<'a>(fake: &'a Fake, names: &[&str]) -> Watch<impl Fn(&Path) -> Stamp + 'a> {
        Watch::with(
            names.iter().map(PathBuf::from).collect(),
            Duration::from_millis(500),
            move |p| fake.stat(p),
        )
    }

    #[test]
    fn nothing_changing_never_says_so() {
        let fake = Fake::new();
        fake.touch("a", 1);
        let mut w = watch(&fake, &["a"]);
        let start = Instant::now();
        for i in 0..20 {
            assert!(!w.changed(start + Duration::from_millis(500) * i));
        }
    }

    #[test]
    fn a_change_is_acted_on_once_after_the_files_are_still() {
        let fake = Fake::new();
        fake.touch("a", 1);
        fake.touch("b", 1);
        let mut w = watch(&fake, &["a", "b"]);
        let t0 = Instant::now();
        let step = Duration::from_millis(500);
        assert!(!w.changed(t0));
        fake.touch("b", 2); // an import is edited
        assert!(!w.changed(t0 + step), "seen, but wait until it is still");
        assert!(w.changed(t0 + step * 2), "still: now");
        assert!(!w.changed(t0 + step * 3), "once");
        assert!(!w.changed(t0 + step * 4));
    }

    #[test]
    fn a_file_that_is_still_being_written_waits_and_reloads_once() {
        let fake = Fake::new();
        fake.touch("a", 1);
        let mut w = watch(&fake, &["a"]);
        let t0 = Instant::now();
        let step = Duration::from_millis(500);
        assert!(!w.changed(t0));
        fake.touch("a", 2);
        assert!(!w.changed(t0 + step));
        fake.touch("a", 3); // written again before it was read
        assert!(!w.changed(t0 + step * 2));
        assert!(w.changed(t0 + step * 3));
        assert!(!w.changed(t0 + step * 4));
    }

    #[test]
    fn a_file_that_appears_or_goes_is_a_change() {
        let fake = Fake::new();
        let mut w = watch(&fake, &["a"]); // not there yet
        let t0 = Instant::now();
        let step = Duration::from_millis(500);
        assert!(!w.changed(t0));
        fake.touch("a", 1);
        assert!(!w.changed(t0 + step));
        assert!(w.changed(t0 + step * 2));
        fake.remove("a");
        assert!(!w.changed(t0 + step * 3));
        assert!(w.changed(t0 + step * 4));
    }

    #[test]
    fn it_does_not_look_more_than_once_an_interval() {
        let fake = Fake::new();
        fake.touch("a", 1);
        fake.touch("b", 1);
        let mut w = watch(&fake, &["a", "b"]);
        let after_new = fake.looks.get();
        assert_eq!(after_new, 2);
        let t0 = Instant::now();
        // a loop that calls it every 10 ms for a second and a half: three looks, not 150
        for i in 0..150 {
            w.changed(t0 + Duration::from_millis(10) * i);
        }
        assert_eq!(fake.looks.get() - after_new, 2 * 3);
    }

    #[test]
    fn new_paths_replace_the_old_and_forget_a_pending_change() {
        let fake = Fake::new();
        fake.touch("a", 1);
        fake.touch("c", 1);
        let mut w = watch(&fake, &["a"]);
        let t0 = Instant::now();
        let step = Duration::from_millis(500);
        fake.touch("a", 2);
        assert!(!w.changed(t0));
        w.set_paths(vec![PathBuf::from("a"), PathBuf::from("c")]);
        assert!(
            !w.changed(t0 + step),
            "the change was dealt with by whoever set the paths"
        );
        fake.touch("c", 2);
        assert!(!w.changed(t0 + step * 2));
        assert!(w.changed(t0 + step * 3));
    }

    #[test]
    fn real_files_are_noticed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        std::fs::write(&path, "one").unwrap();
        let mut w = Watch::new(vec![path.clone()]);
        let t0 = Instant::now();
        assert!(!w.changed(t0));
        std::fs::write(&path, "two, longer").unwrap(); // the size differs, whatever the clock
        assert!(!w.changed(t0 + INTERVAL));
        assert!(w.changed(t0 + INTERVAL * 2));
        std::fs::remove_file(&path).unwrap();
        assert!(!w.changed(t0 + INTERVAL * 3));
        assert!(w.changed(t0 + INTERVAL * 4));
    }
}
