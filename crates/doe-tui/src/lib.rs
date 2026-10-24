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
pub mod search;
pub mod status;
pub mod terminal;
pub mod tree;
pub mod tree_view;
pub mod ui;

use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, bail};
use crossterm::event::{self, Event, KeyEventKind};

use crate::app::App;
use crate::terminal::{Guard, Signals};
use crate::ui::Theme;

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
}

/// How often the loop looks at the signals when no key is pressed.
const TICK: Duration = Duration::from_millis(100);

/// Open the viewer and return when the person quits.
///
/// # Errors
///
/// Fails, before the terminal is touched, when stdin or stdout is not a
/// terminal or when the schema cannot be compiled or the file read; and when
/// the terminal fails while the viewer is open.
pub fn run(options: &Options) -> anyhow::Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!(
            "--tui needs a terminal on stdin and on stdout (use --json or --pretty to print to a pipe)"
        );
    }
    // Load first: a schema that does not compile is an ordinary error, printed
    // on the terminal as it was, not on an alternate screen that is left at once.
    let loaded = load::load(options).with_context(|| {
        format!(
            "could not parse {} as {}",
            options.file.display(),
            options.type_or_schema
        )
    })?;
    let mut app = App::new(options.file.clone(), loaded);
    let theme = Theme::new(options.color);

    let signals = Signals::install().context("could not install the signal handlers")?;
    let (_guard, mut terminal) = Guard::enter().context("could not set up the terminal")?;
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
    }
    Ok(())
}
