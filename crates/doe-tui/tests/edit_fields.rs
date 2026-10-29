//! Changing the schema that is being built: undo and redo, removing, renaming and
//! retyping fields, and the byte order. No terminal.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::app::{App, Focus};
use doe_tui::draft::{Field, Kind};
use doe_tui::load::load;
use doe_tui::ui::{Theme, draw};
use doe_tui::{Options, prepare_build};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

/// A viewer of ten bytes (1 to 10) with the schema `a` (u1), `b` (u2), `c` (3 bytes) built.
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
    app.focus = Focus::Tree;
    for (id, kind) in [
        ("a", Kind::Unsigned(1)),
        ("b", Kind::Unsigned(2)),
        ("c", Kind::Bytes(3)),
    ] {
        app.edit_draft(|d| {
            d.push(Field {
                id: id.to_owned(),
                kind,
            })
        })
        .unwrap();
    }
    (app, dir)
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn key(app: &mut App, c: char) {
    press(app, KeyCode::Char(c));
}

fn answer(app: &mut App, text: &str) {
    app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    for c in text.chars() {
        key(app, c);
    }
    press(app, KeyCode::Enter);
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

fn names(app: &App) -> Vec<String> {
    fields(app).into_iter().map(|f| f.0).collect()
}

fn select(app: &mut App, index: usize) {
    app.tree.select_path(&[index]);
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
fn u_takes_the_last_change_back_and_ctrl_r_does_it_again() {
    let (mut app, _dir) = building();
    key(&mut app, 'u');
    assert_eq!(names(&app), ["a", "b"]);
    assert!(app.notice.as_deref().unwrap().contains("undid adding c"));
    key(&mut app, 'u');
    key(&mut app, 'u');
    assert_eq!(names(&app), Vec::<String>::new());
    key(&mut app, 'u');
    assert!(app.notice.as_deref().unwrap().contains("nothing to undo"));
    app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
    assert_eq!(names(&app), ["a"]);
    for _ in 0..2 {
        app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
    }
    assert_eq!(names(&app), ["a", "b", "c"]);
    app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("nothing to do again")
    );
    // a new change forgets what was taken back
    key(&mut app, 'u');
    app.edit_draft(|d| {
        d.push(Field {
            id: "other".to_owned(),
            kind: Kind::Unsigned(1),
        })
    })
    .unwrap();
    app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("nothing to do again")
    );
    assert_eq!(names(&app), ["a", "b", "other"]);
}

#[test]
fn d_removes_the_last_field_at_once_and_another_one_after_asking() {
    let (mut app, _dir) = building();
    select(&mut app, 2);
    key(&mut app, 'd');
    assert_eq!(names(&app), ["a", "b"]);
    select(&mut app, 0);
    key(&mut app, 'd');
    assert_eq!(names(&app), ["a", "b"], "asked first");
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("d again removes a and moves the 1 field after it up")
    );
    key(&mut app, 'd');
    assert_eq!(fields(&app), [("b".to_owned(), 0, 2)]);
    // the only field left is the last one: it goes at once
    select(&mut app, 0);
    key(&mut app, 'd');
    assert_eq!(names(&app), Vec::<String>::new());
}

#[test]
fn a_key_between_the_two_d_asks_again() {
    let (mut app, _dir) = building();
    select(&mut app, 0);
    key(&mut app, 'd');
    press(&mut app, KeyCode::Down);
    select(&mut app, 0);
    key(&mut app, 'd');
    assert_eq!(names(&app), ["a", "b", "c"]);
}

#[test]
fn r_renames_the_selected_field_and_asks_again_for_a_bad_or_taken_name() {
    let (mut app, _dir) = building();
    select(&mut app, 1);
    key(&mut app, 'r');
    assert_eq!(app.prompt.as_ref().unwrap().line.text(), "b");
    answer(&mut app, "Bad");
    assert!(app.prompt.is_some());
    assert!(app.notice.as_deref().unwrap().contains("lower case"));
    answer(&mut app, "a");
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("there is a field called a")
    );
    answer(&mut app, "size");
    assert!(app.prompt.is_none());
    assert_eq!(names(&app), ["a", "size", "c"]);
    assert_eq!(app.tree.selected().name, "size");
    key(&mut app, 'u');
    assert_eq!(names(&app), ["a", "b", "c"]);
}

#[test]
fn c_changes_the_kind_and_the_fields_after_move_and_a_change_that_does_not_fit_is_refused() {
    let (mut app, _dir) = building();
    select(&mut app, 1);
    key(&mut app, 'c');
    assert_eq!(app.prompt.as_ref().unwrap().line.text(), "u2");
    answer(&mut app, "u4");
    press(&mut app, KeyCode::Enter); // repeat: once
    assert_eq!(
        fields(&app),
        [
            ("a".to_owned(), 0, 1),
            ("b".to_owned(), 1, 4),
            ("c".to_owned(), 5, 3)
        ]
    );
    // 1 + 8 + 3 is more than the ten bytes of the file
    key(&mut app, 'c');
    answer(&mut app, "u8");
    press(&mut app, KeyCode::Enter); // repeat: once
    assert!(
        app.notice.as_deref().unwrap().contains("does not fit"),
        "{:?}",
        app.notice
    );
    assert_eq!(
        fields(&app)[1],
        ("b".to_owned(), 1, 4),
        "the field stays as it was"
    );
    // bytes ask the length, which is proposed
    select(&mut app, 2);
    key(&mut app, 'c');
    assert_eq!(app.prompt.as_ref().unwrap().line.text(), "bytes");
    answer(&mut app, "bytes");
    assert_eq!(app.prompt.as_ref().unwrap().line.text(), "3");
    answer(&mut app, "5");
    press(&mut app, KeyCode::Enter); // once
    assert_eq!(fields(&app)[2], ("c".to_owned(), 5, 5));
}

#[test]
fn e_changes_the_byte_order_and_u_changes_it_back() {
    let (mut app, _dir) = building();
    select(&mut app, 1);
    assert!(
        screen(&mut app).contains("b = 770") || screen(&mut app).contains("0x0302"),
        "{}",
        screen(&mut app)
    );
    key(&mut app, 'E');
    assert!(app.notice.as_deref().unwrap().contains("big-endian"));
    assert_eq!(app.builder.as_ref().unwrap().draft.endian().name(), "be");
    select(&mut app, 1);
    let shown = screen(&mut app);
    assert!(
        shown.contains("b = 515") || shown.contains("0x0203"),
        "{shown}"
    );
    key(&mut app, 'u');
    assert_eq!(app.builder.as_ref().unwrap().draft.endian().name(), "le");
}

#[test]
fn a_key_that_needs_a_field_says_so_when_none_is_selected_and_nothing_changes_without_build() {
    let (mut app, _dir) = building();
    app.tree.select_path(&[]);
    for k in ['d', 'c', 'r'] {
        key(&mut app, k);
        assert!(app.prompt.is_none(), "{k}");
        assert!(
            app.notice.as_deref().unwrap().contains("select a field"),
            "{k}"
        );
    }
    assert_eq!(names(&app), ["a", "b", "c"]);

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
    let mut plain = App::new(options.file.clone(), load(&options).unwrap());
    for k in ['u', 'd', 'c', 'E'] {
        key(&mut plain, k);
        assert!(plain.notice.as_deref().unwrap().contains("--build"), "{k}");
    }
    // r reads again, as it did
    key(&mut plain, 'r');
    assert!(plain.prompt.is_none());
}

#[test]
fn every_change_can_be_undone_back_to_the_empty_schema() {
    let (mut app, _dir) = building();
    select(&mut app, 1);
    key(&mut app, 'c');
    answer(&mut app, "u4");
    press(&mut app, KeyCode::Enter); // repeat: once
    select(&mut app, 0);
    key(&mut app, 'r');
    answer(&mut app, "first");
    key(&mut app, 'E');
    select(&mut app, 2);
    key(&mut app, 'd');
    for _ in 0..7 {
        key(&mut app, 'u');
    }
    assert_eq!(names(&app), Vec::<String>::new());
    assert_eq!(app.builder.as_ref().unwrap().draft.endian().name(), "le");
    assert!(
        !app.builder.as_ref().unwrap().dirty(),
        "the draft is the one it began as"
    );
}
