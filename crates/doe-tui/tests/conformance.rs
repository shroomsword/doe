//! The tree and the bytes agree on the conformance samples: the field of a byte
//! holds that byte, and a field that is selected is where the pane scrolls to.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use std::path::{Path, PathBuf};

use doe::parser::trace::TraceNode;
use doe_tui::Options;
use doe_tui::bytes::FileBytes;
use doe_tui::hex_view::{Cover, HexState, Selection};
use doe_tui::load::load;
use doe_tui::tree::TreeModel;

fn conformance() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance")
}

/// The formats that have samples: the name, the folder of the samples, the
/// definition, and where its imports are looked for.
fn formats() -> Vec<(String, PathBuf, PathBuf, Vec<PathBuf>)> {
    let samples = conformance().join("samples");
    let gallery = conformance().join("kaitai_struct_formats");
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&samples).expect("samples").flatten() {
        let name = entry.file_name().into_string().expect("UTF-8");
        if name == "gallery" {
            for dir in std::fs::read_dir(entry.path()).expect("gallery").flatten() {
                let dir_name = dir.file_name().into_string().expect("UTF-8");
                for format in std::fs::read_dir(dir.path()).expect("dir").flatten() {
                    let format_name = format.file_name().into_string().expect("UTF-8");
                    found.push((
                        format!("gallery/{dir_name}/{format_name}"),
                        format.path(),
                        gallery.join(&dir_name).join(format!("{format_name}.ksy")),
                        vec![gallery.clone()],
                    ));
                }
            }
        } else {
            let definition = conformance().join("formats").join(format!("{name}.ksy"));
            found.push((name, entry.path(), definition, Vec::new()));
        }
    }
    found.sort();
    found
}

/// Every node of the trace, with its path, and whether the bytes cannot lead to
/// it: it is inside an instance (a byte selects the field of the sequence) or
/// inside a transformed field (its offsets are of other bytes).
fn walk(
    node: &TraceNode,
    path: &mut Vec<usize>,
    out: &mut Vec<(Vec<usize>, bool)>,
    processed: bool,
) {
    let processed = processed || node.processed || node.is_instance;
    out.push((path.clone(), processed));
    for (index, child) in node.children.iter().enumerate() {
        path.push(index);
        walk(child, path, out, processed);
        path.pop();
    }
}

#[test]
fn the_tree_and_the_bytes_agree_on_the_conformance_samples() {
    let mut samples_checked = 0usize;
    let mut bytes_checked = 0usize;
    let mut nodes_checked = 0usize;
    for (name, folder, definition, include_paths) in formats() {
        for sample in std::fs::read_dir(&folder).expect("samples").flatten() {
            let file = sample.path();
            if file.extension().is_none_or(|ext| ext != "bin") {
                continue;
            }
            let label = format!("{name}/{}", file.file_name().unwrap().to_string_lossy());
            let loaded = load(&Options {
                file: file.clone(),
                type_or_schema: definition.to_string_lossy().into_owned(),
                include_paths: include_paths.clone(),
                color: false,
                watch: false,
            })
            .unwrap_or_else(|e| panic!("{label}: {e}"));
            let size = loaded.size;
            let mut tree = TreeModel::new(loaded.trace, loaded.value);
            let mut file_bytes = FileBytes::open(&file).unwrap();
            samples_checked += 1;

            // the field of a byte holds the byte, and no child of it does
            let stride = (size / 400).max(1);
            let mut offset = 0;
            while offset < size {
                if let Some(path) = tree.find_byte(offset) {
                    let node = tree.node(&path).unwrap();
                    assert!(
                        node.span.unwrap().contains_byte(offset),
                        "{label}: byte {offset} not in {}",
                        node.name
                    );
                    assert!(
                        !node.children.iter().any(|c| !c.processed
                            && !c.is_instance
                            && c.span.is_some_and(|s| s.contains_byte(offset))),
                        "{label}: byte {offset}: {} is not the innermost",
                        node.name
                    );
                    // selecting it marks the byte in the pane
                    let processed = node.processed;
                    tree.select_path(&path);
                    assert_eq!(
                        tree.selected_path(),
                        path.as_slice(),
                        "{label}: byte {offset}"
                    );
                    if !processed {
                        let selection = Selection::of(tree.selected()).unwrap();
                        assert_ne!(selection.cover(offset), Cover::No, "{label}: byte {offset}");
                    }
                    bytes_checked += 1;
                }
                offset += stride;
            }

            // a field that is selected is where the pane shows
            let mut nodes = Vec::new();
            walk(tree.node(&[]).unwrap(), &mut Vec::new(), &mut nodes, false);
            let step = (nodes.len() / 300).max(1);
            for (path, processed) in nodes.iter().step_by(step) {
                tree.select_path(path);
                let row = tree.selected().clone();
                if *processed || row.instance {
                    continue;
                }
                let Some(selection) = Selection::of(&row) else {
                    continue;
                };
                if selection.first_byte() >= size {
                    continue;
                }
                let mut state = HexState::new();
                state.reveal(selection.first_byte(), size);
                assert_eq!(
                    state.cursor(),
                    selection.first_byte(),
                    "{label}: {}",
                    row.name
                );
                let per_line = state.bytes_per_line() as u64;
                let lines = state.page_lines() as u64;
                assert!(
                    state.top() <= selection.first_byte()
                        && selection.first_byte() < state.top() + lines * per_line,
                    "{label}: {} at {} is not in the lines from {}",
                    row.name,
                    selection.first_byte(),
                    state.top()
                );
                // and the byte it starts at belongs to it or to something inside it
                let found = tree
                    .find_byte(selection.first_byte())
                    .expect("a field holds the byte");
                let inner = tree.node(&found).unwrap();
                assert!(
                    inner.span.unwrap().contains_byte(selection.first_byte()),
                    "{label}: {}",
                    row.name
                );
                // the pane can read the bytes it shows
                let window = file_bytes.window(state.top(), 16);
                assert!(!window.is_empty(), "{label}: {}", row.name);
                nodes_checked += 1;
            }
        }
    }
    assert!(samples_checked > 20, "{samples_checked} samples");
    assert!(bytes_checked > 500, "{bytes_checked} bytes");
    assert!(nodes_checked > 200, "{nodes_checked} nodes");
}
