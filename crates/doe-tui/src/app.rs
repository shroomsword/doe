//! The state of the viewer and what a key does to it. Nothing here touches a
//! terminal, so it is tested by calling it.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use doe::DoeError;

use crate::load::Loaded;
use crate::tree::TreeModel;
use crate::tree_view::TreeView;

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
    /// Where the tree pane is scrolled to.
    pub view: TreeView,
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
            view: TreeView::new(),
            quit: false,
        }
    }

    /// What a key does. `q`, `Esc` and Ctrl-C leave (raw mode turns Ctrl-C
    /// into a key; it is not a signal while the viewer is open). The tree:
    /// `↑`/`k`, `↓`/`j`, `PgUp`, `PgDn`, `Home`/`g`, `End`/`G` move;
    /// `→`/`l` opens a node or goes into it, `←`/`h` closes it or goes to its
    /// parent; `Enter` and `Space` open or close.
    pub fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let tree = &mut self.tree;
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('c') if ctrl => self.quit = true,
            KeyCode::Up | KeyCode::Char('k') => tree.up(),
            KeyCode::Down | KeyCode::Char('j') => tree.down(),
            KeyCode::PageUp => tree.page_up(self.view.page()),
            KeyCode::PageDown => tree.page_down(self.view.page()),
            KeyCode::Home | KeyCode::Char('g') => tree.home(),
            KeyCode::End | KeyCode::Char('G') => tree.end(),
            KeyCode::Right | KeyCode::Char('l') => {
                if tree.selected().openable && !tree.selected().open {
                    tree.expand();
                } else if tree.selected().openable {
                    tree.first_child();
                } else {
                    // the "more" row shows the next piece
                    tree.expand();
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if tree.selected().open {
                    tree.collapse();
                } else {
                    tree.parent();
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => tree.toggle(),
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

    fn key(app: &mut App, code: KeyCode) {
        press(app, code, KeyModifiers::NONE);
    }

    #[test]
    fn the_keys_move_in_the_tree_and_open_and_close_nodes() {
        let dir = tempfile::tempdir().unwrap();
        let schema = dir.path().join("t.yaml");
        std::fs::write(
            &schema,
            "id: t\nseq:\n  - id: a\n    type: u1\n  - id: inner\n    type: pair\ntypes:\n  pair:\n    seq:\n      - id: x\n        type: u1\n      - id: y\n        type: u1\n",
        )
        .unwrap();
        let file = dir.path().join("t.bin");
        std::fs::write(&file, [1_u8, 2, 3]).unwrap();
        let loaded = crate::load::load(&Options {
            file: file.clone(),
            type_or_schema: schema.to_string_lossy().into_owned(),
            include_paths: Vec::new(),
            color: false,
        })
        .unwrap();
        let mut app = App::new(file, loaded);
        let name = |app: &App| app.tree.selected().name.clone();

        assert_eq!(name(&app), "t");
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Char('j'));
        assert_eq!(name(&app), "inner");
        key(&mut app, KeyCode::Right); // opens
        assert_eq!(app.tree.rows().len(), 5);
        key(&mut app, KeyCode::Right); // goes into it
        assert_eq!(name(&app), "x");
        key(&mut app, KeyCode::Char('k'));
        key(&mut app, KeyCode::Left); // `inner` is open: closes it
        assert_eq!(app.tree.rows().len(), 3);
        key(&mut app, KeyCode::Left); // closed: the parent
        assert_eq!(name(&app), "t");
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.tree.rows().len(), 1, "enter closed the root");
        key(&mut app, KeyCode::Char(' '));
        assert_eq!(app.tree.rows().len(), 3);
        key(&mut app, KeyCode::End);
        assert_eq!(name(&app), "inner");
        key(&mut app, KeyCode::Home);
        assert_eq!(name(&app), "t");
        key(&mut app, KeyCode::Char('G'));
        // the view has not been drawn, so a page is one row
        key(&mut app, KeyCode::PageUp);
        assert_eq!(name(&app), "a");
        key(&mut app, KeyCode::PageDown);
        assert_eq!(name(&app), "inner");
        key(&mut app, KeyCode::Char('g'));
        assert_eq!(name(&app), "t");
        assert!(!app.quit);
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
