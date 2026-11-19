//! Matching the elements of a list by a key field in the terminal viewer's comparison (#444): `--compare-by`, `#`, the note
//! of a key that cannot be used, and the choice that is kept for the next time. On the test backend.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::app::App;
use doe_tui::load::try_load;
use doe_tui::{Comparing, Options};

/// A table of records up to the end of the file: an id, a letter and a value.
const SCHEMA: &str = "meta:\n  id: t\n  endian: le\nseq:\n  - id: records\n    type: record\n    repeat: eos\ntypes:\n  record:\n    seq:\n      - id: id\n        type: u1\n      - id: letter\n        type: u1\n      - id: value\n        type: u1\n";

/// ids 1 to 3.
const A: [u8; 9] = [1, b'a', 10, 2, b'b', 20, 3, b'c', 30];
/// the same with a record at the front.
const B: [u8; 12] = [9, b'z', 90, 1, b'a', 10, 2, b'b', 20, 3, b'c', 30];

struct Fixture {
    dir: tempfile::TempDir,
    by: Vec<(String, String)>,
    keep: bool,
}

impl Fixture {
    fn new(a: &[u8], b: &[u8]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("t.yaml"), SCHEMA).unwrap();
        std::fs::write(dir.path().join("a.bin"), a).unwrap();
        std::fs::write(dir.path().join("b.bin"), b).unwrap();
        Fixture {
            dir,
            by: Vec::new(),
            keep: false,
        }
    }

    fn options(&self) -> Options {
        Options {
            file: self.dir.path().join("a.bin"),
            type_or_schema: self
                .dir
                .path()
                .join("t.yaml")
                .to_string_lossy()
                .into_owned(),
            include_paths: Vec::new(),
            color: false,
            watch: false,
            theme: None,
            theme_dir: None,
            config_file: None,
            outer_file: None,
            theme_on_command_line: false,
            transparent: None,
            mouse: true,
            footer: true,
            compare: Some(Comparing {
                file: self.dir.path().join("b.bin"),
                side_by_side: false,
                by: self.by.clone(),
            }),
            restore: self.keep.then(|| self.dir.path().join("state")),
            radix: None,
        }
    }

    fn open(&self) -> App {
        let options = self.options();
        let mut app = App::loading(options.file.clone(), &options.type_or_schema);
        app.options = Some(options.clone());
        app.finish_loading(try_load(&options));
        let mut other = options.clone();
        other
            .file
            .clone_from(&options.compare.as_ref().unwrap().file);
        app.finish_compare(try_load(&other));
        app
    }
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

/// `#` on the list, the field typed, Enter.
fn match_by(app: &mut App, field: &str) {
    press(app, KeyCode::Char('j'));
    press(app, KeyCode::Char('#'));
    // the field that is proposed is cleared first
    app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    type_text(app, field);
    press(app, KeyCode::Enter);
}

fn summary(app: &App) -> String {
    app.compare_summary().unwrap()
}

#[test]
fn by_index_an_inserted_record_shifts_every_later_one() {
    let app = Fixture::new(&A, &B).open();
    let s = app.compare.as_ref().unwrap().comparison.summary;
    assert!(s.changed > 3, "{s:?}");
    assert!(!summary(&app).contains("matched by key"));
}

#[test]
fn compare_by_on_the_command_line_pairs_the_records_by_the_field() {
    let mut fixture = Fixture::new(&A, &B);
    fixture.by = vec![("records".to_owned(), "id".to_owned())];
    let mut app = fixture.open();
    let c = app.compare.as_ref().unwrap();
    assert_eq!(c.comparison.summary.only_b, 1);
    assert_eq!(c.comparison.summary.changed, 0);
    assert!(
        summary(&app).contains("matched by key: records by id"),
        "{}",
        summary(&app)
    );
    // what only B has is a row under the list, once the list is open
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('l'));
    assert!(
        app.tree.rows().iter().any(|r| r.name == "[0]"),
        "the ghost row"
    );
}

#[test]
fn the_key_asked_for_with_a_hash_matches_the_list_and_says_so() {
    let mut app = Fixture::new(&A, &B).open();
    match_by(&mut app, "id");
    assert_eq!(app.compare_keys.get("records"), Some("id"));
    let c = app.compare.as_ref().unwrap();
    assert_eq!(
        c.comparison.summary.differences(),
        1,
        "{:?}",
        c.comparison.summary
    );
    assert!(
        app.notice.as_deref().unwrap().contains("records by id"),
        "{:?}",
        app.notice
    );
    assert!(summary(&app).contains("records by id"));
}

#[test]
fn nothing_typed_goes_back_to_the_index_even_when_the_schema_has_a_key() {
    let mut fixture = Fixture::new(&A, &B);
    fixture.by = vec![("records".to_owned(), "id".to_owned())];
    let mut app = fixture.open();
    assert_eq!(app.compare.as_ref().unwrap().comparison.summary.changed, 0);
    match_by(&mut app, "");
    assert_eq!(app.compare_keys.get("records"), Some(""));
    assert!(app.compare.as_ref().unwrap().comparison.summary.changed > 3);
    assert!(
        app.notice.as_deref().unwrap().contains("by index"),
        "{:?}",
        app.notice
    );
}

#[test]
fn a_field_that_cannot_match_is_said_and_the_list_is_matched_by_index() {
    let b = [1, b'a', 10, 1, b'b', 20];
    let mut app = Fixture::new(&A, &b).open();
    match_by(&mut app, "nothing");
    let notice = app.notice.clone().unwrap();
    assert!(
        notice.contains("no field nothing") && notice.contains("by index"),
        "{notice}"
    );
    assert!(app.compare.as_ref().unwrap().comparison.a.keyed.is_empty());
}

#[test]
fn the_choice_is_kept_for_the_schema_and_used_the_next_time() {
    let mut fixture = Fixture::new(&A, &B);
    fixture.keep = true;
    let mut app = fixture.open();
    match_by(&mut app, "id");
    drop(app);
    // another viewer on the same schema starts with it
    let again = fixture.open();
    assert_eq!(again.compare_keys.get("records"), Some("id"));
    assert_eq!(again.compare.as_ref().unwrap().comparison.summary.only_b, 1);
    // an option on the command line beats what was kept
    let mut other = fixture;
    other.by = vec![("records".to_owned(), "letter".to_owned())];
    let app = other.open();
    assert_eq!(app.compare_keys.get("records"), Some("letter"));
}

#[test]
fn the_key_is_asked_for_on_a_list_only() {
    let mut app = Fixture::new(&A, &B).open();
    // the root is selected: nothing to match
    press(&mut app, KeyCode::Char('#'));
    assert!(app.prompt.is_none());
    assert!(
        app.notice.as_deref().unwrap().contains("select a list"),
        "{:?}",
        app.notice
    );
    // and not while the other file is the current one
    press(&mut app, KeyCode::Char('V'));
    press(&mut app, KeyCode::Char('#'));
    assert!(app.prompt.is_none());
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("go back to the first file"),
        "{:?}",
        app.notice
    );
}

#[test]
fn a_hash_without_a_comparison_says_so() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("t.yaml"), SCHEMA).unwrap();
    std::fs::write(dir.path().join("a.bin"), A).unwrap();
    let mut options = Fixture::new(&A, &B).options();
    options.compare = None;
    options.file = dir.path().join("a.bin");
    options.type_or_schema = dir.path().join("t.yaml").to_string_lossy().into_owned();
    let mut app = App::loading(options.file.clone(), &options.type_or_schema);
    app.options = Some(options.clone());
    app.finish_loading(try_load(&options));
    press(&mut app, KeyCode::Char('#'));
    assert!(
        app.notice.as_deref().unwrap().contains("not comparing"),
        "{:?}",
        app.notice
    );
}
