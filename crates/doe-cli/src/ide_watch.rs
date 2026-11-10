//! The type libraries of `doe --ide`, kept current.
//!
//! The include paths and the configuration file are watched with the file
//! system's own change notifications (`inotify` on Linux, `FSEvents` on macOS,
//! `ReadDirectoryChangesW` on Windows: the `notify` crate). A save is several
//! events, and a `git checkout` is hundreds, so they are coalesced over a short
//! quiet period; then the configuration is read again, the include paths are
//! taken from it, the folders that are watched are brought into line with them,
//! and the *generation* is raised. A page that holds the stream of generations
//! (`GET /api/types/events`) is told, and asks for the list again. Nothing
//! polls: not the server, and not the page.
//!
//! When the notifications cannot be had (a watch limit, a folder that raises no
//! events) that is said in the stream, and the page's *Reload* button is the
//! way: it reads the libraries again, whatever the watcher says.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use doe::config::discover_types;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// How long the file system must be quiet before the libraries are read again.
/// A save (create, write, rename) is over in a few milliseconds; this is long
/// enough to take a burst of them as one and short enough to feel immediate.
const QUIET: Duration = Duration::from_millis(200);
/// The longest a burst is waited out: a library that is being written for ever
/// (a build in an include path) still gets read now and then.
const LONGEST_BURST: Duration = Duration::from_secs(2);
/// The most streams of events held open at once (each is a thread).
const MAX_STREAMS: usize = 8;

/// What the page is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Snapshot {
    /// Raised each time the libraries were read again.
    pub(crate) generation: u64,
    /// Why the libraries could not be read, when they could not (the last good
    /// ones are kept).
    pub(crate) error: Option<String>,
    /// Why they are not being watched, when they are not.
    pub(crate) unwatched: Option<String>,
}

#[derive(Debug)]
struct State {
    snapshot: Snapshot,
    paths: Vec<PathBuf>,
}

/// The include paths of the server, as they are now, and the number that says
/// they changed.
#[derive(Debug)]
pub(crate) struct Libraries {
    state: Mutex<State>,
    changed: Condvar,
    streams: AtomicUsize,
}

/// A place among the streams of events; it is given back when dropped.
#[derive(Debug)]
pub(crate) struct StreamSlot(Arc<Libraries>);

impl Drop for StreamSlot {
    fn drop(&mut self) {
        self.0.streams.fetch_sub(1, Ordering::SeqCst);
    }
}

/// `paths` made absolute, so that a change of the working directory does not
/// change what they mean.
pub(crate) fn absolute(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .map(|p| std::path::absolute(p).unwrap_or_else(|_| p.clone()))
        .collect()
}

impl Libraries {
    pub(crate) fn new(paths: Vec<PathBuf>) -> Arc<Self> {
        Arc::new(Libraries {
            state: Mutex::new(State {
                snapshot: Snapshot {
                    generation: 0,
                    error: None,
                    unwatched: None,
                },
                paths,
            }),
            changed: Condvar::new(),
            streams: AtomicUsize::new(0),
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The include paths now.
    pub(crate) fn paths(&self) -> Vec<PathBuf> {
        self.lock().paths.clone()
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        self.lock().snapshot.clone()
    }

    /// Wait for a generation other than `seen`; `None` when `timeout` passes.
    pub(crate) fn wait_change(&self, seen: u64, timeout: Duration) -> Option<Snapshot> {
        let guard = self.lock();
        let (guard, _) = self
            .changed
            .wait_timeout_while(guard, timeout, |s| s.snapshot.generation == seen)
            .unwrap_or_else(PoisonError::into_inner);
        (guard.snapshot.generation != seen).then(|| guard.snapshot.clone())
    }

    /// Publish what was found: new paths (when they could be had), the error and
    /// the reason for not watching. Always a new generation.
    fn publish(
        &self,
        paths: Option<Vec<PathBuf>>,
        error: Option<String>,
        unwatched: Option<String>,
    ) {
        let mut state = self.lock();
        if let Some(paths) = paths {
            state.paths = paths;
        }
        state.snapshot = Snapshot {
            generation: state.snapshot.generation + 1,
            error,
            unwatched,
        };
        drop(state);
        self.changed.notify_all();
    }

    /// Say why the libraries are not watched, without a new generation.
    pub(crate) fn set_unwatched(&self, unwatched: Option<String>) {
        self.lock().snapshot.unwatched = unwatched;
    }

    /// Read the libraries again now (the *Reload* button): a new generation
    /// whether or not the watcher saw anything.
    pub(crate) fn reload(&self, source: &dyn Fn() -> anyhow::Result<Vec<PathBuf>>) {
        let unwatched = self.lock().snapshot.unwatched.clone();
        let (paths, error) = read(source, &self.paths());
        self.publish(Some(paths), error, unwatched);
    }

    /// A place for one more stream, when there is one.
    pub(crate) fn stream_slot(self: &Arc<Self>) -> Option<StreamSlot> {
        if self.streams.fetch_add(1, Ordering::SeqCst) >= MAX_STREAMS {
            self.streams.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(StreamSlot(Arc::clone(self)))
    }
}

/// Read the include paths from the configuration again, and check that the
/// types can be listed from them. The paths that were good stay when they
/// cannot be read.
fn read(
    source: &dyn Fn() -> anyhow::Result<Vec<PathBuf>>,
    before: &[PathBuf],
) -> (Vec<PathBuf>, Option<String>) {
    let (paths, mut error) = match source() {
        Ok(paths) => (absolute(&paths), None),
        Err(e) => (
            before.to_vec(),
            Some(format!("the configuration could not be read: {e:#}")),
        ),
    };
    if let Err(duplicates) = discover_types(&paths) {
        let first = duplicates
            .first()
            .and_then(|d| d.to_string().lines().next().map(str::to_owned))
            .unwrap_or_default();
        error.get_or_insert(format!(
            "the types cannot be listed: {first} ({} in all)",
            duplicates.len()
        ));
    }
    (paths, error)
}

// ── which events matter, and what is watched ─────────────────────────────────

/// Whether a change to `path` can change the libraries: a schema file or a folder
/// below an include path, the configuration file, or a folder above either (that
/// was created, so that a path that did not exist is there now). The temporary
/// files of editors and the folders of git are not.
pub(crate) fn is_relevant(path: &Path, roots: &[PathBuf], config: Option<&Path>) -> bool {
    if config.is_some_and(|c| c == path) {
        return true;
    }
    let above = |target: &Path| target.starts_with(path);
    if roots.iter().any(|r| above(r)) || config.is_some_and(above) {
        return true;
    }
    if !roots.iter().any(|r| path.starts_with(r)) {
        return false;
    }
    if path.components().any(|c| c.as_os_str() == ".git") {
        return false;
    }
    let name = path.file_name().and_then(OsStr::to_str).unwrap_or("");
    if name.starts_with(".#") || name.ends_with('~') || name == "4913" {
        return false;
    }
    match path.extension().and_then(OsStr::to_str) {
        Some(ext) => ["yaml", "yml", "ksy"]
            .iter()
            .any(|known| ext.eq_ignore_ascii_case(known)),
        // a folder (created, removed or moved)
        None => !name.starts_with('.'),
    }
}

fn is_relevant_event(
    event: &notify::Result<Event>,
    roots: &[PathBuf],
    config: Option<&Path>,
) -> bool {
    match event {
        // the watcher lost events or failed: look again
        Err(_) => true,
        Ok(event) => {
            if matches!(
                event.kind,
                EventKind::Access(_) | EventKind::Modify(notify::event::ModifyKind::Metadata(_))
            ) {
                return false;
            }
            event.need_rescan() || event.paths.iter().any(|p| is_relevant(p, roots, config))
        }
    }
}

/// What to watch: each include path, below it too; the folder of the
/// configuration file (an editor saves by renaming a new file over the old one,
/// which a watch on the file itself would not survive); and, for a path that is
/// not there, the nearest folder that is, to see it come.
pub(crate) fn wanted(roots: &[PathBuf], config: Option<&Path>) -> HashMap<PathBuf, RecursiveMode> {
    let mut wanted: HashMap<PathBuf, RecursiveMode> = HashMap::new();
    let mut want = |dir: PathBuf, mode: RecursiveMode| {
        let entry = wanted.entry(dir).or_insert(mode);
        if mode == RecursiveMode::Recursive {
            *entry = RecursiveMode::Recursive;
        }
    };
    for root in roots {
        if root.is_dir() {
            want(root.clone(), RecursiveMode::Recursive);
        } else if let Some(ancestor) = root.ancestors().skip(1).find(|a| a.is_dir()) {
            want(ancestor.to_path_buf(), RecursiveMode::NonRecursive);
        }
    }
    if let Some(config) = config
        && let Some(ancestor) = config.ancestors().skip(1).find(|a| a.is_dir())
    {
        want(ancestor.to_path_buf(), RecursiveMode::NonRecursive);
    }
    wanted
}

/// The watcher, and what it watches.
struct Watching {
    watcher: RecommendedWatcher,
    watched: HashMap<PathBuf, RecursiveMode>,
}

impl Watching {
    /// Bring the watches into line with `wanted`; the reasons that some could
    /// not be had, as one text.
    fn sync(&mut self, wanted: &HashMap<PathBuf, RecursiveMode>) -> Option<String> {
        let stale: Vec<PathBuf> = self
            .watched
            .iter()
            .filter(|(dir, mode)| wanted.get(*dir) != Some(*mode))
            .map(|(dir, _)| dir.clone())
            .collect();
        for dir in stale {
            // it may be gone already
            let _ = self.watcher.unwatch(&dir);
            self.watched.remove(&dir);
        }
        let mut failures = Vec::new();
        for (dir, mode) in wanted {
            if self.watched.contains_key(dir) {
                continue;
            }
            match self.watcher.watch(dir, *mode) {
                Ok(()) => {
                    self.watched.insert(dir.clone(), *mode);
                }
                Err(e) => failures.push(format!("{}: {e}", dir.display())),
            }
        }
        failures.sort();
        (!failures.is_empty()).then(|| format!("could not watch {}", failures.join("; ")))
    }
}

/// Watch the libraries of `libraries` and keep it current. `source` gives the
/// include paths (it reads the configuration file again); `config` is that
/// file. The reason is returned when nothing could be watched at all; the
/// libraries then work as they did, and are read again by `reload`.
pub(crate) fn start(
    libraries: &Arc<Libraries>,
    config: Option<PathBuf>,
    source: Box<dyn Fn() -> anyhow::Result<Vec<PathBuf>> + Send>,
) -> Result<(), String> {
    let (sender, receiver) = mpsc::channel();
    let watcher = notify::recommended_watcher(move |event| {
        // the receiver is gone when the server is
        let _ = sender.send(event);
    })
    .map_err(|e| format!("the type libraries are not watched: {e}"))?;
    let mut watching = Watching {
        watcher,
        watched: HashMap::new(),
    };
    let unwatched = watching.sync(&wanted(&libraries.paths(), config.as_deref()));
    libraries.set_unwatched(unwatched);
    let libraries = Arc::clone(libraries);
    std::thread::Builder::new()
        .name("doe-watch".to_owned())
        .spawn(move || run(&libraries, &receiver, watching, config.as_deref(), &*source))
        .map_err(|e| format!("the type libraries are not watched: {e}"))?;
    Ok(())
}

fn run(
    libraries: &Libraries,
    receiver: &Receiver<notify::Result<Event>>,
    mut watching: Watching,
    config: Option<&Path>,
    source: &dyn Fn() -> anyhow::Result<Vec<PathBuf>>,
) {
    while let Ok(first) = receiver.recv() {
        let mut changed = is_relevant_event(&first, &libraries.paths(), config);
        let started = Instant::now();
        loop {
            match receiver.recv_timeout(QUIET) {
                Ok(event) => {
                    changed |= is_relevant_event(&event, &libraries.paths(), config);
                    if started.elapsed() >= LONGEST_BURST {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        if !changed {
            continue;
        }
        let (paths, error) = read(source, &libraries.paths());
        let unwatched = watching.sync(&wanted(&paths, config));
        if let Some(why) = &unwatched {
            log::warn!("{why}");
        }
        log::debug!(
            "the type libraries were read again: {} path(s)",
            paths.len()
        );
        libraries.publish(Some(paths), error, unwatched);
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;

    fn roots() -> Vec<PathBuf> {
        vec![PathBuf::from("/lib/types")]
    }

    #[test]
    fn schemas_and_folders_below_an_include_path_matter() {
        let r = roots();
        for path in [
            "/lib/types/png.yaml",
            "/lib/types/sub/gif.YML",
            "/lib/types/sub/x.ksy",
            "/lib/types/newdir",
            "/lib/types",
            "/lib",
        ] {
            assert!(is_relevant(Path::new(path), &r, None), "{path}");
        }
    }

    #[test]
    fn the_leavings_of_editors_and_git_do_not() {
        let r = roots();
        for path in [
            "/lib/types/.png.yaml.swp",
            "/lib/types/png.yaml~",
            "/lib/types/.#png.yaml",
            "/lib/types/4913",
            "/lib/types/notes.txt",
            "/lib/types/.git/index",
            "/lib/types/.git/refs/heads/x.yaml",
            "/lib/types/.hidden",
            "/elsewhere/png.yaml",
        ] {
            assert!(!is_relevant(Path::new(path), &r, None), "{path}");
        }
    }

    #[test]
    fn the_configuration_file_matters_and_its_neighbours_do_not() {
        let config = Path::new("/home/me/.config/doe/config.toml");
        let r = roots();
        assert!(is_relevant(config, &r, Some(config)));
        assert!(is_relevant(Path::new("/home/me/.config"), &r, Some(config)));
        assert!(!is_relevant(
            Path::new("/home/me/.config/doe/other.toml"),
            &r,
            Some(config)
        ));
    }

    #[test]
    fn what_is_watched_follows_the_paths_that_exist() {
        let dir = tempfile::tempdir().unwrap();
        let here = dir.path().join("types");
        std::fs::create_dir(&here).unwrap();
        let later = dir.path().join("later").join("types");
        let config = dir.path().join("etc").join("config.toml");
        let w = wanted(&[here.clone(), later], Some(&config));
        assert_eq!(w.get(&here), Some(&RecursiveMode::Recursive));
        // the nearest folder that is there stands for both of the others
        assert_eq!(w.get(dir.path()), Some(&RecursiveMode::NonRecursive));
        assert_eq!(w.len(), 2);
    }

    fn plain(paths: Vec<PathBuf>) -> Box<dyn Fn() -> anyhow::Result<Vec<PathBuf>> + Send> {
        Box::new(move || Ok(paths.clone()))
    }

    fn write_type(dir: &Path, file: &str, id: &str, doc: &str) {
        std::fs::write(
            dir.join(file),
            format!("meta:\n  id: {id}\n  title: {doc}\nseq:\n  - id: a\n    type: u1\n"),
        )
        .unwrap();
    }

    const WAIT: Duration = Duration::from_secs(20);

    #[test]
    fn a_type_added_to_an_include_path_is_noticed() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let libraries = Libraries::new(vec![root.clone()]);
        start(&libraries, None, plain(vec![root.clone()])).unwrap();
        assert_eq!(libraries.snapshot().unwatched, None);
        write_type(&root, "one.yaml", "one", "first");
        let snapshot = libraries.wait_change(0, WAIT).expect("a change was seen");
        assert_eq!(snapshot.error, None);
        assert!(snapshot.generation >= 1);
        assert_eq!(discover_types(&libraries.paths()).unwrap().len(), 1);
    }

    #[test]
    fn a_save_that_renames_a_file_over_the_type_is_one_change() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        write_type(&root, "one.yaml", "one", "first");
        let libraries = Libraries::new(vec![root.clone()]);
        start(&libraries, None, plain(vec![root.clone()])).unwrap();
        // the way an editor saves: a temporary file, then a rename over the old
        let temporary = root.join(".one.yaml.swp");
        std::fs::write(&temporary, "x").unwrap();
        write_type(&root, "one.yaml.new", "one", "second");
        std::fs::rename(root.join("one.yaml.new"), root.join("one.yaml")).unwrap();
        let first = libraries.wait_change(0, WAIT).expect("a change was seen");
        // and the burst was taken as one
        assert_eq!(libraries.wait_change(first.generation, QUIET * 4), None);
    }

    #[test]
    fn the_leavings_of_editors_are_not_a_change() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let libraries = Libraries::new(vec![root.clone()]);
        start(&libraries, None, plain(vec![root.clone()])).unwrap();
        std::fs::write(root.join(".one.yaml.swp"), "x").unwrap();
        std::fs::write(root.join("4913"), "x").unwrap();
        assert_eq!(libraries.wait_change(0, Duration::from_secs(1)), None);
    }

    #[test]
    fn a_duplicate_id_is_said_and_the_paths_stay() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let libraries = Libraries::new(vec![root.clone()]);
        start(&libraries, None, plain(vec![root.clone()])).unwrap();
        write_type(&root, "a.yaml", "same", "a");
        write_type(&root, "b.yaml", "same", "b");
        let mut snapshot = libraries.wait_change(0, WAIT).expect("a change was seen");
        while snapshot.error.is_none() {
            snapshot = libraries
                .wait_change(snapshot.generation, WAIT)
                .expect("the second file was seen");
        }
        assert!(snapshot.error.unwrap().contains("duplicate type id 'same'"));
        assert_eq!(libraries.paths(), vec![root]);
    }

    #[test]
    fn a_path_added_to_the_configuration_is_watched_and_one_that_cannot_be_read_keeps_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let first = base.join("first");
        let second = base.join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        let config = base.join("config.toml");
        std::fs::write(&config, "").unwrap();
        // the "configuration" is the text of the file: one path per line
        let path = config.clone();
        let (a, b) = (first.clone(), second.clone());
        let source = move || -> anyhow::Result<Vec<PathBuf>> {
            let text = std::fs::read_to_string(&path)?;
            anyhow::ensure!(!text.contains("broken"), "line 1: broken");
            let mut paths = vec![a.clone()];
            if text.contains("second") {
                paths.push(b.clone());
            }
            Ok(paths)
        };
        let libraries = Libraries::new(vec![first.clone()]);
        start(&libraries, Some(config.clone()), Box::new(source)).unwrap();

        std::fs::write(&config, "second\n").unwrap();
        let seen = libraries
            .wait_change(0, WAIT)
            .expect("the configuration was seen");
        assert_eq!(libraries.paths(), vec![first.clone(), second.clone()]);
        // the new folder is watched now
        write_type(&second, "two.yaml", "two", "second");
        let seen = libraries
            .wait_change(seen.generation, WAIT)
            .expect("the new path was watched");
        assert_eq!(seen.error, None);

        std::fs::write(&config, "broken\n").unwrap();
        let mut seen = libraries.wait_change(seen.generation, WAIT).expect("seen");
        while seen.error.is_none() {
            seen = libraries.wait_change(seen.generation, WAIT).expect("seen");
        }
        assert!(seen.error.unwrap().contains("could not be read"));
        assert_eq!(libraries.paths(), vec![first, second]);
    }

    #[test]
    fn a_path_that_is_created_later_is_watched() {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let later = base.join("later");
        let libraries = Libraries::new(vec![later.clone()]);
        start(&libraries, None, plain(vec![later.clone()])).unwrap();
        std::fs::create_dir(&later).unwrap();
        let seen = libraries.wait_change(0, WAIT).expect("the folder was seen");
        write_type(&later, "x.yaml", "x", "in the new folder");
        libraries
            .wait_change(seen.generation, WAIT)
            .expect("the new folder was watched");
    }

    #[test]
    fn reloading_by_hand_is_a_new_generation() {
        let libraries = Libraries::new(Vec::new());
        libraries.reload(&|| Ok(Vec::new()));
        assert_eq!(libraries.snapshot().generation, 1);
        libraries.reload(&|| Err(anyhow::anyhow!("broken")));
        let snapshot = libraries.snapshot();
        assert_eq!(snapshot.generation, 2);
        assert!(snapshot.error.unwrap().contains("broken"));
    }

    #[test]
    fn only_so_many_streams_are_held() {
        let libraries = Libraries::new(Vec::new());
        let slots: Vec<_> = (0..MAX_STREAMS)
            .map(|_| libraries.stream_slot().unwrap())
            .collect();
        assert!(libraries.stream_slot().is_none());
        drop(slots);
        assert!(libraries.stream_slot().is_some());
    }
}
