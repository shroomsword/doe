//! A viewer in the terminal for a decoded binary file: `doe --tui FILE TYPE`.
//!
//! The parts are kept apart so that all but the last are tested without a
//! terminal:
//!
//! - [`load`] parses the file and keeps what was read, also when the parse
//!   fails;
//! - [`app`] is the state of the viewer and what a key does to it;
//! - [`ui`] draws the state on a `ratatui` frame;
//! - [`terminal`] puts the terminal in raw mode on an alternate screen and
//!   always puts it back: on drop, on a panic and on a signal;
//! - [`run`] ties them together with the event loop.

pub mod app;
pub mod bytes;
pub mod format;
pub mod hex_view;
pub mod inspect;
pub mod keymap;
pub mod load;
pub mod prompt;
pub mod report;
pub mod search;
pub mod status;
pub mod terminal;
pub mod tree;
pub mod tree_view;
pub mod ui;
pub mod watch;

use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use crossterm::event::{self, Event, KeyEventKind};

use crate::app::App;
use crate::terminal::{Guard, Signals};
use crate::ui::Theme;
use crate::watch::Watch;

/// What the viewer is asked to show.
#[derive(Debug, Clone)]
pub struct Options {
    /// The binary file.
    pub file: PathBuf,
    /// A type name or the path of a schema file, as on the command line.
    pub type_or_schema: String,
    /// Where schemas are looked for, config included.
    pub include_paths: Vec<PathBuf>,
    /// Whether to use colours (`--color` and `NO_COLOR` are decided by the caller).
    pub color: bool,
    /// Whether to read the schema and the file again when they change on disk.
    pub watch: bool,
}

/// The stack of the thread that parses: as large as the engine may need for the
/// deepest schemas it allows (the main thread has 8 MiB; a new thread 2 MiB).
const LOAD_STACK: usize = 64 * 1024 * 1024;

/// Parse in another thread, so that the first screen does not wait for it.
fn spawn_load(
    options: Options,
) -> std::sync::mpsc::Receiver<Result<load::Loaded, Box<load::LoadFailure>>> {
    let (send, receive) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("doe-parse".to_owned())
        .stack_size(LOAD_STACK)
        .spawn({
            let send = send.clone();
            move || {
                // The viewer may have been left already: then nobody wants the result.
                let _ = send.send(load::try_load(&options));
            }
        });
    if let Err(error) = spawned {
        log::error!("could not start the thread that parses: {error}");
        drop(send);
    }
    receive
}

/// The longest the screen is left undrawn while keys keep coming.
const MAX_BEHIND: Duration = Duration::from_millis(50);

/// How often the screen is redrawn while the file is being parsed (for the seconds on it).
const LOADING_REDRAW: Duration = Duration::from_millis(250);

/// How often the loop looks at the signals when no key is pressed.
const TICK: Duration = Duration::from_millis(100);

/// Open the viewer and return when the person quits.
///
/// # Errors
///
/// Fails, before the terminal is touched, when stdin or stdout is not a
/// terminal; and when the terminal fails while the viewer is open. A schema
/// that cannot be used or a file that cannot be read is shown in the viewer,
/// not refused: after it is left (the terminal is put back first), the last
/// parse having failed is an error, with the message that was on the screen.
///
/// # Panics
///
/// When the environment variable `DOE_TUI_PANIC_FOR_TEST` is set, after the
/// terminal is set up: the tests use it to check that a panic puts the terminal
/// back.
pub fn run(options: &Options) -> anyhow::Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!(
            "--tui needs a terminal on stdin and on stdout (use --json or --pretty to print to a pipe)"
        );
    }
    // The terminal is set up and the first screen drawn at once; the file is
    // parsed in another thread, which can take a while for a file with millions of
    // fields. What cannot be parsed is shown in the viewer, not refused before it
    // opens: a schema with a mistake in it, a type that does not exist, a file that
    // cannot be read. The person sees what and where, and leaves with `q`.
    let mut app = App::loading(options.file.clone(), &options.type_or_schema);
    app.options = Some(options.clone());
    let loader = spawn_load(options.clone());
    let mut loader = Some(loader);
    let mut watch: Option<Watch> = None;
    let theme = Theme::new(options.color);

    let signals = Signals::install().context("could not install the signal handlers")?;
    let (guard, mut terminal) = Guard::enter().context("could not set up the terminal")?;
    // For the test that a panic inside the viewer puts the terminal back.
    assert!(
        std::env::var_os("DOE_TUI_PANIC_FOR_TEST").is_none(),
        "DOE_TUI_PANIC_FOR_TEST is set"
    );
    // The screen is drawn when something has changed, not on every turn of the loop.
    let mut dirty = true;
    let mut last_draw = Instant::now();
    while !app.quit && !signals.fired() {
        // A held key sends keys faster than a screen can be drawn and read: while
        // more input is waiting, the screen is only drawn every so often, so that the
        // viewer is never behind its keys.
        let input_waiting = event::poll(Duration::ZERO).context("could not read the keyboard")?;
        if dirty && (!input_waiting || last_draw.elapsed() >= MAX_BEHIND) {
            terminal
                .draw(|frame| ui::draw(frame, &mut app, &theme))
                .context("could not draw the screen")?;
            dirty = false;
            last_draw = Instant::now();
        }
        // While a search goes on, keys are only looked at, not waited for; between
        // them it looks at the next fields. Redrawing between the steps keeps the
        // screen alive and any key stops the search.
        let wait = if app.searching() {
            Duration::ZERO
        } else {
            TICK
        };
        if event::poll(wait).context("could not read the keyboard")? {
            match event::read().context("could not read the keyboard")? {
                // Windows also reports the release of a key.
                Event::Key(key) if key.kind != KeyEventKind::Release => app.on_key(key),
                Event::Paste(text) => app.on_paste(&text),
                _ => {}
            }
            dirty = true;
        } else if app.searching() {
            app.search_step();
            dirty = true;
        }
        // The parse is done: the tree takes the place of the message. (If the thread
        // that parses stopped without an answer, that is the answer.)
        if let Some(receiver) = &loader {
            let result = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err(Box::new(load::LoadFailure::stopped(options))))
                }
            };
            if let Some(result) = result {
                app.finish_loading(result);
                loader = None;
                if options.watch {
                    watch = Some(Watch::new(app.watched_files()));
                }
                dirty = true;
            }
        }
        // The time that the parse has taken is on the screen, a few times a second.
        if app.loading && last_draw.elapsed() >= LOADING_REDRAW {
            dirty = true;
        }
        // With --watch, a change of the schema or the file on disk is read again. The
        // files are looked at once in a while, not on every turn of the loop.
        if let Some(watch) = &mut watch
            && watch.changed(Instant::now())
        {
            app.reload();
            watch.set_paths(app.watched_files());
            dirty = true;
        }
    }
    // The terminal is put back first, so that what is said next is on the screen the person returns to.
    let message = app.finish_message();
    drop(terminal);
    drop(guard);
    match message {
        Some(message) => bail!(message),
        None => Ok(()),
    }
}
