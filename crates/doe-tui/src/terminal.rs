//! The terminal while the viewer is open: raw mode on an alternate screen, put
//! back in every way the viewer can end. [`Guard`] does it when it is dropped
//! and when the program panics; [`Signals`] tells the loop that it was asked to
//! stop (`SIGTERM`, `SIGHUP`, and `SIGINT` sent from outside, since Ctrl-C at the
//! keyboard is a key in raw mode), so that it ends normally and the guard drops.

use std::io::{self, Stdout};
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Once};

use crossterm::cursor::{Hide, Show};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

/// What is done to the terminal, apart so that a test can watch it.
pub trait Screen: Send + Sync {
    /// Raw mode, the alternate screen, no cursor.
    ///
    /// # Errors
    ///
    /// Fails when the terminal cannot be set up; then nothing is left changed.
    fn enter(&self) -> io::Result<()>;

    /// Undo `enter`. It cannot fail in a way that anyone can act on, so it reports nothing.
    fn leave(&self);
}

/// The terminal of the process, by `crossterm`.
#[derive(Debug, Clone, Copy)]
pub struct CrosstermScreen;

impl Screen for CrosstermScreen {
    fn enter(&self) -> io::Result<()> {
        enable_raw_mode()?;
        if let Err(e) = execute!(io::stdout(), EnterAlternateScreen, Hide) {
            self.leave();
            return Err(e);
        }
        Ok(())
    }

    fn leave(&self) {
        // Every step is tried, and none can be handled if it fails.
        let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

/// Puts the screen back once, whoever asks first.
struct Restorer {
    screen: Arc<dyn Screen>,
    done: AtomicBool,
}

impl Restorer {
    fn restore(&self) {
        if !self.done.swap(true, Ordering::SeqCst) {
            self.screen.leave();
        }
    }
}

/// The guard that is open now, for the panic hook.
static ACTIVE: Mutex<Option<Arc<Restorer>>> = Mutex::new(None);

fn take_active() -> Option<Arc<Restorer>> {
    ACTIVE.lock().ok().and_then(|mut active| active.take())
}

/// Make a panic put the terminal back before its message is printed, so that
/// the message is not lost on the alternate screen.
fn install_panic_hook() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if let Some(restorer) = take_active() {
                restorer.restore();
            }
            previous(info);
        }));
    });
}

/// The terminal is set up as long as this exists.
#[derive(Debug)]
pub struct Guard {
    restorer: Arc<Restorer>,
}

impl std::fmt::Debug for Restorer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Restorer")
            .field("done", &self.done)
            .finish_non_exhaustive()
    }
}

impl Guard {
    /// Set up the terminal of the process and give a `ratatui` terminal on it.
    ///
    /// # Errors
    ///
    /// Fails when the terminal cannot be set up.
    pub fn enter() -> io::Result<(Guard, Terminal<CrosstermBackend<Stdout>>)> {
        let guard = Guard::with_screen(Arc::new(CrosstermScreen))?;
        let terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        Ok((guard, terminal))
    }

    /// Set up `screen`; it is left again when the guard is dropped or the program panics.
    ///
    /// # Errors
    ///
    /// Fails when `screen` cannot be entered.
    pub fn with_screen(screen: Arc<dyn Screen>) -> io::Result<Guard> {
        install_panic_hook();
        screen.enter()?;
        let restorer = Arc::new(Restorer {
            screen,
            done: AtomicBool::new(false),
        });
        if let Ok(mut active) = ACTIVE.lock() {
            *active = Some(Arc::clone(&restorer));
        }
        Ok(Guard { restorer })
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        // Not the hook's any more, whether or not it has run.
        if let Ok(mut active) = ACTIVE.lock()
            && active
                .as_ref()
                .is_some_and(|a| Arc::ptr_eq(a, &self.restorer))
        {
            *active = None;
        }
        self.restorer.restore();
    }
}

/// Whether the process was asked to stop by a signal.
#[derive(Debug, Clone)]
pub struct Signals {
    fired: Arc<AtomicBool>,
}

impl Signals {
    /// Catch `SIGINT`, `SIGTERM` and `SIGHUP` (nothing on platforms without them).
    ///
    /// # Errors
    ///
    /// Fails when a handler cannot be installed.
    pub fn install() -> io::Result<Signals> {
        let fired = Arc::new(AtomicBool::new(false));
        #[cfg(unix)]
        for signal in [
            signal_hook::consts::SIGINT,
            signal_hook::consts::SIGTERM,
            signal_hook::consts::SIGHUP,
        ] {
            signal_hook::flag::register(signal, Arc::clone(&fired))?;
        }
        Ok(Signals { fired })
    }

    /// Whether a signal has come.
    #[must_use]
    pub fn fired(&self) -> bool {
        self.fired.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::panic,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use std::sync::atomic::AtomicUsize;

    use super::*;

    /// The hook and the guard that is open are the process's: one test at a time.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    #[derive(Debug, Default)]
    struct Fake {
        entered: AtomicUsize,
        left: AtomicUsize,
        fail: AtomicBool,
    }

    impl Screen for Fake {
        fn enter(&self) -> io::Result<()> {
            if self.fail.load(Ordering::SeqCst) {
                return Err(io::Error::other("no terminal"));
            }
            self.entered.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn leave(&self) {
            self.left.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn counts(fake: &Fake) -> (usize, usize) {
        (
            fake.entered.load(Ordering::SeqCst),
            fake.left.load(Ordering::SeqCst),
        )
    }

    #[test]
    fn the_terminal_is_put_back_when_the_guard_is_dropped() {
        let _one = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let fake = Arc::new(Fake::default());
        let guard = Guard::with_screen(fake.clone()).unwrap();
        assert_eq!(counts(&fake), (1, 0));
        drop(guard);
        assert_eq!(counts(&fake), (1, 1));
    }

    #[test]
    fn the_terminal_is_put_back_when_the_program_panics_and_only_once() {
        let _one = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let fake = Arc::new(Fake::default());
        let outcome = panic::catch_unwind(|| {
            let _guard = Guard::with_screen(fake.clone()).unwrap();
            panic!("the viewer has a bug");
        });
        assert!(outcome.is_err());
        // The hook put it back (before the message), and the drop that
        // followed did not do it again.
        assert_eq!(counts(&fake), (1, 1));
    }

    #[test]
    fn a_terminal_that_cannot_be_set_up_is_not_touched_again() {
        let _one = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let fake = Arc::new(Fake::default());
        fake.fail.store(true, Ordering::SeqCst);
        assert!(Guard::with_screen(fake.clone()).is_err());
        assert_eq!(counts(&fake), (0, 0));
    }

    #[test]
    fn a_panic_after_the_guard_is_gone_leaves_the_terminal_alone() {
        let _one = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let fake = Arc::new(Fake::default());
        drop(Guard::with_screen(fake.clone()).unwrap());
        let _ = panic::catch_unwind(|| panic!("later"));
        assert_eq!(counts(&fake), (1, 1));
    }

    #[cfg(unix)]
    #[test]
    fn a_signal_is_seen_by_the_loop() {
        let signals = Signals::install().unwrap();
        assert!(!signals.fired());
        signal_hook::low_level::raise(signal_hook::consts::SIGHUP).unwrap();
        assert!(signals.fired());
    }
}
