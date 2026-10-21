//! Where schema files come from.
//!
//! The compiler follows `imports` by looking for files. What a file is, and
//! where it is looked for, is the job of a [`SchemaSource`]: [`FsSource`] reads
//! the filesystem, as doe always did; [`MemorySource`] holds the files in a map,
//! for a program that has no filesystem (WebAssembly in a browser) or whose
//! schemas are not files. The rules of lookup (next to the importing file, then
//! the include paths; a leading slash means the include paths only) are the
//! compiler's and are the same for both.

use std::collections::HashMap;
use std::fmt::Debug;
use std::path::{Component, Path, PathBuf};

use crate::error::{DoeError, Result};

/// A place that schema files can be looked for and read.
pub trait SchemaSource: Debug + Send + Sync {
    /// Whether there is a file at `path`.
    fn exists(&self, path: &Path) -> bool;

    /// The text of the file at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`DoeError::Io`] if the file cannot be read.
    fn read(&self, path: &Path) -> Result<String>;
}

/// The files of the filesystem.
#[derive(Debug, Clone, Copy, Default)]
pub struct FsSource;

impl SchemaSource for FsSource {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn read(&self, path: &Path) -> Result<String> {
        std::fs::read_to_string(path).map_err(|source| DoeError::Io {
            path: path.to_owned(),
            source,
        })
    }
}

/// Files held in memory, by path.
///
/// Paths are compared as they would be on a filesystem: `a/./b.ksy` and
/// `a/c/../b.ksy` are the file `a/b.ksy`. The paths are relative to nothing;
/// there is no current directory, so an import written `./x` from a file that
/// is `main.ksy` is `x.ksy` beside it.
#[derive(Debug, Clone, Default)]
pub struct MemorySource {
    files: HashMap<PathBuf, String>,
}

impl MemorySource {
    /// A source with no files.
    pub fn new() -> Self {
        MemorySource::default()
    }

    /// Add a file, replacing the one at the same path.
    pub fn insert(&mut self, path: impl AsRef<Path>, text: impl Into<String>) {
        self.files.insert(normalize(path.as_ref()), text.into());
    }

    /// The same, in the style of a builder.
    #[must_use]
    pub fn with_file(mut self, path: impl AsRef<Path>, text: impl Into<String>) -> Self {
        self.insert(path, text);
        self
    }

    /// How many files there are.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether there are no files.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

impl SchemaSource for MemorySource {
    fn exists(&self, path: &Path) -> bool {
        self.files.contains_key(&normalize(path))
    }

    fn read(&self, path: &Path) -> Result<String> {
        self.files
            .get(&normalize(path))
            .cloned()
            .ok_or_else(|| DoeError::Io {
                path: path.to_owned(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"),
            })
    }
}

/// `path` without `.` components and with `..` applied to the component before
/// it, so that two spellings of one file are one key. A `..` that goes above
/// the start is kept: it names nothing that is in the source.
fn normalize(path: &Path) -> PathBuf {
    let mut parts: Vec<Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match parts.last() {
                Some(Component::Normal(_)) => {
                    parts.pop();
                }
                _ => parts.push(component),
            },
            other => parts.push(other),
        }
    }
    parts.iter().collect()
}
