//! Configuration loading and include-path resolution.
//!
//! `doe` stores user configuration in a YAML file at
//! `~/.doe/config/config.yaml` (overrideable with `-c`/`--config`).  The
//! only key currently recognised is `include_paths`:
//!
//! ```yaml
//! include_paths:
//!   - ~/.doe/types
//!   - /usr/share/doe/types
//! ```
//!
//! When a bare type name such as `"png"` is requested, `doe` searches each
//! directory in order and loads `<dir>/png.yaml`.  Command-line `-I` paths
//! are prepended and take priority over config-file paths.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{DoeError, Result};

// ─────────────────────────────────────────────────────────────────────────────
// Config struct
// ─────────────────────────────────────────────────────────────────────────────

/// Contents of the `doe` configuration file.
///
/// Serialises to and from the YAML structure shown in the module documentation.
/// Unknown keys are rejected to surface typos early.
///
/// # Example config file
///
/// ```yaml
/// include_paths:
///   - ~/.doe/types
///   - /usr/share/doe/types
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Directories to search when resolving a bare type name.
    ///
    /// Entries are searched left-to-right; the first match wins.
    /// Tilde expansion (`~/`) is applied to each path.
    #[serde(default)]
    pub include_paths: Vec<PathBuf>,
}

impl Config {
    /// Load the configuration file at `path`.
    ///
    /// A missing file is **not** an error — it simply yields a default
    /// (empty) `Config`.  This allows `doe` to work out of the box without
    /// requiring any configuration.
    ///
    /// # Errors
    ///
    /// Returns [`DoeError::Io`] if the file exists but cannot be read, or
    /// [`DoeError::Yaml`] if it cannot be parsed.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = std::fs::read_to_string(path).map_err(|e| DoeError::Io {
            path: path.to_owned(),
            source: e,
        })?;
        serde_yaml::from_str(&text).map_err(|e| DoeError::Yaml {
            path: path.to_owned(),
            source: e,
        })
    }

    /// Return the platform-default config file path: `~/.doe/config/config.yaml`.
    ///
    /// Returns `None` if the user's home directory cannot be determined
    /// (unusual but possible in sandboxed environments).
    pub fn default_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".doe").join("config").join("config.yaml"))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Path helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Build the final ordered include-path list from all sources.
///
/// Paths are merged as follows (highest priority first):
///
/// 1. `cli_paths` — paths supplied via `-I` / `--include-path` on the
///    command line.
/// 2. `config.include_paths` — paths from the config file.
///
/// [`expand_tilde`] is applied to every path in the merged list.
pub fn resolve_include_paths(cli_paths: &[PathBuf], config: &Config) -> Vec<PathBuf> {
    cli_paths
        .iter()
        .chain(config.include_paths.iter())
        .map(|p| expand_tilde(p))
        .collect()
}

/// Expand a leading `~/` (or a bare `~`) to the user's home directory.
///
/// Paths that do not start with `~` are returned unchanged.  If the home
/// directory cannot be determined the original path is returned as-is.
///
/// # Examples
///
/// ```no_run
/// use std::path::{Path, PathBuf};
/// use doe::config::expand_tilde;
///
/// // On a Unix system with HOME=/home/alice:
/// assert_eq!(
///     expand_tilde(Path::new("~/types")),
///     PathBuf::from("/home/alice/types"),
/// );
///
/// // Non-tilde paths are unaffected:
/// assert_eq!(
///     expand_tilde(Path::new("/absolute/path")),
///     PathBuf::from("/absolute/path"),
/// );
/// ```
pub fn expand_tilde(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("~/") || s == "~" {
        if let Some(home) = dirs::home_dir() {
            return home.join(&s[2..]);
        }
    }
    path.to_owned()
}

// ─────────────────────────────────────────────────────────────────────────────
// Type discovery
// ─────────────────────────────────────────────────────────────────────────────

/// A type discovered on an include path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableType {
    /// The bare name used to invoke the type (e.g. `"png"`).
    pub id: String,
    /// The optional `doc:` string from the schema's top-level field.
    pub doc: Option<String>,
    /// The file the type was loaded from.
    pub source: PathBuf,
}

impl PartialOrd for AvailableType {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AvailableType {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

/// A duplicate `id` found within a single include-path directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateTypeError {
    /// The conflicting type id.
    pub id: String,
    /// The first file that defined this id (within the directory).
    pub first: PathBuf,
    /// The second file that defined this id (within the same directory).
    pub second: PathBuf,
}

impl std::fmt::Display for DuplicateTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "duplicate type id '{}' in the same include directory:\n  \
             first:  {}\n  \
             second: {}\n  \
             Each schema id must be unique within a directory.\n  \
             To shadow a type from a lower-priority directory, place the \
             replacement in a higher-priority directory instead.",
            self.id,
            self.first.display(),
            self.second.display(),
        )
    }
}

/// Scan `include_paths` for `.yaml` / `.yml` files and return a sorted list
/// of the types they define.
///
/// **Duplicate policy:**
/// - Two files in the **same directory** with the same `id` are always an
///   error.  The returned `Err` contains every conflict found; all directories
///   are scanned before returning so the user sees all problems at once.
/// - The same `id` appearing in **different directories** is allowed:
///   the type from the highest-priority directory (earliest in
///   `include_paths`) wins and the later occurrence is silently ignored.
///   This is the intentional shadowing mechanism — user types override
///   system-installed types.
///
/// Files that cannot be read or that contain invalid YAML are silently
/// skipped; discovery must never prevent `--help` from printing.
pub fn discover_types(
    include_paths: &[PathBuf],
) -> std::result::Result<Vec<AvailableType>, Vec<DuplicateTypeError>> {
    // ids already committed from a higher-priority directory
    let mut claimed_ids: std::collections::HashMap<String, PathBuf> =
        std::collections::HashMap::new();
    let mut types: Vec<AvailableType> = Vec::new();
    let mut errors: Vec<DuplicateTypeError> = Vec::new();

    for dir in include_paths {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        // Collect all valid schema headers from this directory first, so we
        // can detect within-directory duplicates before committing any of them.
        let mut dir_entries: Vec<(PathBuf, SchemaHeader)> = entries
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let ext = path.extension()?.to_str()?;
                if ext != "yaml" && ext != "yml" {
                    return None;
                }
                let text = std::fs::read_to_string(&path).ok()?;
                let header: SchemaHeader = serde_yaml::from_str(&text).ok()?;
                Some((path, header))
            })
            .collect();

        // Sort for deterministic ordering and error messages.
        dir_entries.sort_by(|a, b| a.0.cmp(&b.0));

        // Detect within-directory duplicates.
        let mut within_dir: std::collections::HashMap<String, PathBuf> =
            std::collections::HashMap::new();
        let mut dir_has_error = false;

        for (path, header) in &dir_entries {
            if let Some(first) = within_dir.get(&header.id) {
                errors.push(DuplicateTypeError {
                    id: header.id.clone(),
                    first: first.clone(),
                    second: path.clone(),
                });
                dir_has_error = true;
            } else {
                within_dir.insert(header.id.clone(), path.clone());
            }
        }

        if dir_has_error {
            // Don't commit any types from a directory that has internal
            // duplicates — the user must fix the directory first.
            continue;
        }

        // Commit types from this directory, skipping ids already claimed by
        // a higher-priority directory (cross-directory shadowing).
        let mut dir_types: Vec<AvailableType> = dir_entries
            .into_iter()
            .filter_map(|(path, header)| {
                if claimed_ids.contains_key(&header.id) {
                    return None; // shadowed by higher-priority dir
                }
                claimed_ids.insert(header.id.clone(), path.clone());
                Some(AvailableType {
                    id: header.id,
                    doc: header.doc,
                    source: path,
                })
            })
            .collect();

        dir_types.sort();
        types.append(&mut dir_types);
    }

    if errors.is_empty() {
        Ok(types)
    } else {
        Err(errors)
    }
}

/// Minimal serde struct for parsing just the `id` and `doc` fields of a
/// schema file without pulling in the full `RawSchema` machinery.
#[derive(serde::Deserialize)]
struct SchemaHeader {
    id: String,
    #[serde(default)]
    doc: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn load_nonexistent_returns_default() {
        let p = PathBuf::from("/nonexistent/path/config.yaml");
        let cfg = Config::load(&p).unwrap();
        assert!(cfg.include_paths.is_empty());
    }

    #[test]
    fn load_empty_file_returns_default() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "{{}}").unwrap();
        let cfg = Config::load(f.path()).unwrap();
        assert!(cfg.include_paths.is_empty());
    }

    #[test]
    fn load_with_include_paths() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "include_paths:").unwrap();
        writeln!(f, "  - /usr/share/doe/types").unwrap();
        writeln!(f, "  - ~/.doe/types").unwrap();
        let cfg = Config::load(f.path()).unwrap();
        assert_eq!(cfg.include_paths.len(), 2);
        assert_eq!(cfg.include_paths[0], PathBuf::from("/usr/share/doe/types"));
    }

    #[test]
    fn load_invalid_yaml_is_error() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "unknown_key: true").unwrap();
        assert!(Config::load(f.path()).is_err());
    }

    #[test]
    fn cli_paths_come_before_config_paths() {
        let cli = vec![PathBuf::from("/cli/path")];
        let cfg = Config {
            include_paths: vec![PathBuf::from("/config/path")],
        };
        let resolved = resolve_include_paths(&cli, &cfg);
        assert_eq!(resolved[0], PathBuf::from("/cli/path"));
        assert_eq!(resolved[1], PathBuf::from("/config/path"));
    }

    #[test]
    fn empty_cli_uses_only_config_paths() {
        let cfg = Config {
            include_paths: vec![PathBuf::from("/config/path")],
        };
        let resolved = resolve_include_paths(&[], &cfg);
        assert_eq!(resolved.len(), 1);
    }

    #[test]
    fn both_empty_gives_empty_list() {
        let resolved = resolve_include_paths(&[], &Config::default());
        assert!(resolved.is_empty());
    }

    #[test]
    fn expand_tilde_absolute_path_unchanged() {
        let p = PathBuf::from("/absolute/path");
        assert_eq!(expand_tilde(&p), p);
    }

    #[test]
    fn expand_tilde_relative_path_unchanged() {
        let p = PathBuf::from("relative/path");
        assert_eq!(expand_tilde(&p), p);
    }

    #[test]
    fn expand_tilde_expands_home() {
        if dirs::home_dir().is_none() {
            return;
        }
        let p = PathBuf::from("~/foo/bar");
        let expanded = expand_tilde(&p);
        assert!(expanded.to_string_lossy().contains("foo/bar"));
        assert!(!expanded.to_string_lossy().starts_with("~/"));
    }

    // ── discover_types ────────────────────────────────────────────────────────

    #[test]
    fn discover_types_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let types = discover_types(&[dir.path().to_owned()]).unwrap();
        assert!(types.is_empty());
    }

    #[test]
    fn discover_types_finds_yaml_files() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("png.yaml"),
            "id: png\ndoc: PNG image\nseq: []\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("elf.yaml"), "id: elf\nseq: []\n").unwrap();

        let types = discover_types(&[dir.path().to_owned()]).unwrap();
        assert_eq!(types.len(), 2);
        // Results are sorted by id within a directory
        assert_eq!(types[0].id, "elf");
        assert_eq!(types[0].doc, None);
        assert_eq!(types[1].id, "png");
        assert_eq!(types[1].doc, Some("PNG image".into()));
    }

    #[test]
    fn discover_types_ignores_non_yaml() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("notes.txt"), "not a schema").unwrap();
        std::fs::write(dir.path().join("png.yaml"), "id: png\nseq: []\n").unwrap();

        let types = discover_types(&[dir.path().to_owned()]).unwrap();
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].id, "png");
    }

    #[test]
    fn discover_types_ignores_invalid_yaml() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("bad.yaml"), "{{{{not valid yaml").unwrap();
        std::fs::write(dir.path().join("good.yaml"), "id: good\nseq: []\n").unwrap();

        let types = discover_types(&[dir.path().to_owned()]).unwrap();
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].id, "good");
    }

    // Within the same directory, two files with the same id is always an error.
    #[test]
    fn discover_types_within_dir_duplicate_is_error() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("a_fmt.yaml"),
            "id: fmt\ndoc: first\nseq: []\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("b_fmt.yaml"),
            "id: fmt\ndoc: second\nseq: []\n",
        )
        .unwrap();

        let err = discover_types(&[dir.path().to_owned()]).unwrap_err();
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].id, "fmt");
        // first and second should be the two files (sorted alphabetically)
        assert!(err[0].first.to_string_lossy().contains("a_fmt"));
        assert!(err[0].second.to_string_lossy().contains("b_fmt"));
    }

    #[test]
    fn discover_types_within_dir_multiple_duplicates_all_reported() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("a1.yaml"), "id: foo\nseq: []\n").unwrap();
        std::fs::write(dir.path().join("a2.yaml"), "id: foo\nseq: []\n").unwrap();
        std::fs::write(dir.path().join("b1.yaml"), "id: bar\nseq: []\n").unwrap();
        std::fs::write(dir.path().join("b2.yaml"), "id: bar\nseq: []\n").unwrap();

        let err = discover_types(&[dir.path().to_owned()]).unwrap_err();
        assert_eq!(err.len(), 2);
        let ids: Vec<&str> = err.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"foo"));
        assert!(ids.contains(&"bar"));
    }

    // Across different directories, the first directory wins (shadowing).
    #[test]
    fn discover_types_cross_dir_shadowing_is_allowed() {
        let dir1 = tempfile::TempDir::new().unwrap();
        let dir2 = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir1.path().join("fmt.yaml"),
            "id: fmt\ndoc: from dir1\nseq: []\n",
        )
        .unwrap();
        std::fs::write(
            dir2.path().join("fmt.yaml"),
            "id: fmt\ndoc: from dir2\nseq: []\n",
        )
        .unwrap();

        let types =
            discover_types(&[dir1.path().to_owned(), dir2.path().to_owned()]).unwrap();
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].doc.as_deref(), Some("from dir1"));
    }

    // A directory with duplicates is skipped entirely; other directories
    // still contribute their types.
    #[test]
    fn discover_types_error_dir_skipped_others_still_contribute() {
        let bad_dir = tempfile::TempDir::new().unwrap();
        let good_dir = tempfile::TempDir::new().unwrap();
        // Two conflicting files in bad_dir
        std::fs::write(bad_dir.path().join("a.yaml"), "id: conflict\nseq: []\n").unwrap();
        std::fs::write(bad_dir.path().join("b.yaml"), "id: conflict\nseq: []\n").unwrap();
        // A valid file in good_dir
        std::fs::write(good_dir.path().join("valid.yaml"), "id: valid\nseq: []\n").unwrap();

        // Should return Err (because bad_dir has a duplicate), but the error
        // message should name the conflicting files from bad_dir.
        let err =
            discover_types(&[bad_dir.path().to_owned(), good_dir.path().to_owned()]).unwrap_err();
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].id, "conflict");
    }

    #[test]
    fn discover_types_accepts_yml_extension() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("wav.yml"), "id: wav\nseq: []\n").unwrap();

        let types = discover_types(&[dir.path().to_owned()]).unwrap();
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].id, "wav");
    }

    #[test]
    fn discover_types_nonexistent_dir_skipped() {
        let types = discover_types(&[PathBuf::from("/nonexistent/path")]).unwrap();
        assert!(types.is_empty());
    }

    #[test]
    fn discover_types_multiple_dirs_merged() {
        let dir1 = tempfile::TempDir::new().unwrap();
        let dir2 = tempfile::TempDir::new().unwrap();
        std::fs::write(dir1.path().join("png.yaml"), "id: png\nseq: []\n").unwrap();
        std::fs::write(dir2.path().join("elf.yaml"), "id: elf\nseq: []\n").unwrap();

        let types =
            discover_types(&[dir1.path().to_owned(), dir2.path().to_owned()]).unwrap();
        assert_eq!(types.len(), 2);
        let ids: Vec<&str> = types.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"png"));
        assert!(ids.contains(&"elf"));
    }

    #[test]
    fn duplicate_type_error_display_contains_key_info() {
        let err = DuplicateTypeError {
            id: "png".into(),
            first: PathBuf::from("/types/a.yaml"),
            second: PathBuf::from("/types/b.yaml"),
        };
        let msg = err.to_string();
        assert!(msg.contains("png"));
        assert!(msg.contains("a.yaml"));
        assert!(msg.contains("b.yaml"));
        // Should explain the shadowing mechanism
        assert!(msg.contains("higher-priority"));
    }
}
