//! The performance budget of the viewer, measured on inputs that are generated
//! here, in CI as everywhere else.
//!
//! The viewer must not be what makes a big file hard to look at. The budget:
//!
//! | what | budget |
//! |---|---|
//! | the first screen, from a parsed tree of a million fields | [`FIRST_SCREEN`] |
//! | any key, worst of some hundreds (held keys, page keys, goto, `End`) | [`KEY`] |
//! | any redraw of a 120×40 screen (99 in 100; a stall of one is bounded by five times it) | [`FRAME`] |
//! | a step of a search through the million fields | [`SEARCH_STEP`] |
//! | memory the viewer adds to the parsed tree (Linux; peak resident) | [`MEMORY_KIB`] |
//! | a file of 6 GiB: open, first screen, go to the end | [`SPARSE`] and [`MEMORY_KIB`] |
//!
//! The times are the budget for a **debug** build on a slow machine: the numbers
//! that are measured are tens of times smaller (they are printed with
//! `--nocapture`), so a failure here is a change in how the work grows, not noise.
//! Work is also counted, which does not depend on the machine: the rows that are
//! built for any key never exceed a window, and a frame reads a window of the file.
//!
//! The parse itself (the engine reading a million fields) is not part of the
//! budget of the viewer, and happens in another thread while the first screen is
//! up; its time is printed.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    clippy::cast_precision_loss,
    reason = "Panicking and printing are permitted in tests (rust-practices)."
)]

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::Options;
use doe_tui::app::App;
use doe_tui::load::load;
use doe_tui::tree::{CHUNK, MAX_WINDOW};
use doe_tui::ui::{Theme, draw};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

/// The first screen of a tree of a million fields, built and drawn.
const FIRST_SCREEN: Duration = Duration::from_millis(500);
/// The slowest key.
const KEY: Duration = Duration::from_millis(100);
/// The slowest redraw.
const FRAME: Duration = Duration::from_millis(100);
/// The slowest step of a search.
const SEARCH_STEP: Duration = Duration::from_millis(100);
/// Opening a file of gigabytes, drawing it, and going to its end.
const SPARSE: Duration = Duration::from_millis(500);
/// What the viewer may add to the memory of the process, at its peak.
const MEMORY_KIB: u64 = 32 * 1024;

/// A million fields.
const MANY: usize = 1_000_000;

/// The peak resident memory of this process in KiB, and a way to start the peak
/// again from where the process is now. Linux only: nothing elsewhere.
fn peak_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

fn reset_peak() -> bool {
    std::fs::write("/proc/self/clear_refs", "5").is_ok()
}

fn key(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        key(app, KeyCode::Char(c));
    }
}

/// Draw and say how long it took.
fn frame(terminal: &mut Terminal<TestBackend>, app: &mut App, theme: Theme) -> Duration {
    let start = Instant::now();
    terminal.draw(|f| draw(f, app, &theme)).unwrap();
    let took = start.elapsed();
    FRAMES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(took);
    took
}

/// The time of every frame drawn since the last test began (the tests run one at a time).
static FRAMES: Mutex<Vec<Duration>> = Mutex::new(Vec::new());

/// Run `f` and keep the longest time of a call.
struct Worst(Duration, String);

impl Worst {
    fn new() -> Self {
        Worst(Duration::ZERO, String::new())
    }

    fn time(&mut self, what: &str, f: impl FnOnce()) {
        let start = Instant::now();
        f();
        let took = start.elapsed();
        if took > self.0 {
            self.0 = took;
            what.clone_into(&mut self.1);
        }
    }
}

/// The peak memory is the process's, and the time is the machine's: the tests run one at a time.
static ALONE: Mutex<()> = Mutex::new(());

#[test]
fn a_million_fields_open_scroll_search_and_jump_within_the_budget() {
    let _alone = ALONE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let schema = dir.path().join("big.yaml");
    std::fs::write(
        &schema,
        format!("id: big\nseq:\n  - id: items\n    type: u1\n    repeat: expr\n    repeat-expr: {MANY}\n"),
    )
    .unwrap();
    let file = dir.path().join("big.bin");
    let mut data = vec![1_u8; MANY];
    data[MANY - 1] = 200; // the only 0xc8, at the far end
    std::fs::write(&file, &data).unwrap();
    drop(data);

    // the parse: the engine's, printed, not part of the budget
    let start = Instant::now();
    let loaded = load(&Options {
        file: file.clone(),
        type_or_schema: schema.to_string_lossy().into_owned(),
        include_paths: Vec::new(),
        color: false,
        watch: false,
        theme: None,
        theme_dir: None,
    })
    .unwrap();
    eprintln!("parse of {MANY} fields: {:?}", start.elapsed());

    let tracking = reset_peak();
    let before = peak_kib();
    let theme = Theme::new(true);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    FRAMES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();

    // the first screen: the model, and the frame
    let start = Instant::now();
    let mut app = App::new(file, loaded);
    let first = frame(&mut terminal, &mut app, theme);
    let total = start.elapsed();
    eprintln!("first screen: {total:?} (the frame {first:?})");
    assert!(total < FIRST_SCREEN, "the first screen took {total:?}");
    assert!(first < FRAME * 5, "the frame took {first:?}");

    // open the list and hold the keys: some hundreds of each, a frame after every key
    let mut keys = Worst::new();
    let mut frames = Worst::new();
    let mut step = |app: &mut App, code: KeyCode| {
        keys.time(
            &format!("{code:?} with {} rows", app.tree.rows().len()),
            || key(app, code),
        );
        let took = frame(&mut terminal, app, theme);
        if took > frames.0 {
            frames.0 = took;
            frames.1 = format!("after {code:?}");
        }
    };
    step(&mut app, KeyCode::Down);
    step(&mut app, KeyCode::Right); // opens `items`: a window of rows, not a million
    assert!(
        app.tree.rows().len() <= 2 + 1 + CHUNK,
        "{}",
        app.tree.rows().len()
    );
    for _ in 0..300 {
        step(&mut app, KeyCode::Down);
    }
    for _ in 0..100 {
        step(&mut app, KeyCode::PageDown);
    }
    for _ in 0..300 {
        step(&mut app, KeyCode::Up);
    }
    // the "more" rows: each shows the next window, and the window stays bounded
    for _ in 0..30 {
        step(&mut app, KeyCode::End);
        step(&mut app, KeyCode::Right);
        assert!(
            app.tree.rows().len() <= 2 + 2 + MAX_WINDOW,
            "{}",
            app.tree.rows().len()
        );
    }
    // the bytes: a held key, and the ends of the file
    step(&mut app, KeyCode::Tab);
    for _ in 0..300 {
        step(&mut app, KeyCode::Down);
    }
    step(&mut app, KeyCode::Char('G'));
    step(&mut app, KeyCode::Tab);
    assert_eq!(app.tree.selected().name, format!("[{}]", MANY - 1));
    assert!(
        app.tree.rows().len() <= 2 + 2 + MAX_WINDOW,
        "the window went with the key"
    );

    // goto: the millionth field, and the middle
    for offset in [MANY - 1, MANY / 2, 5, MANY - 2] {
        keys.time(&format!("goto {offset}"), || {
            key(&mut app, KeyCode::Char('g'));
            typed(&mut app, &offset.to_string());
            key(&mut app, KeyCode::Enter);
        });
        let took = frame(&mut terminal, &mut app, theme);
        frames.0 = frames.0.max(took);
        assert_eq!(app.tree.selected().name, format!("[{offset}]"));
        assert!(app.tree.rows().len() <= 2 + 2 + MAX_WINDOW);
    }

    // a search through the million fields from the top, a step at a time with a frame between
    key(&mut app, KeyCode::Home);
    keys.time("search start", || {
        key(&mut app, KeyCode::Char('/'));
        typed(&mut app, "0xc8");
        key(&mut app, KeyCode::Enter);
    });
    let mut search_steps = 0;
    let mut worst_step = Duration::ZERO;
    while app.searching() {
        let start = Instant::now();
        app.search_step();
        worst_step = worst_step.max(start.elapsed());
        let took = frame(&mut terminal, &mut app, theme);
        frames.0 = frames.0.max(took);
        search_steps += 1;
    }
    eprintln!("search: {search_steps} steps, the longest {worst_step:?}");
    assert!(
        worst_step < SEARCH_STEP,
        "a step of the search took {worst_step:?}"
    );
    assert!(
        search_steps > 100,
        "{search_steps}: the work is spread over many steps"
    );
    assert_eq!(app.tree.selected().name, format!("[{}]", MANY - 1));

    eprintln!(
        "slowest key {:?} ({}), slowest frame {:?} ({})",
        keys.0, keys.1, frames.0, frames.1
    );
    assert!(keys.0 < KEY, "a key took {:?}", keys.0);
    // A shared runner can stall a single frame (273 ms was seen on macOS, where
    // 10-44 ms is usual), so the budget is for 99 frames of 100 and a stall has a
    // larger bound.
    let mut times = FRAMES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    times.sort();
    let p99 = times[times.len() * 99 / 100];
    eprintln!("{} frames, the 99th percentile {p99:?}", times.len());
    assert!(p99 < FRAME, "a frame at the 99th percentile took {p99:?}");
    assert!(
        frames.0 < FRAME * 5,
        "a frame took {:?} ({})",
        frames.0,
        frames.1
    );
    drop(times);

    // the memory the viewer added to the tree
    if let (true, Some(before), Some(after)) = (tracking, before, peak_kib()) {
        let added = after.saturating_sub(before);
        eprintln!("memory added by the viewer: {added} KiB");
        assert!(
            added < MEMORY_KIB,
            "the viewer added {added} KiB at its peak"
        );
    }
}

/// A file of several gigabytes (sparse: it takes no disk) with a header that the
/// schema reads: opening it reads the header, the first screen reads a window,
/// and going to its end reads another.
#[cfg(unix)]
#[test]
fn a_file_of_gigabytes_opens_and_scrolls_without_being_read() {
    use std::io::{Seek, SeekFrom, Write};

    let _alone = ALONE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let dir = tempfile::tempdir().unwrap();
    let schema = dir.path().join("huge.yaml");
    std::fs::write(
        &schema,
        "id: huge\nseq:\n  - id: magic\n    contents: \"DOE\"\n  - id: count\n    type: u4le\n  - id: body\n    size-eos: true\n    doc: gigabytes of nothing\n",
    )
    .unwrap();
    let file = dir.path().join("huge.bin");
    let size: u64 = 6 * 1024 * 1024 * 1024;
    {
        let mut f = std::fs::File::create(&file).unwrap();
        f.set_len(size).unwrap();
        f.write_all(b"DOE\x07\0\0\0").unwrap();
        f.seek(SeekFrom::Start(size - 4)).unwrap();
        f.write_all(&[0xde, 0xad, 0xbe, 0xef]).unwrap();
    }

    let tracking = reset_peak();
    let before = peak_kib();
    let theme = Theme::new(true);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();

    let start = Instant::now();
    let loaded = load(&Options {
        file: file.clone(),
        type_or_schema: schema.to_string_lossy().into_owned(),
        include_paths: Vec::new(),
        color: false,
        watch: false,
        theme: None,
        theme_dir: None,
    })
    .unwrap();
    assert_eq!(loaded.size, size);
    let mut app = App::new(file, loaded);
    frame(&mut terminal, &mut app, theme);
    let opened = start.elapsed();
    eprintln!("6 GiB file: open and first screen {opened:?}");
    assert!(opened < SPARSE, "opening took {opened:?}");

    // the end of the file: the bytes, and the field that holds them
    let start = Instant::now();
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Char('G'));
    let last = frame(&mut terminal, &mut app, theme);
    let to_end = start.elapsed();
    eprintln!("6 GiB file: to the end {to_end:?} (the frame {last:?})");
    assert!(to_end < SPARSE && last < FRAME * 5, "{to_end:?}");
    assert_eq!(app.hex.cursor(), size - 1);
    assert_eq!(app.tree.selected().name, "body");
    let screen = terminal.backend().to_string();
    assert!(
        screen.contains("de ad be ef"),
        "the last bytes are on the screen:\n{screen}"
    );
    assert!(
        screen.contains("0x0000000000"),
        "offsets past 4 GiB have 12 digits"
    );

    // a held key through the bytes, then goto in the middle
    let mut worst = Worst::new();
    for _ in 0..500 {
        worst.time("PageUp", || key(&mut app, KeyCode::PageUp));
        frame(&mut terminal, &mut app, theme);
    }
    key(&mut app, KeyCode::Char('g'));
    typed(&mut app, "0x80000000");
    key(&mut app, KeyCode::Enter);
    frame(&mut terminal, &mut app, theme);
    assert_eq!(app.hex.cursor(), 0x8000_0000);
    assert!(worst.0 < KEY, "a key took {:?}", worst.0);

    if let (true, Some(before), Some(after)) = (tracking, before, peak_kib()) {
        let added = after.saturating_sub(before);
        eprintln!("memory added by opening and scrolling 6 GiB: {added} KiB");
        assert!(added < MEMORY_KIB, "{added} KiB");
    }
}
