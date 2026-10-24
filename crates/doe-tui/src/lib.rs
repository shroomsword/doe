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
pub fn run(options: &Options) -> anyhow::Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!(
            "--tui needs a terminal on stdin and on stdout (use --json or --pretty to print to a pipe)"
        );
    }
    // What cannot be parsed is shown in the viewer, not refused before it opens:
    // a schema with a mistake in it, a type that does not exist, a file that
    // cannot be read. The person sees what and where, and leaves with `q`.
    let mut app = match load::try_load(options) {
        Ok(loaded) => App::new(options.file.clone(), loaded),
        Err(failure) => {
            let schema = load::schema_file(options);
            App::broken(options.file.clone(), &failure, schema.as_deref())
        }
    };
    app.options = Some(options.clone());
    let mut watch = options.watch.then(|| Watch::new(app.watched_files()));
    let theme = Theme::new(options.color);

    let signals = Signals::install().context("could not install the signal handlers")?;
    let (guard, mut terminal) = Guard::enter().context("could not set up the terminal")?;
    while !app.quit && !signals.fired() {
        terminal
            .draw(|frame| ui::draw(frame, &mut app, &theme))
            .context("could not draw the screen")?;
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
        } else if app.searching() {
            app.search_step();
        }
        // With --watch, a change of the schema or the file on disk is read again. The
        // files are looked at once in a while, not on every turn of the loop.
        if let Some(watch) = &mut watch
            && watch.changed(Instant::now())
        {
            app.reload();
            watch.set_paths(app.watched_files());
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
