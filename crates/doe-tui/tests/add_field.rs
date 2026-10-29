//! Adding a field to the schema that is being built, from the hex pane: the
//! questions, what is proposed, and what happens to the schema, the tree and the
//! cursor. No terminal.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::app::{App, Focus};
use doe_tui::draft::Kind;
use doe_tui::load::load;
use doe_tui::ui::{Theme, draw};
use doe_tui::{Options, prepare_build};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

/// A viewer of a file of ten bytes (1 to 10) that builds `built.yaml`, with the hex pane focused.
fn building() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("data.bin");
    std::fs::write(&file, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]).unwrap();
    let options = Options {
        file: file.clone(),
        type_or_schema: String::new(),
        include_paths: vec![],
        color: false,
        watch: false,
        theme: None,
        theme_dir: None,
        radix: None,
    };
    let (options, builder) = prepare_build(&options, &dir.path().join("built.yaml")).unwrap();
    let mut app = App::new(file, load(&options).unwrap());
    app.options = Some(options);
    app.builder = Some(builder);
    app.focus = Focus::Hex;
    (app, dir)
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn key(app: &mut App, c: char) {
    press(app, KeyCode::Char(c));
}

/// Answer the question on the screen with `text` (replacing what was proposed).
fn answer(app: &mut App, text: &str) {
    app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    for c in text.chars() {
        key(app, c);
    }
    press(app, KeyCode::Enter);
}

/// The label of the question that is open.
fn asking(app: &App) -> Option<&'static str> {
    app.prompt.as_ref().map(|p| p.kind.label())
}

fn fields(app: &App) -> Vec<(String, u64, u64)> {
    let draft = &app.builder.as_ref().unwrap().draft;
    draft
        .fields()
        .iter()
        .enumerate()
        .map(|(i, f)| (f.id.clone(), draft.offset_of(i), f.kind.size()))
        .collect()
}

fn screen(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 14)).unwrap();
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
fn a_adds_a_byte_where_the_schema_ends_with_a_name_proposed_and_moves_on() {
    let (mut app, _dir) = building();
    key(&mut app, 'a');
    assert!(asking(&app).unwrap().starts_with("kind ("));
    press(&mut app, KeyCode::Enter); // u1 is proposed
    assert!(asking(&app).unwrap().starts_with("repeat"));
    press(&mut app, KeyCode::Enter); // once
    assert_eq!(asking(&app), Some("field name: "));
    press(&mut app, KeyCode::Enter); // u1_1 is proposed
    assert!(app.prompt.is_none());
    assert_eq!(fields(&app), [("u1_1".to_owned(), 0, 1)]);
    assert_eq!(app.hex.cursor(), 1, "the cursor is after the field");
    assert_eq!(app.tree.selected().name, "u1_1");
    assert!(app.notice.as_deref().unwrap().contains("added u1_1 at 0x0"));
    // and again: the next field starts where the last one ended
    key(&mut app, 'a');
    answer(&mut app, "u2");
    press(&mut app, KeyCode::Enter); // repeat: once
    answer(&mut app, "size");
    assert_eq!(fields(&app)[1], ("size".to_owned(), 1, 2));
    assert_eq!(app.hex.cursor(), 3);
    let shown = screen(&mut app);
    assert!(shown.contains("size"), "{shown}");
}

#[test]
fn a_selection_gives_the_kind_and_the_length_that_are_proposed() {
    let (mut app, _dir) = building();
    // two bytes selected: u2
    key(&mut app, 'v');
    key(&mut app, 'l');
    key(&mut app, 'a');
    press(&mut app, KeyCode::Enter); // u2
    press(&mut app, KeyCode::Enter); // once
    press(&mut app, KeyCode::Enter); // u2_1
    assert_eq!(fields(&app), [("u2_1".to_owned(), 0, 2)]);
    assert_eq!(app.hex.selected_range(), None, "the selection is used up");
    // three bytes selected from the end of the schema: bytes of that length
    key(&mut app, 'v');
    key(&mut app, 'l');
    key(&mut app, 'l');
    key(&mut app, 'a');
    answer(&mut app, "bytes");
    assert_eq!(asking(&app), Some("length in bytes: "));
    assert_eq!(app.prompt.as_ref().unwrap().line.text(), "3");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter); // once
    assert_eq!(app.prompt.as_ref().unwrap().line.text(), "bytes_1");
    press(&mut app, KeyCode::Enter);
    assert_eq!(fields(&app)[1], ("bytes_1".to_owned(), 2, 3));
}

#[test]
fn text_asks_for_the_length_and_the_encoding() {
    let (mut app, _dir) = building();
    key(&mut app, 'a');
    answer(&mut app, "str");
    assert_eq!(asking(&app), Some("length in bytes: "));
    answer(&mut app, "4");
    assert!(asking(&app).unwrap().starts_with("encoding"));
    assert_eq!(app.prompt.as_ref().unwrap().line.text(), "ASCII");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter); // once
    answer(&mut app, "name");
    let kind = app.builder.as_ref().unwrap().draft.fields()[0].kind.clone();
    assert_eq!(
        kind,
        Kind::Text {
            len: 4,
            encoding: "ASCII".to_owned()
        }
    );
    // the tree shows the text that was read
    assert!(
        screen(&mut app).contains("\"\\u{1}\\u{2}"),
        "{}",
        screen(&mut app)
    );
}

#[test]
fn a_kind_or_a_length_that_is_not_one_is_asked_again_and_esc_gives_up() {
    let (mut app, _dir) = building();
    key(&mut app, 'a');
    answer(&mut app, "u3");
    assert!(asking(&app).unwrap().starts_with("kind ("));
    assert!(app.notice.as_deref().unwrap().contains("not a kind"));
    answer(&mut app, "bytes");
    answer(&mut app, "0");
    assert_eq!(asking(&app), Some("length in bytes: "));
    assert!(app.notice.as_deref().unwrap().contains("at least one byte"));
    press(&mut app, KeyCode::Esc);
    assert!(app.prompt.is_none());
    assert_eq!(fields(&app), []);
    // a new start does not remember the old answers
    key(&mut app, 'a');
    assert_eq!(app.prompt.as_ref().unwrap().line.text(), "u1");
}

#[test]
fn a_field_that_does_not_fit_the_file_or_a_name_that_is_taken_is_refused_and_asked_again() {
    let (mut app, _dir) = building();
    key(&mut app, 'a');
    answer(&mut app, "bytes");
    answer(&mut app, "11");
    press(&mut app, KeyCode::Enter); // once
    answer(&mut app, "too_long");
    assert!(
        app.notice.as_deref().unwrap().contains("does not fit"),
        "{:?}",
        app.notice
    );
    assert_eq!(asking(&app), Some("field name: "));
    assert_eq!(fields(&app), []);
    press(&mut app, KeyCode::Esc);

    key(&mut app, 'a');
    answer(&mut app, "u1");
    press(&mut app, KeyCode::Enter); // repeat: once
    answer(&mut app, "x");
    key(&mut app, 'a');
    answer(&mut app, "u1");
    press(&mut app, KeyCode::Enter); // repeat: once
    answer(&mut app, "x");
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("there is a field called x")
    );
    assert_eq!(asking(&app), Some("field name: "));
    answer(&mut app, "y");
    assert_eq!(
        fields(&app)
            .iter()
            .map(|f| f.0.as_str())
            .collect::<Vec<_>>(),
        ["x", "y"]
    );
}

#[test]
fn capital_a_adds_a_field_at_the_cursor_with_the_skipped_bytes_before_it() {
    let (mut app, _dir) = building();
    for _ in 0..3 {
        key(&mut app, 'l');
    }
    key(&mut app, 'A');
    answer(&mut app, "u2");
    press(&mut app, KeyCode::Enter); // repeat: once
    answer(&mut app, "later");
    assert_eq!(
        fields(&app),
        [("skip_1".to_owned(), 0, 3), ("later".to_owned(), 3, 2)]
    );
    assert_eq!(app.hex.cursor(), 5);
    // the cursor is in a field now: A there is refused
    app.hex.set_cursor(1, 10);
    key(&mut app, 'A');
    assert!(app.prompt.is_none());
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("in a field already")
    );
}

#[test]
fn a_field_that_fails_after_a_gap_takes_the_gap_back() {
    let (mut app, _dir) = building();
    for _ in 0..8 {
        key(&mut app, 'l');
    }
    key(&mut app, 'A');
    answer(&mut app, "u4");
    press(&mut app, KeyCode::Enter); // repeat: once
    answer(&mut app, "late");
    // bytes 8, 9 are the file's last two: a u4 does not fit
    assert_eq!(fields(&app), [], "the skipped bytes are not left behind");
    assert_eq!(asking(&app), Some("field name: "));
}

#[test]
fn the_keys_say_so_when_nothing_is_being_built() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("t.yaml"),
        "id: t\nseq:\n  - id: a\n    type: u1\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("t.bin"), [1]).unwrap();
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
    app.focus = Focus::Hex;
    key(&mut app, 'a');
    assert!(app.prompt.is_none());
    assert!(app.notice.as_deref().unwrap().contains("--build"));
}
