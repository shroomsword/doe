//! The Files view of the terminal viewer: a tar as directories and files, `F` for the tree of fields. No terminal.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::Options;
use doe_tui::app::App;
use doe_tui::load::load;
use doe_tui::ui::{Theme, draw};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn app(schema: &str, file: &Path) -> App {
    let options = Options {
        file: file.to_path_buf(),
        type_or_schema: schema.to_owned(),
        include_paths: vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("../../types")],
        color: false,
        watch: false,
        theme: None,
        theme_dir: None,
        config_file: None,
        outer_file: None,
        theme_on_command_line: false,
        transparent: None,
        mouse: true,
        restore: None,
        radix: None,
    };
    let mut app = App::new(options.file.clone(), load(&options).unwrap());
    app.options = Some(options);
    app
}

fn tar() -> App {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../types/samples/sample.tar");
    app("tar", &file)
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn key(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

fn screen(app: &mut App) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(130, 16)).unwrap();
    terminal.draw(|f| draw(f, app, &Theme::new(false))).unwrap();
    let buffer = terminal.backend().buffer().clone();
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

fn has(lines: &[String], needle: &str) -> bool {
    lines.iter().any(|l| l.contains(needle))
}

#[test]
fn a_tar_opens_as_directories_and_files_and_f_switches_to_the_fields_and_back() {
    let mut app = tar();
    assert!(app.files_shown());
    let lines = screen(&mut app);
    // the directory first, closed; the size, mode and time of the entries once it is open
    assert!(has(&lines, "▸ d docs"), "{lines:#?}");
    assert!(has(&lines, "data.bin"), "{lines:#?}");
    press(&mut app, KeyCode::Right);
    let lines = screen(&mut app);
    assert!(has(&lines, "▾ d docs"), "{lines:#?}");
    assert!(has(&lines, "hello.txt"), "{lines:#?}");
    // down to hello.txt: its row has the size, the mode and the time
    press(&mut app, KeyCode::Down);
    let lines = screen(&mut app);
    let row = lines.iter().find(|l| l.contains("hello.txt")).unwrap();
    assert!(
        row.contains("6 B") && row.contains("-rw-r--r--") && row.contains("2023-11-14 22:13"),
        "{row}"
    );
    // the entry's field is selected in the tree, and the bytes pane follows it
    assert_eq!(app.tree.selected().name, "[1]");
    assert_eq!(app.tree.selected().span.map(|s| s.start / 8), Some(512));
    assert_eq!(app.hex.cursor(), 512);
    // F: the tree of fields, with that field selected; F again: the files, on the same entry
    key(&mut app, 'F');
    assert!(!app.files_shown());
    let lines = screen(&mut app);
    assert!(has(&lines, "▾ entries"), "{lines:#?}");
    key(&mut app, 'F');
    assert!(app.files_shown());
    assert_eq!(
        app.files.as_ref().unwrap().selected_path(),
        "docs/hello.txt"
    );
}

#[test]
fn a_schema_that_is_not_a_file_system_has_no_files_and_the_key_says_so() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("t.yaml"),
        "id: t\nseq:\n  - id: all\n    size: 4\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("t.bin"), [1, 2, 3, 4]).unwrap();
    let mut app = app(
        &dir.path().join("t.yaml").to_string_lossy(),
        &dir.path().join("t.bin"),
    );
    assert!(app.files.is_none() && !app.files_shown());
    key(&mut app, 'F');
    assert!(!app.files_shown());
    assert!(
        app.notice
            .as_deref()
            .is_some_and(|n| n.contains("file system")),
        "{:?}",
        app.notice
    );
}

#[test]
fn the_help_lists_the_key() {
    let mut app = tar();
    key(&mut app, '?');
    let lines = screen(&mut app);
    assert!(has(&lines, "files of an archive"), "{lines:#?}");
}
