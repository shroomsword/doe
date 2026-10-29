//! The cursor of the bytes pane that moves by bits, and the keys that go through
//! the fields that hold one byte. No terminal.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::Options;
use doe_tui::app::App;
use doe_tui::load::load;
use doe_tui::ui::{Theme, draw};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Modifier;

// head: byte 0; hi: the first 3 bits of byte 1; lo: the other 5; tail: byte 2.
// window is an instance over all three bytes; second one over the last two.
const SCHEMA: &str = "id: t
seq:
  - id: head
    type: u1
  - id: hi
    type: b3
  - id: lo
    type: b5
  - id: tail
    type: u1
instances:
  window:
    pos: 0
    size: 3
  second:
    pos: 1
    size: 2
";

fn app() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("t.yaml"), SCHEMA).unwrap();
    std::fs::write(dir.path().join("t.bin"), [1, 0xff, 3]).unwrap();
    let options = Options {
        file: dir.path().join("t.bin"),
        type_or_schema: dir.path().join("t.yaml").to_string_lossy().into_owned(),
        include_paths: vec![dir.path().to_path_buf()],
        color: false,
        watch: false,
        theme: None,
        theme_dir: None,
        radix: None,
    };
    let mut app = App::new(options.file.clone(), load(&options).unwrap());
    app.options = Some(options);
    (app, dir)
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn selected(app: &App) -> String {
    app.tree.selected().name.clone()
}

fn screen(app: &mut App) -> (String, ratatui::buffer::Buffer) {
    let mut terminal = Terminal::new(TestBackend::new(110, 12)).unwrap();
    terminal.draw(|f| draw(f, app, &Theme::new(false))).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n");
    (text, buffer)
}

#[test]
fn b_moves_the_cursor_by_bits_and_the_field_of_the_bit_is_selected() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Right); // byte 1: hi and lo share it, hi is found
    assert_eq!((app.hex.cursor(), selected(&app).as_str()), (1, "hi"));
    press(&mut app, KeyCode::Char('b'));
    assert_eq!(app.hex.bit(), Some(0), "the first bit of hi");
    for _ in 0..2 {
        press(&mut app, KeyCode::Right);
    }
    assert_eq!((app.hex.bit(), selected(&app).as_str()), (Some(2), "hi"));
    press(&mut app, KeyCode::Right); // bit 3 of byte 1 is the first of lo
    assert_eq!((app.hex.bit(), selected(&app).as_str()), (Some(3), "lo"));
    assert_eq!(app.hex.bit_position(), Some(11));
    press(&mut app, KeyCode::Left);
    assert_eq!(selected(&app), "hi");
    // a bit move goes over the end of a byte
    for _ in 0..6 {
        press(&mut app, KeyCode::Right);
    }
    assert_eq!((app.hex.cursor(), app.hex.bit()), (2, Some(0)));
    assert_eq!(selected(&app), "tail");
    // and stops at the end of the file
    for _ in 0..20 {
        press(&mut app, KeyCode::Right);
    }
    assert_eq!((app.hex.cursor(), app.hex.bit()), (2, Some(7)));
    // b again: bytes
    press(&mut app, KeyCode::Char('b'));
    assert_eq!(app.hex.bit(), None);
    press(&mut app, KeyCode::Left);
    assert_eq!(app.hex.cursor(), 1);
}

#[test]
fn the_tree_moves_the_bit_cursor_to_the_start_of_the_field() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Char('b'));
    press(&mut app, KeyCode::Tab); // the tree
    // down from hi to lo
    press(&mut app, KeyCode::Down);
    assert_eq!(selected(&app), "lo");
    assert_eq!((app.hex.cursor(), app.hex.bit()), (1, Some(3)));
}

#[test]
fn the_screen_says_the_bit_and_underlines_the_byte() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Right);
    let (shown, _) = screen(&mut app);
    assert!(!shown.contains("· bit "), "{shown}");
    press(&mut app, KeyCode::Char('b'));
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    let (shown, buffer) = screen(&mut app);
    assert!(shown.contains("bits 11..16 · bit 3 of byte 0x1"), "{shown}");
    // the cursor byte (01 ff 03: the second) is underlined
    let line = shown.lines().find(|l| l.contains("01 ff 03")).unwrap();
    let row = u16::try_from(shown.lines().position(|l| l == line).unwrap()).unwrap();
    let x = u16::try_from(line.chars().position(|c| c == 'f').unwrap()).unwrap();
    assert!(
        buffer[(x, row)].modifier.contains(Modifier::UNDERLINED),
        "{shown}"
    );
}

#[test]
fn a_byte_held_by_several_fields_says_so_and_dot_goes_through_them() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right); // byte 2: tail, and the instances window and second
    assert_eq!((app.hex.cursor(), selected(&app).as_str()), (2, "tail"));
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("3 fields hold this byte")
    );
    press(&mut app, KeyCode::Char('.'));
    let first = selected(&app);
    assert_ne!(first, "tail");
    assert!(app.notice.as_deref().unwrap().contains("field 2 of 3"));
    press(&mut app, KeyCode::Char('.'));
    let second = selected(&app);
    assert!(second != "tail" && second != first, "{first} {second}");
    press(&mut app, KeyCode::Char('.')); // round
    assert_eq!(selected(&app), "tail");
    press(&mut app, KeyCode::Char(','));
    assert_eq!(selected(&app), second);
    // moving the cursor starts again
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Right);
    assert_eq!(selected(&app), "tail");
}

#[test]
fn dot_on_a_byte_that_a_field_and_an_instance_hold_goes_to_the_instance() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Char('.')); // byte 0: head, and the window
    assert!(app.notice.as_deref().unwrap().contains("field 2 of 2"));
}
