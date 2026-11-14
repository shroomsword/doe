//! A field of the schema that is being built can have as its type a schema of the type paths (#401): chosen in the browser of
//! types or typed, with a window size and a repeat, imported by the schema, saved, reloaded, and refused when it cannot be
//! used.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::app::{App, Focus};
use doe_tui::draft::{Draft, Kind};
use doe_tui::load::load;
use doe_tui::{Options, prepare_build};

/// The type paths: `pair` (two u2, 4 bytes), `packet` (a length u1 and that many bytes, so its size is not fixed) and `needs`
/// (which takes a parameter); the file: a pair, a packet of 3 bytes, a pair.
fn building() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let types = dir.path().join("types");
    std::fs::create_dir(&types).unwrap();
    std::fs::write(
        types.join("pair.yaml"),
        "meta:\n  id: pair\n  endian: le\ndoc: two numbers\nseq:\n  - id: a\n    type: u2\n  - id: b\n    type: u2\n",
    )
    .unwrap();
    std::fs::write(
        types.join("packet.yaml"),
        "meta:\n  id: packet\nseq:\n  - id: len\n    type: u1\n  - id: body\n    size: len\n",
    )
    .unwrap();
    std::fs::write(
        types.join("needs.yaml"),
        "meta:\n  id: needs\nparams:\n  - id: n\n    type: u4\nseq:\n  - id: x\n    size: n\n",
    )
    .unwrap();
    let file = dir.path().join("data.bin");
    std::fs::write(&file, [1, 0, 2, 0, 3, 0xaa, 0xbb, 0xcc, 5, 0, 6, 0, 9, 9]).unwrap();
    let options = Options {
        file: file.clone(),
        type_or_schema: String::new(),
        include_paths: vec![types],
        color: false,
        watch: false,
        theme: None,
        theme_dir: None,
        config_file: None,
        outer_file: None,
        theme_on_command_line: false,
        transparent: None,
        mouse: true,
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

fn answer(app: &mut App, text: &str) {
    app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    for c in text.chars() {
        key(app, c);
    }
    press(app, KeyCode::Enter);
}

fn asking(app: &App) -> Option<&'static str> {
    app.prompt.as_ref().map(|p| p.kind.label())
}

fn draft(app: &App) -> &Draft {
    &app.builder.as_ref().unwrap().draft
}

fn fields(app: &App) -> Vec<(String, String, u64, u64)> {
    let d = draft(app);
    d.fields()
        .iter()
        .enumerate()
        .map(|(i, f)| (f.id.clone(), f.kind.label(), d.offset_of(i), f.kind.size()))
        .collect()
}

#[test]
fn a_type_chosen_in_the_browser_is_a_field_that_the_schema_imports() {
    let (mut app, _dir) = building();
    key(&mut app, 'a');
    answer(&mut app, "type");
    // the browser of the types of the type paths is open, with the schema being built not among them
    assert!(app.types.is_some() && app.prompt.is_none());
    let browser = app.types.as_mut().unwrap();
    browser.select_type("pair");
    press(&mut app, KeyCode::Enter);
    assert!(app.types.is_none());
    assert!(
        asking(&app).unwrap().starts_with("size in bytes"),
        "{:?}",
        asking(&app)
    );
    press(&mut app, KeyCode::Enter); // as much as it needs
    assert!(asking(&app).unwrap().starts_with("repeat"));
    press(&mut app, KeyCode::Enter); // once
    assert_eq!(asking(&app), Some("field name: "));
    assert_eq!(app.prompt.as_ref().unwrap().line.text(), "pair_1");
    press(&mut app, KeyCode::Enter);
    assert!(app.prompt.is_none());
    // the field takes the bytes the type read in the file
    assert_eq!(
        fields(&app),
        [("pair_1".to_owned(), "pair".to_owned(), 0, 4)]
    );
    assert_eq!(draft(&app).imports(), ["pair"]);
    // the schema says so, and the tree has the type's fields
    let yaml = std::fs::read_to_string(app.builder.as_ref().unwrap().schema_path()).unwrap();
    assert!(yaml.contains("imports:\n  - pair\n"), "{yaml}");
    assert!(yaml.contains("    type: pair\n"), "{yaml}");
    assert_eq!(app.tree.selected().name, "pair_1");
    assert_eq!(app.hex.cursor(), 4);
    // and a field after it starts where the type ended
    key(&mut app, 'a');
    press(&mut app, KeyCode::Enter); // u1
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    assert_eq!(fields(&app).len(), 2, "{:?} {:?}", app.notice, asking(&app));
    assert_eq!(fields(&app)[1].2, 4);
}

#[test]
fn a_type_can_be_typed_by_its_id_with_a_window_and_repeated() {
    let (mut app, _dir) = building();
    key(&mut app, 'a');
    answer(&mut app, "pair");
    assert!(asking(&app).unwrap().starts_with("size in bytes"));
    answer(&mut app, "4"); // read from a window of 4 bytes
    answer(&mut app, "1"); // once
    press(&mut app, KeyCode::Enter);
    assert_eq!(fields(&app)[0].3, 4);
    let yaml = draft(&app).to_text();
    assert!(yaml.contains("    type: pair\n    size: 4"), "{yaml}");

    // a type whose size the file decides is repeated to the end of what it fits: a packet, then pairs
    let (mut app, _dir) = building();
    key(&mut app, 'a');
    answer(&mut app, "pair");
    press(&mut app, KeyCode::Enter);
    answer(&mut app, "2"); // twice: bytes 0..4 and 4..8 ... the second is not a pair of the file's bytes, but it parses
    press(&mut app, KeyCode::Enter);
    assert!(app.prompt.is_none(), "{:?}", app.notice);
    let kind = &draft(&app).fields()[0].kind;
    assert!(
        matches!(kind, Kind::Repeat { inner, .. } if matches!(**inner, Kind::Type { .. })),
        "{kind:?}"
    );
    assert_eq!(draft(&app).fields()[0].kind.size(), 8);
    assert_eq!(draft(&app).imports(), ["pair"]);
}

#[test]
fn a_type_that_is_not_one_cannot_be_used_and_is_asked_again() {
    let (mut app, _dir) = building();
    key(&mut app, 'a');
    answer(&mut app, "nothing_like_it");
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("not a kind of field or a type"),
        "{:?}",
        app.notice
    );
    assert!(asking(&app).unwrap().starts_with("kind ("));
    // a type that takes parameters is refused, and says why
    answer(&mut app, "needs");
    assert!(
        app.notice.as_deref().unwrap().contains("takes parameters"),
        "{:?}",
        app.notice
    );
    assert!(asking(&app).unwrap().starts_with("kind ("));
    // Esc in the browser: no type, the question again
    answer(&mut app, "type");
    press(&mut app, KeyCode::Esc);
    assert!(app.types.is_none());
    assert!(
        asking(&app).unwrap().starts_with("kind ("),
        "{:?}",
        asking(&app)
    );
    assert!(app.notice.as_deref().unwrap().contains("no type chosen"));
}

#[test]
fn a_type_that_does_not_fit_the_file_is_refused_and_the_schema_stays() {
    let dir_file = |app: &App| app.options.as_ref().unwrap().file.clone();
    let (mut app, _dir) = building();
    // cut the file to two bytes: a pair does not fit
    std::fs::write(dir_file(&app), [1, 0]).unwrap();
    app.reload();
    key(&mut app, 'a');
    answer(&mut app, "pair");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    assert!(draft(&app).fields().is_empty(), "{:?}", fields(&app));
    assert!(app.notice.is_some());
}

#[test]
fn a_schema_with_types_is_read_back_by_the_builder() {
    let mut draft = Draft::new("outer").unwrap();
    draft
        .push(doe_tui::draft::Field {
            id: "head".to_owned(),
            kind: Kind::Type {
                id: "pair".to_owned(),
                size: Some(4),
                bytes: 4,
            },
        })
        .unwrap();
    draft
        .push(doe_tui::draft::Field {
            id: "more".to_owned(),
            kind: Kind::Repeat {
                inner: Box::new(Kind::Type {
                    id: "pair".to_owned(),
                    size: None,
                    bytes: 8,
                }),
                count: doe_tui::draft::Count::Fixed(2),
                sample: 2,
            },
        })
        .unwrap();
    let text = draft.to_yaml();
    assert_eq!(text.matches("imports:").count(), 1);
    assert_eq!(text.matches("  - pair\n").count(), 1);
    let read = Draft::from_yaml(&text).unwrap();
    assert_eq!(read.fields().len(), 2);
    assert_eq!(read.to_text(), draft.to_text());
    assert_eq!(read.imports(), ["pair"]);
    // a type that is not imported is not one the builder makes
    let bad = text.replace("imports:\n  - pair\n", "");
    assert!(Draft::from_yaml(&bad).is_err());
}

#[test]
fn a_type_that_imports_the_schema_being_built_is_refused_with_the_reason() {
    let (mut app, dir) = building();
    // the schema being built is `built`; this type imports it
    std::fs::write(
        dir.path().join("types/loopy.yaml"),
        "meta:\n  id: loopy\nimports:\n  - built\nseq:\n  - id: x\n    type: u1\n",
    )
    .unwrap();
    key(&mut app, 'a');
    answer(&mut app, "loopy");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    assert!(draft(&app).fields().is_empty(), "{:?}", fields(&app));
    let notice = app.notice.clone().unwrap_or_default();
    assert!(
        notice.to_lowercase().contains("built")
            || notice.to_lowercase().contains("circular")
            || notice.to_lowercase().contains("import"),
        "{notice}"
    );
}

#[test]
fn the_saved_schema_compiles_with_the_library_and_parses_a_real_file() {
    let library = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../types");
    let dir = tempfile::tempdir().unwrap();
    let file = library.join("samples/sample.a");
    let options = Options {
        file: file.clone(),
        type_or_schema: String::new(),
        include_paths: vec![library.clone()],
        color: false,
        watch: false,
        theme: None,
        theme_dir: None,
        config_file: None,
        outer_file: None,
        theme_on_command_line: false,
        transparent: None,
        mouse: true,
        radix: None,
    };
    let (options, builder) = prepare_build(&options, &dir.path().join("archive.yaml")).unwrap();
    let mut app = App::new(file.clone(), load(&options).unwrap());
    app.options = Some(options);
    app.builder = Some(builder);
    app.focus = Focus::Hex;
    key(&mut app, 'a');
    answer(&mut app, "ar");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    assert!(app.prompt.is_none(), "{:?}", app.notice);
    assert_eq!(draft(&app).imports(), ["ar"]);
    assert_eq!(
        draft(&app).fields()[0].kind.size(),
        std::fs::metadata(&file).unwrap().len()
    );
    // the schema on disk is a schema `doe` reads, with the library's `ar` inside it
    let schema = app.builder.as_ref().unwrap().schema_path().clone();
    let out = doe::parse_file_with(
        &file,
        schema.to_str().unwrap(),
        &[library],
        Some(std::path::Path::new("/nonexistent/config.toml")),
        doe::OutputFormat::Json,
        &doe::ParseOptions::default(),
    )
    .unwrap();
    assert!(
        out.contains("hello.txt") || out.contains("entries"),
        "{}",
        out.chars().take(300).collect::<String>()
    );
}
