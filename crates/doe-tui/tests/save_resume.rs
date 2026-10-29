//! Saving the schema that was built, and going on with it later. No terminal.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe::{OutputFormat, ParseOptions};
use doe_tui::app::{App, Focus};
use doe_tui::draft::{Count, Field, Kind};
use doe_tui::load::load;
use doe_tui::{Options, prepare_build};

const DATA: [u8; 16] = [
    0x7f,
    0x45,
    3,
    1,
    0,
    2,
    0,
    3,
    0,
    b'a',
    b'b',
    b'c',
    0,
    0b1010_0011,
    9,
    10,
];

fn options(dir: &Path) -> Options {
    let file = dir.join("data.bin");
    std::fs::write(&file, DATA).unwrap();
    Options {
        file,
        type_or_schema: String::new(),
        include_paths: vec![],
        color: false,
        watch: false,
        theme: None,
        theme_dir: None,
        radix: None,
    }
}

/// A viewer that builds `out`, started the way `run_build` starts it (a draft that was read back has the
/// lengths of this file set).
fn building(dir: &Path, out: &Path) -> Result<App, String> {
    let (options, mut builder) = prepare_build(&options(dir), out).map_err(|e| e.to_string())?;
    let loaded = load(&options).map_err(|e| e.to_string())?;
    builder.draft.fit_to_trace(&loaded.trace);
    let mut app = App::new(options.file.clone(), loaded);
    app.options = Some(options);
    app.builder = Some(builder);
    app.focus = Focus::Hex;
    Ok(app)
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn key(app: &mut App, c: char) {
    press(app, KeyCode::Char(c));
}

fn add(app: &mut App, id: &str, kind: Kind) {
    app.edit_draft(|d| {
        d.push(Field {
            id: id.to_owned(),
            kind,
        })
    })
    .unwrap();
}

fn build_everything(app: &mut App) {
    add(app, "magic", Kind::Contents(vec![0x7f, 0x45]));
    add(app, "count", Kind::Unsigned(1));
    add(
        app,
        "items",
        Kind::Repeat {
            inner: Box::new(Kind::Unsigned(2)),
            count: Count::Field("count".to_owned()),
            sample: 3,
        },
    );
    add(
        app,
        "label",
        Kind::Terminated {
            terminator: 0,
            encoding: "ASCII".to_owned(),
            sample: 4,
        },
    );
    add(app, "flags", Kind::Bits(3));
    add(app, "mode", Kind::Bits(5));
    add(
        app,
        "tail",
        Kind::Repeat {
            inner: Box::new(Kind::Unsigned(1)),
            count: Count::ToEnd,
            sample: 2,
        },
    );
}

fn doe_output(file: &Path, schema: &Path) -> String {
    doe::parse_file_with(
        file,
        &schema.to_string_lossy(),
        &[],
        Some(Path::new("/nonexistent/config.toml")),
        OutputFormat::JsonPretty,
        &ParseOptions::default(),
    )
    .unwrap()
}

#[test]
fn w_saves_the_schema_and_then_the_viewer_leaves_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("saved.yaml");
    let mut app = building(dir.path(), &out).unwrap();
    add(&mut app, "a", Kind::Unsigned(1));
    key(&mut app, 'q');
    assert!(!app.quit);
    assert!(app.notice.as_deref().unwrap().contains("w saves it"));
    key(&mut app, 'w');
    assert!(
        app.notice.as_deref().unwrap().contains("saved"),
        "{:?}",
        app.notice
    );
    assert!(app.notice.as_deref().unwrap().contains("(1 field)"));
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.starts_with("# Made with doe --tui --build"), "{text}");
    assert!(
        text.contains("id: saved") && text.contains("- id: a"),
        "{text}"
    );
    assert!(!app.builder.as_ref().unwrap().dirty());
    key(&mut app, 'q');
    assert!(app.quit);
    // nothing but the schema and the file is left in the directory
    let mut names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["data.bin", "saved.yaml"]);
    // and it is saved again after a change
    add(&mut app, "b", Kind::Unsigned(1));
    key(&mut app, 'w');
    assert!(std::fs::read_to_string(&out).unwrap().contains("- id: b"));
}

#[test]
fn the_schema_that_was_saved_reads_the_file_as_the_viewer_did() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("rich.yaml");
    let mut app = building(dir.path(), &out).unwrap();
    build_everything(&mut app);
    key(&mut app, 'w');
    let builder = app.builder.as_ref().unwrap();
    let viewer = doe_output(&app.options.as_ref().unwrap().file, &builder.schema_path());
    let saved = doe_output(&app.options.as_ref().unwrap().file, &out);
    assert_eq!(saved, viewer);
    assert!(saved.contains("\"abc\""), "{saved}");
}

#[test]
fn a_saved_schema_is_gone_on_with_and_a_change_is_saved_over_it() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("rich.yaml");
    let mut first = building(dir.path(), &out).unwrap();
    build_everything(&mut first);
    key(&mut first, 'w');
    let before = first.builder.as_ref().unwrap().draft.clone();
    drop(first);

    let mut again = building(dir.path(), &out).unwrap();
    let builder = again.builder.as_ref().unwrap();
    assert_eq!(builder.draft, before, "the lengths come from the file");
    assert!(!builder.dirty(), "nothing has changed yet");
    key(&mut again, 'q');
    assert!(again.quit, "leaves at once");
    // the tree is the one of the saved schema
    assert!(again.error.is_none() && again.report.is_none());
    assert_eq!(again.type_name, "rich");
    // go on: change a field, and save over the file
    let mut again = building(dir.path(), &out).unwrap();
    again.tree.select_path(&[6]);
    again.focus = Focus::Tree;
    key(&mut again, 'r');
    again.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    for c in "rest".chars() {
        key(&mut again, c);
    }
    press(&mut again, KeyCode::Enter);
    assert!(again.builder.as_ref().unwrap().dirty());
    key(&mut again, 'w');
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(
        text.contains("- id: rest") && !text.contains("- id: tail"),
        "{text}"
    );
    assert!(text.starts_with("# Made with doe"), "{text}");
}

#[test]
fn a_schema_that_the_builder_did_not_make_is_refused_with_the_line() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("theirs.yaml");
    std::fs::write(
        &out,
        "meta:\n  id: theirs\n  endian: le\nseq:\n  - id: a\n    type: u1\ninstances:\n  x:\n    value: 1\n",
    )
    .unwrap();
    let error = building(dir.path(), &out).unwrap_err();
    assert!(
        error.contains("cannot be gone on with") && error.contains("line 7"),
        "{error}"
    );
    // a file that is not a schema at all
    std::fs::write(&out, "just text").unwrap();
    assert!(building(dir.path(), &out).unwrap_err().contains("line 1"));
}

#[test]
fn a_file_that_appeared_while_building_is_not_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("late.yaml");
    let mut app = building(dir.path(), &out).unwrap();
    add(&mut app, "a", Kind::Unsigned(1));
    std::fs::write(&out, "someone else's").unwrap();
    key(&mut app, 'w');
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("it will not replace it"),
        "{:?}",
        app.notice
    );
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "someone else's");
    assert!(app.builder.as_ref().unwrap().dirty());
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_is_not_written_through() {
    let dir = tempfile::tempdir().unwrap();
    let target: PathBuf = dir.path().join("target.yaml");
    let out = dir.path().join("link.yaml");
    let mut app = building(dir.path(), &out).unwrap();
    add(&mut app, "a", Kind::Unsigned(1));
    std::os::unix::fs::symlink(&target, &out).unwrap();
    key(&mut app, 'w');
    assert!(
        app.notice.as_deref().unwrap().contains("symbolic link"),
        "{:?}",
        app.notice
    );
    assert!(!target.exists());
}

#[test]
fn w_says_so_when_nothing_is_being_built() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("t.yaml"),
        "id: t\nseq:\n  - id: a\n    type: u1\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("t.bin"), [1]).unwrap();
    let options = Options {
        file: dir.path().join("t.bin"),
        type_or_schema: dir.path().join("t.yaml").to_string_lossy().into_owned(),
        include_paths: vec![],
        color: false,
        watch: false,
        theme: None,
        theme_dir: None,
        radix: None,
    };
    let mut app = App::new(options.file.clone(), load(&options).unwrap());
    key(&mut app, 'w');
    assert!(app.notice.as_deref().unwrap().contains("--build"));
}
