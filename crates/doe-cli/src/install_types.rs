//! `doe --install-types`: writes the type library and the Kaitai Struct
//! gallery that are built into the binary (see `build.rs`) to a directory,
//! `~/.doe/types` by default, where `doe` looks for types without being told.
//!
//! A manifest (`.doe-types`) written beside the files records the version of
//! `doe` and a hash of every file as it was written. A later run uses it to
//! tell a file that is still as `doe` wrote it (replaced by the new version's)
//! from one that the person changed (kept, unless `--force`), and to remove a
//! file that a newer version no longer has. The hash only detects changes; it
//! is not a security measure.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context as _;

include!(concat!(env!("OUT_DIR"), "/types_bundle.rs"));

/// The name of the manifest in the installed directory.
pub(crate) const MANIFEST: &str = ".doe-types";

/// What an installation did, for the report.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Summary {
    /// Files that were not there.
    pub(crate) written: usize,
    /// Files replaced by this version's, because they were as `doe` wrote them
    /// (or with `--force`).
    pub(crate) updated: usize,
    /// Files already as this version has them.
    pub(crate) unchanged: usize,
    /// Files removed because this version no longer has them.
    pub(crate) removed: usize,
    /// Files the person changed, which were left alone (`--force` replaces them).
    pub(crate) kept: Vec<String>,
}

/// Install the types built into this binary into `dir`.
///
/// # Errors
///
/// A build without types in it, a manifest that cannot be read, or a file
/// that cannot be written.
pub(crate) fn run(dir: &Path, force: bool, out: &mut impl Write) -> anyhow::Result<Summary> {
    anyhow::ensure!(
        !TYPES.is_empty(),
        "this build of doe has no types in it: it was built without the `bundled-types` feature, or without the files of types/ and conformance/kaitai_struct_formats"
    );
    let summary = install(dir, TYPES, env!("CARGO_PKG_VERSION"), force)?;
    writeln!(
        out,
        "types in {}: {} written, {} updated, {} unchanged, {} removed",
        dir.display(),
        summary.written,
        summary.updated,
        summary.unchanged,
        summary.removed
    )?;
    if !summary.kept.is_empty() {
        writeln!(
            out,
            "{} changed since they were installed, and kept (--install-types --force replaces them):",
            summary.kept.len()
        )?;
        for name in &summary.kept {
            writeln!(out, "  {name}")?;
        }
    }
    writeln!(
        out,
        "searched after the -I and config paths; `default_types = false` in the config turns that off"
    )?;
    Ok(summary)
}

/// Install `files` (`path`, content) into `dir`; see the module documentation.
pub(crate) fn install(
    dir: &Path,
    files: &[(&str, &[u8])],
    version: &str,
    force: bool,
) -> anyhow::Result<Summary> {
    let manifest_path = dir.join(MANIFEST);
    let old = read_manifest(&manifest_path)?;
    let mut new: BTreeMap<String, String> = BTreeMap::new();
    let mut summary = Summary::default();
    fs::create_dir_all(dir).with_context(|| format!("could not create {}", dir.display()))?;
    for (name, content) in files {
        let path = target(dir, name);
        let wanted = hash(content);
        let current = fs::read(&path).ok().map(|bytes| hash(&bytes));
        match current {
            None => {
                write_file(&path, content)?;
                summary.written += 1;
                new.insert((*name).to_owned(), wanted);
            }
            Some(h) if h == wanted => {
                summary.unchanged += 1;
                new.insert((*name).to_owned(), wanted);
            }
            Some(h) if force || old.get(*name) == Some(&h) => {
                write_file(&path, content)?;
                summary.updated += 1;
                new.insert((*name).to_owned(), wanted);
            }
            Some(_) => {
                summary.kept.push((*name).to_owned());
                // still not as installed: the next run keeps seeing it as the person's
                if let Some(recorded) = old.get(*name) {
                    new.insert((*name).to_owned(), recorded.clone());
                }
            }
        }
    }
    // files of an earlier version that this one no longer has
    for (name, recorded) in &old {
        if files.iter().any(|(n, _)| n == name) {
            continue;
        }
        let path = target(dir, name);
        match fs::read(&path).map(|bytes| hash(&bytes)) {
            Ok(h) if h == *recorded || force => {
                fs::remove_file(&path)
                    .with_context(|| format!("could not remove {}", path.display()))?;
                summary.removed += 1;
            }
            Ok(_) => summary.kept.push(name.clone()),
            Err(_) => {}
        }
    }
    let mut text = format!("doe {version}\n");
    text.extend(new.iter().map(|(name, h)| format!("{h}\t{name}\n")));
    fs::write(&manifest_path, text)
        .with_context(|| format!("could not write {}", manifest_path.display()))?;
    Ok(summary)
}

/// The path of the installed file `name` (which uses `/`) below `dir`.
fn target(dir: &Path, name: &str) -> PathBuf {
    name.split('/')
        .fold(dir.to_path_buf(), |path, part| path.join(part))
}

fn write_file(path: &Path, content: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    fs::write(path, content).with_context(|| format!("could not write {}", path.display()))
}

/// The hashes the manifest at `path` records, by file name; none when there is
/// no manifest.
fn read_manifest(path: &Path) -> anyhow::Result<BTreeMap<String, String>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(e).with_context(|| format!("could not read {}", path.display())),
    };
    Ok(text
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once('\t'))
        .map(|(h, name)| (name.to_owned(), h.to_owned()))
        .collect())
}

/// FNV-1a (64 bits) of `bytes`, in hex.
fn hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(dir: &Path, name: &str) -> String {
        fs::read_to_string(target(dir, name)).unwrap()
    }

    #[test]
    fn writes_files_and_a_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let files: &[(&str, &[u8])] = &[("doe/a.yaml", b"one"), ("kaitai/x/b.ksy", b"two")];
        let summary = install(dir.path(), files, "1.0.0", false).unwrap();
        assert_eq!(
            (summary.written, summary.updated, summary.unchanged),
            (2, 0, 0)
        );
        assert_eq!(read(dir.path(), "kaitai/x/b.ksy"), "two");
        let manifest = read(dir.path(), MANIFEST);
        assert!(manifest.starts_with("doe 1.0.0\n"));
        assert!(manifest.contains("\tdoe/a.yaml"));
    }

    #[test]
    fn a_second_run_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let files: &[(&str, &[u8])] = &[("doe/a.yaml", b"one")];
        install(dir.path(), files, "1.0.0", false).unwrap();
        let summary = install(dir.path(), files, "1.0.0", false).unwrap();
        assert_eq!(
            (summary.written, summary.updated, summary.unchanged),
            (0, 0, 1)
        );
    }

    #[test]
    fn a_file_the_person_changed_is_kept_unless_forced() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &[("doe/a.yaml", b"one")], "1.0.0", false).unwrap();
        fs::write(dir.path().join("doe/a.yaml"), "mine").unwrap();

        let newer: &[(&str, &[u8])] = &[("doe/a.yaml", b"three")];
        let summary = install(dir.path(), newer, "1.1.0", false).unwrap();
        assert_eq!(summary.kept, ["doe/a.yaml"]);
        assert_eq!(read(dir.path(), "doe/a.yaml"), "mine");
        // and it is still the person's on the run after
        let again = install(dir.path(), newer, "1.1.0", false).unwrap();
        assert_eq!(again.kept, ["doe/a.yaml"]);

        let forced = install(dir.path(), newer, "1.1.0", true).unwrap();
        assert_eq!(forced.updated, 1);
        assert_eq!(read(dir.path(), "doe/a.yaml"), "three");
    }

    #[test]
    fn an_unchanged_file_is_replaced_by_the_newer_version() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &[("doe/a.yaml", b"one")], "1.0.0", false).unwrap();
        let summary = install(dir.path(), &[("doe/a.yaml", b"two")], "1.1.0", false).unwrap();
        assert_eq!(summary.updated, 1);
        assert_eq!(read(dir.path(), "doe/a.yaml"), "two");
        assert!(read(dir.path(), MANIFEST).starts_with("doe 1.1.0\n"));
    }

    #[test]
    fn a_file_that_was_there_before_is_the_persons() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("doe")).unwrap();
        fs::write(dir.path().join("doe/a.yaml"), "mine").unwrap();
        let summary = install(dir.path(), &[("doe/a.yaml", b"one")], "1.0.0", false).unwrap();
        assert_eq!(summary.kept, ["doe/a.yaml"]);
        assert_eq!(read(dir.path(), "doe/a.yaml"), "mine");
    }

    #[test]
    fn a_file_a_newer_version_dropped_is_removed_unless_changed() {
        let dir = tempfile::tempdir().unwrap();
        install(
            dir.path(),
            &[("doe/a.yaml", b"one"), ("doe/b.yaml", b"two")],
            "1.0.0",
            false,
        )
        .unwrap();
        fs::write(dir.path().join("doe/b.yaml"), "mine").unwrap();
        let summary = install(dir.path(), &[], "1.1.0", false).unwrap();
        assert_eq!(summary.removed, 1);
        assert!(!dir.path().join("doe/a.yaml").exists());
        assert_eq!(summary.kept, ["doe/b.yaml"]);
        assert!(dir.path().join("doe/b.yaml").exists());
    }

    #[test]
    fn the_hash_tells_contents_apart() {
        assert_eq!(hash(b"a"), hash(b"a"));
        assert_ne!(hash(b"a"), hash(b"b"));
        assert_eq!(hash(b"").len(), 16);
    }
}
