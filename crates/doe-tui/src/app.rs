//! The state of the viewer and what a key does to it. Nothing here touches a
//! terminal, so it is tested by calling it.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use doe::DoeError;

use crate::load::Loaded;
use crate::tree::TreeModel;

/// The viewer.
#[derive(Debug)]
pub struct App {
    /// The file that is shown.
    pub file: PathBuf,
    /// The type it was parsed as.
    pub type_name: String,
    /// Its size in bytes.
    pub size: u64,
    /// Why the parse stopped, if it did.
    pub error: Option<DoeError>,
    /// The tree of what was read.
    pub tree: TreeModel,
    /// Whether the person asked to leave.
    pub quit: bool,
}

impl App {
    /// A viewer for a parsed file.
    #[must_use]
    pub fn new(file: PathBuf, loaded: Loaded) -> Self {
        App {
            file,
            type_name: loaded.type_name,
            size: loaded.size,
            error: loaded.error,
            tree: TreeModel::new(loaded.trace, loaded.value),
            quit: false,
        }
    }

    /// What a key does: `q`, `Esc` and Ctrl-C leave. (Raw mode turns Ctrl-C
    /// into a key; it is not a signal while the viewer is open.)
    pub fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('c') if ctrl => self.quit = true,
            _ => {}
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;
    use crate::Options;
    use crate::load::load;

    /// A viewer for a three-byte file and a schema for it.
    pub(crate) fn app() -> (App, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let schema = dir.path().join("t.yaml");
        std::fs::write(
            &schema,
            "id: t\nseq:\n  - id: a\n    type: u1\n  - id: b\n    type: u2le\n",
        )
        .unwrap();
        let file = dir.path().join("t.bin");
        std::fs::write(&file, [7_u8, 0x34, 0x12]).unwrap();
        let options = Options {
            file: file.clone(),
            type_or_schema: schema.to_string_lossy().into_owned(),
            include_paths: Vec::new(),
            color: false,
        };
        (App::new(file, load(&options).unwrap()), dir)
    }

    fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        app.on_key(KeyEvent::new(code, modifiers));
    }

    #[test]
    fn q_escape_and_ctrl_c_leave() {
        for (code, modifiers) in [
            (KeyCode::Char('q'), KeyModifiers::NONE),
            (KeyCode::Esc, KeyModifiers::NONE),
            (KeyCode::Char('c'), KeyModifiers::CONTROL),
        ] {
            let (mut app, _dir) = app();
            assert!(!app.quit);
            press(&mut app, code, modifiers);
            assert!(app.quit, "{code:?}");
        }
    }

    #[test]
    fn other_keys_do_nothing() {
        let (mut app, _dir) = app();
        press(&mut app, KeyCode::Char('c'), KeyModifiers::NONE);
        press(&mut app, KeyCode::Char('x'), KeyModifiers::CONTROL);
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(!app.quit);
    }

    #[test]
    fn the_file_is_parsed() {
        let (app, _dir) = app();
        assert_eq!(app.type_name, "t");
        assert_eq!(app.size, 3);
        assert!(app.error.is_none());
    }
}
