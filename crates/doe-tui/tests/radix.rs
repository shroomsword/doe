//! The radix of the integers in the viewer: `x` for all of them, `X` for a
//! field, what the screen and the inspector show, and that a field keeps its
//! choice when the schema is read again. No terminal.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe::radix::Radix;
use doe_tui::Options;
use doe_tui::app::App;
use doe_tui::load::load;
use doe_tui::radixes::Radixes;
use doe_tui::ui::{Theme, draw};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

const SCHEMA: &str = "meta:
  id: numbers
  endian: le
seq:
  - id: a
    type: u2
  - id: b
    type: s1
  - id: big
    type: u8
  - id: list
    type: u1
    repeat: expr
    repeat-expr: 3
  - id: kind
    type: u1
    enum: kinds
  - id: ratio
    type: f4
  - id: name
    type: str
    size: 2
    encoding: ascii
enums:
  kinds:
    1: text
";

/// a = 31, b = -1, big = 2^53 + 1, list = 1 2 3, kind = 1, ratio = 1.5, name = "hi"
const INPUT: [u8; 21] = [
    0x1f, 0x00, 0xff, 0x01, 0, 0, 0, 0, 0, 0x20, 0, 1, 2, 3, 1, 0x00, 0x00, 0xc0, 0x3f, 0x68, 0x69,
];

struct Fixture {
    _dir: tempfile::TempDir,
    options: Options,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let schema: PathBuf = dir.path().join("numbers.yaml");
    std::fs::write(&schema, SCHEMA).unwrap();
    let file = dir.path().join("data.bin");
    std::fs::write(&file, INPUT).unwrap();
    let options = Options {
        file,
        type_or_schema: schema.to_string_lossy().into_owned(),
        include_paths: vec![],
        color: false,
        watch: false,
        theme: None,
        theme_dir: None,
        radix: None,
    };
    Fixture { _dir: dir, options }
}

fn app(f: &Fixture) -> App {
    let mut app = App::new(f.options.file.clone(), load(&f.options).unwrap());
    app.options = Some(f.options.clone());
    app
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn key(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
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

/// The line of the screen that has this field in the tree.
fn row(app: &mut App, name: &str) -> String {
    screen(app)
        .lines()
        .find(|line| line.contains(&format!(" {name} = ")))
        .unwrap()
        .to_owned()
}

/// Select a field by name and open it if it can be.
fn go_to(app: &mut App, name: &str) {
    press(app, KeyCode::Home);
    for _ in 0..40 {
        if app.tree.selected().name == name {
            return;
        }
        press(app, KeyCode::Down);
        if app.tree.selected().name == "list" && name.starts_with('[') && !app.tree.selected().open
        {
            press(app, KeyCode::Enter);
        }
    }
    assert_eq!(app.tree.selected().name, name, "{name} is not in the tree");
}

/// What is written after `name =` in a row.
fn value(app: &mut App, name: &str) -> String {
    let line = row(app, name);
    let (_, rest) = line.split_once(" = ").unwrap();
    rest.split("  ").next().unwrap().trim().to_owned()
}

#[test]
fn x_goes_through_the_radixes_and_every_integer_follows() {
    let f = fixture();
    let mut app = app(&f);
    go_to(&mut app, "list");
    press(&mut app, KeyCode::Enter);

    assert_eq!(value(&mut app, "a"), "31 (0x1f)");
    assert_eq!(value(&mut app, "b"), "-1 (-0x1)");

    key(&mut app, 'x');
    assert_eq!(app.notice.as_deref(), Some("integers in hexadecimal"));
    assert_eq!(value(&mut app, "a"), "0x001f");
    assert_eq!(value(&mut app, "b"), "-0x01");
    assert_eq!(value(&mut app, "big"), "0x0020000000000001");
    assert_eq!(value(&mut app, "[2]"), "0x03");
    assert_eq!(value(&mut app, "kind"), "text (0x01)");
    // floats and strings are what they were
    assert_eq!(value(&mut app, "ratio"), "1.5");
    assert_eq!(value(&mut app, "name"), "\"hi\"");

    key(&mut app, 'x');
    assert_eq!(value(&mut app, "a"), "0b0000000000011111");
    key(&mut app, 'x');
    assert_eq!(value(&mut app, "a"), "0o000037");
    key(&mut app, 'x');
    assert_eq!(value(&mut app, "a"), "31 (0x1f)");
}

#[test]
fn a_field_with_a_radix_of_its_own_has_it_for_every_element_and_after_a_reload() {
    let f = fixture();
    let mut app = app(&f);
    go_to(&mut app, "list");
    press(&mut app, KeyCode::Enter);
    key(&mut app, 'x');
    go_to(&mut app, "[1]");

    key(&mut app, 'X');
    assert_eq!(app.notice.as_deref(), Some("this field in decimal"));
    key(&mut app, 'X');
    assert_eq!(app.notice.as_deref(), Some("this field in hexadecimal"));
    key(&mut app, 'X');
    assert_eq!(app.notice.as_deref(), Some("this field in binary"));
    for (name, text) in [
        ("[0]", "0b00000001"),
        ("[1]", "0b00000010"),
        ("[2]", "0b00000011"),
    ] {
        assert_eq!(value(&mut app, name), text, "{name}");
    }
    // the others stay as the global choice has them
    assert_eq!(value(&mut app, "a"), "0x001f");

    // it holds after the schema is read again, and when the global choice changes
    key(&mut app, 'r');
    key(&mut app, 'x');
    assert_eq!(value(&mut app, "[0]"), "0b00000001");
    assert_eq!(value(&mut app, "a"), "0b0000000000011111");

    // two more presses: octal, and then back to following `x`
    key(&mut app, 'X');
    assert_eq!(value(&mut app, "[2]"), "0o003");
    key(&mut app, 'X');
    assert_eq!(
        app.notice.as_deref(),
        Some("this field follows the others (binary)")
    );
    assert_eq!(value(&mut app, "[2]"), "0b00000011");
}

#[test]
fn x_on_a_field_that_is_not_an_integer_says_so() {
    let f = fixture();
    let mut app = app(&f);
    go_to(&mut app, "ratio");
    key(&mut app, 'X');
    assert_eq!(
        app.notice.as_deref(),
        Some("the selected field is not an integer")
    );
    assert_eq!(app.radixes.fields(), 0);
    // the root and a list are not integers either
    go_to(&mut app, "list");
    key(&mut app, 'X');
    assert_eq!(app.radixes.fields(), 0);
}

#[test]
fn the_viewer_can_start_in_another_radix() {
    let mut f = fixture();
    f.options.radix = Some(Radix::Binary);
    let mut app = app(&f);
    app.radixes = Radixes::new(Radix::Binary);
    assert_eq!(value(&mut app, "a"), "0b0000000000011111");
}

#[test]
fn the_inspector_has_the_numbers_in_the_radix_and_the_twos_complement() {
    let f = fixture();
    let mut app = app(&f);
    go_to(&mut app, "b");
    key(&mut app, 'i');
    key(&mut app, 'x');
    let text = screen(&mut app);
    assert!(text.contains("unsigned LE"), "{text}");
    assert!(text.contains("0xff"), "{text}");
    assert!(text.contains("-0x01 (two's complement 0xff)"), "{text}");
}
