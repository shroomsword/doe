//! The worked example of the README's *Building a schema*: the keys that are in the README
//! are pressed in a viewer on a small file, and the schema that is saved is the one the
//! README shows. If the keys or the output change, the README has to.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::string_slice,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe::{OutputFormat, ParseOptions};
use doe_tui::app::{App, Focus};
use doe_tui::load::load;
use doe_tui::{Options, prepare_build};

const README: &str = include_str!("../../../README.md");

/// The fenced block that follows the line `<!-- example: NAME -->` in the README.
fn block(name: &str) -> String {
    let marker = format!("<!-- example: {name} -->");
    let after = README
        .split_once(&marker)
        .unwrap_or_else(|| panic!("the README has no {marker}"))
        .1;
    let mut lines = after.lines().skip_while(|l| !l.starts_with("```")).skip(1);
    let mut out = Vec::new();
    for line in lines.by_ref() {
        if line.starts_with("```") {
            break;
        }
        out.push(line);
    }
    out.join("\n") + "\n"
}

/// Press the keys of one line of the script: spaces separate the groups, which are typed
/// as they are, except `<Enter>`, `<C-u>` and `<Esc>`.
fn press_line(app: &mut App, line: &str) {
    // what follows two spaces is the comment
    let keys = line.split("  ").next().unwrap_or("");
    for group in keys.split_whitespace() {
        let mut rest = group;
        while !rest.is_empty() {
            let (code, modifiers, length) = if rest.starts_with("<Enter>") {
                (KeyCode::Enter, KeyModifiers::NONE, "<Enter>".len())
            } else if rest.starts_with("<Esc>") {
                (KeyCode::Esc, KeyModifiers::NONE, "<Esc>".len())
            } else if rest.starts_with("<C-u>") {
                (KeyCode::Char('u'), KeyModifiers::CONTROL, "<C-u>".len())
            } else {
                let c = rest.chars().next().unwrap();
                (KeyCode::Char(c), KeyModifiers::NONE, c.len_utf8())
            };
            app.on_key(KeyEvent::new(code, modifiers));
            rest = &rest[length..];
        }
    }
}

#[test]
fn the_keys_of_the_readme_build_the_schema_of_the_readme() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("blip.bin");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/example/blip.bin"),
        &file,
    )
    .unwrap();
    let out = dir.path().join("blip.yaml");
    let options = Options {
        file: file.clone(),
        type_or_schema: String::new(),
        include_paths: vec![],
        color: false,
        watch: false,
        theme: None,
        theme_dir: None,
        radix: None,
    };
    let (options, builder) = prepare_build(&options, &out).unwrap();
    let mut app = App::new(file.clone(), load(&options).unwrap());
    app.options = Some(options);
    app.builder = Some(builder);
    app.focus = Focus::Hex;

    for line in block("keys").lines().filter(|l| !l.trim().is_empty()) {
        press_line(&mut app, line);
        assert!(
            app.report.is_none(),
            "{line:?} left the viewer with an error: {:?}",
            app.notice
        );
    }
    assert!(
        app.prompt.is_none(),
        "a question is still open at the end: {:?}",
        app.prompt.as_ref().map(|p| p.kind.label())
    );
    assert!(
        !app.builder.as_ref().unwrap().dirty(),
        "the last key saved it"
    );

    let saved = std::fs::read_to_string(&out).unwrap();
    let schema = saved
        .strip_prefix(doe_tui::builder::HEADER)
        .expect("the file starts with the header");
    assert_eq!(
        schema,
        block("blip.yaml"),
        "the README shows another schema:\n{schema}"
    );

    // and it reads the file as the README says
    let printed = doe::parse_file_with(
        &file,
        &out.to_string_lossy(),
        &[],
        Some(Path::new("/nonexistent/config.toml")),
        OutputFormat::Text {
            indent: "  ".to_owned(),
        },
        &ParseOptions::default(),
    )
    .unwrap();
    assert_eq!(
        printed,
        block("blip.txt"),
        "the README prints another tree:\n{printed}"
    );
}
