//! Configuration loading and include-path resolution.
//!
//! `doe` stores user configuration in a TOML file at
//! `$XDG_CONFIG_HOME/doe/config.toml`, falling back to
//! `~/.config/doe/config.toml` (including on macOS), or the Windows
//! known-folder location on Windows.  The location can be overridden with
//! `-c`/`--config`.  Two keys are recognised:
//!
//! ```toml
//! include_paths = ["~/.doe/types", "/usr/share/doe/types"]
//!
//! [log]
//! level = "debug"
//! ```
//!
//! When a bare type name such as `"png"` is requested, `doe` searches each
//! directory in order and loads `<dir>/png.yaml`.  Command-line `-I` paths
//! are prepended and take priority over config-file paths.

use std::path::{Path, PathBuf};

use etcetera::BaseStrategy;
use serde::{Deserialize, Serialize};

use crate::error::{DoeError, Result};

// ─────────────────────────────────────────────────────────────────────────────
// Config struct
// ─────────────────────────────────────────────────────────────────────────────

/// Contents of the `doe` configuration file.
///
/// Serialises to and from the TOML structure shown in the module documentation.
/// Unknown keys are rejected to surface typos early.
///
/// # Example config file
///
/// ```toml
/// include_paths = ["~/.doe/types", "/usr/share/doe/types"]
///
/// [log]
/// level = "debug"
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

    /// The colour theme of the terminal viewer (`doe --tui`): the id of a
    /// built-in theme, the name of a file in the `themes` directory next to
    /// the config file, or the path of a theme file. `--theme` overrides it.
    #[serde(default)]
    pub theme: Option<String>,

    /// The radix in which the terminal viewer (`doe --tui`) first shows
    /// integers: `dec`, `hex`, `bin` or `oct`. `--radix` overrides it.
    #[serde(default)]
    pub radix: Option<String>,

    /// Logging settings.
    #[serde(default)]
    pub log: LogConfig,
}

/// The `[log]` table of the configuration file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LogConfig {
    /// Verbosity of the log file. The built-in default applies when unset.
    #[serde(default)]
    pub level: Option<LogLevel>,
}

/// A log verbosity, from `off` (nothing) to `trace` (everything).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Log nothing.
    Off,
    /// Errors only.
    Error,
    /// Errors and warnings.
    Warn,
    /// Also informational messages.
    Info,
    /// Also debug messages.
    Debug,
    /// Also trace messages.
    Trace,
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
    /// [`DoeError::Toml`] if it cannot be parsed.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            log::debug!("no config file at {}; using defaults", path.display());
            return Ok(Config::default());
        }
        log::debug!("reading config file {}", path.display());
        let text = std::fs::read_to_string(path).map_err(|e| DoeError::Io {
            path: path.to_owned(),
            source: e,
        })?;
        toml::from_str(&text).map_err(|e| DoeError::Toml {
            path: path.to_owned(),
            source: e,
        })
    }

    /// Load the configuration from `path`, or from [`Config::default_path`]
    /// when `path` is `None`.
    ///
    /// If no path is given and the default location cannot be determined, the
    /// default (empty) configuration is returned.
    ///
    /// # Errors
    ///
    /// See [`Config::load`].
    pub fn load_or_default(path: Option<&Path>) -> Result<Self> {
        match path.map(Path::to_owned).or_else(Self::default_path) {
            Some(path) => Self::load(&path),
            None => Ok(Config::default()),
        }
    }

    /// Return the platform-default config file path:
    /// `$XDG_CONFIG_HOME/doe/config.toml` (`~/.config/doe/config.toml` when the
    /// variable is unset), or the Windows known-folder equivalent.
    ///
    /// Returns `None` if the user's home directory cannot be determined
    /// (unusual but possible in sandboxed environments).
    pub fn default_path() -> Option<PathBuf> {
        etcetera::choose_base_strategy()
            .ok()
            .map(|strategy| strategy.config_dir().join("doe").join("config.toml"))
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
    if let Ok(home) = etcetera::home_dir() {
        if s == "~" {
            return home;
        }
        if let Some(rest) = s.strip_prefix("~/") {
            return home.join(rest);
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
///
/// # Errors
///
/// Returns every [`DuplicateTypeError`] found: two files in the same
/// directory declaring the same `id`.
pub fn discover_types(
    include_paths: &[PathBuf],
) -> std::result::Result<Vec<AvailableType>, Vec<DuplicateTypeError>> {
    // ids already committed from a higher-priority directory
    let mut claimed_ids: std::collections::HashMap<String, PathBuf> =
        std::collections::HashMap::new();
    let mut types: Vec<AvailableType> = Vec::new();
    let mut errors: Vec<DuplicateTypeError> = Vec::new();

    for dir in include_paths {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };

        // Collect all valid schema headers from this directory first, so we
        // can detect within-directory duplicates before committing any of them.
        let mut dir_entries: Vec<(PathBuf, ResolvedHeader)> = entries
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let ext = path.extension()?.to_str()?;
                if !crate::SCHEMA_EXTENSIONS.contains(&ext) {
                    return None;
                }
                let text = std::fs::read_to_string(&path).ok()?;
                let header: SchemaHeader = crate::schema::raw::parse_yaml(&text).ok()?;
                let id = header.id.or_else(|| header.meta?.id)?;
                Some((
                    path,
                    ResolvedHeader {
                        id,
                        doc: header.doc,
                    },
                ))
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
    #[serde(default)]
    id: Option<String>,
    /// Kaitai Struct files give the id as `meta.id`.
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

/// A [`SchemaHeader`] with its id found.
struct ResolvedHeader {
    id: String,
    doc: Option<String>,
}
