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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AvailableType {
    /// The bare name used to invoke the type (e.g. `"png"`).
    pub id: String,
    /// The optional `doc:` string from the schema's top-level field.
    pub doc: Option<String>,
}

/// Scan `include_paths` for `.yaml` / `.yml` files and return a sorted,
/// deduplicated list of the types they define.
///
/// Each directory in `include_paths` is searched in order.  Files that
/// cannot be read or that contain invalid YAML are silently skipped — type
/// discovery is best-effort and must never prevent `--help` from printing.
/// A type `id` that appears in multiple directories is reported only once
/// (first directory wins, matching schema resolution priority).
pub fn discover_types(include_paths: &[PathBuf]) -> Vec<AvailableType> {
    let mut seen_ids = std::collections::HashSet::new();
    let mut types = Vec::new();

    for dir in include_paths {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let mut dir_types: Vec<AvailableType> = entries
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let ext = path.extension()?.to_str()?;
                if ext != "yaml" && ext != "yml" {
                    return None;
                }
                let text = std::fs::read_to_string(&path).ok()?;
                // Parse only the fields we need; ignore schemas with errors.
                let schema: SchemaHeader = serde_yaml::from_str(&text).ok()?;
                if seen_ids.contains(&schema.id) {
                    return None;
                }
                Some(AvailableType { id: schema.id, doc: schema.doc })
            })
            .collect();

        // Sort within this directory for deterministic output.
        dir_types.sort();

        for t in dir_types {
            seen_ids.insert(t.id.clone());
            types.push(t);
        }
    }

    types
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
        let cfg = Config { include_paths: vec![PathBuf::from("/config/path")] };
        let resolved = resolve_include_paths(&cli, &cfg);
        assert_eq!(resolved[0], PathBuf::from("/cli/path"));
        assert_eq!(resolved[1], PathBuf::from("/config/path"));
    }

    #[test]
    fn empty_cli_uses_only_config_paths() {
        let cfg = Config { include_paths: vec![PathBuf::from("/config/path")] };
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
        if dirs::home_dir().is_none() { return; }
        let p = PathBuf::from("~/foo/bar");
        let expanded = expand_tilde(&p);
        assert!(expanded.to_string_lossy().contains("foo/bar"));
        assert!(!expanded.to_string_lossy().starts_with("~/"));
    }

    // ── discover_types ────────────────────────────────────────────────────────

    #[test]
    fn discover_types_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let types = discover_types(&[dir.path().to_owned()]);
        assert!(types.is_empty());
    }

    #[test]
    fn discover_types_finds_yaml_files() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("png.yaml"),
            "id: png\ndoc: PNG image\nseq: []\n").unwrap();
        std::fs::write(dir.path().join("elf.yaml"),
            "id: elf\nseq: []\n").unwrap();

        let types = discover_types(&[dir.path().to_owned()]);
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

        let types = discover_types(&[dir.path().to_owned()]);
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].id, "png");
    }

    #[test]
    fn discover_types_ignores_invalid_yaml() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("bad.yaml"), "{{{{not valid yaml").unwrap();
        std::fs::write(dir.path().join("good.yaml"), "id: good\nseq: []\n").unwrap();

        let types = discover_types(&[dir.path().to_owned()]);
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].id, "good");
    }

    #[test]
    fn discover_types_first_dir_wins_on_duplicate_id() {
        let dir1 = tempfile::TempDir::new().unwrap();
        let dir2 = tempfile::TempDir::new().unwrap();
        std::fs::write(dir1.path().join("fmt.yaml"),
            "id: fmt\ndoc: from dir1\nseq: []\n").unwrap();
        std::fs::write(dir2.path().join("fmt.yaml"),
            "id: fmt\ndoc: from dir2\nseq: []\n").unwrap();

        let types = discover_types(&[
            dir1.path().to_owned(),
            dir2.path().to_owned(),
        ]);
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].doc.as_deref(), Some("from dir1"));
    }

    #[test]
    fn discover_types_accepts_yml_extension() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("wav.yml"), "id: wav\nseq: []\n").unwrap();

        let types = discover_types(&[dir.path().to_owned()]);
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].id, "wav");
    }

    #[test]
    fn discover_types_nonexistent_dir_skipped() {
        let types = discover_types(&[PathBuf::from("/nonexistent/path")]);
        assert!(types.is_empty());
    }

    #[test]
    fn discover_types_multiple_dirs_merged() {
        let dir1 = tempfile::TempDir::new().unwrap();
        let dir2 = tempfile::TempDir::new().unwrap();
        std::fs::write(dir1.path().join("png.yaml"), "id: png\nseq: []\n").unwrap();
        std::fs::write(dir2.path().join("elf.yaml"), "id: elf\nseq: []\n").unwrap();

        let types = discover_types(&[
            dir1.path().to_owned(),
            dir2.path().to_owned(),
        ]);
        assert_eq!(types.len(), 2);
        let ids: Vec<&str> = types.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"png"));
        assert!(ids.contains(&"elf"));
    }
}
