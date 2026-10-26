//! The table of the Kaitai Struct gallery (`conformance/gallery_samples.tsv`):
//! a row for every definition, the files it is tested with, and the numbers
//! that the README and the CI log show. The files themselves are parsed and
//! compared with the reference compiler's output by `conformance.rs`.

#![allow(
    clippy::print_stderr,
    reason = "the test prints the numbers, as the CI log shows them"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::conformance::{conformance_dir, gallery_dir};

struct Row {
    status: String,
    sample: String,
    source: String,
    licence: String,
    note: String,
}

fn table() -> BTreeMap<String, Row> {
    let text = std::fs::read_to_string(conformance_dir().join("gallery_samples.tsv")).unwrap();
    let mut lines = text.lines();
    assert_eq!(
        lines.next().unwrap(),
        "format\tstatus\tsample\tsource\tlicence\tnote"
    );
    lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let cells: Vec<&str> = line.split('\t').collect();
            assert_eq!(cells.len(), 6, "a row has six cells: {line:?}");
            (
                cells[0].to_owned(),
                Row {
                    status: cells[1].to_owned(),
                    sample: cells[2].to_owned(),
                    source: cells[3].to_owned(),
                    licence: cells[4].to_owned(),
                    note: cells[5].to_owned(),
                },
            )
        })
        .collect()
}

/// The definitions of the gallery, without `.ksy`, as the table names them.
fn definitions() -> BTreeSet<String> {
    let root = gallery_dir();
    let mut found = BTreeSet::new();
    let mut stack: Vec<PathBuf> = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name.starts_with('_') {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "ksy") {
                let relative = path.strip_prefix(&root).unwrap().with_extension("");
                found.insert(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found
}

fn has_samples(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|e| e.path().extension().is_some_and(|x| x == "bin"))
    })
}

/// The line that the README shows, and the CI log.
fn summary(rows: &BTreeMap<String, Row>) -> String {
    let count = |status: &str| rows.values().filter(|r| r.status == status).count();
    format!(
        "{} definitions: {} with a file written by its own tool, {} with a file built by hand, {} failing on a file, {} part of another, {} without a sample",
        rows.len(),
        count("real"),
        count("crafted"),
        count("failing"),
        count("part-of"),
        count("none"),
    )
}

#[test]
fn every_definition_of_the_gallery_has_a_row_and_every_row_a_definition() {
    if !gallery_dir().join("README.md").exists() {
        eprintln!("the gallery is not checked out: nothing to check");
        return;
    }
    let rows = table();
    let defined = definitions();
    let named: BTreeSet<String> = rows.keys().cloned().collect();
    let missing: Vec<_> = defined.difference(&named).collect();
    let gone: Vec<_> = named.difference(&defined).collect();
    assert!(
        missing.is_empty() && gone.is_empty(),
        "run `python3 conformance/gallery_table.py`: definitions without a row {missing:?}, rows without a definition {gone:?}"
    );
}

#[test]
fn a_row_says_what_it_is_tested_with_and_where_the_files_come_from() {
    let samples = conformance_dir().join("samples").join("gallery");
    let rows = table();
    for (format, row) in &rows {
        match row.status.as_str() {
            "real" | "crafted" | "failing" => {
                assert_eq!(
                    row.sample, *format,
                    "{format}: the samples are in the directory of the format"
                );
                assert!(
                    has_samples(&samples.join(&row.sample)),
                    "{format}: no .bin in samples/gallery/{}",
                    row.sample
                );
                assert!(
                    row.source != "-" && !row.source.is_empty(),
                    "{format}: where do the files come from?"
                );
                assert!(
                    row.licence != "-" && !row.licence.is_empty(),
                    "{format}: what allows the files to be here?"
                );
            }
            "part-of" => {
                assert!(
                    row.note.starts_with("imported by "),
                    "{format}: which definitions import it?"
                );
                assert_eq!(row.sample, "-", "{format}");
            }
            "none" => assert_eq!(
                row.sample, "-",
                "{format}: a row without a sample names none"
            ),
            other => panic!(
                "{format}: `{other}` is not a status (real, crafted, failing, part-of, none)"
            ),
        }
    }
    // every directory of samples has its row
    let mut stack = vec![(samples.clone(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !entry.path().is_dir() {
                continue;
            }
            let path = format!("{prefix}{name}");
            if has_samples(&entry.path()) {
                let row = rows
                    .get(&path)
                    .unwrap_or_else(|| panic!("samples/gallery/{path} has no row"));
                assert!(
                    matches!(row.status.as_str(), "real" | "crafted" | "failing"),
                    "{path}: it has samples, so it is real, crafted or failing"
                );
            } else {
                stack.push((entry.path(), format!("{path}/")));
            }
        }
    }
}

/// The table of directories and the formats that are tested, as the README
/// shows it (`gallery_table.py --readme` writes it).
fn readme_block(rows: &BTreeMap<String, Row>) -> String {
    let mut by_dir: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (name, row) in rows {
        if matches!(row.status.as_str(), "real" | "crafted")
            && let Some((directory, leaf)) = name.split_once('/')
        {
            by_dir.entry(directory).or_default().push(leaf);
        }
    }
    let mut lines = vec![
        "<!-- gallery-formats:begin -->".to_owned(),
        "| Directory | Formats |".to_owned(),
        "| --- | --- |".to_owned(),
    ];
    for (directory, mut formats) in by_dir {
        formats.sort_unstable();
        let formats: Vec<String> = formats.iter().map(|f| format!("`{f}`")).collect();
        lines.push(format!("| `{directory}` | {} |", formats.join(", ")));
    }
    lines.push("<!-- gallery-formats:end -->".to_owned());
    lines.join("\n")
}

#[test]
fn the_formats_that_are_tested_are_in_the_readme() {
    let rows = table();
    // on Windows the checkout has CRLF line ends
    let readme = std::fs::read_to_string(conformance_dir().join("../README.md"))
        .unwrap()
        .replace("\r\n", "\n");
    let block = readme_block(&rows);
    assert!(
        readme.contains(&block),
        "the table of formats in the README is out of date; run `python3 conformance/gallery_table.py --readme`. It should be:\n{block}"
    );
}

#[test]
fn the_numbers_of_the_gallery_are_in_the_readme_and_in_the_ci_log() {
    let rows = table();
    let line = summary(&rows);
    eprintln!("gallery: {line}");
    if let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        use std::io::Write as _;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
        {
            drop(writeln!(file, "### Kaitai Struct gallery\n\n{line}\n"));
        }
    }
    let readme = std::fs::read_to_string(conformance_dir().join("../README.md")).unwrap();
    assert!(
        readme.contains(&line),
        "the README does not have the current numbers; its text should be:\n{line}"
    );
}
