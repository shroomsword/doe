use std::path::Path;

use doe::*;

// ── is_schema_path ────────────────────────────────────────────────────────

#[test]
fn schema_path_dot_prefix() {
    assert!(is_schema_path("./my.yaml"));
}
#[test]
fn schema_path_absolute() {
    assert!(is_schema_path("/usr/share/types/png.yaml"));
}
#[test]
fn schema_path_tilde() {
    assert!(is_schema_path("~/types/png.yaml"));
}
#[test]
fn schema_path_yaml_extension() {
    assert!(is_schema_path("some_file.yaml"));
}
#[test]
fn schema_path_yml_extension() {
    assert!(is_schema_path("some_file.yml"));
}
#[test]
fn bare_type_name_not_a_path() {
    assert!(!is_schema_path("png"));
}
#[test]
fn bare_type_no_extension() {
    assert!(!is_schema_path("my_format"));
}

// ── load_schema_file ──────────────────────────────────────────────────────

#[test]
fn load_schema_file_ok() {
    use std::io::Write;
    use tempfile::NamedTempFile;
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "id: t\nseq:\n  - id: x\n    type: u1").unwrap();
    let s = load_schema_file(f.path()).unwrap();
    assert_eq!(s.id, "t");
    assert!(s.source_path.is_some());
}

#[test]
fn load_schema_file_missing_is_error() {
    assert!(load_schema_file(Path::new("/nonexistent.yaml")).is_err());
}

#[test]
fn load_schema_file_bad_yaml_is_error() {
    use std::io::Write;
    use tempfile::NamedTempFile;
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "unknown_key: true").unwrap();
    assert!(load_schema_file(f.path()).is_err());
}

// ── parse_file end-to-end ─────────────────────────────────────────────────

#[test]
fn parse_file_text_output() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut schema_f = NamedTempFile::new().unwrap();
    writeln!(
        schema_f,
        "id: simple\nseq:\n  - id: a\n    type: u1\n  - id: b\n    type: u2le"
    )
    .unwrap();

    let mut bin_f = NamedTempFile::new().unwrap();
    bin_f.write_all(&[0x07, 0x34, 0x12]).unwrap();

    let out = parse_file(
        bin_f.path(),
        schema_f.path().to_str().unwrap(),
        &[],
        Some(Path::new("/nonexistent")),
        OutputFormat::Text {
            indent: "  ".into(),
        },
    )
    .unwrap();

    assert!(out.contains("simple\n"));
    assert!(out.contains("a  7"));
    assert!(out.contains("b  4660"));
}

#[test]
fn parse_file_json_output() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut schema_f = NamedTempFile::new().unwrap();
    writeln!(
        schema_f,
        "id: pair\nseq:\n  - id: x\n    type: u1\n  - id: y\n    type: u1"
    )
    .unwrap();

    let mut bin_f = NamedTempFile::new().unwrap();
    bin_f.write_all(&[0x01, 0x02]).unwrap();

    let out = parse_file(
        bin_f.path(),
        schema_f.path().to_str().unwrap(),
        &[],
        Some(Path::new("/nonexistent")),
        OutputFormat::JsonPretty,
    )
    .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["$type"], "pair");
    assert_eq!(parsed["x"], 1u64);
    assert_eq!(parsed["y"], 2u64);
}

#[test]
fn parse_file_via_include_path() {
    use std::io::Write;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("mytype.yaml"),
        "id: mytype\nseq:\n  - id: val\n    type: u4le\n",
    )
    .unwrap();

    let mut bin_f = tempfile::NamedTempFile::new().unwrap();
    bin_f.write_all(&0xdead_beefu32.to_le_bytes()).unwrap();

    let out = parse_file(
        bin_f.path(),
        "mytype",
        &[dir.path().to_owned()],
        Some(Path::new("/nonexistent")),
        OutputFormat::Text {
            indent: "  ".into(),
        },
    )
    .unwrap();

    assert!(out.contains("3735928559"));
}

#[test]
fn parse_file_missing_binary_is_error() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut schema_f = NamedTempFile::new().unwrap();
    writeln!(schema_f, "id: t\nseq:\n  - id: x\n    type: u1").unwrap();

    let result = parse_file(
        Path::new("/nonexistent/file.bin"),
        schema_f.path().to_str().unwrap(),
        &[],
        Some(Path::new("/nonexistent")),
        OutputFormat::Text {
            indent: "  ".into(),
        },
    );
    assert!(result.is_err());
}

#[test]
fn backslash_and_platform_absolute_paths_are_schema_paths() {
    assert!(is_schema_path(r"\\server\share\types\elf"));
    assert!(is_schema_path(r"\types\elf"));
    // An absolute path in the platform's own syntax, such as `C:\...` on Windows.
    let absolute = std::env::temp_dir().join("elf");
    assert!(is_schema_path(&absolute.to_string_lossy()));
}

#[test]
fn file_errors_keep_the_detail_in_their_source_not_in_the_message() {
    use std::error::Error;

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("bad.yaml");
    std::fs::write(&path, "id: t\nseq: [unterminated\n").unwrap();
    let err = load_schema_file(&path).unwrap_err();
    assert!(matches!(err, doe::DoeError::Yaml { .. }), "{err:?}");
    assert_eq!(
        err.to_string(),
        format!("YAML parse error in {}", path.display())
    );
    assert!(err.source().unwrap().to_string().contains("line 2"));

    let missing = load_schema_file(&dir.path().join("none.yaml")).unwrap_err();
    assert_eq!(
        missing.to_string(),
        format!(
            "could not read file {}",
            dir.path().join("none.yaml").display()
        )
    );
    assert!(missing.source().is_some());
}

#[test]
fn sized_sub_types_work_over_a_file() {
    use std::io::Write;

    let dir = tempfile::TempDir::new().unwrap();
    let schema = dir.path().join("framed.yaml");
    std::fs::write(
        &schema,
        "id: framed\nseq:\n  - id: len\n    type: u1\n  - id: body\n    type: rec\n    size: len\n  - id: after\n    type: u1\ntypes:\n  rec:\n    seq:\n      - id: a\n        type: u1\n      - id: rest\n        type: bytes\n        size-eos: true\n",
    )
    .unwrap();
    let data = dir.path().join("framed.bin");
    let mut bytes = vec![5u8];
    bytes.extend_from_slice(&[1, 2, 3, 4, 5]);
    bytes.push(0x7f);
    std::fs::File::create(&data)
        .unwrap()
        .write_all(&bytes)
        .unwrap();
    let json = parse_file(
        &data,
        schema.to_str().unwrap(),
        &[],
        Some(Path::new("/nonexistent/config.toml")),
        OutputFormat::Json,
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["body"]["a"], 1);
    assert_eq!(v["body"]["rest"]["len"], 4);
    assert_eq!(v["after"], 0x7f);
}

#[test]
fn parse_options_default_to_showing_instances() {
    assert!(ParseOptions::default().instances);
    let dir = tempfile::TempDir::new().unwrap();
    let schema = dir.path().join("inst.yaml");
    std::fs::write(
        &schema,
        "id: inst\nseq:\n  - id: a\n    type: u1\ninstances:\n  doubled:\n    value: a * 2\n",
    )
    .unwrap();
    let data = dir.path().join("inst.bin");
    std::fs::write(&data, [21]).unwrap();
    let run = |options: &ParseOptions| {
        parse_file_with(
            &data,
            schema.to_str().unwrap(),
            &[],
            Some(Path::new("/nonexistent/config.toml")),
            OutputFormat::Json,
            options,
        )
        .unwrap()
    };
    let with: serde_json::Value = serde_json::from_str(&run(&ParseOptions::default())).unwrap();
    assert_eq!(with["doubled"], 42);
    let without: serde_json::Value =
        serde_json::from_str(&run(&ParseOptions { instances: false })).unwrap();
    assert!(without.get("doubled").is_none());
    assert_eq!(without["a"], 21);
}

#[test]
fn schema_path_ksy_extension() {
    assert!(is_schema_path("riff.ksy"));
    assert!(is_schema_path("formats/riff.KSY"));
    assert!(!is_schema_path("riff"));
}
