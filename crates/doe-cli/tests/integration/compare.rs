//! `doe FILE TYPE --compare OTHER` without `--tui` (#445): the differences as text or JSON, and the exit status.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

const NO_CONFIG: &str = "/nonexistent/doe-config.yaml";

/// A state and config directory for the tests, so that none of them writes to the real home directory (and the log
/// file can be opened).
fn sandbox() -> &'static Path {
    static DIR: OnceLock<TempDir> = OnceLock::new();
    DIR.get_or_init(|| TempDir::new().unwrap()).path()
}

fn doe() -> Command {
    let mut command = Command::cargo_bin("doe").unwrap();
    command
        .env("XDG_STATE_HOME", sandbox().join("state"))
        .env("XDG_CONFIG_HOME", sandbox().join("config"))
        .env("DOE_TYPES_DIR", sandbox().join("no-types"))
        .env_remove("NO_COLOR")
        .args(["-c", NO_CONFIG]);
    command
}

fn types() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../types")
}

/// Two versions of the `ar` sample of the library: the modification time of the first member differs.
fn ar_versions(dir: &Path) -> (PathBuf, PathBuf) {
    let sample = std::fs::read(types().join("samples/sample.a")).unwrap();
    let a = dir.join("a.a");
    let b = dir.join("b.a");
    std::fs::write(&a, &sample).unwrap();
    let newer = String::from_utf8_lossy(&sample).replacen("1700000000", "1700000009", 1);
    std::fs::write(&b, newer.as_bytes()).unwrap();
    (a, b)
}

/// A table of records and files that have the lists matched by index or by a key.
const TABLE: &str = "meta:\n  id: t\n  endian: le\nseq:\n  - id: records\n    type: record\n    repeat: eos\ntypes:\n  record:\n    seq:\n      - id: id\n        type: u1\n      - id: value\n        type: u1\n";

fn table(dir: &Path, first: &[u8], second: &[u8]) -> (PathBuf, PathBuf, PathBuf) {
    let schema = dir.join("t.yaml");
    std::fs::write(&schema, TABLE).unwrap();
    let a = dir.join("a.bin");
    let b = dir.join("b.bin");
    std::fs::write(&a, first).unwrap();
    std::fs::write(&b, second).unwrap();
    (schema, a, b)
}

#[test]
fn two_versions_of_a_library_sample_list_exactly_the_field_that_differs() {
    let dir = TempDir::new().unwrap();
    let (a, b) = ar_versions(dir.path());
    doe()
        .arg("-I")
        .arg(types())
        .arg(&a)
        .arg("ar")
        .arg("--compare")
        .arg(&b)
        .assert()
        .code(1)
        .stdout("members[0].mtime: \"1700000000\" -> \"1700000009\"\n1 of 24 fields differ\n")
        .stderr("");
}

#[test]
fn identical_files_say_so_and_exit_zero() {
    let dir = TempDir::new().unwrap();
    let (a, _) = ar_versions(dir.path());
    doe()
        .arg("-I")
        .arg(types())
        .arg(&a)
        .arg("ar")
        .arg("--compare")
        .arg(&a)
        .assert()
        .success()
        .stdout("identical\n");
}

#[test]
fn the_json_has_the_same_differences_as_the_text_in_the_same_order() {
    let dir = TempDir::new().unwrap();
    let (schema, a, b) = table(
        dir.path(),
        &[1, 10, 2, 20, 3, 30],
        &[1, 11, 2, 20, 3, 31, 4, 40],
    );
    let text = doe()
        .arg(&a)
        .arg(&schema)
        .arg("--compare")
        .arg(&b)
        .assert()
        .code(1);
    let text = String::from_utf8(text.get_output().stdout.clone()).unwrap();
    let json = doe()
        .arg(&a)
        .arg(&schema)
        .arg("--compare")
        .arg(&b)
        .arg("--json")
        .assert()
        .code(1);
    let value: Value = serde_json::from_slice(&json.get_output().stdout).unwrap();
    assert_eq!(value["identical"], false);
    let listed: Vec<String> = value["differences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["path"].as_str().unwrap().to_owned())
        .collect();
    // every path of the JSON is a line of the text, in the same order
    let lines: Vec<&str> = text.lines().collect();
    let mut at = 0;
    for path in &listed {
        let found = lines[at..]
            .iter()
            .position(|l| l.trim_start_matches("+ ").starts_with(path.as_str()))
            .unwrap_or_else(|| panic!("{path} is not in {text}"));
        at += found + 1;
    }
    assert_eq!(
        listed.len(),
        lines.len() - 1,
        "one line each and a summary: {text}"
    );
    assert_eq!(
        value["summary"],
        serde_json::json!({"changed": 3, "fields_a": 11, "fields_b": 14, "only_a": 0, "only_b": 1})
    );
    // pretty is the same data
    let pretty = doe()
        .arg(&a)
        .arg(&schema)
        .arg("--compare")
        .arg(&b)
        .arg("--pretty")
        .assert()
        .code(1);
    let again: Value = serde_json::from_slice(&pretty.get_output().stdout).unwrap();
    assert_eq!(again, value);
}

#[test]
fn what_only_one_file_has_is_one_line_and_a_list_length_by_index_is_one() {
    let dir = TempDir::new().unwrap();
    let (schema, a, b) = table(dir.path(), &[1, 10, 2, 20], &[1, 10, 2, 20, 3, 30]);
    doe()
        .arg(&a)
        .arg(&schema)
        .arg("--compare")
        .arg(&b)
        .assert()
        .code(1)
        .stdout("records: length 2 -> 3\n+ records[2]: only in B (2 fields)\n1 of 11 fields differ · 1 only in B\n");
    // and the other way round
    doe()
        .arg(&b)
        .arg(&schema)
        .arg("--compare")
        .arg(&a)
        .assert()
        .code(1)
        .stdout("records: length 3 -> 2\nrecords[2]: only in A (2 fields)\n1 of 11 fields differ · 1 only in A\n");
}

#[test]
fn compare_by_matches_the_elements_by_a_field() {
    let dir = TempDir::new().unwrap();
    // a record inserted at the front
    let (schema, a, b) = table(dir.path(), &[1, 10, 2, 20], &[9, 90, 1, 10, 2, 20]);
    let by_index = doe()
        .arg(&a)
        .arg(&schema)
        .arg("--compare")
        .arg(&b)
        .assert()
        .code(1);
    let by_index = String::from_utf8(by_index.get_output().stdout.clone()).unwrap();
    assert!(by_index.lines().count() > 3, "{by_index}");
    doe()
        .arg(&a)
        .arg(&schema)
        .arg("--compare")
        .arg(&b)
        .args(["--compare-by", "records=id"])
        .assert()
        .code(1)
        .stdout("+ records[0]: only in B (2 fields)\n0 of 11 fields differ · 1 only in B\n");
    // a field that cannot match is a note on stderr, and the list is matched by index
    doe()
        .arg(&a)
        .arg(&schema)
        .arg("--compare")
        .arg(&b)
        .args(["--compare-by", "records=nothing"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "records: matched by index, not by nothing",
        ));
}

#[test]
fn the_bytes_that_differ_are_listed_on_request() {
    let dir = TempDir::new().unwrap();
    let (a, b) = ar_versions(dir.path());
    let run = |extra: &[&str]| {
        let mut command = doe();
        command
            .arg("-I")
            .arg(types())
            .arg(&a)
            .arg("ar")
            .arg("--compare")
            .arg(&b)
            .args(extra);
        command.assert().code(1)
    };
    run(&["--diff-bytes"]).stdout(predicate::str::contains(
        "bytes A: 0x21..0x22\nbytes B: 0x21..0x22\n",
    ));
    assert!(!String::from_utf8_lossy(&run(&[]).get_output().stdout).contains("bytes A"));
    let json = run(&["--json", "--diff-bytes"]);
    let value: Value = serde_json::from_slice(&json.get_output().stdout).unwrap();
    assert_eq!(
        value["bytes"],
        serde_json::json!({"a": [[33, 34]], "b": [[33, 34]]})
    );
    assert_eq!(
        value["differences"][0]["a_bytes"],
        serde_json::json!([[33, 34]])
    );
    // without the flag the JSON has none
    let plain = run(&["--json"]);
    let value: Value = serde_json::from_slice(&plain.get_output().stdout).unwrap();
    assert!(value.get("bytes").is_none() && value["differences"][0].get("a_bytes").is_none());
}

#[test]
fn the_list_is_cut_but_the_summary_and_the_status_are_whole() {
    let dir = TempDir::new().unwrap();
    let (schema, a, b) = table(dir.path(), &[1, 10, 2, 20, 3, 30], &[1, 11, 2, 21, 3, 31]);
    doe()
        .arg(&a)
        .arg(&schema)
        .arg("--compare")
        .arg(&b)
        .args(["--max-differences", "1"])
        .assert()
        .code(1)
        .stdout("records[0].value: 10 -> 11\n… and 2 more\n3 of 11 fields differ\n");
    let json = doe()
        .arg(&a)
        .arg(&schema)
        .arg("--compare")
        .arg(&b)
        .args(["--json", "--max-differences", "2"])
        .assert()
        .code(1);
    let value: Value = serde_json::from_slice(&json.get_output().stdout).unwrap();
    assert_eq!(value["differences"].as_array().unwrap().len(), 2);
    assert_eq!(
        (value["truncated"].clone(), value["omitted"].clone()),
        (Value::Bool(true), Value::from(1))
    );
    assert_eq!(value["summary"]["changed"], 3);
    // 0 lists them all
    let all = doe()
        .arg(&a)
        .arg(&schema)
        .arg("--compare")
        .arg(&b)
        .args(["--json", "--max-differences", "0"])
        .assert()
        .code(1);
    let value: Value = serde_json::from_slice(&all.get_output().stdout).unwrap();
    assert_eq!(value["differences"].as_array().unwrap().len(), 3);
    assert_eq!(value["truncated"], false);
}

#[test]
fn errors_exit_with_two_and_say_why() {
    let dir = TempDir::new().unwrap();
    let (schema, a, b) = table(dir.path(), &[1, 10], &[1, 10]);
    // a file that is not there
    doe()
        .arg(&a)
        .arg(&schema)
        .arg("--compare")
        .arg(dir.path().join("missing.bin"))
        .assert()
        .code(2)
        .stderr(
            predicate::str::contains("could not compare")
                .and(predicate::str::contains("missing.bin")),
        );
    // a schema that does not compile
    let broken = dir.path().join("broken.yaml");
    std::fs::write(
        &broken,
        "meta:\n  id: t\nseq:\n  - id: x\n    type: nothing\n",
    )
    .unwrap();
    doe()
        .arg(&a)
        .arg(&broken)
        .arg("--compare")
        .arg(&b)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("could not compare"));
    // a file that does not parse with the schema: not compared as far as it was read
    let short = dir.path().join("short.bin");
    std::fs::write(&short, [1]).unwrap();
    doe()
        .arg(&short)
        .arg(&schema)
        .arg("--compare")
        .arg(&short)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("does not parse"));
    // a malformed --compare-by is a usage error, which clap says with 2 as well
    doe()
        .arg(&a)
        .arg(&schema)
        .arg("--compare")
        .arg(&b)
        .args(["--compare-by", "records"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("LIST=FIELD"));
}

#[test]
fn the_flags_of_the_viewer_and_of_the_list_need_what_they_belong_to() {
    let dir = TempDir::new().unwrap();
    let (schema, a, b) = table(dir.path(), &[1, 10], &[1, 10]);
    // --diff-bytes and --max-differences are for the command line, not for --tui, and need --compare
    doe()
        .arg(&a)
        .arg(&schema)
        .arg("--diff-bytes")
        .assert()
        .code(2);
    doe()
        .arg(&a)
        .arg(&schema)
        .arg("--compare")
        .arg(&b)
        .args(["--tui", "--diff-bytes"])
        .assert()
        .code(2);
    // --compare-view is for the viewer
    doe()
        .arg(&a)
        .arg(&schema)
        .arg("--compare")
        .arg(&b)
        .args(["--compare-view", "side-by-side"])
        .assert()
        .code(2);
}

#[test]
fn the_other_commands_keep_one_for_an_error() {
    let dir = TempDir::new().unwrap();
    let (schema, a, _) = table(dir.path(), &[1, 10], &[1, 10]);
    doe()
        .arg(dir.path().join("missing.bin"))
        .arg(&schema)
        .assert()
        .code(1);
    doe()
        .arg(&a)
        .arg(dir.path().join("missing.yaml"))
        .assert()
        .code(1);
}
