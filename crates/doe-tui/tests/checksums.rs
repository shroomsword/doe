//! The checksum column of the Files view in the terminal viewer: `m`, the thread that makes the MD5s, `y`, the dashes.
//! No terminal.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use std::path::Path;
use std::time::{Duration, Instant};

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
        theme_on_command_line: false,
        transparent: None,
        radix: None,
    };
    let mut app = App::new(options.file.clone(), load(&options).unwrap());
    app.options = Some(options);
    app
}

fn sample(schema: &str, name: &str) -> App {
    app(
        schema,
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../types/samples")
            .join(name),
    )
}

fn key(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn screen(app: &mut App) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(220, 16)).unwrap();
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

/// Let the thread finish, taking what it sends the way the event loop does.
fn finish(app: &mut App) {
    let start = Instant::now();
    while app.files.as_ref().unwrap().checksums.running()
        && start.elapsed() < Duration::from_secs(20)
    {
        app.poll_checksums();
        std::thread::sleep(Duration::from_millis(5));
    }
    app.poll_checksums();
}

fn row_of<'a>(lines: &'a [String], name: &str) -> &'a str {
    lines
        .iter()
        .find(|l| l.contains(name))
        .unwrap_or_else(|| panic!("{name} is not on the screen: {lines:#?}"))
}

#[test]
fn m_adds_a_column_of_md5s_that_fill_in_and_m_takes_it_away() {
    let mut app = sample("tar", "sample.tar");
    let before = screen(&mut app);
    assert!(!row_of(&before, "data.bin").contains("c56bd5480f6e5413cb62a0ad9666613a"));
    key(&mut app, 'm');
    assert!(app.files.as_ref().unwrap().show_checksums);
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("not a security measure")
    );
    finish(&mut app);
    // open docs and look at the rows: the digest is between the size and the mode
    press(&mut app, KeyCode::Right);
    let lines = screen(&mut app);
    let data = row_of(&lines, "data.bin");
    assert!(data.contains("c56bd5480f6e5413cb62a0ad9666613a"), "{data}");
    let hello = row_of(&lines, "hello.txt");
    assert!(
        hello.contains("6 B") && hello.contains("b1946ac92492d2347c6235b4d2611184"),
        "{hello}"
    );
    assert!(hello.find("6 B").unwrap() < hello.find("b1946").unwrap());
    assert!(hello.find("b1946").unwrap() < hello.find("-rw-r--r--").unwrap());
    // the directory has none
    let docs = row_of(&lines, "▾ d docs");
    assert!(!docs.contains('…') && !docs.contains('—'), "{docs}");
    key(&mut app, 'm');
    let after = screen(&mut app);
    assert!(!row_of(&after, "data.bin").contains("c56bd5"));
}

#[test]
fn y_copies_the_digest_of_the_selected_file_and_says_so_when_there_is_none() {
    let mut app = sample("tar", "sample.tar");
    key(&mut app, 'y');
    assert!(
        app.notice.as_deref().unwrap().contains("press m"),
        "{:?}",
        app.notice
    );
    key(&mut app, 'm');
    finish(&mut app);
    press(&mut app, KeyCode::Right);
    // docs/hello.txt is the second row
    press(&mut app, KeyCode::Down);
    assert_eq!(
        app.files.as_ref().unwrap().selected_path(),
        "docs/hello.txt"
    );
    key(&mut app, 'y');
    assert_eq!(
        app.take_clipboard().as_deref(),
        Some("b1946ac92492d2347c6235b4d2611184")
    );
    // the directory has nothing to copy
    press(&mut app, KeyCode::Up);
    key(&mut app, 'y');
    assert!(app.take_clipboard().is_none());
    assert!(
        app.notice.as_deref().unwrap().contains("no checksum"),
        "{:?}",
        app.notice
    );
}

#[test]
fn a_zip_has_a_digest_for_a_stored_entry_and_a_dash_with_the_reason_for_a_deflated_one() {
    let mut app = sample("zip_archive", "sample.zip");
    key(&mut app, 'm');
    finish(&mut app);
    press(&mut app, KeyCode::Right);
    let lines = screen(&mut app);
    assert!(row_of(&lines, "hello.txt").contains("b1946ac92492d2347c6235b4d2611184"));
    let data = row_of(&lines, "data.bin");
    assert!(data.contains('—') && !data.contains('…'), "{data}");
    // moving onto it says why
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.files.as_ref().unwrap().selected_path(), "data.bin");
    assert!(
        app.notice.as_deref().unwrap().contains("compressed"),
        "{:?}",
        app.notice
    );
}

#[test]
fn ar_and_cpio_have_their_digests_too() {
    let mut app = sample("ar", "sample.a");
    key(&mut app, 'm');
    finish(&mut app);
    let lines = screen(&mut app);
    assert!(row_of(&lines, "hello.txt").contains("5d41402abc4b2a76b9719d911017c592"));
    assert!(row_of(&lines, "bye.txt").contains("4bf24bc9ad9372c452f8f4588dee7f32"));
    // a cpio has an empty file too
    let mut cpio = sample("cpio", "sample.cpio");
    key(&mut cpio, 'm');
    finish(&mut cpio);
    let lines = screen(&mut cpio);
    assert!(
        row_of(&lines, "odd").contains("a2b6f2a6066ed8700d83335fc50a2b8e"),
        "{lines:#?}"
    );
}

#[test]
fn the_fields_view_says_where_the_checksums_are() {
    let mut app = sample("tar", "sample.tar");
    key(&mut app, 'F');
    key(&mut app, 'm');
    assert!(
        app.notice.as_deref().unwrap().contains("press F"),
        "{:?}",
        app.notice
    );
    assert!(!app.files.as_ref().unwrap().show_checksums);
}

#[test]
fn in_a_narrow_pane_the_time_and_the_mode_go_before_the_name_does() {
    let mut app = sample("tar", "sample.tar");
    key(&mut app, 'm');
    finish(&mut app);
    press(&mut app, KeyCode::Right);
    // 150 columns: the tree pane has 45% of them
    let mut terminal = Terminal::new(TestBackend::new(150, 16)).unwrap();
    terminal
        .draw(|f| draw(f, &mut app, &Theme::new(false)))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let lines: Vec<String> = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect();
    let hello = row_of(&lines, "hello.txt");
    assert!(
        hello.contains("b1946ac92492d2347c6235b4d2611184"),
        "{hello}"
    );
    assert!(!hello.contains("-rw-r--r--"), "{hello}");
}
