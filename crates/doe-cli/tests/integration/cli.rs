use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

const NO_CONFIG: &str = "/nonexistent/doe-config.yaml";

/// A state and config directory shared by the tests, so none of them writes to
/// the real home directory.
fn sandbox() -> &'static Path {
    static DIR: OnceLock<TempDir> = OnceLock::new();
    DIR.get_or_init(|| TempDir::new().unwrap()).path()
}

fn doe() -> Command {
    let mut command = Command::cargo_bin("doe").unwrap();
    command
        .env("XDG_STATE_HOME", sandbox().join("state"))
        .env("XDG_CONFIG_HOME", sandbox().join("config"))
        .env_remove("NO_COLOR");
    command
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
        .env("XDG_STATE_HOME", sandbox().join("state"))
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

// ── Color ─────────────────────────────────────────────────────────────────

fn failing(args: &[&str]) -> assert_cmd::assert::Assert {
    let dir = TempDir::new().unwrap();
    let log = dir.path().join("test.log");
    doe()
        .args(["-c", NO_CONFIG, "--log-file"])
        .arg(&log)
        .args(args)
        .args(["/nonexistent/file.bin", "no_such_type"])
        .assert()
        .code(1)
}

#[test]
fn color_is_off_when_stderr_is_not_a_terminal() {
    failing(&[]).stderr(predicate::str::contains("\u{1b}[").not());
}

#[test]
fn color_always_colors_the_error_label() {
    failing(&["--color", "always"]).stderr(predicate::str::contains("\u{1b}["));
}

#[test]
fn color_never_strips_escapes_even_when_forced_by_other_settings() {
    failing(&["--color", "never"]).stderr(predicate::str::contains("\u{1b}[").not());
}

#[test]
fn invalid_color_value_is_a_usage_error() {
    doe()
        .args(["--color", "rainbow", "x", "y"])
        .assert()
        .code(2);
}

// ── Verbosity ─────────────────────────────────────────────────────────────

#[test]
fn default_shows_errors_but_not_info() {
    failing(&[])
        .stderr(predicate::str::contains("error: could not parse"))
        .stderr(predicate::str::contains("info:").not());
}

#[test]
fn single_v_adds_informational_messages() {
    failing(&["-v"])
        .stderr(predicate::str::contains(
            "info: parsing /nonexistent/file.bin",
        ))
        .stderr(predicate::str::contains("debug:").not());
}

#[test]
fn double_v_adds_debug_messages() {
    failing(&["-vv"]).stderr(predicate::str::contains("debug: config file:"));
}

#[test]
fn single_q_keeps_errors() {
    failing(&["-q"]).stderr(predicate::str::contains("error: could not parse"));
}

#[test]
fn double_q_silences_stderr_completely() {
    failing(&["-qq"]).stderr(predicate::str::is_empty());
}

#[test]
fn quiet_and_verbose_conflict() {
    doe().args(["-q", "-v", "x", "y"]).assert().code(2);
}

// ── Log file ──────────────────────────────────────────────────────────────

fn run_logged(dir: &Path, args: &[&str]) -> String {
    let log = dir.join("doe.log");
    doe()
        .args(["-c", NO_CONFIG, "--log-file"])
        .arg(&log)
        .args(args)
        .args(["/nonexistent/file.bin", "no_such_type"])
        .assert()
        .code(1);
    std::fs::read_to_string(log).unwrap_or_default()
}

#[test]
fn log_file_defaults_to_debug_with_timestamp_level_module_and_message() {
    let dir = TempDir::new().unwrap();
    let text = run_logged(dir.path(), &[]);
    let line = text
        .lines()
        .find(|l| l.contains("config file:"))
        .unwrap_or_else(|| panic!("no debug line in {text:?}"));
    assert!(line.contains(" DEBUG doe "), "{line}");
    assert!(line.starts_with("20") && line.contains('T'), "{line}");
    assert!(text.contains("ERROR"), "{text}");
}

#[test]
fn log_level_flag_limits_the_log_file() {
    let dir = TempDir::new().unwrap();
    let text = run_logged(dir.path(), &["--log-level", "error"]);
    assert!(text.contains("ERROR"), "{text}");
    assert!(!text.contains("DEBUG"), "{text}");
}

#[test]
fn log_level_off_leaves_the_log_file_empty() {
    let dir = TempDir::new().unwrap();
    assert_eq!(run_logged(dir.path(), &["--log-level", "off"]), "");
}

#[test]
fn log_file_is_appended_to_not_truncated() {
    let dir = TempDir::new().unwrap();
    let first = run_logged(dir.path(), &[]);
    let second = run_logged(dir.path(), &[]);
    assert!(second.len() > first.len());
    assert!(second.starts_with(&first));
}

#[test]
fn log_level_from_config_file_applies() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "[log]\nlevel = \"error\"\n").unwrap();
    let log = dir.path().join("doe.log");
    doe()
        .arg("-c")
        .arg(&config)
        .arg("--log-file")
        .arg(&log)
        .args(["/nonexistent/file.bin", "no_such_type"])
        .assert()
        .code(1);
    let text = std::fs::read_to_string(log).unwrap();
    assert!(text.contains("ERROR") && !text.contains("DEBUG"), "{text}");
}

#[test]
fn log_level_flag_overrides_the_config_file() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "[log]\nlevel = \"error\"\n").unwrap();
    let log = dir.path().join("doe.log");
    doe()
        .arg("-c")
        .arg(&config)
        .arg("--log-file")
        .arg(&log)
        .args([
            "--log-level",
            "debug",
            "/nonexistent/file.bin",
            "no_such_type",
        ])
        .assert()
        .code(1);
    assert!(std::fs::read_to_string(log).unwrap().contains("DEBUG"));
}

// Windows has no XDG state directory: the log goes to the local data folder, which an environment variable cannot
// redirect, so this is checked on the platforms that follow XDG.
#[cfg(not(windows))]
#[test]
fn default_log_file_lives_in_the_xdg_state_directory() {
    let state = TempDir::new().unwrap();
    Command::cargo_bin("doe")
        .unwrap()
        .env("XDG_STATE_HOME", state.path())
        .args(["-c", NO_CONFIG, "/nonexistent/file.bin", "no_such_type"])
        .assert()
        .code(1);
    let log = std::fs::read_to_string(state.path().join("doe").join("doe.log")).unwrap();
    assert!(log.contains("ERROR"), "{log}");
}

#[test]
fn unwritable_log_file_is_a_warning_not_a_failure() {
    let dir = TempDir::new().unwrap();
    let blocker = dir.path().join("file");
    std::fs::write(&blocker, "").unwrap();
    let (schema, binary) = simple_fixture(dir.path());
    doe()
        .args(["-c", NO_CONFIG, "--log-file"])
        .arg(blocker.join("sub").join("doe.log"))
        .arg(&binary)
        .arg(&schema)
        .assert()
        .success()
        .stderr(predicate::str::contains("warning: could not open log file"));
}

// ── Configuration file ────────────────────────────────────────────────────

#[test]
fn include_paths_come_from_the_toml_config() {
    let dir = TempDir::new().unwrap();
    let (_, binary) = simple_fixture(dir.path());
    let config = dir.path().join("config.toml");
    std::fs::write(
        &config,
        format!("include_paths = [{:?}]\n", dir.path().display().to_string()),
    )
    .unwrap();
    doe()
        .arg("-c")
        .arg(&config)
        .arg(&binary)
        .arg("simple")
        .assert()
        .success()
        .stdout(predicate::str::contains("b  4660"));
}

#[test]
fn invalid_toml_config_names_the_file_and_line() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "include_paths = [\n  oops\n]\n").unwrap();
    doe()
        .arg("-c")
        .arg(&config)
        .args(["x", "y"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(config.display().to_string()))
        .stderr(predicate::str::contains("line 2"));
}

#[test]
fn unknown_config_key_is_an_error() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "include_path = []\n").unwrap();
    doe()
        .arg("-c")
        .arg(&config)
        .args(["x", "y"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("unknown field"));
}

#[test]
fn old_yaml_config_in_the_default_location_is_ignored() {
    let config_home = TempDir::new().unwrap();
    let dir = TempDir::new().unwrap();
    let (_, binary) = simple_fixture(dir.path());
    let old = config_home.path().join("doe");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(
        old.join("config.yaml"),
        format!("include_paths:\n  - {}\n", dir.path().display()),
    )
    .unwrap();
    doe()
        .env("XDG_CONFIG_HOME", config_home.path())
        .arg(&binary)
        .arg("simple")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("schema not found"));
}

// ── Completions and man page ──────────────────────────────────────────────

#[test]
fn completions_are_generated_for_every_shell() {
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        let assert = doe().args(["completions", shell]).assert().success();
        let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert!(out.contains("doe"), "{shell}: {out}");
        assert!(out.contains("completions"), "{shell}");
        assert!(assert.get_output().stderr.is_empty(), "{shell}");
    }
}

#[test]
fn completions_for_an_unknown_shell_is_a_usage_error() {
    doe()
        .args(["completions", "nushell"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn completions_without_a_shell_is_a_usage_error() {
    doe().arg("completions").assert().code(2);
}

#[test]
fn completions_work_with_global_flags() {
    doe()
        .args(["completions", "bash", "--color", "never", "-q"])
        .assert()
        .success();
}

#[test]
fn man_page_is_roff_and_hidden_from_help() {
    doe()
        .arg("man")
        .assert()
        .success()
        .stdout(predicate::str::contains(".TH doe 1"))
        .stdout(predicate::str::contains(r"include\-path"));
    doe()
        .args(["-c", NO_CONFIG, "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("completions"))
        .stdout(predicate::str::contains("\n  man ").not());
}

#[test]
fn completions_and_man_do_not_create_a_log_file() {
    let state = TempDir::new().unwrap();
    for args in [["completions", "bash"], ["man", ""]] {
        let mut command = Command::cargo_bin("doe").unwrap();
        command.env("XDG_STATE_HOME", state.path());
        command.args(args.iter().filter(|a| !a.is_empty()));
        command.assert().success();
    }
    assert!(!state.path().join("doe").exists());
}

#[test]
fn library_debug_messages_reach_stderr_at_double_v() {
    let dir = TempDir::new().unwrap();
    let (_, binary) = simple_fixture(dir.path());
    doe()
        .args(["-c", NO_CONFIG, "-vv", "-I"])
        .arg(dir.path())
        .arg(&binary)
        .arg("simple")
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "debug: loading schema simple from",
        ))
        .stderr(predicate::str::contains("debug: no config file at"));
}
