//! Signatures, text that ends, repeats and bit fields in the schema that is being
//! built, through the questions of `a`. No terminal.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::app::{App, Focus};
use doe_tui::draft::{Count, Kind};
use doe_tui::load::load;
use doe_tui::ui::{Theme, draw};
use doe_tui::{Options, prepare_build};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

// magic 7f 45; count 3; three u2; "abc" and a zero; a byte of 3 + 5 bits; two bytes.
const DATA: [u8; 16] = [
    0x7f,
    0x45,
    3,
    1,
    0,
    2,
    0,
    3,
    0,
    b'a',
    b'b',
    b'c',
    0,
    0b1010_0011,
    9,
    10,
];

fn building() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("data.bin");
    std::fs::write(&file, DATA).unwrap();
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
    let (options, builder) = prepare_build(&options, &dir.path().join("rich.yaml")).unwrap();
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

fn answer(app: &mut App, text: &str) {
    app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    for c in text.chars() {
        key(app, c);
    }
    press(app, KeyCode::Enter);
}

fn kinds(app: &App) -> Vec<(String, Kind)> {
    let draft = &app.builder.as_ref().unwrap().draft;
    draft
        .fields()
        .iter()
        .map(|f| (f.id.clone(), f.kind.clone()))
        .collect()
}

fn screen(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(130, 20)).unwrap();
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
fn a_schema_with_every_kind_is_built_field_by_field_and_reads_the_file() {
    let (mut app, _dir) = building();
    // the signature: the selected bytes are proposed
    key(&mut app, 'v');
    key(&mut app, 'l');
    key(&mut app, 'a');
    answer(&mut app, "magic");
    assert_eq!(app.prompt.as_ref().unwrap().line.text(), "7f 45");
    press(&mut app, KeyCode::Enter);
    answer(&mut app, "signature");
    assert_eq!(app.builder.as_ref().unwrap().draft.end(), 2);
    // a count
    key(&mut app, 'a');
    answer(&mut app, "u1");
    press(&mut app, KeyCode::Enter);
    answer(&mut app, "count");
    // three u2, as many as the count says
    key(&mut app, 'a');
    answer(&mut app, "u2");
    answer(&mut app, "count");
    answer(&mut app, "items");
    // text up to the zero: the length is found in the file
    key(&mut app, 'a');
    answer(&mut app, "strz");
    press(&mut app, KeyCode::Enter); // terminator 0
    press(&mut app, KeyCode::Enter); // ASCII
    answer(&mut app, "label");
    assert_eq!(app.hex.cursor(), 13);
    // two bit fields in the byte
    key(&mut app, 'a');
    answer(&mut app, "b3");
    answer(&mut app, "flags");
    assert_eq!(
        (app.hex.cursor(), app.hex.bit()),
        (13, Some(3)),
        "the next bit field goes on in this byte"
    );
    key(&mut app, 'a');
    answer(&mut app, "b5");
    answer(&mut app, "mode");
    assert_eq!(app.hex.cursor(), 14);
    // the rest of the file, byte by byte
    key(&mut app, 'a');
    answer(&mut app, "u1");
    answer(&mut app, "eos");
    answer(&mut app, "tail");

    let all = kinds(&app);
    assert_eq!(all[0].1, Kind::Contents(vec![0x7f, 0x45]));
    assert_eq!(
        all[2].1,
        Kind::Repeat {
            inner: Box::new(Kind::Unsigned(2)),
            count: Count::Field("count".to_owned()),
            sample: 3
        }
    );
    assert_eq!(
        all[3].1,
        Kind::Terminated {
            terminator: 0,
            encoding: "ASCII".to_owned(),
            sample: 4
        }
    );
    assert_eq!(
        (all[4].1.clone(), all[5].1.clone()),
        (Kind::Bits(3), Kind::Bits(5))
    );
    assert_eq!(
        all[6].1,
        Kind::Repeat {
            inner: Box::new(Kind::Unsigned(1)),
            count: Count::ToEnd,
            sample: 2
        }
    );
    assert_eq!(app.builder.as_ref().unwrap().draft.end(), 16);
    // the viewer shows it as an ordinary parse
    assert!(app.error.is_none() && app.report.is_none());
    app.tree.select_path(&[]);
    let names: Vec<String> = app.tree.rows().iter().map(|r| r.name.clone()).collect();
    assert!(names.contains(&"label".to_owned()), "{names:?}");
    let shown = screen(&mut app);
    assert!(shown.contains("\"abc\""), "{shown}");
    // and the schema is the one that was built
    let yaml = app.builder.as_ref().unwrap().draft.to_yaml();
    assert!(
        yaml.contains("repeat-expr: count") && yaml.contains("repeat: eos"),
        "{yaml}"
    );
}

#[test]
fn what_a_kind_needs_is_checked_against_the_file() {
    let (mut app, _dir) = building();
    // no byte of 0xff to end the text
    key(&mut app, 'a');
    answer(&mut app, "strz");
    answer(&mut app, "0xff");
    press(&mut app, KeyCode::Enter);
    answer(&mut app, "x");
    assert!(
        app.notice.as_deref().unwrap().contains("no byte 0xff"),
        "{:?}",
        app.notice
    );
    assert_eq!(app.builder.as_ref().unwrap().draft.fields(), []);
    press(&mut app, KeyCode::Esc);
    // hex that is not hex, a bad terminator, a repeat that is not one: asked again
    key(&mut app, 'a');
    answer(&mut app, "magic");
    answer(&mut app, "zz");
    assert!(app.notice.as_deref().unwrap().contains("not hex bytes"));
    press(&mut app, KeyCode::Esc);
    key(&mut app, 'a');
    answer(&mut app, "strz");
    answer(&mut app, "300");
    assert!(app.notice.as_deref().unwrap().contains("not a byte"));
    press(&mut app, KeyCode::Esc);
    key(&mut app, 'a');
    answer(&mut app, "u2");
    answer(&mut app, "0");
    assert!(app.notice.as_deref().unwrap().contains("at least once"));
    press(&mut app, KeyCode::Esc);
    // a count that is no field before this one
    key(&mut app, 'a');
    answer(&mut app, "u2");
    answer(&mut app, "nothing");
    answer(&mut app, "x");
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("no field called nothing"),
        "{:?}",
        app.notice
    );
    press(&mut app, KeyCode::Esc);
    // eos where the rest of the file is not a whole number of elements (16 is not a multiple of 3)
    key(&mut app, 'a');
    answer(&mut app, "bytes");
    answer(&mut app, "3");
    answer(&mut app, "eos");
    answer(&mut app, "x");
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("not a whole number"),
        "{:?}",
        app.notice
    );
    assert_eq!(app.builder.as_ref().unwrap().draft.fields(), []);
}

#[test]
fn a_field_that_a_repeat_counts_by_cannot_be_removed_or_changed_and_undo_still_works() {
    let (mut app, _dir) = building();
    // the count is the third byte of the file, 3
    key(&mut app, 'a');
    answer(&mut app, "bytes");
    answer(&mut app, "2");
    press(&mut app, KeyCode::Enter);
    answer(&mut app, "skipped");
    key(&mut app, 'a');
    answer(&mut app, "u1");
    press(&mut app, KeyCode::Enter);
    answer(&mut app, "n");
    key(&mut app, 'a');
    answer(&mut app, "u2");
    answer(&mut app, "n");
    answer(&mut app, "items");
    app.focus = Focus::Tree;
    app.tree.select_path(&[1]);
    key(&mut app, 'd');
    key(&mut app, 'd');
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("which has to be an integer field"),
        "{:?}",
        app.notice
    );
    assert_eq!(app.builder.as_ref().unwrap().draft.fields().len(), 3);
    for _ in 0..3 {
        key(&mut app, 'u');
    }
    assert_eq!(app.builder.as_ref().unwrap().draft.fields(), []);
}
