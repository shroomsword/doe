//! Where you were (#429): the viewer keeps the open nodes, the selection, the cursor and the view of a file and a schema
//! when it is left, and puts them back when the same file is opened with the same schema. A temporary state directory.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::Options;
use doe_tui::app::{App, Focus};
use doe_tui::load::try_load;

const SCHEMA: &str = "meta:\n  id: s\n  endian: le\nseq:\n  - id: count\n    type: u1\n  - id: items\n    type: item\n    repeat: expr\n    repeat-expr: count\n  - id: tail\n    type: u2\ntypes:\n  item:\n    seq:\n      - id: a\n        type: u1\n      - id: b\n        type: u1\n";

/// `count` of 3, three items of two bytes, a tail of two.
const DATA: [u8; 9] = [3, 1, 2, 3, 4, 5, 6, 7, 8];

struct Fixture {
    dir: tempfile::TempDir,
    state: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("s.yaml"), SCHEMA).unwrap();
        std::fs::write(dir.path().join("s.bin"), DATA).unwrap();
        Fixture {
            dir,
            state: tempfile::tempdir().unwrap(),
        }
    }

    fn options(&self, restore: bool) -> Options {
        Options {
            file: self.dir.path().join("s.bin"),
            type_or_schema: self
                .dir
                .path()
                .join("s.yaml")
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
            restore: restore.then(|| self.state.path().to_path_buf()),
            radix: None,
        }
    }

    /// The viewer, started as the program starts it: it is loading, and then the parse arrives.
    fn open(&self) -> App {
        Fixture::open_with(&self.options(true))
    }

    fn open_with(options: &Options) -> App {
        let mut app = App::loading(options.file.clone(), &options.type_or_schema);
        app.options = Some(options.clone());
        app.finish_loading(try_load(options));
        app
    }

    /// The files in the state directory.
    fn records(&self) -> Vec<PathBuf> {
        let dir = self.state.path().join("viewer");
        match std::fs::read_dir(dir) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "toml"))
                .collect(),
            Err(_) => Vec::new(),
        }
    }
}

fn key(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn selected(app: &App) -> Vec<String> {
    app.tree
        .names_to(app.tree.selected_path())
        .into_iter()
        .skip(1)
        .collect()
}

fn open_names(app: &App) -> Vec<String> {
    app.tree
        .rows()
        .iter()
        .filter(|r| r.open)
        .map(|r| r.name.clone())
        .collect()
}

/// Open `items`, then `items[1]`, and select its `b`.
fn go_to_b(app: &mut App) {
    // the rows: s, count, items, tail
    press(app, KeyCode::Down);
    press(app, KeyCode::Down);
    assert_eq!(selected(app), ["items"]);
    press(app, KeyCode::Right);
    press(app, KeyCode::Down);
    press(app, KeyCode::Down);
    assert_eq!(selected(app), ["items", "[1]"]);
    press(app, KeyCode::Right);
    press(app, KeyCode::Down);
    press(app, KeyCode::Down);
    assert_eq!(selected(app), ["items", "[1]", "b"]);
}

#[test]
fn a_viewer_that_is_left_and_opened_again_is_where_it_was() {
    let f = Fixture::new();
    let mut app = f.open();
    assert!(app.notice.is_none(), "nothing to restore the first time");
    go_to_b(&mut app);
    key(&mut app, 'i');
    key(&mut app, 'x');
    key(&mut app, 'D');
    let cursor = app.hex.cursor();
    assert_eq!(cursor, 4, "the bytes follow the field");
    app.save_state();
    assert_eq!(f.records().len(), 1);

    let app = f.open();
    assert_eq!(selected(&app), ["items", "[1]", "b"]);
    assert_eq!(open_names(&app), ["s", "items", "[1]"]);
    assert_eq!(app.hex.cursor(), cursor);
    assert!(app.inspector);
    assert_eq!(app.radixes.global, doe::radix::Radix::Hex);
    assert!(
        !app.tree.docs_shown(),
        "D toggled the descriptions off, as it was left"
    );
    assert_eq!(app.notice.as_deref(), Some("restored where you were"));
}

#[test]
fn the_scroll_the_focus_the_range_and_the_radix_of_a_field_come_back() {
    let f = Fixture::new();
    let mut app = f.open();
    go_to_b(&mut app);
    // the bytes have the keys, a range is begun at the cursor and goes on
    key(&mut app, 'f');
    press(&mut app, KeyCode::Tab);
    key(&mut app, 'v');
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    assert_eq!(app.hex.selected_range(), Some((4, 6)));
    assert_eq!(app.focus, Focus::Hex);
    app.save_state();

    let app = f.open();
    assert_eq!(app.focus, Focus::Hex);
    assert!(app.fullscreen);
    assert_eq!(app.hex.selected_range(), Some((4, 6)));
    assert_eq!(app.hex.cursor(), 6);
}

#[test]
fn what_is_gone_is_skipped_and_the_notice_says_so() {
    let f = Fixture::new();
    let mut app = f.open();
    go_to_b(&mut app);
    app.save_state();
    // the schema no longer has `b`
    std::fs::write(
        f.dir.path().join("s.yaml"),
        SCHEMA.replace("      - id: b\n        type: u1\n", ""),
    )
    .unwrap();
    let app = f.open();
    // `items[1]` is still open and `b` is gone: the selection is not placed
    assert!(open_names(&app).contains(&"items".to_owned()));
    let notice = app.notice.unwrap();
    assert!(
        notice.starts_with("restored where you were (1 places are gone"),
        "{notice}"
    );
}

#[test]
fn a_file_that_changed_is_told_and_the_cursor_stays_inside_it() {
    let f = Fixture::new();
    let mut app = f.open();
    go_to_b(&mut app);
    key(&mut app, 'f');
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::End);
    assert_eq!(app.hex.cursor(), 8);
    app.save_state();
    // a shorter file: the tail is cut, which the schema does not like, but the viewer shows what was read
    std::fs::write(f.dir.path().join("s.bin"), [3, 1, 2, 3, 4, 5]).unwrap();
    let app = f.open();
    assert!(app.hex.cursor() <= 5, "{}", app.hex.cursor());
    let notice = app.notice.unwrap();
    assert!(
        notice.contains("the file has changed") || notice.contains("changed"),
        "{notice}"
    );
}

#[test]
fn a_failed_parse_keeps_the_failed_field_selected() {
    let f = Fixture::new();
    let mut app = f.open();
    go_to_b(&mut app);
    app.save_state();
    // the file is cut inside the second item: the parse fails there
    std::fs::write(f.dir.path().join("s.bin"), [3, 1, 2, 3]).unwrap();
    let app = f.open();
    assert!(app.error.is_some());
    let failed = app.tree.failed_path().unwrap();
    assert_eq!(app.tree.selected_path(), failed.as_slice());
}

#[test]
fn another_type_or_another_file_has_a_record_of_its_own() {
    let f = Fixture::new();
    let mut app = f.open();
    go_to_b(&mut app);
    app.save_state();
    // the same file with a schema that has another path
    let other = f.dir.path().join("other.yaml");
    std::fs::write(&other, SCHEMA).unwrap();
    let mut options = f.options(true);
    options.type_or_schema = other.to_string_lossy().into_owned();
    let app = Fixture::open_with(&options);
    assert!(app.notice.is_none());
    assert_eq!(selected(&app), Vec::<String>::new());
    // another file with the schema
    std::fs::copy(f.dir.path().join("s.bin"), f.dir.path().join("t.bin")).unwrap();
    let mut options = f.options(true);
    options.file = f.dir.path().join("t.bin");
    let app = Fixture::open_with(&options);
    assert!(app.notice.is_none());
}

#[test]
fn a_record_that_is_corrupt_is_ignored() {
    let f = Fixture::new();
    let mut app = f.open();
    go_to_b(&mut app);
    app.save_state();
    for record in f.records() {
        std::fs::write(record, "[[[ not toml at all").unwrap();
    }
    let app = f.open();
    assert!(app.notice.is_none());
    assert!(app.error.is_none());
    assert_eq!(selected(&app), Vec::<String>::new());
}

#[test]
fn nothing_is_read_or_written_without_a_state_directory() {
    let f = Fixture::new();
    let mut app = Fixture::open_with(&f.options(false));
    go_to_b(&mut app);
    app.save_state();
    assert!(f.records().is_empty(), "nothing was kept");
    assert!(!f.state.path().join("viewer").exists());
    // and what was kept is not read
    let mut app = f.open();
    go_to_b(&mut app);
    app.save_state();
    assert_eq!(f.records().len(), 1);
    let app = Fixture::open_with(&f.options(false));
    assert!(app.notice.is_none());
    assert_eq!(selected(&app), Vec::<String>::new());
}

#[test]
fn the_record_holds_no_bytes_of_the_file() {
    let f = Fixture::new();
    std::fs::write(
        f.dir.path().join("s.bin"),
        [3, b'S', b'E', b'C', b'R', b'E', b'T', 8, 9],
    )
    .unwrap();
    let mut app = f.open();
    go_to_b(&mut app);
    app.save_state();
    let record = &f.records()[0];
    let text = std::fs::read_to_string(record).unwrap();
    assert!(!text.contains("SECRET"), "{text}");
    // the paths of the file and the positions are what it holds
    assert!(text.contains("items[1].b"), "{text}");
}

#[test]
fn a_viewer_that_never_parsed_keeps_nothing() {
    let f = Fixture::new();
    let options = f.options(true);
    let app = App::loading(options.file.clone(), &options.type_or_schema);
    app.save_state();
    assert!(f.records().is_empty(), "nothing was kept");
    // a schema that cannot be used
    std::fs::write(
        f.dir.path().join("s.yaml"),
        "seq:\n  - id: a\n    type: nope\n",
    )
    .unwrap();
    let mut app = f.open();
    key(&mut app, 'i');
    app.save_state();
    assert!(f.records().is_empty(), "nothing was kept");
}

#[test]
fn the_records_directory_is_only_made_below_the_one_that_was_given() {
    let f = Fixture::new();
    let mut app = f.open();
    go_to_b(&mut app);
    app.save_state();
    let names: Vec<_> = std::fs::read_dir(f.state.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name())
        .collect();
    assert_eq!(names, [Path::new("viewer").as_os_str()]);
}
