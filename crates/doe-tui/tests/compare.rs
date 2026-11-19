//! Comparing two files with one schema in the terminal viewer (#423): the marks, the ghosts, the keys that go from one
//! difference to the next, `V`, and the two ways to draw it. On the test backend.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe::compare::Mark;
use doe_tui::app::{App, Focus};
use doe_tui::load::try_load;
use doe_tui::tree::RowKind;
use doe_tui::ui::{Theme, draw};
use doe_tui::{CompareView, Comparing, Options};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Modifier;

const SCHEMA: &str = "meta:\n  id: s\n  endian: le\nseq:\n  - id: version\n    type: u1\n  - id: count\n    type: u1\n  - id: items\n    type: item\n    repeat: expr\n    repeat-expr: count\n  - id: tail\n    type: u2\ntypes:\n  item:\n    seq:\n      - id: a\n        type: u1\n      - id: b\n        type: u1\n";

/// version 1, two items, a tail.
const A: [u8; 8] = [1, 2, 10, 11, 12, 13, 7, 0];

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new(a: &[u8], b: &[u8]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("s.yaml"), SCHEMA).unwrap();
        std::fs::write(dir.path().join("a.bin"), a).unwrap();
        std::fs::write(dir.path().join("b.bin"), b).unwrap();
        Fixture { dir }
    }

    fn options(&self, view: CompareView) -> Options {
        Options {
            file: self.dir.path().join("a.bin"),
            type_or_schema: self
                .dir
                .path()
                .join("s.yaml")
                .to_string_lossy()
                .into_owned(),
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
            compare: Some(Comparing {
                file: self.dir.path().join("b.bin"),
                side_by_side: view == CompareView::SideBySide,
                by: Vec::new(),
            }),
            restore: None,
            radix: None,
        }
    }

    /// The viewer as the program starts it: the first file is parsed, then the second, and they are compared.
    fn open(&self, view: CompareView) -> App {
        let options = self.options(view);
        let mut app = App::loading(options.file.clone(), &options.type_or_schema);
        app.options = Some(options.clone());
        app.finish_loading(try_load(&options));
        let mut other = options.clone();
        other
            .file
            .clone_from(&options.compare.as_ref().unwrap().file);
        app.finish_compare(try_load(&other));
        app
    }
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn key(app: &mut App, c: char) {
    press(app, KeyCode::Char(c));
}

fn names(app: &App) -> Vec<String> {
    app.tree.rows().iter().map(|r| r.name.clone()).collect()
}

fn selected(app: &App) -> String {
    app.tree.selected().name.clone()
}

fn screen(app: &mut App, width: u16, height: u16) -> (Vec<String>, Terminal<TestBackend>) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| draw(f, app, &Theme::new(false))).unwrap();
    let buffer = terminal.backend().buffer();
    let lines = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect();
    (lines, terminal)
}

/// Open `items` and the first two elements, so that the rows of the fields are there.
fn open_all(app: &mut App) {
    key(app, 'j');
    key(app, 'j');
    key(app, 'l');
    key(app, 'j');
    key(app, 'l');
    key(app, 'j');
    key(app, 'j');
    key(app, 'k');
    key(app, 'k');
    key(app, 'k');
    key(app, 'k');
}

#[test]
fn the_tree_of_the_first_file_is_marked_with_what_differs_and_the_title_counts_it() {
    let mut b = A;
    b[3] = 99; // items[0].b
    let f = Fixture::new(&A, &b);
    let mut app = f.open(CompareView::Annotated);
    assert!(app.comparing());
    assert_eq!(app.compare_letter(), Some('A'));
    // open the root's children one by one: items, items[0]
    for _ in 0..3 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(selected(&app), "items");
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Right);
    let rows = app.tree.rows();
    let mark = |name: &str| rows.iter().find(|r| r.name == name).and_then(|r| r.mark);
    assert_eq!(rows[0].mark, Some(Mark::Inside));
    assert_eq!(rows[0].below, 1);
    assert_eq!(mark("items"), Some(Mark::Inside));
    assert_eq!(mark("[0]"), Some(Mark::Inside));
    assert_eq!(mark("b"), Some(Mark::Changed));
    assert_eq!(mark("a"), None);
    assert_eq!(mark("version"), None);
    // the title says how many differ
    let (lines, _) = screen(&mut app, 120, 20);
    assert!(
        lines[0].contains("A: 1 of 11 fields differ"),
        "{}",
        lines[0]
    );
    assert!(
        lines.iter().any(|l| l.contains("≠") && l.contains(" b")),
        "{lines:?}"
    );
    assert!(lines.iter().any(|l| l.contains("±")), "{lines:?}");
}

#[test]
fn files_that_are_the_same_say_so() {
    let f = Fixture::new(&A, &A);
    let mut app = f.open(CompareView::Annotated);
    assert_eq!(app.compare_summary().as_deref(), Some("identical"));
    let (lines, _) = screen(&mut app, 120, 20);
    assert!(lines[0].contains("A: identical"), "{}", lines[0]);
    assert!(app.tree.rows().iter().all(|r| r.mark.is_none()));
    // `>` has nowhere to go
    key(&mut app, '>');
    assert_eq!(app.notice.as_deref(), Some("nothing differs"));
}

#[test]
fn what_only_the_second_file_has_is_shown_under_its_parent_and_what_only_the_first_has_is_marked() {
    let one = [1, 1, 10, 11, 7, 0];
    let mut app = Fixture::new(&one, &A).open(CompareView::Annotated);
    // open `items`: its one element, and a ghost for the element that only the second file has
    for _ in 0..3 {
        press(&mut app, KeyCode::Down);
    }
    press(&mut app, KeyCode::Right);
    let rows = app.tree.rows();
    let ghost = rows
        .iter()
        .find(|r| matches!(r.kind, RowKind::Ghost { .. }))
        .expect("a ghost row");
    assert_eq!(ghost.name, "[1]");
    let (lines, _) = screen(&mut app, 120, 20);
    assert!(
        lines.iter().any(|l| l.contains('+') && l.contains("[1]")),
        "{lines:?}"
    );
    // and the other way round: the first file has the element that the second has not
    let mut app = Fixture::new(&A, &one).open(CompareView::Annotated);
    for _ in 0..3 {
        press(&mut app, KeyCode::Down);
    }
    press(&mut app, KeyCode::Right);
    let only = app.tree.rows().iter().find(|r| r.name == "[1]").unwrap();
    assert_eq!(only.mark, Some(Mark::Only));
    assert!(
        app.tree
            .rows()
            .iter()
            .all(|r| !matches!(r.kind, RowKind::Ghost { .. }))
    );
    let summary = app.compare_summary().unwrap();
    assert!(summary.contains("1 only in A"), "{summary}");
}

#[test]
fn greater_and_less_go_from_one_difference_to_the_next_and_round() {
    let mut b = A;
    b[0] = 2; // version
    b[3] = 99; // items[0].b
    b[6] = 8; // tail
    let f = Fixture::new(&A, &b);
    let mut app = f.open(CompareView::Annotated);
    assert_eq!(selected(&app), "s");
    key(&mut app, '>');
    assert_eq!(selected(&app), "version");
    key(&mut app, '>');
    // the tree opened itself down to the field
    assert_eq!(selected(&app), "b");
    assert_eq!(app.hex.cursor(), 3, "the bytes follow");
    key(&mut app, '>');
    assert_eq!(selected(&app), "tail");
    key(&mut app, '>');
    assert_eq!(selected(&app), "version");
    assert_eq!(app.notice.as_deref(), Some("back at the first difference"));
    key(&mut app, '<');
    assert_eq!(selected(&app), "tail");
    assert_eq!(app.notice.as_deref(), Some("back at the last difference"));
    key(&mut app, '<');
    assert_eq!(selected(&app), "b");
}

#[test]
fn equals_leaves_out_what_is_the_same() {
    let mut b = A;
    b[3] = 99;
    let f = Fixture::new(&A, &b);
    let mut app = f.open(CompareView::Annotated);
    open_all(&mut app);
    key(&mut app, '>');
    let all = names(&app);
    assert!(
        all.contains(&"version".to_owned()) && all.contains(&"tail".to_owned()),
        "{all:?}"
    );
    key(&mut app, '=');
    let only = names(&app);
    assert!(!only.contains(&"version".to_owned()), "{only:?}");
    assert!(!only.contains(&"tail".to_owned()));
    assert!(only.contains(&"b".to_owned()), "{only:?}");
    assert!(
        !only.contains(&"a".to_owned()),
        "the field that is the same in the item is left out"
    );
    key(&mut app, '=');
    assert!(names(&app).contains(&"version".to_owned()));
}

#[test]
fn the_bytes_that_differ_are_marked_and_greater_goes_to_them_in_the_bytes_pane() {
    let mut b = A;
    b[3] = 99;
    b[6] = 8;
    let f = Fixture::new(&A, &b);
    let mut app = f.open(CompareView::Annotated);
    let (_, terminal) = screen(&mut app, 120, 20);
    // the bytes 3 and 6 are drawn underlined, the others are not
    let buffer = terminal.backend().buffer();
    let underlined: Vec<String> = (0..buffer.area.height)
        .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
        .filter(|&(x, y)| buffer[(x, y)].modifier.contains(Modifier::UNDERLINED))
        .map(|(x, y)| buffer[(x, y)].symbol().to_owned())
        .collect();
    // this file has 0x0b at byte 3 and 0x07 at byte 6, in the hex part and as text
    assert!(underlined.iter().any(|s| s == "b"), "{underlined:?}");
    assert!(underlined.iter().any(|s| s == "7"), "{underlined:?}");
    assert_eq!(app.changed_bytes(), &[(3, 4), (6, 7)]);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Focus::Hex);
    key(&mut app, '>');
    assert_eq!(app.hex.cursor(), 3);
    key(&mut app, '>');
    assert_eq!(app.hex.cursor(), 6);
    key(&mut app, '>');
    assert_eq!(app.hex.cursor(), 3);
    assert_eq!(app.notice.as_deref(), Some("back at the first difference"));
    key(&mut app, '<');
    assert_eq!(app.hex.cursor(), 6);
}

#[test]
fn v_shows_the_other_file_at_the_same_field_and_goes_back() {
    let mut b = A;
    b[3] = 99;
    let f = Fixture::new(&A, &b);
    let mut app = f.open(CompareView::Annotated);
    key(&mut app, '>');
    key(&mut app, '>');
    assert_eq!(selected(&app), "b");
    key(&mut app, 'V');
    assert_eq!(app.compare_letter(), Some('B'));
    assert_eq!(selected(&app), "b", "the same field");
    assert!(app.file.ends_with("b.bin"));
    assert_eq!(app.changed_bytes(), &[(3, 4)]);
    // the title and the bytes say which file
    let (lines, _) = screen(&mut app, 120, 20);
    assert!(
        lines[0].contains("B: 1 of 11 fields differ"),
        "{}",
        lines[0]
    );
    assert!(lines.iter().any(|l| l.contains("file B")), "{lines:?}");
    // a field read in the other file shows the value of that file (99 = 0x63)
    let row = app.tree.selected().clone();
    assert_eq!(row.summary.as_deref(), Some("99"));
    key(&mut app, 'V');
    assert_eq!(app.compare_letter(), Some('A'));
    assert_eq!(app.tree.selected().summary.as_deref(), Some("11"));
    assert!(app.file.ends_with("a.bin"));
}

#[test]
fn v_goes_to_where_the_field_is_in_the_other_file_when_it_moved() {
    // `tail` is at byte 4 in the first file and at byte 6 in the second, and it is the same
    let one = [1, 1, 10, 11, 7, 0];
    let mut app = Fixture::new(&one, &A).open(CompareView::Annotated);
    for _ in 0..4 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(selected(&app), "tail");
    assert_eq!(app.hex.cursor(), 4);
    assert_eq!(
        app.tree.selected().mark,
        None,
        "a field that only moved is not a difference"
    );
    key(&mut app, 'V');
    assert_eq!(selected(&app), "tail");
    assert_eq!(app.hex.cursor(), 6);
    key(&mut app, 'V');
    assert_eq!(app.hex.cursor(), 4);
}

#[test]
fn the_inspector_shows_the_value_in_the_other_file() {
    let mut b = A;
    b[3] = 99;
    let f = Fixture::new(&A, &b);
    let mut app = f.open(CompareView::Annotated);
    key(&mut app, '>');
    key(&mut app, '>');
    assert_eq!(app.other_value().as_deref(), Some("B: 99"));
    key(&mut app, 'V');
    assert_eq!(app.other_value().as_deref(), Some("A: 11"));
    // a field that the other file does not have
    let one = [1, 1, 10, 11, 7, 0];
    let mut app = Fixture::new(&A, &one).open(CompareView::Annotated);
    for _ in 0..3 {
        press(&mut app, KeyCode::Down);
    }
    press(&mut app, KeyCode::Right);
    key(&mut app, 'j');
    key(&mut app, 'j');
    assert_eq!(selected(&app), "[1]");
    assert_eq!(app.other_value().as_deref(), Some("B: nothing like it"));
}

#[test]
fn side_by_side_draws_both_trees_under_their_files_and_they_move_together() {
    let mut b = A;
    b[3] = 99;
    let f = Fixture::new(&A, &b);
    let mut app = f.open(CompareView::SideBySide);
    // no ghosts in the first tree: the second has its own
    assert!(
        app.tree
            .rows()
            .iter()
            .all(|r| !matches!(r.kind, RowKind::Ghost { .. }))
    );
    key(&mut app, '>');
    key(&mut app, '>');
    assert_eq!(selected(&app), "b");
    let (lines, _) = screen(&mut app, 140, 20);
    // the files' names over the trees
    assert!(
        lines
            .iter()
            .any(|l| l.contains("A · a.bin") && l.contains("B · b.bin")),
        "{lines:?}"
    );
    // the field is selected in both: the two rows show 11 and 99 on the same line
    let both = lines
        .iter()
        .find(|l| l.contains("b = 11") && l.contains("b = 99"))
        .unwrap_or_else(|| panic!("{lines:?}"));
    assert!(both.find("b = 11").unwrap() < both.find("b = 99").unwrap());
    // what is open here is open there
    let (lines, _) = screen(&mut app, 140, 20);
    assert!(
        lines
            .iter()
            .filter(|l| l.contains("[0]"))
            .all(|l| l.matches("[0]").count() == 2),
        "{lines:?}"
    );
    // the second tree has the marks of the second file
    assert!(
        lines.iter().any(|l| l.matches('≠').count() == 2),
        "{lines:?}"
    );
}

#[test]
fn the_bar_switches_between_the_two_ways_and_the_ghosts_follow() {
    let one = [1, 1, 10, 11, 7, 0];
    let mut app = Fixture::new(&one, &A).open(CompareView::Annotated);
    for _ in 0..3 {
        press(&mut app, KeyCode::Down);
    }
    press(&mut app, KeyCode::Right);
    assert!(
        app.tree
            .rows()
            .iter()
            .any(|r| matches!(r.kind, RowKind::Ghost { .. }))
    );
    key(&mut app, '|');
    assert!(app.compare.as_ref().unwrap().side_by_side);
    assert!(
        app.tree
            .rows()
            .iter()
            .all(|r| !matches!(r.kind, RowKind::Ghost { .. }))
    );
    let (lines, _) = screen(&mut app, 140, 20);
    assert!(lines.iter().any(|l| l.contains("B · b.bin")), "{lines:?}");
    key(&mut app, '|');
    assert!(!app.compare.as_ref().unwrap().side_by_side);
    assert!(
        app.tree
            .rows()
            .iter()
            .any(|r| matches!(r.kind, RowKind::Ghost { .. }))
    );
}

#[test]
fn the_footer_names_the_keys_of_a_comparison_and_reading_again_compares_again() {
    let mut b = A;
    b[3] = 99;
    let f = Fixture::new(&A, &b);
    let mut app = f.open(CompareView::Annotated);
    let (lines, _) = screen(&mut app, 140, 20);
    let footer = &lines[lines.len() - 2];
    assert!(
        footer.contains("> next diff") && footer.contains("V other file"),
        "{footer}"
    );
    // the second file changes on disk, and `r` reads both again
    let mut changed = A;
    changed[3] = 99;
    changed[0] = 5;
    std::fs::write(f.dir.path().join("b.bin"), changed).unwrap();
    key(&mut app, 'r');
    assert_eq!(
        app.compare_summary().as_deref(),
        Some("2 of 11 fields differ")
    );
    // while the other file is shown, `r` reads the first one: it is shown again
    key(&mut app, 'V');
    key(&mut app, 'r');
    assert_eq!(app.compare_letter(), Some('A'));
}

#[test]
fn a_second_file_that_cannot_be_read_is_said_and_the_first_is_shown_as_it_was() {
    let f = Fixture::new(&A, &A);
    let mut options = f.options(CompareView::Annotated);
    options.compare.as_mut().unwrap().file = f.dir.path().join("missing.bin");
    let mut app = App::loading(options.file.clone(), &options.type_or_schema);
    app.options = Some(options.clone());
    app.finish_loading(try_load(&options));
    let mut other = options.clone();
    other.file = options.compare.as_ref().unwrap().file.clone();
    app.finish_compare(try_load(&other));
    assert!(!app.comparing());
    let notice = app.notice.clone().unwrap();
    assert!(
        notice.contains("could not read") && notice.contains("missing.bin"),
        "{notice}"
    );
    // the keys say so
    key(&mut app, '>');
    assert_eq!(
        app.notice.as_deref(),
        Some("not comparing: start with --compare FILE")
    );
}

#[test]
fn the_ways_to_draw_a_comparison_have_names() {
    assert_eq!("annotated".parse(), Ok(CompareView::Annotated));
    assert_eq!("side-by-side".parse(), Ok(CompareView::SideBySide));
    assert_eq!("Side-By-Side".parse(), Ok(CompareView::SideBySide));
    let error = "both".parse::<CompareView>().unwrap_err();
    assert!(
        error.to_string().contains("annotated or side-by-side"),
        "{error}"
    );
    assert_eq!(CompareView::default(), CompareView::Annotated);
}

#[test]
fn where_you_were_is_neither_read_nor_written_while_comparing() {
    let f = Fixture::new(&A, &A);
    let state = tempfile::tempdir().unwrap();
    let mut options = f.options(CompareView::Annotated);
    options.restore = Some(state.path().to_path_buf());
    let mut app = App::loading(options.file.clone(), &options.type_or_schema);
    app.options = Some(options.clone());
    app.finish_loading(try_load(&options));
    key(&mut app, 'j');
    app.save_state();
    assert!(!state.path().join("viewer").exists(), "nothing was written");
}
