//! `f` makes the focused pane fill the screen (#410), and puts both panes back.

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

fn app() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("s.yaml"),
        "meta:\n  id: s\n  endian: le\nseq:\n  - id: first\n    type: u2\n  - id: second\n    type: u2\n  - id: name\n    type: str\n    size: 4\n    encoding: ASCII\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("s.bin"),
        [1, 0, 2, 0, b'a', b'b', b'c', b'd'],
    )
    .unwrap();
    let options = Options {
        file: dir.path().join("s.bin"),
        type_or_schema: dir.path().join("s.yaml").to_string_lossy().into_owned(),
        include_paths: Vec::new(),
        color: false,
        watch: false,
        theme: None,
        theme_dir: None,
        config_file: None,
        outer_file: None,
        theme_on_command_line: false,
        transparent: None,
        radix: None,
    };
    let mut app = App::new(options.file.clone(), load(&options).unwrap());
    app.options = Some(options);
    (app, dir)
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn key(app: &mut App, c: char) {
    press(app, KeyCode::Char(c));
}

fn screen(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
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

/// Whether the tree (a field) and the bytes (the hex of the name) are both on the screen.
fn has_both(screen: &str) -> bool {
    screen.contains("61 62 63 64") && screen.contains("first")
}

#[test]
fn f_makes_the_tree_fill_the_screen_and_puts_both_panes_back() {
    let (mut app, _dir) = app();
    let both = screen(&mut app, 120, 14);
    assert!(has_both(&both), "{both}");
    key(&mut app, 'f');
    let full = screen(&mut app, 120, 14);
    assert!(full.contains("full screen"), "{full}");
    assert!(
        full.contains("first") && !full.contains("61 62 63 64"),
        "{full}"
    );
    // the box of the tree is the whole width
    assert!(
        full.lines().next().unwrap().chars().count() >= 119,
        "{full}"
    );
    key(&mut app, 'f');
    assert!(has_both(&screen(&mut app, 120, 14)));
    assert!(!app.quit);
}

#[test]
fn the_bytes_can_fill_the_screen_and_tab_moves_the_other_pane_in() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Focus::Hex);
    key(&mut app, 'f');
    let full = screen(&mut app, 120, 14);
    assert!(
        full.contains("full screen") && full.contains("61 62 63 64"),
        "{full}"
    );
    assert!(!full.contains("first"), "the tree is not drawn:\n{full}");
    // Tab: the other pane fills the screen
    press(&mut app, KeyCode::Tab);
    let tree = screen(&mut app, 120, 14);
    assert!(
        app.fullscreen && tree.contains("first") && !tree.contains("61 62 63 64"),
        "{tree}"
    );
}

#[test]
fn esc_puts_the_panes_back_before_it_leaves_and_the_selection_stays() {
    let (mut app, _dir) = app();
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    let selected = app.tree.selected().name.clone();
    key(&mut app, 'f');
    press(&mut app, KeyCode::Esc);
    assert!(!app.fullscreen && !app.quit);
    assert_eq!(app.tree.selected().name, selected);
    assert!(has_both(&screen(&mut app, 120, 14)));
    press(&mut app, KeyCode::Esc);
    // the first Esc after that is the one that leaves (it asks once when something was changed; here nothing was)
    assert!(app.quit || app.notice.is_some());
}

#[test]
fn f_is_a_letter_where_text_is_typed() {
    let (mut app, _dir) = app();
    key(&mut app, '/');
    assert!(app.prompt.is_some());
    key(&mut app, 'f');
    assert!(!app.fullscreen);
    assert_eq!(app.prompt.as_ref().unwrap().line.text(), "f");
    press(&mut app, KeyCode::Esc);
    // the help takes the keys too
    key(&mut app, '?');
    key(&mut app, 'f');
    assert!(!app.fullscreen);
}

#[test]
fn a_screen_with_room_for_one_pane_is_not_changed_by_f() {
    let (mut app, _dir) = app();
    key(&mut app, 'f');
    // narrow and short: one pane is drawn anyway
    let small = screen(&mut app, 50, 8);
    assert!(small.contains("first"), "{small}");
    assert!(!small.contains("full screen"), "{small}");
}
