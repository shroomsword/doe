//! The browser of the types on the type paths in the viewer: opening it,
//! filtering, reading a type, choosing one to parse with, and what the screen
//! shows. No terminal.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::Options;
use doe_tui::app::App;
use doe_tui::load::load;
use doe_tui::ui::{Theme, draw};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    file: PathBuf,
}

/// A type path with `pair` (two bytes) and, in a subdirectory, `single` (one byte, with a doc).
fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("small")).unwrap();
    std::fs::write(
        root.join("pair.yaml"),
        "id: pair\nseq:\n  - id: x\n    type: u1\n  - id: y\n    type: u1\n",
    )
    .unwrap();
    std::fs::write(
        root.join("small/other_name.yaml"),
        "id: single\ndoc: One byte\nseq:\n  - id: x\n    type: u1\n",
    )
    .unwrap();
    std::fs::write(
        root.join("small/sig.yaml"),
        "id: sig\nseq:\n  - id: magic\n    contents: [7, 9]\n",
    )
    .unwrap();
    let file = root.join("data.bin");
    std::fs::write(&file, [7, 9]).unwrap();
    Fixture {
        _dir: dir,
        root,
        file,
    }
}

fn app(f: &Fixture, type_name: &str) -> App {
    let options = Options {
        file: f.file.clone(),
        type_or_schema: type_name.to_owned(),
        include_paths: vec![f.root.clone()],
        color: false,
        watch: false,
        theme: None,
        theme_dir: None,
        radix: None,
    };
    let mut app = App::new(f.file.clone(), load(&options).unwrap());
    app.options = Some(options);
    app
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn screen(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(110, 20)).unwrap();
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
fn t_lists_the_types_of_the_type_path_and_its_subdirectories_with_the_one_in_use_marked() {
    let f = fixture();
    let mut app = app(&f, "pair");
    press(&mut app, KeyCode::Char('t'));
    assert!(app.types.is_some());
    let shown = screen(&mut app);
    assert!(
        shown.contains("types on the type paths · 3 of 3"),
        "{shown}"
    );
    assert!(shown.contains("● pair"), "{shown}");
    // a type in a subdirectory, with the file it is in and the first line of its doc
    assert!(shown.contains("single"), "{shown}");
    assert!(shown.contains("other_name.yaml"), "{shown}");
    assert!(shown.contains("One byte"), "{shown}");
    // the selection starts on the type in use
    assert_eq!(app.types.as_ref().unwrap().selected_id(), Some("pair"));
    press(&mut app, KeyCode::Esc);
    assert!(app.types.is_none());
    assert!(
        !app.quit,
        "Esc in the browser closes the browser, not the viewer"
    );
}

#[test]
fn the_filter_narrows_the_list_and_esc_clears_it() {
    let f = fixture();
    let mut app = app(&f, "pair");
    press(&mut app, KeyCode::Char('t'));
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "byte");
    let shown = screen(&mut app);
    assert!(shown.contains("1 of 3"), "{shown}");
    assert!(shown.contains("filter: byte"), "{shown}");
    assert!(!shown.contains("pair.yaml"), "{shown}");
    // keys are text while editing: `q` does not close anything
    type_text(&mut app, "q");
    assert!(app.types.is_some() && !app.quit);
    assert!(screen(&mut app).contains("no type matches the filter"));
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Enter); // stop editing, keep the filter
    assert_eq!(app.types.as_ref().unwrap().filter, "byte");
    press(&mut app, KeyCode::Esc); // clear it
    assert_eq!(app.types.as_ref().unwrap().filter, "");
    assert!(screen(&mut app).contains("3 of 3"));
}

#[test]
fn v_reads_the_text_of_a_type_read_only_and_esc_goes_back_to_the_list() {
    let f = fixture();
    let mut app = app(&f, "pair");
    press(&mut app, KeyCode::Char('t'));
    press(&mut app, KeyCode::Down); // the types are in the order of their ids: pair, sig, single
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Char('v'));
    let shown = screen(&mut app);
    assert!(shown.contains("id: single"), "{shown}");
    assert!(shown.contains("other_name.yaml"), "{shown}");
    press(&mut app, KeyCode::Esc);
    assert!(app.types.as_ref().unwrap().pager.is_none());
    assert!(screen(&mut app).contains("types on the type paths"));
}

#[test]
fn enter_parses_the_open_file_with_the_chosen_type() {
    let f = fixture();
    let mut app = app(&f, "pair");
    press(&mut app, KeyCode::Char('t'));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.types.as_ref().unwrap().selected_id(), Some("single"));
    press(&mut app, KeyCode::Enter);
    assert!(app.types.is_none());
    assert_eq!(app.type_name, "single");
    assert!(app.notice.as_deref().unwrap().contains("parsing as single"));
    // the tree is the one of the new type: one field
    assert_eq!(app.tree.selected().name, "single");
}

#[test]
fn without_a_type_path_it_says_so() {
    let f = fixture();
    let mut app = app(&f, "pair");
    app.options.as_mut().unwrap().include_paths.clear();
    press(&mut app, KeyCode::Char('t'));
    assert!(app.types.is_none());
    assert!(app.notice.as_deref().unwrap().contains("-I"));
}

/// Read the signatures of all the types, as the event loop does between keys.
fn read_signatures(app: &mut App) {
    while app.working() {
        app.work_step();
    }
}

#[test]
fn the_signatures_are_read_in_steps_and_the_types_that_have_one_say_so() {
    let f = fixture();
    let mut app = app(&f, "pair");
    press(&mut app, KeyCode::Char('t'));
    // not read yet: the title says how far it is, and the viewer has work to do
    assert!(app.working());
    assert!(screen(&mut app).contains("reading signatures 0 of 3"));
    read_signatures(&mut app);
    assert!(!app.working());
    let shown = screen(&mut app);
    assert!(!shown.contains("reading signatures"), "{shown}");
    let line = |name: &str| shown.lines().find(|l| l.contains(name)).unwrap().to_owned();
    assert!(line("sig.yaml").contains("signature"), "{shown}");
    assert!(!line("pair.yaml").contains("signature"), "{shown}");
}

#[test]
fn r_ranks_the_types_by_what_fits_the_open_file_and_again_puts_them_back() {
    let f = fixture();
    let mut app = app(&f, "pair");
    press(&mut app, KeyCode::Char('t'));
    press(&mut app, KeyCode::Char('r'));
    assert!(screen(&mut app).contains("ranked by the file (after reading)"));
    read_signatures(&mut app);
    let shown = screen(&mut app);
    assert!(shown.contains("ranked by the file"), "{shown}");
    // the file starts with 7 9: `sig` fits by its signature; the others have none and the
    // name of the file (data.bin) is not theirs, so they are left out
    let browser = app.types.as_ref().unwrap();
    assert_eq!(browser.counts(), (3, 1));
    assert!(
        shown.contains("the file starts with its signature"),
        "{shown}"
    );
    // a filter works on the ranked list
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "zzz");
    assert_eq!(app.types.as_ref().unwrap().counts(), (3, 0));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Esc);
    // r again: all of them, in groups
    press(&mut app, KeyCode::Char('r'));
    assert_eq!(app.types.as_ref().unwrap().counts(), (3, 3));
    assert!(!screen(&mut app).contains("ranked by"));
}

#[test]
fn a_file_that_cannot_be_read_is_said_when_ranking() {
    let f = fixture();
    let mut app = app(&f, "pair");
    std::fs::remove_file(&f.file).unwrap();
    press(&mut app, KeyCode::Char('t'));
    press(&mut app, KeyCode::Char('r'));
    assert!(!app.types.as_ref().unwrap().ranked);
    assert!(app.notice.as_deref().unwrap().contains("could not read"));
}
