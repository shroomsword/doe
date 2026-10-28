//! Detection against the samples of the Kaitai Struct gallery: for each sample,
//! the schema it was made for should be found among the whole gallery, and
//! where another one that also parses it is chosen, the test says so.

#![allow(
    clippy::print_stderr,
    clippy::case_sensitive_file_extension_comparisons,
    reason = "the test reports what it found, and the samples are all .bin"
)]

use std::path::{Path, PathBuf};

use doe::detect::{Library, Options};

use super::conformance::{conformance_dir, gallery_dir};

/// Every directory of the gallery that has definitions, and the gallery itself
/// (its `imports:` are written from the root).
fn include_paths() -> Vec<PathBuf> {
    let root = gallery_dir();
    let mut paths = vec![root.clone()];
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.path().is_dir() && !name.starts_with('.') && !name.starts_with('_') {
                paths.push(entry.path());
                stack.push(entry.path());
            }
        }
    }
    paths.sort();
    paths
}

/// `(format path like "image/png", sample file)` for every gallery sample.
fn samples() -> Vec<(String, PathBuf)> {
    let root = conformance_dir().join("samples").join("gallery");
    let mut out = Vec::new();
    let mut stack = vec![(root, String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.path().is_dir() {
                stack.push((entry.path(), format!("{prefix}{name}/")));
            } else if name.ends_with(".bin") {
                out.push((prefix.trim_end_matches('/').to_owned(), entry.path()));
            }
        }
    }
    out.sort();
    out
}

/// How many of the samples find their own schema at least. The number is
/// tracked: raise it when more are found, and a fall is a regression. (The
/// samples that are not found are those whose format has no signature that the
/// schema states; every new sample of such a format adds to them, so their
/// number says nothing.)
const LEAST_FOUND: usize = 106;

#[test]
fn the_schema_of_each_gallery_sample_is_found_among_the_whole_gallery() {
    if !gallery_dir().join("README.md").exists() {
        eprintln!("the gallery is not checked out: nothing to detect against");
        return;
    }
    let library = Library::load(&include_paths()).unwrap();
    let mut not_found = Vec::new();
    let mut other = Vec::new();
    let mut right = 0;
    let all = samples();
    assert!(all.len() >= 10, "{} samples", all.len());
    for (format, sample) in &all {
        let wanted = Path::new(format)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        // the files are named by what they are: the extension is not given away
        let found = library.detect(sample, &Options::default()).unwrap();
        match found.chosen() {
            Some(chosen) if chosen.name == wanted => right += 1,
            Some(chosen) => other.push(format!(
                "{}: {} fits, not {wanted} (tried {})",
                sample.display(),
                chosen.name,
                found
                    .attempts
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            None => not_found.push(format!(
                "{}: {wanted}\n{}",
                sample.display(),
                found.report()
            )),
        }
    }
    eprintln!(
        "{right} samples of {} found their own schema; {} another that parses them; {} none",
        all.len(),
        other.len(),
        not_found.len()
    );
    for line in &other {
        eprintln!("  another: {line}");
    }
    for line in &not_found {
        eprintln!("  not found: {line}");
    }
    assert!(
        right >= LEAST_FOUND,
        "only {right} samples found their own schema (at least {LEAST_FOUND} did):
{}",
        not_found.join(
            "
"
        )
    );
}
