//! Where schema files come from.
//!
//! The compiler follows `imports` by looking for files. What a file is, and
//! where it is looked for, is the job of a [`SchemaSource`]: `FsSource` reads
//! the filesystem, as doe always did; [`MemorySource`] holds the files in a map,
//! for a program that has no filesystem (WebAssembly in a browser) or whose
//! schemas are not files. The rules of lookup (next to the importing file, then
//! the include paths; a leading slash means the include paths only) are the
//! compiler's and are the same for both.

use std::collections::HashMap;
use std::fmt::Debug;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::path::{Component, Path, PathBuf};

use crate::error::{DoeError, Result};

/// A place that schema files can be looked for and read.
///
/// The bounds keep [`Compiler`](crate::schema::compiler::Compiler) `Send`, `Sync`
/// and unwind safe, as it was before it could take a source.
pub trait SchemaSource: Debug + Send + Sync + UnwindSafe + RefUnwindSafe {
    /// Whether there is a file at `path`.
    fn exists(&self, path: &Path) -> bool;

    /// The text of the file at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`DoeError::Io`] if the file cannot be read.
    fn read(&self, path: &Path) -> Result<String>;

    /// What is in the directory `dir`: each entry with whether it is a
    /// directory. A source that cannot list returns nothing, and then a type is
    /// found only by the name of its file. A link to a directory is left out,
    /// so that a loop of links ends.
    fn list_dir(&self, _dir: &Path) -> Vec<(PathBuf, bool)> {
        Vec::new()
    }

    /// The text of the file at `path` when it is only looked at to see what it
    /// is, as the search for a type by its id does. A source that remembers the
    /// files it read does not remember these.
    fn peek(&self, path: &Path) -> Option<String> {
        self.read(path).ok()
    }
}

/// How deep below a type path a schema is looked for.
pub(crate) const MAX_SEARCH_DEPTH: usize = 8;

/// How many files are looked at below one type path.
pub(crate) const MAX_SEARCH_FILES: usize = 20_000;

/// The schema files below `dir` (the file extensions of schemas), in sorted
/// order, to a limit of depth and of number.
pub(crate) fn schema_files_below(source: &dyn SchemaSource, dir: &Path) -> Vec<PathBuf> {
    fn walk(source: &dyn SchemaSource, dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
        let mut entries = source.list_dir(dir);
        entries.sort();
        for (path, is_dir) in entries {
            if found.len() >= MAX_SEARCH_FILES {
                return;
            }
            if is_dir {
                if depth < MAX_SEARCH_DEPTH {
                    walk(source, &path, depth + 1, found);
                }
            } else if crate::has_schema_extension(&path.to_string_lossy()) {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(source, dir, 0, &mut found);
    found
}

/// The `id` and `doc` a schema file declares at its top (`id`, or `meta.id` in
/// a Kaitai Struct file), without reading the rest of it into a schema.
#[derive(serde::Deserialize)]
struct SchemaHeader {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    meta: Option<MetaHeader>,
    #[serde(default)]
    doc: Option<String>,
}

#[derive(serde::Deserialize)]
struct MetaHeader {
    #[serde(default)]
    id: Option<String>,
}

/// The id and the doc of the schema in `text`, or nothing if it is not one or
/// has no id.
pub(crate) fn schema_header(text: &str) -> Option<(String, Option<String>)> {
    schema_header_checked(text).ok().flatten()
}

/// The same, with why the text is not YAML that a schema can be, when it is not.
pub(crate) fn schema_header_checked(
    text: &str,
) -> std::result::Result<Option<(String, Option<String>)>, String> {
    let header: SchemaHeader = crate::schema::raw::parse_yaml(text).map_err(|e| e.to_string())?;
    let Some(id) = header.id.or_else(|| header.meta?.id) else {
        return Ok(None);
    };
    Ok(Some((id, header.doc)))
}

/// The files of the filesystem.
#[cfg(feature = "fs")]
#[derive(Debug, Clone, Copy, Default)]
pub struct FsSource;

#[cfg(feature = "fs")]
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

    fn list_dir(&self, dir: &Path) -> Vec<(PathBuf, bool)> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        entries
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let linked = path.symlink_metadata().ok()?.file_type().is_symlink();
                if path.is_dir() {
                    (!linked).then_some((path, true))
                } else {
                    Some((path, false))
                }
            })
            .collect()
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

    fn list_dir(&self, dir: &Path) -> Vec<(PathBuf, bool)> {
        let dir = normalize(dir);
        let mut entries: Vec<(PathBuf, bool)> = Vec::new();
        for file in self.files.keys() {
            let Ok(rest) = file.strip_prefix(&dir) else {
                continue;
            };
            let mut parts = rest.components();
            let Some(first) = parts.next() else {
                continue;
            };
            let entry = (dir.join(first), parts.next().is_some());
            if !entries.contains(&entry) {
                entries.push(entry);
            }
        }
        entries
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
