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
use doe::filesystem::Algorithm;
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
    assert!(app.files.as_ref().unwrap().algorithm.is_some());
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
    key(&mut app, 'M');
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
    assert!(app.files.as_ref().unwrap().algorithm.is_none());
}

#[test]
fn in_a_narrow_pane_the_time_and_the_mode_go_before_the_name_does() {
    let mut app = sample("tar", "sample.tar");
    key(&mut app, 'm');
    finish(&mut app);
    press(&mut app, KeyCode::Right);
    // 170 columns: the tree pane has 45% of them
    let mut terminal = Terminal::new(TestBackend::new(170, 16)).unwrap();
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

#[test]
fn m_goes_from_md5_to_sha1_to_off_and_m_shifted_goes_back_with_the_digests_of_each_kept() {
    let mut app = sample("tar", "sample.tar");
    let algorithm = |app: &App| app.files.as_ref().unwrap().algorithm;
    key(&mut app, 'm');
    assert_eq!(algorithm(&app), Some(Algorithm::Md5));
    finish(&mut app);
    press(&mut app, KeyCode::Right);
    // 40 digits for SHA-1, named in the hint and the status line
    key(&mut app, 'm');
    assert_eq!(algorithm(&app), Some(Algorithm::Sha1));
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("SHA-1 of each file"),
        "{:?}",
        app.notice
    );
    finish(&mut app);
    let lines = screen(&mut app);
    let hello = row_of(&lines, "hello.txt");
    assert!(
        hello.contains("f572d396fae9206628714fb2ce00f72e94f2258f"),
        "{hello}"
    );
    assert!(!hello.contains("b1946ac9"), "{hello}");
    assert!(lines.iter().any(|l| l.contains("m SHA-1")), "{lines:#?}");
    assert!(row_of(&lines, "data.bin").contains("494179714a6cd627239dfededf2de9ef994caf03"));
    // y copies the SHA-1 of the selected file
    press(&mut app, KeyCode::Down);
    key(&mut app, 'y');
    assert_eq!(
        app.take_clipboard().as_deref(),
        Some("f572d396fae9206628714fb2ce00f72e94f2258f")
    );
    assert!(
        app.notice.as_deref().unwrap().contains("copied the SHA-1"),
        "{:?}",
        app.notice
    );
    // M goes back to MD5, whose digests are kept: nothing is computed again
    key(&mut app, 'M');
    assert_eq!(algorithm(&app), Some(Algorithm::Md5));
    assert!(!app.files.as_ref().unwrap().checksums.running());
    assert!(row_of(&screen(&mut app), "hello.txt").contains("b1946ac92492d2347c6235b4d2611184"));
    // and from MD5 back to off, and M from off to SHA-1, m from SHA-1 to off
    key(&mut app, 'M');
    assert_eq!(algorithm(&app), None);
    assert!(!row_of(&screen(&mut app), "hello.txt").contains("b1946"));
    key(&mut app, 'M');
    assert_eq!(algorithm(&app), Some(Algorithm::Sha256));
    key(&mut app, 'M');
    assert_eq!(algorithm(&app), Some(Algorithm::Sha1));
    key(&mut app, 'm');
    key(&mut app, 'm');
    assert_eq!(algorithm(&app), None);
}

#[test]
fn sha256_has_64_digits_and_a_narrow_pane_shows_the_first_16_of_them() {
    let mut app = sample("tar", "sample.tar");
    key(&mut app, 'm');
    key(&mut app, 'm');
    key(&mut app, 'm');
    assert_eq!(
        app.files.as_ref().unwrap().algorithm,
        Some(Algorithm::Sha256)
    );
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .contains("SHA-256 of each file"),
        "{:?}",
        app.notice
    );
    finish(&mut app);
    press(&mut app, KeyCode::Right);
    // wide: the whole digest, 64 digits (the tree pane has 45% of the screen)
    let mut wide = Terminal::new(TestBackend::new(270, 16)).unwrap();
    wide.draw(|f| draw(f, &mut app, &Theme::new(false)))
        .unwrap();
    let buffer = wide.backend().buffer().clone();
    let lines: Vec<String> = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect();
    let hello = row_of(&lines, "hello.txt");
    assert!(
        hello.contains("5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03"),
        "{hello}"
    );
    assert!(lines.iter().any(|l| l.contains("m SHA-256")), "{lines:#?}");
    // 150 columns: the tree pane has 45% of them, 67, too narrow for 64 digits: the first 16 and an ellipsis
    let mut terminal = Terminal::new(TestBackend::new(150, 16)).unwrap();
    terminal
        .draw(|f| draw(f, &mut app, &Theme::new(false)))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let narrow: Vec<String> = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect();
    let hello = row_of(&narrow, "hello.txt");
    assert!(hello.contains("5891b5b522d5df08…"), "{hello}");
    assert!(!hello.contains("5891b5b522d5df086"), "{hello}");
    // y copies the whole digest, whatever is shown
    press(&mut app, KeyCode::Down);
    key(&mut app, 'y');
    assert_eq!(
        app.take_clipboard().as_deref(),
        Some("5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03")
    );
    // the cycle: SHA-256 is followed by none, and M goes back to SHA-1
    key(&mut app, 'm');
    assert_eq!(app.files.as_ref().unwrap().algorithm, None);
    key(&mut app, 'M');
    assert_eq!(
        app.files.as_ref().unwrap().algorithm,
        Some(Algorithm::Sha256)
    );
    key(&mut app, 'M');
    assert_eq!(app.files.as_ref().unwrap().algorithm, Some(Algorithm::Sha1));
}

/// The rows of the screen of a terminal of that width.
fn screen_of(app: &mut App, width: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, 16)).unwrap();
    terminal.draw(|f| draw(f, app, &Theme::new(false))).unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn a_digest_of_up_to_40_digits_is_never_cut_and_a_narrower_pane_drops_the_size_before_it_cuts_a_long_one()
 {
    let mut app = sample("tar", "sample.tar");
    key(&mut app, 'm');
    finish(&mut app);
    press(&mut app, KeyCode::Right);
    // an MD5 in a pane of 52 columns: the size, mode and time are left out, the 32 digits are not cut
    let narrow = screen_of(&mut app, 120);
    let hello = row_of(&narrow, "hello.txt");
    assert!(
        hello.contains("b1946ac92492d2347c6235b4d2611184"),
        "{hello}"
    );
    assert!(!hello.contains('…'), "{hello}");
    // SHA-1 the same, in a pane of 61
    key(&mut app, 'm');
    finish(&mut app);
    let sha1 = screen_of(&mut app, 135);
    assert!(row_of(&sha1, "hello.txt").contains("f572d396fae9206628714fb2ce00f72e94f2258f"));
    // SHA-256 in a pane of 43: no room for the size and 17 cells of digest, so the digest alone, cut
    key(&mut app, 'm');
    finish(&mut app);
    let tight = screen_of(&mut app, 100);
    let hello = row_of(&tight, "hello.txt");
    assert!(hello.contains("5891b5b522d5df08…"), "{hello}");
    assert!(!hello.contains("6 B"), "{hello}");
}
