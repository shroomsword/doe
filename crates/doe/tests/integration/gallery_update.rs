//! A check, for whoever runs the tests locally, that the git submodule of the
//! Kaitai Struct gallery (`conformance/kaitai_struct_formats`) is at the
//! latest commit of the `master` branch of the gallery.
//!
//! It asks the remote of the submodule, so it needs the network. It is not run
//! in CI (where `CI` is set, and a new commit of the gallery is no reason for a
//! change to fail), and `DOE_SKIP_GALLERY_CHECK=1` turns it off. If the remote
//! cannot be reached it says so and passes.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Set to anything but an empty value to skip the check.
const SKIP_VARIABLE: &str = "DOE_SKIP_GALLERY_CHECK";

/// Where the submodule is, from the root of the repository.
const SUBMODULE: &str = "conformance/kaitai_struct_formats";

/// The name of the submodule in `.gitmodules`.
const SUBMODULE_NAME: &str = "kaitai_struct_formats";

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn is_set(variable: &str) -> bool {
    std::env::var_os(variable).is_some_and(|value| !value.is_empty())
}

/// Run `git` in `dir`, and return what it printed if it succeeded.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        // Never wait for a password.
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The commit that the repository records for the submodule.
fn recorded_commit(root: &Path) -> Option<String> {
    git(root, &["rev-parse", &format!("HEAD:{SUBMODULE}")]).filter(|commit| !commit.is_empty())
}

/// The URL of the submodule, from `.gitmodules`.
fn submodule_url(root: &Path) -> Option<String> {
    git(
        root,
        &[
            "config",
            "-f",
            ".gitmodules",
            "--get",
            &format!("submodule.{SUBMODULE_NAME}.url"),
        ],
    )
    .filter(|url| !url.is_empty())
}

/// The commit that `master` points to at `url`, or `None` if it cannot be asked.
fn remote_master(url: &str) -> Option<String> {
    // Give up on a connection that does not move, rather than wait for it.
    let listing = git(
        Path::new("."),
        &[
            "-c",
            "http.lowSpeedLimit=1000",
            "-c",
            "http.lowSpeedTime=15",
            "ls-remote",
            url,
            "refs/heads/master",
        ],
    )?;
    listing
        .split_whitespace()
        .next()
        .map(str::to_owned)
        .filter(|commit| !commit.is_empty())
}

/// `Ok` if the recorded commit is the one `master` points to.
fn check(recorded: &str, master: &str, url: &str) -> Result<(), String> {
    if recorded == master {
        return Ok(());
    }
    Err(format!(
        "the gallery submodule is at {recorded}, but master of {url} is at {master}.\n\
         To update it:\n  \
         git -C {SUBMODULE} fetch origin master\n  \
         git -C {SUBMODULE} checkout {master}\n  \
         git add {SUBMODULE}\n\
         then run `npm run generate` in conformance/ and look at the diff of expected/.\n\
         (This check does not run in CI; set {SKIP_VARIABLE}=1 to skip it.)"
    ))
}

#[allow(
    clippy::print_stderr,
    reason = "a test that is skipped says why on the terminal"
)]
fn skipped(why: &str) {
    eprintln!("gallery update check skipped: {why}");
}

#[test]
fn the_gallery_submodule_is_the_latest_master() {
    if is_set("CI") {
        return skipped("this is CI");
    }
    if is_set(SKIP_VARIABLE) {
        return skipped(&format!("{SKIP_VARIABLE} is set"));
    }
    let root = repository_root();
    let (Some(recorded), Some(url)) = (recorded_commit(&root), submodule_url(&root)) else {
        return skipped("this is not a git checkout that has the submodule");
    };
    let Some(master) = remote_master(&url) else {
        return skipped(&format!("{url} cannot be reached"));
    };
    if let Err(message) = check(&recorded, &master, &url) {
        panic!("{message}");
    }
}

// ── The pieces of the check, without the network ─────────────────────────

#[test]
fn the_recorded_commit_must_be_the_one_master_has() {
    let message = check("aaa", "bbb", "https://example.org/x.git").unwrap_err();
    assert!(
        message.contains("aaa") && message.contains("bbb"),
        "{message}"
    );
    assert!(message.contains("https://example.org/x.git"), "{message}");
    assert!(
        message.contains("git add conformance/kaitai_struct_formats"),
        "{message}"
    );
    assert!(check("aaa", "aaa", "https://example.org/x.git").is_ok());
}

#[test]
fn master_of_a_repository_is_found_and_a_missing_one_is_not() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path();
    let run = |args: &[&str]| {
        git(
            path,
            &[
                &["-c", "user.name=t", "-c", "user.email=t@example.org"],
                args,
            ]
            .concat(),
        )
    };
    run(&["init", "-q", "-b", "master"]).unwrap();
    run(&["commit", "-q", "--allow-empty", "-m", "first"]).unwrap();
    let head = run(&["rev-parse", "HEAD"]).unwrap();
    let url = format!("file://{}", path.display());
    assert_eq!(remote_master(&url), Some(head));
    run(&["commit", "-q", "--allow-empty", "-m", "second"]).unwrap();
    assert_ne!(remote_master(&url), Some("first".to_owned()));
    assert_eq!(
        remote_master(&format!("file://{}/missing", path.display())),
        None
    );
}
