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
        "id: simple\nseq:\n  - id: a\n    type: u1\n  - id: b\n    type: u2le\n",
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
        "id: bad\nseq:\n  - id: n\n    type: u1\n  - id: data\n    type: bytes\n    size: \"n = 1\"\n",
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
        let assert = doe().args(["--completions", shell]).assert().success();
        let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert!(out.contains("doe"), "{shell}: {out}");
        assert!(out.contains("completions"), "{shell}");
        assert!(assert.get_output().stderr.is_empty(), "{shell}");
    }
}

#[test]
fn completions_for_an_unknown_shell_is_a_usage_error() {
    doe()
        .args(["--completions", "nushell"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn completions_without_a_shell_is_a_usage_error() {
    doe().arg("--completions").assert().code(2);
}

#[test]
fn completions_work_with_global_flags() {
    doe()
        .args(["--completions", "bash", "--color", "never", "-q"])
        .assert()
        .success();
}

#[test]
fn completions_and_man_do_not_go_with_parsing_or_each_other() {
    for args in [
        &["--completions", "bash", "file.bin", "t"][..],
        &["--completions", "bash", "--json"],
        &["--completions", "bash", "-I", "dir"],
        &["--completions", "bash", "--man"],
        &["--man", "file.bin"],
        &["--man", "--pretty"],
        &["--man", "--help"],
    ] {
        doe()
            .args(args)
            .assert()
            .code(2)
            .stderr(predicate::str::contains("cannot be used with"));
    }
}

#[test]
fn man_page_is_roff_and_listed_in_help() {
    doe()
        .arg("--man")
        .assert()
        .success()
        .stdout(predicate::str::contains(".TH doe 1"))
        .stdout(predicate::str::contains(r"include\-path"));
    doe()
        .args(["-c", NO_CONFIG, "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--completions <SHELL>"))
        .stdout(predicate::str::contains("--man"))
        .stdout(predicate::str::contains("Commands:").not());
}

#[test]
fn a_type_inside_a_schema_is_named_with_two_colons() {
    let dir = TempDir::new().unwrap();
    let schema = dir.path().join("top.yaml");
    std::fs::write(
        &schema,
        "id: top\nseq:\n  - id: a\n    type: u1\ntypes:\n  part:\n    seq:\n      - id: p\n        type: u2le\n    types:\n      deep:\n        seq:\n          - id: d\n            type: u1\n",
    )
    .unwrap();
    let binary = dir.path().join("top.bin");
    std::fs::write(&binary, [0x07, 0x34, 0x12]).unwrap();
    let with = |type_arg: String| {
        let mut command = doe();
        command
            .args(["-c", NO_CONFIG, "--json"])
            .arg(&binary)
            .arg(type_arg);
        command
    };
    let top = schema.to_str().unwrap();
    with(top.to_owned())
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""a":7"#));
    with(format!("{top}::part"))
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""$type":"top::part""#))
        .stdout(predicate::str::contains(r#""p":13319"#));
    with(format!("{top}::part::deep"))
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""d":7"#));
    // a type that is not there lists the ones that are
    with(format!("{top}::nope"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("nope"))
        .stderr(predicate::str::contains("part::deep"));
}

#[test]
fn the_terminal_viewer_is_in_help_the_man_page_and_the_completions() {
    let text = |args: &[&str]| {
        let assert = doe().args(args).assert().success();
        String::from_utf8(assert.get_output().stdout.clone()).unwrap()
    };
    let help = text(&["-c", NO_CONFIG, "--help"]);
    for flag in ["--tui", "--watch"] {
        assert!(help.contains(flag), "{flag} is not in --help: {help}");
    }
    // roff writes the hyphens of a flag escaped
    let man = text(&["--man"]);
    for flag in [r"\-\-tui", r"\-\-watch"] {
        assert!(man.contains(flag), "{flag} is not in the man page");
    }
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        let completions = text(&["--completions", shell]);
        for flag in ["tui", "watch"] {
            assert!(completions.contains(flag), "{flag} is not in {shell}");
        }
    }
}

#[test]
fn the_schema_page_is_a_page_of_its_own() {
    doe()
        .arg("--man=schema")
        .assert()
        .success()
        .stdout(predicate::str::contains(".TH doe\\-schema 5"))
        .stdout(predicate::str::contains(".SH \"SCHEMA FORMAT\""))
        .stdout(predicate::str::contains(
            ".SH \"KAITAI STRUCT COMPATIBILITY\"",
        ))
        .stdout(predicate::str::contains(".SH \"OUTPUT\"").not());
    doe()
        .arg("--man=doe")
        .assert()
        .success()
        .stdout(predicate::str::contains(".SH \"OUTPUT\""))
        .stdout(predicate::str::contains(".SH \"SCHEMA FORMAT\"").not())
        .stdout(predicate::str::contains("doe\\-schema"));
    doe()
        .arg("--man=nonsense")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("possible values: doe, schema"));
}

/// The pages are valid roff: `groff` with every warning on says nothing. Skipped
/// where `groff` is not installed.
#[test]
fn the_pages_render_without_warnings() {
    use std::io::Write as _;
    use std::process::{Command as Process, Stdio};

    for page in ["doe", "schema"] {
        let output = doe().arg(format!("--man={page}")).output().unwrap();
        assert!(output.status.success());
        let Ok(mut groff) = Process::new("groff")
            .args(["-k", "-t", "-man", "-ww", "-z"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
        else {
            return;
        };
        groff
            .stdin
            .take()
            .unwrap()
            .write_all(&output.stdout)
            .unwrap();
        let result = groff.wait_with_output().unwrap();
        let warnings = String::from_utf8_lossy(&result.stderr);
        assert!(warnings.is_empty(), "{page}: {warnings}");
    }
}

#[test]
fn completions_and_man_do_not_create_a_log_file() {
    let state = TempDir::new().unwrap();
    for args in [&["--completions", "bash"][..], &["--man"]] {
        let mut command = Command::cargo_bin("doe").unwrap();
        command.env("XDG_STATE_HOME", state.path());
        command.args(args);
        command.assert().success();
    }
    assert!(!state.path().join("doe").exists());
}

#[test]
fn a_file_called_completions_or_man_is_parsed() {
    let dir = TempDir::new().unwrap();
    let (_, binary) = simple_fixture(dir.path());
    for name in ["completions", "man"] {
        let renamed = dir.path().join(name);
        std::fs::copy(&binary, &renamed).unwrap();
        doe()
            .args(["-c", NO_CONFIG, "-I"])
            .arg(dir.path())
            .arg(&renamed)
            .arg("simple")
            .assert()
            .success();
    }
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

// ── Reading the input ─────────────────────────────────────────────────────

/// A schema that reads a word at the start of a file, skips a gigabyte and
/// reads the last word, for a file of `GIGS` GiB.
const GIGS: u64 = 6;

fn sparse_fixture(dir: &Path) -> (PathBuf, PathBuf) {
    let size = GIGS << 30;
    let schema = dir.join("sparse.yaml");
    std::fs::write(
        &schema,
        format!(
            "id: sparse\nseq:\n  - id: head\n    type: u4le\n  - id: hole\n    type: bytes\n    size: 1000000000\n  - id: next\n    type: u4le\n  - id: last\n    type: u4le\n    pos: {}\n",
            size - 4
        ),
    )
    .unwrap();
    let binary = dir.join("sparse.bin");
    let file = std::fs::File::create(&binary).unwrap();
    file.set_len(size).unwrap();
    let mut file = file;
    std::io::Write::write_all(&mut file, &0x1111_1111u32.to_le_bytes()).unwrap();
    std::io::Seek::seek(&mut file, std::io::SeekFrom::Start(1_000_000_004)).unwrap();
    std::io::Write::write_all(&mut file, &0x2222_2222u32.to_le_bytes()).unwrap();
    std::io::Seek::seek(&mut file, std::io::SeekFrom::Start(size - 4)).unwrap();
    std::io::Write::write_all(&mut file, &0x3333_3333u32.to_le_bytes()).unwrap();
    (schema, binary)
}

#[cfg(target_os = "linux")]
#[test]
fn a_file_much_larger_than_the_address_space_limit_is_parsed() {
    let dir = TempDir::new().unwrap();
    let (schema, binary) = sparse_fixture(dir.path());
    let doe_path = doe().get_program().to_owned();
    // 2 GiB of address space is far less than the 6 GiB file, so reading it
    // whole could not succeed.
    let output = std::process::Command::new("sh")
        .args(["-c", "ulimit -v 2000000 && exec \"$@\""])
        .arg("sh")
        .arg(doe_path)
        .arg(&binary)
        .arg(&schema)
        .args(["--json", "--config", NO_CONFIG])
        .env("XDG_STATE_HOME", sandbox().join("state"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["head"], 0x1111_1111);
    assert_eq!(json["hole"]["len"], 1_000_000_000);
    assert_eq!(json["hole"]["truncated"], true);
    assert_eq!(json["next"], 0x2222_2222);
    assert_eq!(json["last"], 0x3333_3333);
}

#[test]
fn a_large_bytes_field_is_summarized() {
    let dir = TempDir::new().unwrap();
    let schema = dir.path().join("big.yaml");
    std::fs::write(
        &schema,
        "id: big\nseq:\n  - id: body\n    type: bytes\n    size: 100000\n  - id: tail\n    type: u1\n",
    )
    .unwrap();
    let binary = dir.path().join("big.bin");
    std::fs::write(&binary, vec![0xabu8; 100_001]).unwrap();
    doe()
        .arg(&binary)
        .arg(&schema)
        .args(["--config", NO_CONFIG])
        .assert()
        .success()
        .stdout(predicate::str::contains("body  <100000 bytes at 0x0>"))
        .stdout(predicate::str::contains("tail  171"));
}

#[cfg(unix)]
#[test]
fn input_that_cannot_seek_is_read_in_full() {
    let dir = TempDir::new().unwrap();
    let (schema, _) = simple_fixture(dir.path());
    doe()
        .arg("/dev/stdin")
        .arg(&schema)
        .args(["--config", NO_CONFIG])
        .write_stdin([0x07, 0x34, 0x12])
        .assert()
        .success()
        .stdout(predicate::str::contains("a  7"))
        .stdout(predicate::str::contains("b  4660"));
}

// ── instances ─────────────────────────────────────────────────────────────

fn instances_fixture(dir: &Path) -> (PathBuf, PathBuf) {
    let schema = dir.join("tbl.yaml");
    std::fs::write(
        &schema,
        "id: tbl\nseq:\n  - id: count\n    type: u1\n  - id: table_off\n    type: u1\n  - id: body\n    type: bytes\n    size: body_len\ninstances:\n  body_len:\n    value: count * 2\n  entries:\n    pos: table_off\n    type: u1\n    repeat: expr\n    repeat-expr: count\n  total:\n    value: entries[0] + entries[1]\n",
    )
    .unwrap();
    let binary = dir.join("tbl.bin");
    std::fs::write(&binary, [2, 8, 0xaa, 0xbb, 0xcc, 0xdd, 0, 0, 5, 6]).unwrap();
    (schema, binary)
}

#[test]
fn instances_are_printed_after_the_fields_with_their_offsets() {
    let dir = TempDir::new().unwrap();
    let (schema, binary) = instances_fixture(dir.path());
    let output = doe()
        .arg(&binary)
        .arg(&schema)
        .args(["--config", NO_CONFIG])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let at = |prefix: &str| lines.iter().position(|l| l.starts_with(prefix)).unwrap();
    assert!(at("count") < at("body"));
    assert!(at("body") < at("body_len"));
    assert!(at("body_len") < at("entries @0x8"));
    assert!(at("entries @0x8") < at("total"));
    assert!(text.contains("total  11"), "{text}");
}

#[test]
fn json_output_has_instances_inline_and_their_offsets() {
    let dir = TempDir::new().unwrap();
    let (schema, binary) = instances_fixture(dir.path());
    let output = doe()
        .arg(&binary)
        .arg(&schema)
        .args(["--json", "--config", NO_CONFIG])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["total"], 11);
    assert_eq!(json["entries"], serde_json::json!([5, 6]));
    assert_eq!(json["$offsets"], serde_json::json!({"entries": 8}));
}

#[test]
fn no_instances_leaves_them_out_but_fields_still_get_what_they_use() {
    let dir = TempDir::new().unwrap();
    let (schema, binary) = instances_fixture(dir.path());
    doe()
        .arg(&binary)
        .arg(&schema)
        .args(["--no-instances", "--config", NO_CONFIG])
        .assert()
        .success()
        .stdout(predicate::str::contains("body  <4 bytes>"))
        .stdout(predicate::str::contains("body_len").not())
        .stdout(predicate::str::contains("entries").not())
        .stdout(predicate::str::contains("total").not());
}

// ── --ide ────────────────────────────────────────────────────────────────────

#[cfg(feature = "ide")]
mod ide {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpStream;
    use std::process::Child;

    use super::*;

    /// A running `doe --ide`, stopped when dropped.
    struct Server {
        child: Child,
        port: u16,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn start() -> Server {
        let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("doe"))
            .args(["--ide", "--port", "0"])
            .env("XDG_STATE_HOME", sandbox().join("state"))
            .env("XDG_CONFIG_HOME", sandbox().join("config"))
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let port = line
            .split("http://127.0.0.1:")
            .nth(1)
            .and_then(|rest| rest.split('/').next())
            .and_then(|port| port.parse().ok())
            .unwrap_or_else(|| panic!("no address in {line:?}"));
        Server { child, port }
    }

    fn request(port: u16, head: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.write_all(head.as_bytes()).unwrap();
        let mut answer = Vec::new();
        stream.read_to_end(&mut answer).unwrap();
        String::from_utf8_lossy(&answer).into_owned()
    }

    #[test]
    fn serves_the_page_and_refuses_other_hosts_and_origins() {
        let server = start();
        let port = server.port;
        let page = request(
            port,
            &format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
        );
        assert!(page.starts_with("HTTP/1.1 200 OK"), "{page}");
        assert!(
            page.contains("<!doctype html>") || page.contains("<!DOCTYPE html>"),
            "{page}"
        );

        let wrong_host = request(port, "GET / HTTP/1.1\r\nHost: evil.example\r\n\r\n");
        assert!(wrong_host.starts_with("HTTP/1.1 403"), "{wrong_host}");
        let wrong_origin = request(
            port,
            &format!(
                "GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: http://evil.example\r\n\r\n"
            ),
        );
        assert!(wrong_origin.starts_with("HTTP/1.1 403"), "{wrong_origin}");
    }

    #[test]
    fn the_ide_flag_does_not_mix_with_parsing() {
        doe().args(["--ide", "file", "type"]).assert().failure();
        doe().args(["--ide", "--json"]).assert().failure();
        doe().args(["--ide", "--man"]).assert().failure();
        doe()
            .args(["--port", "80", "file", "type"])
            .assert()
            .failure();
        doe().args(["--open", "file", "type"]).assert().failure();
    }
}

#[cfg(not(feature = "ide"))]
#[test]
fn ide_says_it_is_not_in_this_build() {
    doe()
        .arg("--ide")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not in this build"));
}

// ── --tui ────────────────────────────────────────────────────────────────────

#[test]
fn tui_does_not_mix_with_output_options() {
    for flags in [["--json"], ["--pretty"], ["--no-instances"], ["--ide"]] {
        doe()
            .arg("--tui")
            .args(flags)
            .args(["file", "type"])
            .assert()
            .failure();
    }
}

#[cfg(feature = "tui")]
#[test]
fn tui_without_a_terminal_is_an_error_and_prints_no_escape_codes() {
    let dir = TempDir::new().unwrap();
    let (schema, binary) = simple_fixture(dir.path());
    // assert_cmd runs with pipes: no terminal on stdin or stdout.
    let out = doe()
        .arg("--tui")
        .arg(&binary)
        .arg(&schema)
        .assert()
        .failure()
        .stderr(predicate::str::contains("needs a terminal"))
        .get_output()
        .clone();
    assert!(!out.stdout.contains(&0x1b) && !out.stderr.contains(&0x1b));
    // a bad schema is the ordinary error, with or without a terminal
    doe()
        .arg("--tui")
        .arg(&binary)
        .arg("/nonexistent.yaml")
        .assert()
        .failure();
}

#[cfg(not(feature = "tui"))]
#[test]
fn tui_says_it_is_not_in_this_build() {
    doe()
        .args(["--tui", "file", "type"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not in this build"));
}

#[cfg(all(feature = "tui", unix))]
mod tui_in_a_terminal {
    use std::io::Read;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use portable_pty::{CommandBuilder, PtySize, native_pty_system};

    use super::*;

    const ENTER_SCREEN: &str = "\x1b[?1049h";
    const LEAVE_SCREEN: &str = "\x1b[?1049l";
    const SHOW_CURSOR: &str = "\x1b[?25h";

    /// `doe --tui` running in a pseudo-terminal.
    struct Session {
        child: Box<dyn portable_pty::Child + Send + Sync>,
        writer: Box<dyn std::io::Write + Send>,
        output: mpsc::Receiver<Vec<u8>>,
        seen: String,
        dir: TempDir,
        _master: Box<dyn portable_pty::MasterPty + Send>,
    }

    impl Session {
        fn start(extra: &[&str], env: &[(&str, &str)]) -> Session {
            let dir = TempDir::new().unwrap();
            let (schema, binary) = simple_fixture(dir.path());
            Session::start_in(dir, schema, binary, extra, env)
        }

        /// `doe --tui` on the files of the fixture, with `schema` as the text of the
        /// schema and `data` as the file (none: the file does not exist).
        fn start_with(schema: &str, data: Option<&[u8]>) -> Session {
            Session::start_with_args(schema, data, &[])
        }

        fn start_with_args(schema: &str, data: Option<&[u8]>, extra: &[&str]) -> Session {
            let dir = TempDir::new().unwrap();
            let schema_path = dir.path().join("t.yaml");
            std::fs::write(&schema_path, schema).unwrap();
            let binary = dir.path().join("t.bin");
            if let Some(data) = data {
                std::fs::write(&binary, data).unwrap();
            }
            Session::start_in(dir, schema_path, binary, extra, &[])
        }

        fn start_in(
            dir: TempDir,
            schema: PathBuf,
            binary: PathBuf,
            extra: &[&str],
            env: &[(&str, &str)],
        ) -> Session {
            let pair = native_pty_system()
                .openpty(PtySize {
                    rows: 24,
                    cols: 80,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .unwrap();
            let mut command = CommandBuilder::new(assert_cmd::cargo::cargo_bin("doe"));
            command.args(extra);
            command.arg("--tui");
            if !extra.contains(&"-c") {
                command.args(["-c", NO_CONFIG]);
            }
            command.arg(binary);
            command.arg(schema);
            command.env("XDG_STATE_HOME", sandbox().join("state"));
            command.env("XDG_CONFIG_HOME", sandbox().join("config"));
            command.env_remove("NO_COLOR");
            for (name, value) in env {
                command.env(name, value);
            }
            let child = pair.slave.spawn_command(command).unwrap();
            drop(pair.slave);
            let mut reader = pair.master.try_clone_reader().unwrap();
            let writer = pair.master.take_writer().unwrap();
            let (send, output) = mpsc::channel();
            std::thread::spawn(move || {
                let mut buffer = [0_u8; 4096];
                while let Ok(n) = reader.read(&mut buffer) {
                    if n == 0 || send.send(buffer[..n].to_vec()).is_err() {
                        break;
                    }
                }
            });
            Session {
                child,
                writer,
                output,
                seen: String::new(),
                dir,
                _master: pair.master,
            }
        }

        /// Read until `text` has been printed.
        fn wait_for(&mut self, text: &str) {
            let end = Instant::now() + Duration::from_secs(20);
            while !self.seen.contains(text) {
                let left = end.saturating_duration_since(Instant::now());
                match self.output.recv_timeout(left) {
                    Ok(bytes) => self.seen.push_str(&String::from_utf8_lossy(&bytes)),
                    Err(e) => panic!("never saw {text:?} in {:?}: {e}", self.seen),
                }
            }
        }

        fn send(&mut self, bytes: &[u8]) {
            use std::io::Write as _;
            self.writer.write_all(bytes).unwrap();
            self.writer.flush().unwrap();
        }

        /// Wait for the program to end, and for what it printed to the end.
        fn finish(mut self) -> (bool, String) {
            let end = Instant::now() + Duration::from_secs(20);
            let status = loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    break status;
                }
                assert!(
                    Instant::now() < end,
                    "doe --tui did not end; saw {:?}",
                    self.seen
                );
                std::thread::sleep(Duration::from_millis(20));
            };
            while let Ok(bytes) = self.output.recv_timeout(Duration::from_millis(300)) {
                self.seen.push_str(&String::from_utf8_lossy(&bytes));
            }
            (status.success(), self.seen)
        }
    }

    fn assert_screen_restored(seen: &str) {
        assert!(seen.contains(ENTER_SCREEN), "{seen:?}");
        let left = seen
            .rfind(LEAVE_SCREEN)
            .unwrap_or_else(|| panic!("the screen was not left: {seen:?}"));
        assert!(left > seen.find(ENTER_SCREEN).unwrap());
        assert!(
            seen.rfind(SHOW_CURSOR)
                .is_some_and(|show| show > seen.find(ENTER_SCREEN).unwrap()),
            "{seen:?}"
        );
    }

    #[test]
    fn a_panic_inside_the_viewer_puts_the_terminal_back_before_its_message() {
        let session = Session::start(&[], &[("DOE_TUI_PANIC_FOR_TEST", "1")]);
        let (success, seen) = session.finish();
        assert!(!success);
        assert_screen_restored(&seen);
        // the message is after the screen was left, so it is not lost with the alternate screen
        let message = seen
            .find("DOE_TUI_PANIC_FOR_TEST is set")
            .unwrap_or_else(|| panic!("{seen:?}"));
        assert!(message > seen.rfind(LEAVE_SCREEN).unwrap(), "{seen:?}");
    }

    #[test]
    fn s_lists_the_schemas_and_a_type_is_chosen_from_the_list() {
        let schema = "id: t\nseq:\n  - id: a\n    type: u1\ntypes:\n  part:\n    seq:\n      - id: p\n        type: u1\n";
        let mut session = Session::start_with(schema, Some(&[5]));
        session.wait_for("parsed");
        session.send(b"s");
        session.wait_for("t::part");
        // the selection starts on the type that is parsed (t); the next line is t::part
        session.send(b"j");
        let before = session.seen.len();
        session.send(b"\r");
        // the screen only writes the cells that changed, so the notice comes in pieces
        session.wait_for("as t");
        session.send(b"q");
        let (success, seen) = session.finish();
        assert!(success);
        assert!(
            seen.get(before..).is_some_and(|rest| rest.contains("as t")),
            "{seen:?}"
        );
        assert_screen_restored(&seen);
    }

    #[test]
    fn x_writes_the_integers_in_another_radix_and_radix_starts_the_viewer_with_one() {
        let schema = "id: t\nseq:\n  - id: a\n    type: u1\n";
        let mut session = Session::start_with(schema, Some(&[5]));
        session.wait_for("parsed");
        session.send(b"x");
        session.wait_for("0x05");
        session.send(b"x");
        // the screen writes only the cells that changed, so the digits come in pieces
        session.wait_for("0000101");
        session.send(b"q");
        let (success, seen) = session.finish();
        assert!(success, "{seen:?}");
        assert_screen_restored(&seen);

        let dir = TempDir::new().unwrap();
        let schema = dir.path().join("t.yaml");
        std::fs::write(&schema, "id: t\nseq:\n  - id: a\n    type: u1\n").unwrap();
        let binary = dir.path().join("t.bin");
        std::fs::write(&binary, [5]).unwrap();
        let mut session = Session::start_in(dir, schema, binary, &["--radix", "oct"], &[]);
        session.wait_for("0o005");
        session.send(b"q");
        let (success, seen) = session.finish();
        assert!(success, "{seen:?}");
    }

    #[test]
    fn a_radix_that_is_not_one_is_refused() {
        doe()
            .args(["-c", NO_CONFIG, "--tui", "--radix", "base3", "x", "y"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("unknown radix"))
            .stderr(predicate::str::contains("dec, hex, bin or oct"));
    }

    #[test]
    fn a_type_can_be_named_on_the_command_line_of_the_viewer() {
        let dir = TempDir::new().unwrap();
        let schema = dir.path().join("t.yaml");
        std::fs::write(
            &schema,
            "id: t\nseq:\n  - id: a\n    type: u1\ntypes:\n  part:\n    seq:\n      - id: p\n        type: u1\n",
        )
        .unwrap();
        let binary = dir.path().join("t.bin");
        std::fs::write(&binary, [5]).unwrap();
        let named = PathBuf::from(format!("{}::part", schema.display()));
        let mut session = Session::start_in(dir, named, binary, &[], &[]);
        // it was parsed (a type that is not there would leave with a failure, see below)
        session.wait_for("parsed");
        session.send(b"q");
        let (success, seen) = session.finish();
        assert!(success, "{seen:?}");
        assert!(seen.contains("::part"), "{seen:?}");
    }

    #[test]
    fn a_type_that_is_not_in_the_schema_is_shown_as_the_error_it_is() {
        let dir = TempDir::new().unwrap();
        let (schema, binary) = simple_fixture(dir.path());
        let named = PathBuf::from(format!("{}::nope", schema.display()));
        let mut session = Session::start_in(dir, named, binary, &[], &[]);
        session.wait_for("nope");
        session.send(b"q");
        let (success, seen) = session.finish();
        // the viewer opens with the error (as for any schema that cannot be used) and the exit status says so
        assert!(!success, "{seen:?}");
        assert!(seen.contains("no type nope"), "{seen:?}");
    }

    #[test]
    fn q_leaves_and_the_terminal_is_as_it_was() {
        let mut session = Session::start(&[], &[]);
        session.wait_for("q quit");
        session.send(b"q");
        let (success, seen) = session.finish();
        assert!(success);
        assert_screen_restored(&seen);
    }

    #[test]
    fn ctrl_c_leaves_too() {
        let mut session = Session::start(&[], &[]);
        session.wait_for("q quit");
        session.send(&[0x03]);
        let (success, seen) = session.finish();
        assert!(success);
        assert_screen_restored(&seen);
    }

    #[test]
    fn a_hangup_leaves_with_the_terminal_put_back() {
        let mut session = Session::start(&[], &[]);
        session.wait_for("q quit");
        // What the terminal closing does to the program.
        session.child.kill().unwrap();
        let (_, seen) = session.finish();
        assert_screen_restored(&seen);
    }

    #[test]
    fn the_type_and_the_parse_are_shown() {
        let mut session = Session::start(&[], &[]);
        session.wait_for("simple");
        session.wait_for("parsed");
        session.send(b"q");
        assert!(session.finish().0);
    }

    #[test]
    fn the_bytes_are_shown_next_to_the_tree_and_tab_moves_the_keys_to_them() {
        let mut session = Session::start(&[], &[]);
        // a terminal of 24 lines by 80 columns: the bytes are below the tree
        session.wait_for("07");
        session.wait_for("34");
        session.send(b"\t");
        session.send(b"l");
        session.send(b"q");
        let (success, seen) = session.finish();
        assert!(success);
        assert_screen_restored(&seen);
        // the hex pane's offset column and the bytes of the fixture file are on the screen
        assert!(seen.contains("00000000"), "{seen:?}");
        // the cursor moved to byte 1, which is the field `b`: the title of the bytes says so
        assert!(seen.contains("bytes"), "{seen:?}");
    }

    #[test]
    fn help_search_goto_and_a_paste_work_in_the_terminal() {
        let mut session = Session::start(&[], &[]);
        session.wait_for("07");
        // the help, closed by the next key
        session.send(b"?");
        session.wait_for("Anywhere");
        session.send(b"x");
        // a search for a field name, the text arriving as a bracketed paste
        session.send(b"/");
        session.wait_for("/");
        session.send(b"\x1b[200~b\x1b[201~");
        session.send(b"\r");
        // a jump to an offset
        session.send(b"g");
        session.wait_for("goto offset: ");
        session.send(b"0x2\r");
        // the bar says where the field `b` is, after the jump
        session.wait_for("simple/b");
        session.send(b"q");
        let (success, seen) = session.finish();
        assert!(success);
        assert_screen_restored(&seen);
        // bracketed paste was switched on while the viewer was open and off after it
        let on = seen.find("\x1b[?2004h").expect("bracketed paste on");
        let off = seen.rfind("\x1b[?2004l").expect("bracketed paste off");
        assert!(on < off, "{seen:?}");
    }

    #[test]
    fn a_parse_that_fails_is_shown_and_the_exit_code_says_so() {
        // `b` needs two bytes and there is one
        let mut session = Session::start_with(
            "id: rec\nseq:\n  - id: a\n    type: u1\n  - id: b\n    type: u2le\n",
            Some(&[7, 1]),
        );
        session.wait_for("the parse failed");
        session.wait_for("error");
        session.send(b"q");
        let (success, seen) = session.finish();
        assert!(!success, "the parse failed: the exit code must say so");
        assert_screen_restored(&seen);
        // after the viewer, on the normal screen, the message stays for the person to read
        let after = seen.rsplit("\x1b[?1049l").next().unwrap();
        assert!(after.contains("the parse failed at rec/b"), "{after:?}");
    }

    #[test]
    fn a_schema_that_does_not_compile_is_shown_and_the_viewer_stays_open() {
        let mut session = Session::start_with(
            "id: bad\nseq:\n  - id: a\n    type: nope\n",
            Some(&[1, 2, 3]),
        );
        session.wait_for("cannot be parsed");
        session.wait_for("t.yaml:4:11");
        session.send(b"q");
        let (success, seen) = session.finish();
        assert!(!success);
        assert_screen_restored(&seen);
        assert!(seen.contains("nope"), "{seen:?}");
    }

    #[test]
    fn a_missing_file_is_shown_and_the_viewer_stays_open() {
        let mut session = Session::start_with("id: ok\nseq:\n  - id: a\n    type: u1\n", None);
        session.wait_for("cannot be parsed");
        session.send(b"\t"); // keys work: nothing panics without a file
        session.send(b"q");
        let (success, seen) = session.finish();
        assert!(!success);
        assert_screen_restored(&seen);
    }

    #[test]
    fn a_parse_that_works_leaves_with_a_success_code() {
        let mut session = Session::start(&[], &[]);
        session.wait_for("parsed");
        session.send(b"q");
        assert!(session.finish().0);
    }

    const ONE_FIELD: &str = "id: rec\nseq:\n  - id: first\n    type: u1\n";
    const TWO_FIELDS: &str =
        "id: rec\nseq:\n  - id: first\n    type: u1\n  - id: added\n    type: u1\n";

    #[test]
    fn r_reads_the_schema_again() {
        let mut session = Session::start_with(ONE_FIELD, Some(&[1, 2]));
        session.wait_for("first");
        std::fs::write(session.dir.path().join("t.yaml"), TWO_FIELDS).unwrap();
        session.send(b"r");
        session.wait_for("added");
        session.send(b"q");
        let (success, seen) = session.finish();
        assert!(success, "the last read worked");
        assert_screen_restored(&seen);
    }

    #[test]
    fn a_reload_that_fails_keeps_the_tree_and_the_exit_says_so() {
        let mut session = Session::start_with(ONE_FIELD, Some(&[1, 2]));
        session.wait_for("first");
        std::fs::write(
            session.dir.path().join("t.yaml"),
            "id: rec\nseq:\n  - id: first\n    type: nope\n",
        )
        .unwrap();
        session.send(b"r");
        session.wait_for("could not read again");
        session.send(b"q");
        let (success, _) = session.finish();
        assert!(!success);
    }

    #[test]
    fn watch_reads_the_schema_again_when_it_changes_on_disk() {
        let mut session = Session::start_with_args(ONE_FIELD, Some(&[1, 2]), &["--watch"]);
        session.wait_for("first");
        std::fs::write(session.dir.path().join("t.yaml"), TWO_FIELDS).unwrap();
        // no key: the viewer notices by itself, a second or so later
        session.wait_for("added");
        session.send(b"q");
        let (success, seen) = session.finish();
        assert!(success);
        assert_screen_restored(&seen);
    }

    #[test]
    fn watch_without_tui_is_refused() {
        doe().args(["--watch", "file", "type"]).assert().failure();
    }

    #[test]
    fn the_first_screen_is_up_while_the_file_is_parsed() {
        let mut session = Session::start(&[], &[]);
        // the screen with the message is drawn before the parse is done, then the tree
        session.wait_for("parsing");
        session.wait_for("parsed");
        session.send(b"q");
        assert!(session.finish().0);
    }

    /// What `doe --tui` printed with these options until it showed its first screen.
    fn first_screen(extra: &[&str], env: &[(&str, &str)]) -> String {
        let mut session = Session::start(extra, env);
        session.wait_for("q quit");
        session.send(b"q");
        session.finish().1
    }

    fn has_colour(seen: &str) -> bool {
        seen.contains("\x1b[38;") || (30..38).any(|n| seen.contains(&format!("\x1b[{n}m")))
    }

    #[test]
    fn colour_follows_no_color_and_the_color_option() {
        assert!(has_colour(&first_screen(&[], &[])));
        // NO_COLOR is honoured (by doe, and by the terminal library as well)
        assert!(!has_colour(&first_screen(&[], &[("NO_COLOR", "1")])));
        assert!(!has_colour(&first_screen(&["--color", "never"], &[])));
        assert!(has_colour(&first_screen(&["--color", "always"], &[])));
    }

    // The escape codes that ask for a colour, in the three depths. Nord: the
    // background #2e3440 (46, 52, 64) and the text #d8dee9 (216, 222, 233).
    const NORD_BACKGROUND: &str = "48;2;46;52;64";

    #[test]
    fn a_theme_is_drawn_with_its_colours_at_the_depth_of_the_terminal() {
        let truecolor = [("COLORTERM", "truecolor"), ("TERM", "xterm-256color")];
        let seen = first_screen(&["--theme", "nord"], &truecolor);
        assert!(seen.contains(NORD_BACKGROUND), "{seen:?}");
        assert!(seen.contains("38;2;216;222;233"), "{seen:?}");
        // the name may be written as it is in the table
        let seen = first_screen(&["--theme", "Nord"], &truecolor);
        assert!(seen.contains(NORD_BACKGROUND), "{seen:?}");

        // 256 colours: indexed colours and no 24-bit ones
        let seen = first_screen(
            &["--theme", "nord"],
            &[("COLORTERM", ""), ("TERM", "xterm-256color")],
        );
        assert!(indexed(&seen).iter().any(|&n| n >= 16), "{seen:?}");
        assert!(
            !seen.contains("38;2;") && !seen.contains("48;2;"),
            "{seen:?}"
        );

        // 16 colours: no extended colours at all
        let seen = first_screen(
            &["--theme", "nord"],
            &[("COLORTERM", ""), ("TERM", "xterm")],
        );
        assert!(
            !seen.contains("38;2;") && !seen.contains("48;2;"),
            "{seen:?}"
        );
        // (the bright colours are written by the terminal library as 8 to 15 of the 256)
        assert!(indexed(&seen).iter().all(|&n| n < 16), "{seen:?}");
        assert!(has_colour(&seen), "{seen:?}");
    }

    /// The numbers of the colours that are asked for as `38;5;N` or `48;5;N`.
    fn indexed(seen: &str) -> Vec<u32> {
        let mut numbers = Vec::new();
        for prefix in ["38;5;", "48;5;"] {
            for part in seen.split(prefix).skip(1) {
                let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
                numbers.extend(digits.parse::<u32>());
            }
        }
        numbers
    }

    #[test]
    fn terminal_is_the_terminals_own_colours_and_no_color_wins_over_a_theme() {
        let truecolor = [("COLORTERM", "truecolor")];
        let seen = first_screen(&["--theme", "terminal"], &truecolor);
        assert!(
            !seen.contains("48;2;") && !seen.contains("38;2;"),
            "{seen:?}"
        );
        assert!(has_colour(&seen));
        assert!(!has_colour(&first_screen(
            &["--theme", "nord", "--color", "never"],
            &truecolor
        )));
        assert!(!has_colour(&first_screen(
            &["--theme", "nord"],
            &[("COLORTERM", "truecolor"), ("NO_COLOR", "1")]
        )));
    }

    #[test]
    fn a_theme_that_is_not_there_is_refused_before_the_screen_is_touched() {
        let session = Session::start(&["--theme", "nope"], &[]);
        let (success, seen) = session.finish();
        assert!(!success);
        assert!(seen.contains("nope"), "{seen:?}");
        assert!(
            seen.contains("solarized-dark"),
            "the themes there are: {seen:?}"
        );
        assert!(!seen.contains(ENTER_SCREEN), "{seen:?}");
    }

    /// A config file with `theme = "<name>"` and a theme `mine` next to it.
    fn config_with_theme(name: &str) -> (TempDir, String) {
        let dir = TempDir::new().unwrap();
        let config = dir.path().join("config.toml");
        std::fs::write(&config, format!("theme = \"{name}\"\n")).unwrap();
        let themes = dir.path().join("themes");
        std::fs::create_dir(&themes).unwrap();
        let mut theme = String::from("name = \"Mine\"\n[colors]\n");
        for (key, color) in [
            ("background", "#010203"),
            ("foreground", "#fafbfc"),
            ("muted", "#888888"),
            ("line", "#444444"),
            ("panel", "#111111"),
            ("accent", "#66ccff"),
            ("selection", "#223344"),
            ("selection_part", "#112233"),
            ("error", "#ff8888"),
            ("warning", "#ffcc66"),
            ("number", "#ffaa44"),
            ("string", "#88ee88"),
            ("bytes", "#ee8844"),
            ("enum", "#cc88ff"),
            ("boolean", "#66eeee"),
            ("offset", "#888888"),
            ("type", "#66ccff"),
            ("instance", "#888888"),
        ] {
            std::fmt::Write::write_fmt(&mut theme, format_args!("{key} = \"{color}\"\n")).unwrap();
        }
        std::fs::write(themes.join("mine.toml"), theme).unwrap();
        let path = config.to_string_lossy().into_owned();
        (dir, path)
    }

    #[test]
    fn the_config_file_chooses_a_theme_and_the_flag_overrides_it() {
        let truecolor = [("COLORTERM", "truecolor")];
        let (_dir, config) = config_with_theme("nord");
        let seen = first_screen(&["-c", &config], &truecolor);
        assert!(seen.contains(NORD_BACKGROUND), "{seen:?}");
        let seen = first_screen(&["-c", &config, "--theme", "dracula"], &truecolor);
        assert!(!seen.contains(NORD_BACKGROUND), "{seen:?}");
        assert!(
            seen.contains("48;2;40;42;54"),
            "dracula's #282a36: {seen:?}"
        );
    }

    #[test]
    fn a_theme_in_the_directory_next_to_the_config_file_is_found_by_its_name() {
        let truecolor = [("COLORTERM", "truecolor")];
        let (_dir, config) = config_with_theme("mine");
        let seen = first_screen(&["-c", &config], &truecolor);
        assert!(seen.contains("48;2;1;2;3"), "#010203: {seen:?}");
        // and a theme file by its path
        let (dir, config) = config_with_theme("nord");
        let file = dir.path().join("themes/mine.toml");
        let seen = first_screen(
            &["-c", &config, "--theme", &file.to_string_lossy()],
            &truecolor,
        );
        assert!(seen.contains("48;2;1;2;3"), "{seen:?}");
    }

    #[test]
    fn a_theme_key_that_is_not_a_theme_is_refused_with_the_name_of_the_file() {
        let (dir, config) = config_with_theme("nope");
        let session = Session::start(&["-c", &config], &[]);
        let (success, seen) = session.finish();
        drop(dir);
        assert!(!success);
        assert!(seen.contains("nope"), "{seen:?}");
        assert!(!seen.contains(ENTER_SCREEN), "{seen:?}");
    }
}
