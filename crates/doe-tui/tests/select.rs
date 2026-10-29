//! A range of bytes selected in the hex pane. No terminal.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::Options;
use doe_tui::app::{App, Focus};
use doe_tui::load::load;
use doe_tui::ui::{Theme, draw};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Modifier;

fn app() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("t.yaml"),
        "id: t\nseq:\n  - id: all\n    size: 20\n",
    )
    .unwrap();
    let data: Vec<u8> = (0..20).map(|i| 0x10 + i).collect();
    std::fs::write(dir.path().join("t.bin"), &data).unwrap();
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
    app.options = Some(options);
    app.focus = Focus::Hex;
    (app, dir)
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn screen(app: &mut App) -> (String, ratatui::buffer::Buffer) {
    let mut terminal = Terminal::new(TestBackend::new(120, 14)).unwrap();
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
fn v_starts_a_selection_that_the_cursor_extends_in_both_directions_and_v_ends() {
    let (mut app, _dir) = app();
    for _ in 0..4 {
        press(&mut app, KeyCode::Right);
    }
    assert_eq!(app.hex.selected_range(), None);
    press(&mut app, KeyCode::Char('v'));
    assert_eq!(app.hex.selected_range(), Some((4, 4)));
    for _ in 0..3 {
        press(&mut app, KeyCode::Right);
    }
    assert_eq!(app.hex.selected_range(), Some((4, 7)));
    // back past where it began: the other way
    for _ in 0..6 {
        press(&mut app, KeyCode::Left);
    }
    assert_eq!(app.hex.selected_range(), Some((1, 4)));
    press(&mut app, KeyCode::Char('v'));
    assert_eq!(app.hex.selected_range(), None);
}

#[test]
fn q_ends_the_selection_before_it_leaves() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Char('v'));
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Char('q'));
    assert!(!app.quit);
    assert_eq!(app.hex.selected_range(), None);
    press(&mut app, KeyCode::Esc);
    assert!(app.quit);
}

#[test]
fn the_title_says_how_many_bytes_and_where_and_they_are_drawn_apart() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Char('v'));
    let (shown, _) = screen(&mut app);
    assert!(shown.contains("1 byte selected (0x1..0x1)"), "{shown}");
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    let (shown, buffer) = screen(&mut app);
    assert!(shown.contains("3 bytes selected (0x1..0x3)"), "{shown}");
    // the bytes 11 12 13 are italic, 10 and 14 are not
    let row = u16::try_from(
        shown
            .lines()
            .position(|l| l.contains("10 11 12 13 14"))
            .unwrap(),
    )
    .unwrap();
    let line: Vec<char> = shown
        .lines()
        .nth(usize::from(row))
        .unwrap()
        .chars()
        .collect();
    let at = |needle: &str| -> u16 {
        let target: Vec<char> = needle.chars().collect();
        u16::try_from(
            line.windows(target.len())
                .position(|w| w == target.as_slice())
                .unwrap(),
        )
        .unwrap()
    };
    let x = at("10 11 12 13 14");
    assert!(!buffer[(x, row)].modifier.contains(Modifier::ITALIC));
    for k in 1..=3 {
        assert!(
            buffer[(x + 3 * k, row)].modifier.contains(Modifier::ITALIC),
            "byte {k}: {shown}"
        );
    }
    assert!(!buffer[(x + 12, row)].modifier.contains(Modifier::ITALIC));
}

#[test]
fn the_inspector_shows_the_selected_bytes_in_the_forms_of_their_size() {
    let (mut app, _dir) = app();
    app.inspector = true;
    press(&mut app, KeyCode::Char('v'));
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    // four bytes 10 11 12 13: an integer of four bytes in both byte orders
    let (shown, _) = screen(&mut app);
    assert!(shown.contains("0x13121110"), "{shown}");
    assert!(shown.contains("0x10111213"), "{shown}");
}

#[test]
fn y_says_the_hex_and_asks_for_the_clipboard() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Char('y'));
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("nothing is selected")
    );
    assert_eq!(app.take_clipboard(), None);
    press(&mut app, KeyCode::Char('v'));
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Char('y'));
    assert_eq!(app.notice.as_deref(), Some("copied: 10 11 12"));
    assert_eq!(app.take_clipboard().as_deref(), Some("10 11 12"));
    assert_eq!(app.take_clipboard(), None, "it is taken once");
}

#[test]
fn reading_the_file_again_drops_the_selection() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Char('v'));
    press(&mut app, KeyCode::Right);
    assert!(app.hex.selected_range().is_some());
    app.focus = Focus::Tree;
    press(&mut app, KeyCode::Char('r'));
    assert_eq!(app.hex.selected_range(), None);
}
