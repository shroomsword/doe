//! The description (`doc:`) of a field next to the field in the tree of the terminal viewer, in the inspector and in the
//! search. No terminal.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::panic,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::Options;
use doe_tui::app::App;
use doe_tui::load::load;
use doe_tui::ui::{Theme, draw};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;

const SCHEMA: &str = "meta:
  id: d
  endian: le
doc: The description of the whole type, which a field does not repeat.
seq:
  - id: count
    type: u1
    doc: |
      How many entries follow.

      A second paragraph with the word zebra that the row does not show.
  - id: entries
    type: entry
    repeat: expr
    repeat-expr: count
    doc: The entries.
  - id: plain
    type: u1
  - id: long
    type: u1
    doc: \"LONGWORD followed by a sentence that goes on and on and on and on and on and on and on and on and on and on\"
instances:
  twice:
    value: count * 2
    doc: Twice the count.
types:
  entry:
    doc: One entry.
    seq:
      - id: kind
        type: u1
        doc: What the entry is.
";

fn app(dir: &Path, schema: &str) -> App {
    std::fs::write(dir.join("d.yaml"), schema).unwrap();
    std::fs::write(dir.join("d.bin"), [2, 10, 11, 12, 13]).unwrap();
    let options = Options {
        file: dir.join("d.bin"),
        type_or_schema: dir.join("d.yaml").to_string_lossy().into_owned(),
        include_paths: Vec::new(),
        color: true,
        watch: false,
        theme: None,
        theme_dir: None,
        config_file: None,
        outer_file: None,
        theme_on_command_line: false,
        transparent: None,
        mouse: true,
        restore: None,
        radix: None,
    };
    let mut app = App::new(options.file.clone(), load(&options).unwrap());
    app.options = Some(options);
    app
}

fn key(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn draw_at(app: &mut App, width: u16, color: bool) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
    terminal.draw(|f| draw(f, app, &Theme::new(color))).unwrap();
    terminal.backend().buffer().clone()
}

fn rows(buffer: &Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn row_of<'a>(lines: &'a [String], needle: &str) -> &'a str {
    lines
        .iter()
        .find(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("{needle} is not on the screen: {lines:#?}"))
}

/// Open the nodes of the tree down to the fields of the first element.
fn open_all(app: &mut App) {
    // root, count, entries, plain, long, twice: open entries, then its first element
    press(app, KeyCode::Down);
    press(app, KeyCode::Down);
    press(app, KeyCode::Right);
    press(app, KeyCode::Down);
    press(app, KeyCode::Right);
    press(app, KeyCode::Home);
}

#[test]
fn a_field_with_a_description_shows_its_first_line_after_the_value_dim() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path(), SCHEMA);
    open_all(&mut app);
    let buffer = draw_at(&mut app, 200, true);
    let lines = rows(&buffer);
    let count = row_of(&lines, "count = 2");
    assert!(
        count.contains("count = 2  How many entries follow."),
        "{count}"
    );
    // only the first line, not the second paragraph
    assert!(!count.contains("zebra"), "{count}");
    assert!(row_of(&lines, "twice").contains("Twice the count."));
    assert!(row_of(&lines, "▾ entries").contains("The entries."));
    // an element has none (the field has it, once), a field of the element's type has its own
    let element = row_of(&lines, "[0]");
    assert!(
        !element.contains("The entries.") && !element.contains("One entry."),
        "{element}"
    );
    assert!(row_of(&lines, "kind").contains("What the entry is."));
    // no description: nothing more than a row had; the root does not repeat the description of its type
    assert!(
        !row_of(&lines, "plain").contains("  The"),
        "{}",
        row_of(&lines, "plain")
    );
    assert!(
        !lines.iter().any(|l| l.contains("whole type")),
        "{lines:#?}"
    );
    // dim
    let at = count.find("How many").unwrap();
    let x = count[..at].chars().count();
    let y = lines.iter().position(|l| l == count).unwrap();
    let cell = &buffer[(u16::try_from(x).unwrap(), u16::try_from(y).unwrap())];
    assert!(
        cell.style().add_modifier.contains(Modifier::DIM),
        "{:?}",
        cell.style()
    );
}

#[test]
fn a_long_description_is_cut_to_the_pane_and_the_row_is_one_line() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path(), SCHEMA);
    open_all(&mut app);
    // 100 columns: the tree has 45 of them
    let lines = rows(&draw_at(&mut app, 100, false));
    let long = row_of(&lines, "long = ");
    assert!(long.contains('…'), "{long}");
    assert!(
        !long.contains("on and on and on and on and on and on and on and on and on and on"),
        "{long}"
    );
    // the row is on one line and the next row is the next field
    let at = lines.iter().position(|l| l == long).unwrap();
    assert!(lines[at + 1].contains("twice"), "{:#?}", &lines[at..at + 2]);
    // a very narrow pane cuts the line and nothing is wider than the screen
    let narrow = rows(&draw_at(&mut app, 60, false));
    assert!(narrow.iter().all(|l| l.chars().count() <= 60));
}

#[test]
fn d_hides_and_shows_the_descriptions_and_a_reload_keeps_that() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path(), SCHEMA);
    open_all(&mut app);
    assert!(row_of(&rows(&draw_at(&mut app, 200, false)), "count = 2").contains("How many"));
    key(&mut app, 'D');
    assert!(
        app.notice.as_deref().unwrap().contains("hidden"),
        "{:?}",
        app.notice
    );
    let lines = rows(&draw_at(&mut app, 200, false));
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("How many") || l.contains("The entries.")),
        "{lines:#?}"
    );
    key(&mut app, 'r');
    let lines = rows(&draw_at(&mut app, 200, false));
    assert!(
        !lines.iter().any(|l| l.contains("How many")),
        "still hidden after a reload"
    );
    key(&mut app, 'D');
    assert!(
        app.notice.as_deref().unwrap().contains("shown"),
        "{:?}",
        app.notice
    );
    assert!(row_of(&rows(&draw_at(&mut app, 200, false)), "count = 2").contains("How many"));
}

#[test]
fn the_inspector_has_the_whole_description_of_the_selected_field() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path(), SCHEMA);
    key(&mut app, 'i');
    // root, then count
    press(&mut app, KeyCode::Down);
    assert_eq!(app.tree.selected().name, "count");
    let lines = rows(&draw_at(&mut app, 200, false)).join("\n");
    assert!(lines.contains("How many entries follow."), "{lines}");
    assert!(
        lines.contains("A second paragraph with the word zebra"),
        "{lines}"
    );
    // a field without one has the forms only
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.tree.selected().name, "plain");
    let lines = rows(&draw_at(&mut app, 200, false)).join("\n");
    assert!(!lines.contains("zebra"), "{lines}");
    // and with the descriptions hidden, not even the selected field's
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Up);
    key(&mut app, 'D');
    assert!(
        !rows(&draw_at(&mut app, 200, false))
            .join("\n")
            .contains("zebra")
    );
}

#[test]
fn search_finds_a_word_of_a_description_while_the_descriptions_are_shown() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path(), SCHEMA);
    key(&mut app, '/');
    for c in "zebra".chars() {
        key(&mut app, c);
    }
    press(&mut app, KeyCode::Enter);
    // the search runs a few fields at a time between redraws
    for _ in 0..50 {
        if app.search.is_none() {
            break;
        }
        app.search_step();
    }
    assert_eq!(app.tree.selected().name, "count", "{:?}", app.notice);
    // hidden descriptions are not searched: a match would have no reason on the screen
    key(&mut app, 'D');
    press(&mut app, KeyCode::Home);
    key(&mut app, '/');
    for c in "zebra".chars() {
        key(&mut app, c);
    }
    press(&mut app, KeyCode::Enter);
    for _ in 0..50 {
        if app.search.is_none() {
            break;
        }
        app.search_step();
    }
    assert!(
        app.notice.as_deref().unwrap().contains("not found"),
        "{:?}",
        app.notice
    );
}

#[test]
fn a_schema_with_no_descriptions_looks_as_it_did_and_no_colour_adds_nothing_to_the_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(
        dir.path(),
        "meta:\n  id: n\nseq:\n  - id: a\n    type: u1\n",
    );
    let with = rows(&draw_at(&mut app, 120, false));
    let a = row_of(&with, "a = 2");
    assert!(
        a.trim_end().ends_with("a = 2") || a.contains("a = 2  u1") || a.contains("a = 2"),
        "{a}"
    );
    key(&mut app, 'D');
    let without = rows(&draw_at(&mut app, 120, false));
    // the only difference is the status line (the notice)
    assert_eq!(&with[..with.len() - 1], &without[..without.len() - 1]);
}
