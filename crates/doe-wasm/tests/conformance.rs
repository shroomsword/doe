//! The calls of the web IDE give what the command line gives, for every format
//! of the conformance suite and the gallery, with their imports.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use doe::{OutputFormat, parse_file};
use serde_json::{Value, json};

fn conformance() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance")
}

/// Every schema file under `dir`, by its path relative to `dir`.
fn schema_files(dir: &Path, root: &Path, out: &mut BTreeMap<String, String>) {
    for entry in std::fs::read_dir(dir).expect("readable").flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != ".git") {
                schema_files(&path, root, out);
            }
        } else if path
            .extension()
            .is_some_and(|ext| ext == "ksy" || ext == "yaml")
        {
            let relative = path.strip_prefix(root).expect("under the root");
            let text = std::fs::read_to_string(&path).expect("readable");
            out.insert(relative.to_string_lossy().replace('\\', "/"), text);
        }
    }
}

/// The gallery formats that have samples, as `(path like "image/png", folder
/// of the samples)`: a folder of samples is one with `.bin` files, at any depth
/// (`serialization/asn1/asn1_der`). The ones that doe does not read yet
/// (`failing` in `gallery_samples.tsv`, each with an issue) are left out.
fn gallery_formats() -> Vec<(String, PathBuf)> {
    let root = conformance().join("samples").join("gallery");
    let table =
        std::fs::read_to_string(conformance().join("gallery_samples.tsv")).expect("the table");
    let failing: Vec<&str> = table
        .lines()
        .filter_map(|line| {
            let mut cells = line.split('\t');
            let name = cells.next()?;
            (cells.next()? == "failing").then_some(name)
        })
        .collect();
    let mut found = Vec::new();
    let mut stack = vec![(root, String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("gallery").flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let path = format!("{prefix}{}", entry.file_name().to_string_lossy());
            let has_bins = std::fs::read_dir(entry.path())
                .expect("dir")
                .flatten()
                .any(|f| f.file_name().to_string_lossy().ends_with(".bin"));
            if !has_bins {
                stack.push((entry.path(), format!("{path}/")));
            } else if !failing.contains(&path.as_str()) {
                found.push((path, entry.path()));
            }
        }
    }
    found
}

/// The formats that have samples: the name, and where its definition is.
fn formats() -> Vec<(String, PathBuf, PathBuf)> {
    let samples = conformance().join("samples");
    let gallery = conformance().join("kaitai_struct_formats");
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&samples).expect("samples").flatten() {
        let name = entry.file_name().into_string().expect("UTF-8");
        if name == "gallery" {
            for (path, folder) in gallery_formats() {
                let definition = gallery.join(format!("{path}.ksy"));
                found.push((format!("gallery/{path}"), folder, definition));
            }
        } else {
            let definition = conformance().join("formats").join(format!("{name}.ksy"));
            found.push((name, entry.path(), definition));
        }
    }
    found.sort();
    found
}

#[test]
fn the_web_calls_give_the_value_of_the_command_line() {
    let gallery = conformance().join("kaitai_struct_formats");
    let mut gallery_files = BTreeMap::new();
    schema_files(&gallery, &gallery, &mut gallery_files);
    let local = conformance().join("formats");
    let mut local_files = BTreeMap::new();
    schema_files(&local, &local, &mut local_files);

    let mut compared = 0usize;
    for (name, samples, definition) in formats() {
        let in_gallery = name.starts_with("gallery/");
        let (root, files) = if in_gallery {
            (&gallery, &gallery_files)
        } else {
            (&local, &local_files)
        };
        let relative = definition
            .strip_prefix(root)
            .expect("under the root")
            .to_string_lossy()
            .replace('\\', "/");
        let schema = files
            .get(&relative)
            .unwrap_or_else(|| panic!("no definition for {name}"));
        let include: Vec<PathBuf> = if in_gallery {
            vec![gallery.clone()]
        } else {
            Vec::new()
        };
        for sample in std::fs::read_dir(&samples).expect("samples").flatten() {
            let path = sample.path();
            if path.extension().is_none_or(|ext| ext != "bin") {
                continue;
            }
            let bytes = std::fs::read(&path).expect("readable");

            let request = json!({
                "schema": schema,
                "files": files,
                "path": relative,
                "include_paths": if in_gallery { vec![""] } else { vec![] },
            });
            let answer: Value =
                serde_json::from_str(&doe_wasm::parse(&request.to_string(), &bytes)).unwrap();
            assert_eq!(
                answer["ok"],
                true,
                "{name}/{}: {}",
                path.display(),
                answer["error"]
            );

            let cli = parse_file(
                &path,
                definition.to_str().expect("UTF-8"),
                &include,
                Some(Path::new("/nonexistent/config.toml")),
                OutputFormat::Json,
            )
            .unwrap_or_else(|e| panic!("{name}: the command line fails: {e}"));
            let cli: Value = serde_json::from_str(&cli).unwrap();
            assert_eq!(answer["value"], cli, "{name}/{}", path.display());
            assert!(answer["trace"]["children"].is_array());
            compared += 1;
        }
    }
    assert!(compared > 40, "only {compared} samples were compared");
}
