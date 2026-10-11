use std::path::PathBuf;

use doe::config::*;
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
