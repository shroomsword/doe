//! Configuration loading.
//!
//! The config file lives at `~/.doe/config/config.yaml` by default and may
//! be overridden with `-c / --config`.  Its only current key is
//! `include_paths`, a list of directories to search for bare schema names.
//!
//! Command-line `-I` paths are prepended to those from the config file and
//! take priority.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{DoeError, Result};

// ─────────────────────────────────────────────────────────────────────────────
// Config file schema
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Directories to search for bare schema names.
    #[serde(default)]
    pub include_paths: Vec<PathBuf>,
}

impl Config {
    /// Load a config file from `path`.  Returns an empty `Config` if the
    /// file does not exist (that is not an error).
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

    /// Return the default config file path: `~/.doe/config/config.yaml`.
    /// Returns `None` if the home directory cannot be determined.
    pub fn default_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".doe").join("config").join("config.yaml"))
    }
}

/// Resolve the final ordered list of include paths from all sources.
///
/// Priority (highest first):
/// 1. Paths supplied via `-I` on the command line.
/// 2. Paths from the config file.
///
/// Tilde expansion is applied to each path.
pub fn resolve_include_paths(
    cli_paths: &[PathBuf],
    config: &Config,
) -> Vec<PathBuf> {
    cli_paths
        .iter()
        .chain(config.include_paths.iter())
        .map(|p| expand_tilde(p))
        .collect()
}

/// Expand a leading `~/` to the user's home directory.
/// Paths that do not start with `~` are returned unchanged.
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
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // ── Config::load ─────────────────────────────────────────────────────────

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

    // ── resolve_include_paths ─────────────────────────────────────────────────

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

    // ── expand_tilde ─────────────────────────────────────────────────────────

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
        if dirs::home_dir().is_none() { return; } // skip if no home
        let p = PathBuf::from("~/foo/bar");
        let expanded = expand_tilde(&p);
        // Should start with the home dir and end with foo/bar
        assert!(expanded.to_string_lossy().contains("foo/bar"));
        assert!(!expanded.to_string_lossy().starts_with("~/"));
    }
}
