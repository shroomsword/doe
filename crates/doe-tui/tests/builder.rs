//! A viewer that was started with `--build`: the empty schema, a draft that is
//! changed by code, and leaving with and without unsaved changes. No terminal.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::app::{App, Focus};
use doe_tui::draft::{Field, Kind};
use doe_tui::load::load;
use doe_tui::ui::{Theme, draw};
use doe_tui::{Options, prepare_build};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

/// A viewer of a file of five bytes that builds `built.yaml` in the same directory.
fn building() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("data.bin");
    std::fs::write(&file, [1, 2, 3, 4, 5]).unwrap();
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
    let (options, builder) = prepare_build(&options, &dir.path().join("built.yaml")).unwrap();
    let mut app = App::new(file, load(&options).unwrap());
    app.options = Some(options);
    app.builder = Some(builder);
    app.focus = Focus::Hex;
    (app, dir)
}

fn field(id: &str, kind: Kind) -> Field {
    Field {
        id: id.to_owned(),
        kind,
    }
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn screen(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(110, 14)).unwrap();
    terminal.draw(|f| draw(f, app, &Theme::new(false))).unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_viewer_starts_on_an_empty_schema_named_for_the_file() {
    let (mut app, dir) = building();
    assert_eq!(app.type_name, "built");
    assert_eq!(app.tree.selected().name, "built");
    assert!(app.error.is_none() && app.report.is_none());
    let shown = screen(&mut app);
    assert!(shown.contains("building"), "{shown}");
    assert!(
        shown.contains("01 02 03 04 05"),
        "the bytes are there: {shown}"
    );
    // nothing is saved
    assert!(!dir.path().join("built.yaml").exists());
}

#[test]
fn a_change_to_the_draft_is_parsed_again_and_shown_as_the_ordinary_viewer_shows_it() {
    let (mut app, _dir) = building();
    app.edit_draft(|d| d.push(field("magic", Kind::Unsigned(2))))
        .unwrap();
    app.edit_draft(|d| d.push(field("rest", Kind::Bytes(3))))
        .unwrap();
    let shown = screen(&mut app);
    assert!(shown.contains("magic"), "{shown}");
    assert!(shown.contains("0x0201") || shown.contains("513"), "{shown}");
    assert!(shown.contains("building · not saved"), "{shown}");
}

#[test]
fn a_field_that_does_not_fit_the_file_is_refused_and_the_schema_and_the_tree_stay() {
    let (mut app, _dir) = building();
    app.edit_draft(|d| d.push(field("a", Kind::Unsigned(2))))
        .unwrap();
    let before = app.builder.as_ref().unwrap().draft.clone();
    let rows = app.tree.rows().len();
    let error = app
        .edit_draft(|d| d.push(field("too_long", Kind::Bytes(10))))
        .unwrap_err();
    assert!(error.contains("does not fit"), "{error}");
    assert_eq!(app.builder.as_ref().unwrap().draft, before);
    assert_eq!(app.tree.rows().len(), rows);
    // a bad name is the draft's refusal
    let error = app
        .edit_draft(|d| d.push(field("Bad", Kind::Unsigned(1))))
        .unwrap_err();
    assert!(error.contains("lower case"), "{error}");
    // and the schema file on disk is the draft as it was
    let builder = app.builder.as_ref().unwrap();
    assert_eq!(
        std::fs::read_to_string(builder.schema_path()).unwrap(),
        builder.draft.to_yaml()
    );
}

#[test]
fn q_leaves_at_once_when_nothing_changed_and_asks_once_when_something_did() {
    let (mut app, _dir) = building();
    press(&mut app, KeyCode::Char('q'));
    assert!(app.quit);

    let (mut app, _dir) = building();
    app.edit_draft(|d| d.push(field("a", Kind::Unsigned(1))))
        .unwrap();
    press(&mut app, KeyCode::Char('q'));
    assert!(!app.quit);
    assert!(app.notice.as_deref().unwrap().contains("not saved"));
    // another key in between: it asks again
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Char('q'));
    assert!(!app.quit);
    press(&mut app, KeyCode::Char('q'));
    assert!(app.quit);
}

#[test]
fn the_ordinary_viewer_has_no_builder_and_leaves_at_once() {
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
    assert!(app.builder.is_none());
    assert!(
        app.edit_draft(|d| d.remove(0))
            .unwrap_err()
            .contains("not building")
    );
    press(&mut app, KeyCode::Char('q'));
    assert!(app.quit);
    assert!(screen(&mut app).contains("parsed"));
}
