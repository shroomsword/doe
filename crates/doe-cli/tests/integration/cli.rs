use std::path::{Path, PathBuf};
use std::process::Stdio;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

const NO_CONFIG: &str = "/nonexistent/doe-config.yaml";

fn doe() -> Command {
    Command::cargo_bin("doe").unwrap()
}

/// A schema with two integer fields and a binary that matches it.
fn simple_fixture(dir: &Path) -> (PathBuf, PathBuf) {
    let schema = dir.join("simple.yaml");
    std::fs::write(
        &schema,
        "id: simple\nseq:\n  - id: a\n    type: u8\n  - id: b\n    type: u16le\n",
    )
    .unwrap();
    let binary = dir.join("simple.bin");
    std::fs::write(&binary, [0x07, 0x34, 0x12]).unwrap();
    (schema, binary)
}

#[test]
fn text_output_goes_to_stdout() {
    let dir = TempDir::new().unwrap();
    let (schema, binary) = simple_fixture(dir.path());
    doe()
        .args(["-c", NO_CONFIG])
        .arg(&binary)
        .arg(&schema)
        .assert()
        .success()
        .stdout(predicate::str::contains("simple\n"))
        .stdout(predicate::str::contains("a  7"))
        .stdout(predicate::str::contains("b  4660"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn indent_flag_changes_text_indentation() {
    let dir = TempDir::new().unwrap();
    let (schema, binary) = simple_fixture(dir.path());
    doe()
        .args(["-c", NO_CONFIG, "--indent=--"])
        .arg(&binary)
        .arg(&schema)
        .assert()
        .success()
        .stdout(predicate::str::contains("--a  7"));
}

#[test]
fn json_flag_prints_compact_json() {
    let dir = TempDir::new().unwrap();
    let (schema, binary) = simple_fixture(dir.path());
    let assert = doe()
        .args(["-c", NO_CONFIG, "--json"])
        .arg(&binary)
        .arg(&schema)
        .assert()
        .success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["$type"], "simple");
    assert_eq!(parsed["b"], 4660);
    assert!(!out.trim().contains('\n'));
}

#[test]
fn pretty_flag_prints_multiline_json() {
    let dir = TempDir::new().unwrap();
    let (schema, binary) = simple_fixture(dir.path());
    doe()
        .args(["-c", NO_CONFIG, "--pretty"])
        .arg(&binary)
        .arg(&schema)
        .assert()
        .success()
        .stdout(predicate::str::contains("\n  \"a\": 7"));
}

#[test]
fn missing_binary_file_fails_with_exit_code_1() {
    let dir = TempDir::new().unwrap();
    let (schema, _) = simple_fixture(dir.path());
    doe()
        .args(["-c", NO_CONFIG, "/nonexistent/file.bin"])
        .arg(&schema)
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::starts_with("error: could not parse"))
        .stderr(predicate::str::contains("caused by:"));
}

#[test]
fn unknown_type_name_fails_with_exit_code_1() {
    let dir = TempDir::new().unwrap();
    let (_, binary) = simple_fixture(dir.path());
    doe()
        .args(["-c", NO_CONFIG])
        .arg(&binary)
        .arg("no_such_type")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("schema not found"));
}

#[test]
fn missing_arguments_is_a_usage_error() {
    doe()
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Usage:"));
}

#[test]
fn type_name_resolves_through_include_path() {
    let dir = TempDir::new().unwrap();
    let (_, binary) = simple_fixture(dir.path());
    doe()
        .args(["-c", NO_CONFIG, "-I"])
        .arg(dir.path())
        .arg(&binary)
        .arg("simple")
        .assert()
        .success()
        .stdout(predicate::str::contains("b  4660"));
}

#[test]
fn help_lists_no_types_without_include_paths() {
    doe()
        .args(["-c", NO_CONFIG, "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: doe"))
        .stdout(predicate::str::contains(
            "Available types: none (no include paths configured)",
        ));
}

#[test]
fn help_lists_types_on_include_paths() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("mytype.yaml"),
        "id: mytype\ndoc: A test type\nseq: []\n",
    )
    .unwrap();
    doe()
        .args(["-c", NO_CONFIG, "-I"])
        .arg(dir.path())
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Available types:"))
        .stdout(predicate::str::contains("mytype  A test type"));
}

#[test]
fn help_with_duplicate_type_ids_fails() {
    let dir = TempDir::new().unwrap();
    for name in ["one.yaml", "two.yaml"] {
        std::fs::write(dir.path().join(name), "id: dup\nseq: []\n").unwrap();
    }
    doe()
        .args(["-c", NO_CONFIG, "-I"])
        .arg(dir.path())
        .arg("--help")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("duplicate type id 'dup'"));
}

#[test]
fn closed_stdout_exits_successfully_without_panicking() {
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("doe"))
        .args(["-c", NO_CONFIG, "--help"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Close the read end before the program writes its help text.
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "status: {:?}", output.status);
    assert!(output.stderr.is_empty(), "stderr: {:?}", output.stderr);
}

#[test]
fn help_reports_include_paths_without_types() {
    let dir = TempDir::new().unwrap();
    doe()
        .args(["-c", NO_CONFIG, "-I"])
        .arg(dir.path())
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Available types: none found on include paths",
        ));
}

#[test]
fn help_lists_types_that_have_no_doc() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("plain.yaml"), "id: plain\nseq: []\n").unwrap();
    doe()
        .args(["-c", NO_CONFIG, "-I"])
        .arg(dir.path())
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("\n  plain\n"));
}

#[test]
fn invalid_expression_in_schema_fails_instead_of_being_truncated() {
    let dir = TempDir::new().unwrap();
    let schema = dir.path().join("bad.yaml");
    std::fs::write(
        &schema,
        "id: bad\nseq:\n  - id: n\n    type: u8\n  - id: data\n    type: bytes\n    size: \"n = 1\"\n",
    )
    .unwrap();
    let binary = dir.path().join("bad.bin");
    std::fs::write(&binary, [1, 2, 3]).unwrap();
    doe()
        .args(["-c", NO_CONFIG])
        .arg(&binary)
        .arg(&schema)
        .assert()
        .code(1)
        .stderr(predicate::str::contains("unexpected token"));
}
