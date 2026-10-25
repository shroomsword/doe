//! The list of the schemas in the viewer: opening it, reading a schema, choosing
//! a type to parse with, and what the screen shows. No terminal.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::Options;
use doe_tui::app::App;
use doe_tui::load::{load, try_load};
use doe_tui::schemas::Row;
use doe_tui::ui::{Theme, draw};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

const TOP: &str = "meta:
  id: top
  endian: le
  imports:
    - lib
seq:
  - id: a
    type: u1
types:
  chunk:
    seq:
      - id: n
        type: u2
";

const LIB: &str = "meta:
  id: lib
seq: []
types:
  piece:
    seq:
      - id: p
        type: u1
";

struct Fixture {
    _dir: tempfile::TempDir,
    top: PathBuf,
    file: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let top = dir.path().join("top.yaml");
    std::fs::write(&top, TOP).unwrap();
    std::fs::write(dir.path().join("lib.yaml"), LIB).unwrap();
    let file = dir.path().join("data.bin");
    std::fs::write(&file, [7, 1, 0, 9]).unwrap();
    Fixture {
        _dir: dir,
        top,
        file,
    }
}

fn options(f: &Fixture, type_or_schema: &str) -> Options {
    Options {
        file: f.file.clone(),
        type_or_schema: type_or_schema.to_owned(),
        include_paths: vec![],
        color: false,
        watch: false,
        theme: None,
        theme_dir: None,
    }
}

fn app(f: &Fixture, type_or_schema: &str) -> App {
    let options = options(f, type_or_schema);
    let mut app = App::new(f.file.clone(), load(&options).unwrap());
    app.options = Some(options);
    app
}

/// The viewer as it opens when the schema may not compile: the first screen,
/// then the result of the load, whatever it is.
fn opened(f: &Fixture, type_or_schema: &str) -> App {
    let options = options(f, type_or_schema);
    let mut app = App::loading(f.file.clone(), type_or_schema);
    app.options = Some(options.clone());
    app.finish_loading(try_load(&options));
    app
}

/// The schema `top` with an import that is not there.
fn with_missing_import(f: &Fixture) {
    std::fs::write(
        &f.top,
        TOP.replace(
            "    - lib
",
            "    - lib
    - gone
",
        ),
    )
    .unwrap();
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn screen(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(110, 30)).unwrap();
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

fn root_type(app: &App) -> String {
    app.type_name.clone()
}

fn move_to(app: &mut App, row: &Row) {
    let at = app
        .schemas
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .position(|r| r == row)
        .unwrap();
    let from = app.schemas.as_ref().unwrap().selected;
    for _ in 0..at.abs_diff(from) {
        press(
            app,
            if at > from {
                KeyCode::Down
            } else {
                KeyCode::Up
            },
        );
    }
}

#[test]
fn s_lists_the_schemas_with_their_types_and_what_is_missing() {
    let f = fixture();
    with_missing_import(&f);
    // nothing can be parsed (the import is not there), and the list is there all the same
    let mut app = opened(&f, f.top.to_str().unwrap());
    assert!(app.report.as_ref().is_some_and(|r| r.fatal));
    assert!(app.schemas.is_none());
    press(&mut app, KeyCode::Char('s'));
    let shown = screen(&mut app);
    assert!(app.schemas.is_some());
    assert!(shown.contains("schemas and types"), "{shown}");
    assert!(
        shown.contains("top.yaml") && shown.contains("main schema"),
        "{shown}"
    );
    assert!(shown.contains("lib.yaml"), "{shown}");
    assert!(
        shown.contains("top::chunk") && shown.contains("lib::piece"),
        "{shown}"
    );
    assert!(shown.contains("not found gone"), "{shown}");
    assert!(shown.contains("imported by"), "{shown}");
    // when each file was last changed
    assert!(shown.contains("UTC"), "{shown}");
    // Esc closes it
    press(&mut app, KeyCode::Esc);
    assert!(app.schemas.is_none());
    assert!(!app.quit, "Esc in the list closes the list, not the viewer");
}

#[test]
fn enter_on_a_schema_reads_it_with_line_numbers_and_esc_goes_back() {
    let f = fixture();
    let mut app = app(&f, f.top.to_str().unwrap());
    press(&mut app, KeyCode::Char('s'));
    // the type that is parsed is marked, and the selection starts on it
    assert!(screen(&mut app).contains("● top"), "{}", screen(&mut app));
    let list = app.schemas.as_ref().unwrap();
    assert_eq!(
        list.rows[list.selected],
        Row::Type {
            entry: 0,
            name: "top".to_owned()
        }
    );
    move_to(&mut app, &Row::Schema(1));
    press(&mut app, KeyCode::Enter);
    let shown = screen(&mut app);
    assert!(shown.contains("lib.yaml"), "{shown}");
    assert!(shown.contains("1 │ meta:"), "{shown}");
    assert!(shown.contains("read-only"), "{shown}");
    assert!(!shown.contains("schemas and types"), "{shown}");
    // the keys scroll it
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.schemas.as_ref().unwrap().pager.as_ref().unwrap().top, 2);
    // a chosen type is not changed by reading
    assert_eq!(root_type(&app), "top");
    press(&mut app, KeyCode::Esc);
    let shown = screen(&mut app);
    assert!(shown.contains("schemas and types"), "{shown}");
    press(&mut app, KeyCode::Esc);
    assert!(app.schemas.is_none());
}

#[test]
fn a_schema_with_a_mistake_shows_it_in_the_list_and_the_line_in_the_text() {
    let f = fixture();
    std::fs::write(
        f.top.with_file_name("lib.yaml"),
        "id: lib\nseq:\n  - id: [\n",
    )
    .unwrap();
    let mut app = opened(&f, f.top.to_str().unwrap());
    press(&mut app, KeyCode::Char('s'));
    let shown = screen(&mut app);
    assert!(shown.contains("✗"), "{shown}");
    move_to(&mut app, &Row::Schema(1));
    press(&mut app, KeyCode::Enter);
    let shown = screen(&mut app);
    assert!(
        shown.contains("✗ "),
        "the line of the error is marked: {shown}"
    );
}

#[test]
fn enter_on_a_type_parses_with_it_and_the_screen_says_so() {
    let f = fixture();
    let mut app = app(&f, f.top.to_str().unwrap());
    assert_eq!(root_type(&app), "top");
    // a type inside the schema
    press(&mut app, KeyCode::Char('s'));
    move_to(
        &mut app,
        &Row::Type {
            entry: 0,
            name: "top::chunk".to_owned(),
        },
    );
    press(&mut app, KeyCode::Enter);
    assert!(app.schemas.is_none());
    assert_eq!(root_type(&app), "top::chunk");
    assert_eq!(app.notice.as_deref(), Some("parsing as top::chunk"));
    assert_eq!(app.tree.selected().name, "top::chunk");
    assert!(screen(&mut app).contains("n = 263"), "{}", screen(&mut app));
    // the options keep it: a reload parses with the same type
    assert_eq!(
        app.options.as_ref().unwrap().type_or_schema,
        format!("{}::top::chunk", f.top.display())
    );
    press(&mut app, KeyCode::Char('r'));
    assert_eq!(root_type(&app), "top::chunk");

    // a type of an imported schema
    press(&mut app, KeyCode::Char('s'));
    let list = app.schemas.as_ref().unwrap();
    assert_eq!(
        list.rows[list.selected],
        Row::Type {
            entry: 0,
            name: "top::chunk".to_owned()
        },
        "the selection starts on the type that is parsed"
    );
    move_to(
        &mut app,
        &Row::Type {
            entry: 1,
            name: "lib::piece".to_owned(),
        },
    );
    press(&mut app, KeyCode::Enter);
    assert_eq!(root_type(&app), "lib::piece");
    assert!(screen(&mut app).contains("p = 7"), "{}", screen(&mut app));

    // and the schema's own again
    press(&mut app, KeyCode::Char('s'));
    move_to(
        &mut app,
        &Row::Type {
            entry: 0,
            name: "top".to_owned(),
        },
    );
    press(&mut app, KeyCode::Enter);
    assert_eq!(root_type(&app), "top");
    assert_eq!(
        app.options.as_ref().unwrap().type_or_schema,
        f.top.to_str().unwrap()
    );
}

#[test]
fn the_type_can_be_named_on_the_command_line() {
    let f = fixture();
    let app = app(&f, &format!("{}::chunk", f.top.display()));
    assert_eq!(root_type(&app), "top::chunk");
    assert!(app.report.is_none());
    // the schema is still known for the list
    let mut app = app;
    press(&mut app, KeyCode::Char('s'));
    let list = app.schemas.as_ref().unwrap();
    assert_eq!(
        list.rows[list.selected],
        Row::Type {
            entry: 0,
            name: "top::chunk".to_owned()
        }
    );
    // a type that is not there
    let options = options(&f, &format!("{}::nope", f.top.display()));
    let error = load(&options).unwrap_err().to_string();
    assert!(error.contains("nope") && error.contains("chunk"), "{error}");
}

#[test]
fn reading_again_from_the_list_sees_a_schema_that_changed() {
    let f = fixture();
    with_missing_import(&f);
    let mut app = opened(&f, f.top.to_str().unwrap());
    press(&mut app, KeyCode::Char('s'));
    assert_eq!(app.schemas.as_ref().unwrap().entries.len(), 2);
    assert_eq!(app.schemas.as_ref().unwrap().missing.len(), 1);
    // lib is written again, without its type, and top no longer imports `gone`
    std::fs::write(
        f.top.with_file_name("lib.yaml"),
        "meta:\n  id: lib\nseq: []\n",
    )
    .unwrap();
    std::fs::write(&f.top, TOP).unwrap();
    press(&mut app, KeyCode::Char('r'));
    let list = app.schemas.as_ref().unwrap();
    assert!(list.missing.is_empty());
    assert_eq!(list.entries[1].types, ["lib"]);
}

#[test]
fn a_type_that_cannot_be_parsed_with_leaves_the_old_tree_and_says_why() {
    let f = fixture();
    let mut app = app(&f, f.top.to_str().unwrap());
    press(&mut app, KeyCode::Char('s'));
    // the file is too short for a chunk: the parse fails but there is a tree
    std::fs::write(&f.file, [7]).unwrap();
    move_to(
        &mut app,
        &Row::Type {
            entry: 0,
            name: "top::chunk".to_owned(),
        },
    );
    press(&mut app, KeyCode::Enter);
    assert_eq!(root_type(&app), "top::chunk");
    assert!(app.report.is_some());
}

#[test]
fn without_options_there_is_nothing_to_list() {
    let f = fixture();
    let options = options(&f, f.top.to_str().unwrap());
    let mut app = App::new(f.file.clone(), load(&options).unwrap());
    press(&mut app, KeyCode::Char('s'));
    assert!(app.schemas.is_none());
    assert!(app.notice.is_some());
}

#[test]
fn a_library_type_name_has_its_schema_found_on_the_include_paths() {
    let f = fixture();
    let dir: &Path = f.top.parent().unwrap();
    let mut options = options(&f, "top");
    options.include_paths = vec![dir.to_path_buf()];
    let mut app = App::new(f.file.clone(), load(&options).unwrap());
    app.options = Some(options);
    press(&mut app, KeyCode::Char('s'));
    assert_eq!(app.schemas.as_ref().unwrap().entries.len(), 2);
    move_to(
        &mut app,
        &Row::Type {
            entry: 0,
            name: "top::chunk".to_owned(),
        },
    );
    press(&mut app, KeyCode::Enter);
    assert_eq!(root_type(&app), "top::chunk");
    assert_eq!(
        app.options.as_ref().unwrap().type_or_schema,
        "top::top::chunk"
    );
}
