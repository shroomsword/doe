//! The keys that matter now (#427): the row under the panes that follows the pane and the mode, and the box that a
//! key which takes another key shows. Drawn on the test backend.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use doe_tui::Options;
use doe_tui::app::{App, Focus};
use doe_tui::keymap::{self, Action};
use doe_tui::load::load;
use doe_tui::ui::{Theme, draw};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

const SCHEMA: &str = "meta:\n  id: s\n  endian: le\nseq:\n  - id: first\n    type: u2\n  - id: second\n    type: u2\n";

fn options(dir: &tempfile::TempDir) -> Options {
    Options {
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
        mouse: true,
        footer: true,
        compare: None,
        restore: None,
        radix: None,
    }
}

fn app() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("s.yaml"), SCHEMA).unwrap();
    std::fs::write(dir.path().join("s.bin"), [1, 0, 2, 0]).unwrap();
    let options = options(&dir);
    let mut app = App::new(options.file.clone(), load(&options).unwrap());
    app.options = Some(options);
    app.mouse_on = true;
    (app, dir)
}

fn tar() -> App {
    let types = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../types");
    let mut options = options(&tempfile::tempdir().unwrap());
    options.file = types.join("samples/sample.tar");
    "tar".clone_into(&mut options.type_or_schema);
    options.include_paths = vec![types];
    let mut app = App::new(options.file.clone(), load(&options).unwrap());
    app.options = Some(options);
    app.mouse_on = true;
    app.focus = Focus::Tree;
    app
}

fn lines(app: &mut App, width: u16, height: u16) -> Vec<String> {
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
        .collect()
}

/// The row of keys: the one above the status bar, the last row.
fn footer(lines: &[String]) -> &str {
    &lines[lines.len() - 2]
}

fn key(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn click(app: &mut App, column: u16, row: u16) -> bool {
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

#[test]
fn the_footer_follows_the_pane_that_has_the_keys() {
    let (mut app, _dir) = app();
    let l = lines(&mut app, 100, 20);
    assert_eq!(
        footer(&l).trim(),
        "↑↓ move · → open · / search · Y copy as · g goto · J follow · Tab bytes · q quit · ? all keys"
    );
    key(&mut app, KeyCode::Tab);
    let l = lines(&mut app, 100, 20);
    assert_eq!(
        footer(&l).trim(),
        "←→↑↓ move · v select · y copy · g goto · b bits · Tab tree · q quit · ? all keys"
    );
    key(&mut app, KeyCode::Tab);
    let l = lines(&mut app, 100, 20);
    assert!(footer(&l).contains("Tab bytes"));
}

#[test]
fn the_footer_of_the_files_of_an_archive_has_its_keys_and_names_the_checksum() {
    let mut app = tar();
    let l = lines(&mut app, 140, 20);
    assert_eq!(
        footer(&l).trim(),
        "↑↓ move · → open · Enter preview · y copy sum · / search · Tab bytes · F fields · m checksums · q quit · ? all keys"
    );
    key(&mut app, KeyCode::Char('m'));
    let l = lines(&mut app, 140, 20);
    assert!(footer(&l).contains("m MD5"), "{}", footer(&l));
    // `F` goes to the fields, and the footer is the tree's
    key(&mut app, KeyCode::Char('F'));
    let l = lines(&mut app, 140, 20);
    assert!(
        footer(&l).contains("F files") && !footer(&l).contains("Enter preview"),
        "{}",
        footer(&l)
    );
}

#[test]
fn a_prompt_has_its_own_keys_and_a_preview_is_a_dialog() {
    let (mut app, _dir) = app();
    key(&mut app, KeyCode::Char('/'));
    assert!(app.prompt.is_some());
    let l = lines(&mut app, 100, 20);
    assert!(footer(&l).contains("Enter accept · Esc cancel"), "{l:?}");
    key(&mut app, KeyCode::Esc);
    assert!(app.prompt.is_none());
    // the preview of a file of an archive: select a file and press Enter
    let mut app = tar();
    for _ in 0..8 {
        if app.preview.is_some() {
            break;
        }
        key(&mut app, KeyCode::Right);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Enter);
    }
    assert!(app.preview.is_some(), "{:?}", app.notice);
    // the preview covers the footer and has its own keys on its border: the footer is the dialog's
    let ctx = app.footer_context(keymap::Scope::Tree);
    assert_eq!(ctx.situation, keymap::Situation::Dialog);
    let l = lines(&mut app, 100, 20);
    assert!(
        l.iter().any(|line| line.contains("s saves the entry")),
        "{l:?}"
    );
    key(&mut app, KeyCode::Esc);
    assert!(app.preview.is_none());
}

#[test]
fn a_list_or_the_help_shows_what_closes_it() {
    let (mut app, _dir) = app();
    key(&mut app, KeyCode::Char('?'));
    assert!(app.help);
    let ctx = app.footer_context(keymap::Scope::Tree);
    assert_eq!(ctx.situation, keymap::Situation::Dialog);
    let parts = keymap::footer(&ctx);
    assert_eq!(parts[0].keys, "Esc");
}

#[test]
fn a_terminal_under_twelve_rows_has_no_footer_and_k_hides_and_shows_it() {
    let (mut app, _dir) = app();
    let l = lines(&mut app, 100, 11);
    assert!(app.hits.hints.is_empty(), "{:?}", app.hits.hints);
    assert!(!l.iter().any(|line| line.contains("? all keys")), "{l:?}");
    let l = lines(&mut app, 100, 12);
    assert!(!app.hits.hints.is_empty(), "{:?}", app.hits.hints);
    assert!(footer(&l).contains("? all keys"));
    // K hides it, and the row goes back to the panes
    let tall = lines(&mut app, 100, 20);
    let tree_rows_with = tall.iter().filter(|l| l.contains("│")).count();
    key(&mut app, KeyCode::Char('K'));
    assert!(!app.footer_on);
    let l = lines(&mut app, 100, 20);
    assert!(app.hits.hints.is_empty(), "{:?}", app.hits.hints);
    assert!(!l.iter().any(|line| line.contains("? all keys")));
    assert!(l.iter().filter(|l| l.contains("│")).count() > tree_rows_with);
    key(&mut app, KeyCode::Char('K'));
    assert!(app.footer_on);
    let l = lines(&mut app, 100, 20);
    assert!(footer(&l).contains("? all keys"));
}

#[test]
fn the_footer_fits_in_eighty_columns_and_in_forty() {
    let (mut app, _dir) = app();
    for width in [80_u16, 60, 40, 30] {
        let l = lines(&mut app, width, 20);
        let row = footer(&l);
        assert!(row.chars().count() <= usize::from(width), "{row:?}");
        assert!(
            row.contains("q quit") && row.contains("? all keys"),
            "{width}: {row:?}"
        );
    }
    let l = lines(&mut app, 80, 20);
    // the first parts are the ones that stay
    assert!(footer(&l).contains("↑↓ move"), "{}", footer(&l));
}

#[test]
fn the_box_after_y_lists_what_the_next_key_does_and_a_click_copies() {
    let (mut app, _dir) = app();
    key(&mut app, KeyCode::Char('Y'));
    assert!(app.copy_as);
    let l = lines(&mut app, 100, 24);
    let chord = keymap::chord(Action::CopyAs).unwrap();
    // every choice of the table is in the box, with its letter
    for (letter, label) in chord.choices {
        assert!(
            l.iter()
                .any(|line| line.contains(&format!(" {letter}  {label}"))),
            "{letter} {label}: {l:?}"
        );
    }
    assert_eq!(app.hits.choices.len(), chord.choices.len());
    // the footer says what closes it
    assert!(footer(&l).contains("Esc close"), "{}", footer(&l));
    // a click on `j` copies the JSON
    let (rect, letter) = app.hits.choices[2];
    assert_eq!(letter, 'j');
    assert!(click(&mut app, rect.x + 2, rect.y));
    assert!(!app.copy_as);
    assert!(app.take_clipboard().is_some(), "{:?}", app.notice);
}

#[test]
fn the_box_goes_on_esc_on_a_key_and_on_a_click_elsewhere() {
    let (mut app, _dir) = app();
    key(&mut app, KeyCode::Char('Y'));
    key(&mut app, KeyCode::Esc);
    assert!(!app.copy_as);
    assert!(app.take_clipboard().is_none());
    // a second key copies, as it did
    key(&mut app, KeyCode::Char('Y'));
    key(&mut app, KeyCode::Char('p'));
    assert_eq!(app.take_clipboard().as_deref(), Some("$"));
    // a click that is not on a choice puts it away and copies nothing
    key(&mut app, KeyCode::Char('Y'));
    lines(&mut app, 100, 24);
    assert!(click(&mut app, 1, 1));
    assert!(!app.copy_as);
    assert_eq!(app.notice.as_deref(), Some("nothing copied"));
    assert!(app.take_clipboard().is_none());
}

#[test]
fn the_letters_of_the_table_are_the_letters_that_copy() {
    let (mut app, _dir) = app();
    let chord = keymap::chord(Action::CopyAs).unwrap();
    for (letter, _) in chord.choices {
        key(&mut app, KeyCode::Char('Y'));
        key(&mut app, KeyCode::Char(*letter));
        let notice = app.notice.clone().unwrap_or_default();
        assert!(
            !notice.contains("is not one of the choices"),
            "{letter} is in the table and copies nothing: {notice}"
        );
        let _ = app.take_clipboard();
    }
    // and a letter that is not in the table is not a choice
    for letter in ['z', 'q', 'y', 'Q'] {
        assert!(!chord.choices.iter().any(|(l, _)| *l == letter));
        key(&mut app, KeyCode::Char('Y'));
        key(&mut app, KeyCode::Char(letter));
        let notice = app.notice.clone().unwrap_or_default();
        assert!(
            notice.contains("is not one of the choices"),
            "{letter}: {notice}"
        );
    }
}
