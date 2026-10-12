use std::path::PathBuf;

use doe::config::*;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn load_nonexistent_returns_default() {
    let p = PathBuf::from("/nonexistent/path/config.toml");
    let cfg = Config::load(&p).unwrap();
    assert!(cfg.include_paths.is_empty());
}

#[test]
fn load_empty_file_returns_default() {
    let mut f = NamedTempFile::new().unwrap();
    write!(f, "").unwrap();
    let cfg = Config::load(f.path()).unwrap();
    assert!(cfg.include_paths.is_empty());
}

#[test]
fn load_with_include_paths() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(
        f,
        "include_paths = [\"/usr/share/doe/types\", \"~/.doe/types\"]"
    )
    .unwrap();
    let cfg = Config::load(f.path()).unwrap();
    assert_eq!(cfg.include_paths.len(), 2);
    assert_eq!(cfg.include_paths[0], PathBuf::from("/usr/share/doe/types"));
}

#[test]
fn load_unknown_key_is_error() {
    let mut f = NamedTempFile::new().unwrap();
    write!(f, "unknown_key = true").unwrap();
    assert!(matches!(
        Config::load(f.path()).unwrap_err(),
        doe::DoeError::Toml { .. }
    ));
}

#[test]
fn cli_paths_come_before_config_paths() {
    let cli = vec![PathBuf::from("/cli/path")];
    let cfg = Config {
        include_paths: vec![PathBuf::from("/config/path")],
        ..Config::default()
    };
    let resolved = resolve_include_paths(&cli, &cfg);
    assert_eq!(resolved[0], PathBuf::from("/cli/path"));
    assert_eq!(resolved[1], PathBuf::from("/config/path"));
}

#[test]
fn empty_cli_uses_only_config_paths() {
    let cfg = Config {
        include_paths: vec![PathBuf::from("/config/path")],
        ..Config::default()
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
    if etcetera::home_dir().is_err() {
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

    let types = discover_types(&[dir1.path().to_owned(), dir2.path().to_owned()]).unwrap();
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
    let err = discover_types(&[bad_dir.path().to_owned(), good_dir.path().to_owned()]).unwrap_err();
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

    let types = discover_types(&[dir1.path().to_owned(), dir2.path().to_owned()]).unwrap();
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

// ── Default path, tilde expansion, load errors ────────────────────────────

#[test]
fn default_path_is_doe_config_toml_in_the_xdg_config_dir() {
    if let Some(path) = Config::default_path() {
        assert!(path.ends_with("doe/config.toml"), "{path:?}");
        assert!(path.is_absolute());
    }
}

#[test]
fn load_or_default_without_a_path_uses_defaults_or_the_default_file() {
    // The default location may or may not exist on the machine running the
    // tests; either way a missing file is not an error.
    if Config::default_path().is_none_or(|p| !p.exists()) {
        assert!(
            Config::load_or_default(None)
                .unwrap()
                .include_paths
                .is_empty()
        );
    }
}

#[test]
fn load_or_default_prefers_the_explicit_path() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "include_paths = [\"/explicit\"]").unwrap();
    let cfg = Config::load_or_default(Some(f.path())).unwrap();
    assert_eq!(cfg.include_paths, [PathBuf::from("/explicit")]);
}

#[test]
fn log_level_is_read_from_the_log_table() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "[log]\nlevel = \"trace\"").unwrap();
    let cfg = Config::load(f.path()).unwrap();
    assert_eq!(cfg.log.level, Some(LogLevel::Trace));
}

#[test]
fn every_log_level_name_parses() {
    for (name, level) in [
        ("off", LogLevel::Off),
        ("error", LogLevel::Error),
        ("warn", LogLevel::Warn),
        ("info", LogLevel::Info),
        ("debug", LogLevel::Debug),
        ("trace", LogLevel::Trace),
    ] {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "[log]\nlevel = \"{name}\"").unwrap();
        assert_eq!(Config::load(f.path()).unwrap().log.level, Some(level));
    }
}

#[test]
fn invalid_log_level_and_unknown_log_key_are_errors() {
    for text in ["[log]\nlevel = \"loud\"", "[log]\nfile = \"x\""] {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "{text}").unwrap();
        assert!(Config::load(f.path()).is_err(), "{text}");
    }
}

#[test]
fn parse_error_names_the_file_and_the_line() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "include_paths = [\n  \"ok\",\n  oops\n]").unwrap();
    let message = Config::load(f.path()).unwrap_err().to_string();
    assert!(
        message.contains(&f.path().display().to_string()),
        "{message}"
    );
    assert!(message.contains("line 3"), "{message}");
}

#[test]
fn an_old_yaml_config_is_not_valid_toml() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "include_paths:\n  - /usr/share/doe/types").unwrap();
    assert!(matches!(
        Config::load(f.path()).unwrap_err(),
        doe::DoeError::Toml { .. }
    ));
}

#[test]
fn load_directory_instead_of_file_is_io_error() {
    let dir = tempfile::TempDir::new().unwrap();
    let err = Config::load(dir.path()).unwrap_err();
    assert!(matches!(err, doe::DoeError::Io { .. }), "{err:?}");
}

#[test]
fn expand_tilde_alone_is_home() {
    if let Ok(home) = etcetera::home_dir() {
        assert_eq!(expand_tilde(std::path::Path::new("~")), home);
        assert_eq!(
            expand_tilde(std::path::Path::new("~/a/b")),
            home.join("a/b")
        );
    }
}

#[test]
fn expand_tilde_leaves_other_paths_alone() {
    for p in ["~user/x", "a/~", "/abs", "rel"] {
        assert_eq!(
            expand_tilde(std::path::Path::new(p)),
            PathBuf::from(p),
            "{p}"
        );
    }
}

#[test]
fn available_type_orders_by_id() {
    let make = |id: &str| AvailableType {
        id: id.to_owned(),
        doc: None,
        source: PathBuf::from(format!("{id}.yaml")),
    };
    let mut types = [make("zeta"), make("alpha"), make("mid")];
    types.sort();
    let ids: Vec<_> = types.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["alpha", "mid", "zeta"]);
}

#[test]
fn discover_types_reports_id_doc_and_source() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("thing.yaml");
    std::fs::write(&file, "id: thing\ndoc: A thing\nseq: []\n").unwrap();
    let found = discover_types(&[dir.path().to_owned()]).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, "thing");
    assert_eq!(found[0].doc.as_deref(), Some("A thing"));
    assert_eq!(found[0].source, file);
}
