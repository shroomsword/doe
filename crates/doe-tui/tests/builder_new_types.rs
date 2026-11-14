//! New types in the schema builder (#413): the fields of selected bytes become a type, the type is used again and repeated,
//! fields are added into it from the tree, everything can be undone, and the saved schema is read by `doe` and by the builder.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::app::{App, Focus};
use doe_tui::draft::{Container, Draft, Kind};
use doe_tui::load::load;
use doe_tui::{Options, prepare_build};

/// version 1, count 2, then two entries of a kind (u1) and a value (u2).
const FILE: [u8; 8] = [1, 2, 10, 0x34, 0x12, 20, 0x78, 0x56];

fn building(bytes: &[u8]) -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("data.bin");
    std::fs::write(&file, bytes).unwrap();
    let options = Options {
        file: file.clone(),
        type_or_schema: String::new(),
        include_paths: vec![],
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
    let (options, builder) = prepare_build(&options, &dir.path().join("list.yaml")).unwrap();
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

/// Add an integer field at the end of the schema: kind, once, name.
fn add_int(app: &mut App, kind: &str, name: &str) {
    app.focus = Focus::Hex;
    key(app, 'a');
    answer(app, kind);
    press(app, KeyCode::Enter);
    answer(app, name);
    assert!(app.prompt.is_none(), "{:?}", app.notice);
}

/// The version, the count, a kind and a value: the fields of the file up to the first entry.
fn four_fields(app: &mut App) {
    add_int(app, "u1", "version");
    add_int(app, "u1", "count");
    add_int(app, "u1", "kind");
    add_int(app, "u2", "value");
}

/// Select the bytes `first` to `last` of the file in the hex pane.
fn select_bytes(app: &mut App, first: u64, last: u64) {
    app.focus = Focus::Hex;
    app.hex.clear_range();
    app.hex.set_cursor(first, 8);
    key(app, 'v');
    for _ in first..last {
        press(app, KeyCode::Right);
    }
}

#[test]
fn the_fields_of_selected_bytes_become_a_type_that_is_repeated_into_a_list_of_entries() {
    let (mut app, _dir) = building(&FILE);
    four_fields(&mut app);
    assert_eq!(draft(&app).fields().len(), 4);
    let before = draft(&app).to_text();

    // the kind and the value, bytes 2 to 4, become the type `entry`
    select_bytes(&mut app, 2, 4);
    key(&mut app, 'Z');
    assert_eq!(asking(&app), Some("name of the new type: "));
    assert_eq!(app.prompt.as_ref().unwrap().line.text(), "group_1");
    answer(&mut app, "entry");
    assert!(app.prompt.is_none(), "{:?}", app.notice);
    let d = draft(&app);
    assert_eq!(
        d.fields().iter().map(|f| f.id.as_str()).collect::<Vec<_>>(),
        ["version", "count", "entry_1"]
    );
    assert_eq!(d.types()[0].id, "entry");
    assert_eq!(d.fields_in(&Container::Type("entry".to_owned())).len(), 2);
    // the type took the bytes the fields took: where the schema ends has not moved
    assert_eq!(d.end(), 5);
    // the tree has the type's fields under the field that has the type, where the fields were
    assert_eq!(app.tree.selected().name, "entry_1");
    let names: Vec<String> = app.tree.rows().iter().map(|r| r.name.clone()).collect();
    assert!(names.contains(&"entry_1".to_owned()), "{names:?}");

    // the field is an element of a list of as many entries as the count says: change its kind to the type, repeated by `count`
    key(&mut app, 'c');
    answer(&mut app, "entry");
    press(&mut app, KeyCode::Enter); // as much as it needs
    answer(&mut app, "count");
    assert!(app.prompt.is_none(), "{:?}", app.notice);
    let list = &draft(&app).fields()[2].kind;
    assert!(
        matches!(list, Kind::Repeat { inner, count: doe_tui::draft::Count::Field(c), .. } if matches!(**inner, Kind::Type { .. }) && c == "count"),
        "{list:?}"
    );
    assert_eq!(
        draft(&app).end(),
        8,
        "the two entries are the rest of the file"
    );
    let yaml = draft(&app).to_yaml();
    assert!(
        yaml.contains("    type: entry\n    repeat: expr\n    repeat-expr: count\n"),
        "{yaml}"
    );
    assert!(yaml.contains("types:\n  entry:\n    seq:\n"), "{yaml}");
    assert!(!yaml.contains("imports:"), "{yaml}");

    // undo takes it back a step at a time: the repeat, then the grouping, to the fields as they were
    key(&mut app, 'u');
    key(&mut app, 'u');
    assert_eq!(draft(&app).to_text(), before);
    assert_eq!(draft(&app).types().len(), 0);
    // and redo does the grouping again
    app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
    assert_eq!(draft(&app).types().len(), 1);
}

#[test]
fn the_saved_schema_is_one_file_that_doe_and_the_builder_read() {
    let (mut app, dir) = building(&FILE);
    four_fields(&mut app);
    select_bytes(&mut app, 2, 4);
    key(&mut app, 'Z');
    answer(&mut app, "entry");
    key(&mut app, 'c');
    answer(&mut app, "entry");
    press(&mut app, KeyCode::Enter);
    answer(&mut app, "count");
    key(&mut app, 'w');
    let path = app.builder.as_ref().unwrap().schema_path().clone();
    let text = std::fs::read_to_string(&path).unwrap();
    // read back by the builder
    let read = Draft::from_yaml(&text).unwrap();
    assert_eq!(read.to_text(), draft(&app).to_text());
    assert_eq!(read.types().len(), 1);
    // and by doe, with no include path: the types are in the file
    let out = doe::parse_file_with(
        &dir.path().join("data.bin"),
        path.to_str().unwrap(),
        &[],
        Some(std::path::Path::new("/nonexistent/config.toml")),
        doe::OutputFormat::Json,
        &doe::ParseOptions::default(),
    )
    .unwrap();
    for wanted in [
        "\"version\":1",
        "\"count\":2",
        "\"kind\":10",
        "\"value\":4660",
        "\"kind\":20",
        "\"value\":22136",
    ] {
        assert!(
            out.replace([' ', '\n'], "").contains(wanted),
            "{wanted} in {out}"
        );
    }
}

#[test]
fn a_selection_that_is_not_whole_fields_is_refused_and_a_name_that_is_taken_is_asked_again() {
    let (mut app, _dir) = building(&FILE);
    four_fields(&mut app);
    // the middle of the value
    select_bytes(&mut app, 3, 3);
    key(&mut app, 'Z');
    assert!(app.prompt.is_none());
    assert!(
        app.notice.as_deref().unwrap().contains("edges of fields"),
        "{:?}",
        app.notice
    );
    // no selection and no field: say so
    app.hex.clear_range();
    app.tree.select_path(&[]);
    key(&mut app, 'Z');
    assert!(app.prompt.is_none());
    assert!(
        app.notice.as_deref().unwrap().contains("select a field"),
        "{:?}",
        app.notice
    );
    // a field selected in the tree is a group of one; its name is the name of the schema: refused, asked again
    app.tree.select_path(&[3]);
    app.focus = Focus::Tree;
    key(&mut app, 'Z');
    assert_eq!(asking(&app), Some("name of the new type: "));
    answer(&mut app, "list");
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("name of the schema"),
        "{:?}",
        app.notice
    );
    assert_eq!(asking(&app), Some("name of the new type: "));
    answer(&mut app, "val");
    assert!(app.prompt.is_none(), "{:?}", app.notice);
    // the same name for another type
    app.tree.select_path(&[1]);
    key(&mut app, 'Z');
    answer(&mut app, "val");
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("type called val already"),
        "{:?}",
        app.notice
    );
    press(&mut app, KeyCode::Esc);
    assert!(app.prompt.is_none());
    assert_eq!(draft(&app).types().len(), 1);
}

#[test]
fn a_new_empty_type_takes_fields_added_into_it_from_the_tree_and_can_be_taken_apart() {
    let (mut app, _dir) = building(&[5, 1, 2, 3]);
    add_int(&mut app, "u1", "count");
    // an empty type, and a field that has it
    key(&mut app, 'a');
    answer(&mut app, "new");
    assert_eq!(asking(&app), Some("name of the new type: "));
    answer(&mut app, "box");
    assert_eq!(asking(&app), Some("field name: "));
    answer(&mut app, "box_1");
    assert!(app.prompt.is_none(), "{:?}", app.notice);
    assert_eq!(draft(&app).types()[0].id, "box");
    assert_eq!(draft(&app).types()[0].fields.len(), 0);
    assert_eq!(draft(&app).end(), 1);
    // the field is selected: `a` in the tree adds into its type, after the fields it has, and the next goes after that
    app.focus = Focus::Tree;
    assert_eq!(app.tree.selected().name, "box_1");
    key(&mut app, 'a');
    answer(&mut app, "u1");
    press(&mut app, KeyCode::Enter);
    answer(&mut app, "first");
    assert!(app.prompt.is_none(), "{:?}", app.notice);
    key(&mut app, 'a');
    answer(&mut app, "u2");
    press(&mut app, KeyCode::Enter);
    answer(&mut app, "second");
    assert!(app.prompt.is_none(), "{:?}", app.notice);
    let inner = Container::Type("box".to_owned());
    assert_eq!(
        draft(&app)
            .fields_in(&inner)
            .iter()
            .map(|f| f.id.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    // the type's fields are in the file where an instance starts: the type took 3 bytes after the count
    assert_eq!(draft(&app).fields()[1].kind.size(), 3);
    assert_eq!(draft(&app).end(), 4);
    // a field of the type is renamed and changed and removed in the type
    app.tree.select_path(&[1, 0]);
    key(&mut app, 'r');
    answer(&mut app, "tag");
    assert_eq!(
        draft(&app).fields_in(&inner)[0].id,
        "tag",
        "{:?}",
        app.notice
    );
    key(&mut app, 'c');
    answer(&mut app, "u1");
    press(&mut app, KeyCode::Enter);
    assert!(app.prompt.is_none(), "{:?}", app.notice);
    app.tree.select_path(&[1, 1]);
    key(&mut app, 'd');
    key(&mut app, 'd');
    assert_eq!(draft(&app).fields_in(&inner).len(), 1, "{:?}", app.notice);
    // taking the type apart puts its field in the schema, and the type is gone
    app.tree.select_path(&[1]);
    key(&mut app, 'O');
    assert!(draft(&app).types().is_empty(), "{:?}", app.notice);
    assert_eq!(
        draft(&app)
            .fields()
            .iter()
            .map(|f| f.id.as_str())
            .collect::<Vec<_>>(),
        ["count", "tag"]
    );
}

#[test]
fn a_type_that_is_used_twice_cannot_be_taken_apart_and_a_defined_type_is_offered_first_in_the_list()
{
    let (mut app, _dir) = building(&[1, 2, 3, 4]);
    add_int(&mut app, "u1", "a");
    add_int(&mut app, "u1", "b");
    app.tree.select_path(&[1]);
    app.focus = Focus::Tree;
    key(&mut app, 'Z');
    answer(&mut app, "item");
    // another field of that type, typed
    app.focus = Focus::Hex;
    key(&mut app, 'a');
    answer(&mut app, "item");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    assert!(app.prompt.is_none(), "{:?}", app.notice);
    assert_eq!(draft(&app).uses_of("item").len(), 2);
    app.tree.select_path(&[1]);
    key(&mut app, 'O');
    assert!(
        app.notice.as_deref().unwrap().contains("used by the field"),
        "{:?}",
        app.notice
    );
    assert_eq!(draft(&app).types().len(), 1);
    // the list of types has it first, in a group of its own
    key(&mut app, 'a');
    answer(&mut app, "type");
    let browser = app.types.as_ref().unwrap();
    assert_eq!(browser.selected_id(), Some("item"));
}
