//! The mouse (#430): clicks, double clicks, drags and the wheel, with synthetic events after a real drawing, in each
//! layout; the pointer that only moves; and the mouse that the viewer gives back with `z`.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use std::fmt::Write as _;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use doe_tui::Options;
use doe_tui::app::{App, Focus};
use doe_tui::load::load;
use doe_tui::ui::{Theme, draw};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

/// Four fields of two bytes, the last a type of two; in a file of 16 bytes.
const SCHEMA: &str = "meta:\n  id: s\n  endian: le\nseq:\n  - id: first\n    type: u2\n  - id: second\n    type: u2\n  - id: third\n    type: u2\n  - id: pair\n    type: pair\ntypes:\n  pair:\n    seq:\n      - id: x\n        type: u1\n      - id: y\n        type: u1\n";

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
        radix: None,
    }
}

fn app_with(schema: &str, data: &[u8]) -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("s.yaml"), schema).unwrap();
    std::fs::write(dir.path().join("s.bin"), data).unwrap();
    let options = options(&dir);
    let mut app = App::new(options.file.clone(), load(&options).unwrap());
    app.options = Some(options);
    app.mouse_on = true;
    (app, dir)
}

/// A schema of `count` fields of one byte.
fn many_fields(count: usize) -> String {
    let mut schema = String::from("meta:\n  id: s\nseq:\n");
    for i in 0..count {
        let _ = write!(schema, "  - id: f{i}\n    type: u1\n");
    }
    schema
}

fn app() -> (App, tempfile::TempDir) {
    app_with(SCHEMA, &[1, 0, 2, 0, 3, 0, 4, 5])
}

fn draw_now(app: &mut App, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| draw(f, app, &Theme::new(false))).unwrap();
    terminal
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn click(app: &mut App, column: u16, row: u16) -> bool {
    let down = app.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), column, row));
    app.on_mouse(mouse(MouseEventKind::Up(MouseButton::Left), column, row));
    down
}

fn key(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// The cell of the byte `offset` on the first line of the bytes, in its hex part.
fn byte_cell(app: &App, offset: u16) -> (u16, u16) {
    let hex = app.hits.hex.unwrap();
    let start = u16::try_from(hex.digits).unwrap() + 2;
    (hex.area.x + start + 3 * offset + offset / 8, hex.area.y)
}

/// The cell of a row of the tree: its second cell, the row's index from the first one shown.
fn row_cell(app: &App, index: usize) -> (u16, u16) {
    let rows = app.hits.rows.unwrap();
    (
        rows.area.x + 8,
        rows.area.y + u16::try_from(index - rows.first).unwrap(),
    )
}

fn selected_name(app: &App) -> String {
    app.tree.selected().name.clone()
}

#[test]
fn a_click_on_a_row_selects_it_and_gives_the_tree_the_keys_side_by_side() {
    let (mut app, _dir) = app();
    draw_now(&mut app, 120, 24);
    assert!(app.hits.tree_pane.is_some() && app.hits.hex_pane.is_some());
    app.focus = Focus::Hex;
    // the rows: the root, first, second, third, pair
    let (column, row) = row_cell(&app, 2);
    assert!(click(&mut app, column, row));
    assert_eq!(selected_name(&app), "second");
    assert_eq!(app.focus, Focus::Tree);
    // the bytes follow the tree, as they do for the keys
    assert_eq!(app.hex.cursor(), 2);
}

#[test]
fn the_marker_and_a_double_click_open_and_close_a_row() {
    let (mut app, _dir) = app();
    draw_now(&mut app, 120, 24);
    let before = app.tree.rows().len();
    // `pair` is the fifth row after the root (index 4), one level in: the flag cell, two of indent, then the marker
    let pair = app
        .tree
        .rows()
        .iter()
        .position(|r| r.name == "pair")
        .unwrap();
    let rows = app.hits.rows.unwrap();
    let marker = (
        rows.area.x + 1 + 2,
        rows.area.y + u16::try_from(pair).unwrap(),
    );
    click(&mut app, marker.0, marker.1);
    assert_eq!(selected_name(&app), "pair");
    assert!(app.tree.rows().len() > before, "the click on ▸ opened it");
    // a double click on the name closes it
    draw_now(&mut app, 120, 24);
    let (column, row) = row_cell(&app, pair);
    click(&mut app, column, row);
    click(&mut app, column, row);
    assert_eq!(app.tree.rows().len(), before);
    // two slow clicks are not a double click
    draw_now(&mut app, 120, 24);
    click(&mut app, column, row);
    std::thread::sleep(std::time::Duration::from_millis(450));
    click(&mut app, column, row);
    assert_eq!(app.tree.rows().len(), before);
}

#[test]
fn a_click_on_a_byte_puts_the_cursor_there_and_selects_its_field() {
    let (mut app, _dir) = app();
    draw_now(&mut app, 120, 24);
    app.focus = Focus::Tree;
    // byte 6 is `x` of the pair, in the hex part
    let (column, row) = byte_cell(&app, 6);
    click(&mut app, column, row);
    assert_eq!(app.focus, Focus::Hex);
    assert_eq!(app.hex.cursor(), 6);
    assert_eq!(selected_name(&app), "x");
    // and in the text part
    let hex = app.hits.hex.unwrap();
    let per_line = u16::try_from(hex.per_line).unwrap();
    let text = hex.area.x
        + u16::try_from(hex.digits).unwrap()
        + 2
        + (3 * per_line - 1 + (per_line - 1) / 8)
        + 2;
    click(&mut app, text + 2, hex.area.y);
    assert_eq!(app.hex.cursor(), 2);
    assert_eq!(selected_name(&app), "second");
}

#[test]
fn a_drag_over_bytes_selects_a_range_and_the_status_counts_it() {
    let (mut app, _dir) = app();
    let terminal = draw_now(&mut app, 120, 24);
    drop(terminal);
    let (from, row) = byte_cell(&app, 1);
    let (to, _) = byte_cell(&app, 4);
    app.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), from, row));
    assert_eq!(
        app.hex.selected_range(),
        None,
        "a press alone selects nothing"
    );
    assert!(app.on_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), to, row)));
    assert_eq!(app.hex.selected_range(), Some((1, 4)));
    app.on_mouse(mouse(MouseEventKind::Up(MouseButton::Left), to, row));
    // a drag the other way is the same range
    let (back, _) = byte_cell(&app, 0);
    app.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), to, row));
    app.on_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), back, row));
    assert_eq!(app.hex.selected_range(), Some((0, 4)));
    let terminal = draw_now(&mut app, 120, 24);
    let buffer = terminal.backend().buffer();
    let screen: String = (0..buffer.area.height)
        .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
        .map(|(x, y)| buffer[(x, y)].symbol())
        .collect();
    assert!(screen.contains("5 bytes selected"), "{screen}");
}

#[test]
fn the_layouts_that_stack_or_fill_the_screen_have_their_own_cells() {
    // stacked: a narrow terminal that is tall enough
    let (mut app, _dir) = app();
    draw_now(&mut app, 70, 40);
    let tree = app.hits.tree_pane.unwrap();
    let hex = app.hits.hex_pane.unwrap();
    assert!(
        hex.y >= tree.y + tree.height,
        "the bytes are below the tree"
    );
    let (column, row) = row_cell(&app, 3);
    click(&mut app, column, row);
    assert_eq!(selected_name(&app), "third");
    let (column, row) = byte_cell(&app, 6);
    click(&mut app, column, row);
    assert_eq!(app.hex.cursor(), 6);
    // the tree fills the screen: there is no hex pane to hit
    let (mut app, _dir) = app_with(SCHEMA, &[1, 0, 2, 0, 3, 0, 4, 5]);
    app.fullscreen = true;
    app.focus = Focus::Tree;
    draw_now(&mut app, 120, 24);
    assert!(app.hits.hex_pane.is_none() && app.hits.hex.is_none());
    assert_eq!(app.hits.tree_pane.unwrap().width, 120);
    let (column, row) = row_cell(&app, 1);
    click(&mut app, column, row);
    assert_eq!(selected_name(&app), "first");
    // the bytes fill the screen: there is no tree to hit, and a click on a row's cell is a byte
    let (mut app, _dir) = app_with(SCHEMA, &[1, 0, 2, 0, 3, 0, 4, 5]);
    app.fullscreen = true;
    app.focus = Focus::Hex;
    draw_now(&mut app, 120, 24);
    assert!(app.hits.tree_pane.is_none() && app.hits.rows.is_none());
    let (column, row) = byte_cell(&app, 2);
    click(&mut app, column, row);
    assert_eq!(app.hex.cursor(), 2);
}

#[test]
fn a_tree_that_scrolls_does_not_take_its_scrollbar_for_a_row() {
    // 60 fields: more rows than the pane has
    let schema = many_fields(60);
    let (mut app, _dir) = app_with(&schema, &[0; 60]);
    draw_now(&mut app, 120, 20);
    let rows = app.hits.rows.unwrap();
    let pane = app.hits.tree_pane.unwrap();
    // the last column inside the pane is the scrollbar
    assert_eq!(rows.area.x + rows.area.width + 2, pane.x + pane.width);
    let before = selected_name(&app);
    let scrollbar = (rows.area.x + rows.area.width, rows.area.y + 2);
    click(&mut app, scrollbar.0, scrollbar.1);
    assert_eq!(
        selected_name(&app),
        before,
        "a click on the scrollbar selects no row"
    );
    assert_eq!(app.focus, Focus::Tree);
}

#[test]
fn the_wheel_scrolls_the_pane_under_the_pointer_and_a_key_brings_the_selection_back() {
    let schema = many_fields(60);
    let (mut app, _dir) = app_with(&schema, &[0; 60]);
    draw_now(&mut app, 120, 20);
    let rows = app.hits.rows.unwrap();
    assert_eq!(rows.first, 0);
    let selected = app.tree.selected_index();
    // three rows a notch, over the tree
    let (column, row) = (rows.area.x + 4, rows.area.y + 2);
    assert!(app.on_mouse(mouse(MouseEventKind::ScrollDown, column, row)));
    draw_now(&mut app, 120, 20);
    assert_eq!(app.hits.rows.unwrap().first, 3);
    assert_eq!(
        app.tree.selected_index(),
        selected,
        "the selection did not move"
    );
    // it stays where it was put, through other drawings
    draw_now(&mut app, 120, 20);
    assert_eq!(app.hits.rows.unwrap().first, 3);
    app.on_mouse(mouse(MouseEventKind::ScrollUp, column, row));
    app.on_mouse(mouse(MouseEventKind::ScrollUp, column, row));
    draw_now(&mut app, 120, 20);
    assert_eq!(app.hits.rows.unwrap().first, 0, "not past the top");
    // far down, then a key: the view comes back to the selection
    for _ in 0..30 {
        app.on_mouse(mouse(MouseEventKind::ScrollDown, column, row));
    }
    draw_now(&mut app, 120, 20);
    let rows = app.hits.rows.unwrap();
    assert_eq!(
        rows.first,
        rows.len - usize::from(rows.area.height),
        "the last row is at the bottom"
    );
    key(&mut app, KeyCode::Down);
    draw_now(&mut app, 120, 20);
    assert_eq!(app.hits.rows.unwrap().first, app.tree.selected_index());
}

#[test]
fn the_wheel_over_the_bytes_scrolls_the_bytes_and_not_the_tree() {
    let (mut app, _dir) = app_with(
        "meta:\n  id: s\nseq:\n  - id: all\n    size: 600\n",
        &[7; 600],
    );
    draw_now(&mut app, 120, 20);
    let hex = app.hits.hex.unwrap();
    assert_eq!(hex.top, 0);
    let tree_first = app.hits.rows.unwrap().first;
    app.on_mouse(mouse(
        MouseEventKind::ScrollDown,
        hex.area.x + 10,
        hex.area.y + 1,
    ));
    draw_now(&mut app, 120, 20);
    let hex = app.hits.hex.unwrap();
    assert_eq!(hex.top, 3 * hex.per_line as u64);
    assert_eq!(app.hits.rows.unwrap().first, tree_first);
    assert_eq!(app.hex.cursor(), 0, "the cursor stays where it was");
}

#[test]
fn a_pointer_that_only_moves_changes_nothing_and_a_click_outside_is_nothing() {
    let (mut app, _dir) = app();
    draw_now(&mut app, 120, 24);
    assert!(!app.on_mouse(mouse(MouseEventKind::Moved, 10, 5)));
    assert!(!app.on_mouse(mouse(MouseEventKind::Down(MouseButton::Right), 10, 5)));
    assert!(!app.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 500, 500)));
}

#[test]
fn z_gives_the_mouse_back_and_takes_it_again_and_a_refused_mouse_stays_off() {
    let (mut app, _dir) = app();
    draw_now(&mut app, 120, 24);
    key(&mut app, KeyCode::Char('z'));
    assert!(!app.mouse_on);
    assert_eq!(app.take_mouse_change(), Some(false));
    assert_eq!(app.take_mouse_change(), None);
    let (column, row) = row_cell(&app, 2);
    assert!(!click(&mut app, column, row), "the terminal has the mouse");
    key(&mut app, KeyCode::Char('z'));
    assert!(app.mouse_on);
    assert_eq!(app.take_mouse_change(), Some(true));
    // --no-mouse: z does not take it
    let (mut app, _dir) = app_with(SCHEMA, &[0; 8]);
    app.mouse_on = false;
    app.mouse_refused = true;
    key(&mut app, KeyCode::Char('z'));
    assert!(!app.mouse_on);
    assert_eq!(app.take_mouse_change(), None);
    assert!(app.notice.as_deref().unwrap().contains("--no-mouse"));
}

#[test]
fn a_dialog_has_the_screen_and_the_mouse_leaves_it_alone() {
    let (mut app, _dir) = app();
    draw_now(&mut app, 120, 24);
    key(&mut app, KeyCode::Char('?'));
    assert!(app.help);
    let (column, row) = row_cell(&app, 2);
    assert!(!click(&mut app, column, row));
    assert_eq!(selected_name(&app), "s");
}

#[test]
fn a_click_on_an_entry_of_the_files_selects_it_and_the_tree_and_the_bytes_follow() {
    let types = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../types");
    let mut options = options(&tempfile::tempdir().unwrap());
    options.file = types.join("samples/sample.tar");
    options.type_or_schema = "tar".to_owned();
    options.include_paths = vec![types];
    let mut app = App::new(options.file.clone(), load(&options).unwrap());
    app.options = Some(options);
    app.mouse_on = true;
    assert!(app.files_shown());
    draw_now(&mut app, 130, 16);
    let rows = app.hits.rows.unwrap();
    assert!(rows.files);
    assert!(rows.len >= 2);
    let (column, row) = (rows.area.x + 6, rows.area.y + 1);
    assert!(click(&mut app, column, row));
    let files = app.files.as_ref().unwrap();
    assert_eq!(files.selected_index(), 1);
    let entry = files.selected_entry().unwrap();
    assert_eq!(app.tree.selected_path(), entry.trace_path.as_slice());
    assert_eq!(app.focus, Focus::Tree);
}

/// The text on the screen under a rectangle.
fn text_under(terminal: &Terminal<TestBackend>, rect: ratatui::layout::Rect) -> String {
    let buffer = terminal.backend().buffer();
    (rect.x..rect.x + rect.width)
        .map(|x| buffer[(x, rect.y)].symbol())
        .collect()
}

#[test]
fn each_part_of_the_hint_is_where_it_is_drawn_and_a_click_presses_its_key() {
    let (mut app, _dir) = app();
    let terminal = draw_now(&mut app, 120, 24);
    let hints = app.hits.hints.clone();
    let texts: Vec<String> = hints
        .iter()
        .map(|(rect, _)| text_under(&terminal, *rect))
        .collect();
    assert_eq!(texts, ["q quit", "Tab switch pane", "i inspector"]);
    // the focus: Tab switches it
    assert_eq!(app.focus, Focus::Tree);
    let (rect, _) = hints[1];
    assert!(click(&mut app, rect.x + 2, rect.y));
    assert_eq!(app.focus, Focus::Hex);
    // the inspector
    let (rect, _) = hints[2];
    assert!(!app.inspector);
    click(&mut app, rect.x, rect.y);
    assert!(app.inspector);
    // the last cell of a part is the part's, the separator after it is not
    draw_now(&mut app, 120, 24);
    let (rect, _) = app.hits.hints[2];
    click(&mut app, rect.x + rect.width - 1, rect.y);
    assert!(!app.inspector);
    assert!(
        app.hits
            .target(rect.x + rect.width, rect.y)
            .is_none_or(|t| !matches!(t, doe_tui::mouse::Target::Hint(_)))
    );
    // quit
    draw_now(&mut app, 120, 24);
    let (rect, _) = app.hits.hints[0];
    assert!(!app.quit);
    click(&mut app, rect.x, rect.y);
    assert!(app.quit);
}

#[test]
fn the_hint_of_a_full_screen_and_of_a_narrow_pane_has_only_parts_that_are_drawn() {
    let (mut app, _dir) = app();
    app.fullscreen = true;
    app.focus = Focus::Tree;
    let terminal = draw_now(&mut app, 120, 24);
    let hints = app.hits.hints.clone();
    let texts: Vec<String> = hints
        .iter()
        .map(|(rect, _)| text_under(&terminal, *rect))
        .collect();
    assert_eq!(
        texts,
        [
            "q quit",
            "f or Esc restore",
            "Tab other pane",
            "i inspector"
        ]
    );
    click(&mut app, hints[1].0.x, hints[1].0.y);
    assert!(
        !app.fullscreen,
        "the part that says f or Esc restores both panes"
    );
    // a pane too narrow for the whole hint: what is cut is not a part to click
    let (mut app, _dir) = app_with(SCHEMA, &[1, 0, 2, 0, 3, 0, 4, 5]);
    let terminal = draw_now(&mut app, 50, 12);
    let pane = app.hits.tree_pane.unwrap();
    for (rect, _) in &app.hits.hints {
        assert!(
            rect.x + rect.width < pane.x + pane.width,
            "{rect:?} is on the border"
        );
        let text = text_under(&terminal, *rect);
        assert!(!text.trim().is_empty(), "{rect:?}");
    }
}

#[test]
fn the_checksum_part_of_the_hint_of_the_files_goes_to_the_next_algorithm() {
    let types = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../types");
    let mut options = options(&tempfile::tempdir().unwrap());
    options.file = types.join("samples/sample.tar");
    options.type_or_schema = "tar".to_owned();
    options.include_paths = vec![types];
    let mut app = App::new(options.file.clone(), load(&options).unwrap());
    app.options = Some(options);
    app.mouse_on = true;
    let terminal = draw_now(&mut app, 200, 16);
    let texts: Vec<String> = app
        .hits
        .hints
        .iter()
        .map(|(rect, _)| text_under(&terminal, *rect))
        .collect();
    assert_eq!(
        texts,
        ["q quit", "Tab switch pane", "i inspector", "F files/fields"]
    );
    // `m` shows the column, and the hint names its algorithm
    app.focus = Focus::Tree;
    key(&mut app, KeyCode::Char('m'));
    assert!(
        app.files.as_ref().unwrap().algorithm.is_some(),
        "{:?}",
        app.notice
    );
    let terminal = draw_now(&mut app, 200, 16);
    let (rect, _) = *app.hits.hints.last().unwrap();
    let shown = text_under(&terminal, rect);
    assert!(shown.starts_with("m "), "{shown}");
    let before = app.files.as_ref().unwrap().algorithm;
    click(&mut app, rect.x, rect.y);
    assert_ne!(app.files.as_ref().unwrap().algorithm, before);
    // the part for the Files goes to the fields
    draw_now(&mut app, 200, 16);
    let files = app.hits.hints[3].0;
    assert!(app.files_shown());
    click(&mut app, files.x, files.y);
    assert!(!app.files_shown());
}
