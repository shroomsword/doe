//! The state of the viewer and what a key does to it. Nothing here touches a
//! terminal, so it is tested by calling it.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use doe::DoeError;

use crate::bytes::FileBytes;
use crate::hex_view::{HexState, Selection};
use crate::load::Loaded;
use crate::tree::TreeModel;
use crate::tree_view::TreeView;

/// The pane that keys go to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The tree of fields.
    Tree,
    /// The bytes.
    Hex,
}

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
    /// The bytes of the file, read as they are shown (none if it cannot be opened now).
    pub bytes: Option<FileBytes>,
    /// Where the hex pane is.
    pub hex: HexState,
    /// Which pane has the keys.
    pub focus: Focus,
    /// Whether the person asked to leave.
    pub quit: bool,
}

impl App {
    /// A viewer for a parsed file.
    #[must_use]
    pub fn new(file: PathBuf, loaded: Loaded) -> Self {
        App {
            bytes: FileBytes::open(&file).ok(),
            hex: HexState::new(),
            focus: Focus::Tree,
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
    /// into a key; it is not a signal while the viewer is open); `Tab` moves
    /// the keys between the tree and the bytes.
    ///
    /// In the tree: `↑`/`k`, `↓`/`j`, `PgUp`, `PgDn`, `Home`/`g`, `End`/`G`
    /// move; `→`/`l` opens a node or goes into it, `←`/`h` closes it or goes to
    /// its parent; `Enter` and `Space` open or close. The bytes follow the
    /// field that is selected.
    ///
    /// In the bytes: `←`/`h` and `→`/`l` move by a byte, `↑`/`k` and `↓`/`j`
    /// by a line, `PgUp` and `PgDn` by a page, `Home` and `End` to the start
    /// and the end of the line, `g` and `G` to the start and the end of the
    /// file. The tree follows: it selects the innermost field with that byte.
    pub fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('c') if ctrl => self.quit = true,
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    Focus::Tree => Focus::Hex,
                    Focus::Hex => Focus::Tree,
                };
                if self.focus == Focus::Hex {
                    // the cursor starts on the field that is selected
                    self.show_selected_field();
                }
            }
            code => match self.focus {
                Focus::Tree => {
                    let before = self.tree.selected_path().to_vec();
                    self.tree_key(code);
                    if self.tree.selected_path() != before.as_slice() {
                        self.show_selected_field();
                    }
                }
                Focus::Hex => self.hex_key(code),
            },
        }
    }

    /// Scroll the bytes to the selected field, and put the cursor at its start.
    pub fn show_selected_field(&mut self) {
        let size = self.bytes.as_ref().map_or(0, FileBytes::size);
        if let Some(selection) = Selection::of(self.tree.selected()) {
            self.hex.reveal(selection.first_byte(), size);
        }
    }

    fn tree_key(&mut self, code: KeyCode) {
        let tree = &mut self.tree;
        match code {
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

    fn hex_key(&mut self, code: KeyCode) {
        let size = self.bytes.as_ref().map_or(0, FileBytes::size);
        let line = i64::try_from(self.hex.bytes_per_line()).unwrap_or(16);
        let page = line.saturating_mul(i64::try_from(self.hex.page_lines()).unwrap_or(16));
        let cursor = self.hex.cursor();
        match code {
            KeyCode::Left | KeyCode::Char('h') => self.hex.move_by(-1, size),
            KeyCode::Right | KeyCode::Char('l') => self.hex.move_by(1, size),
            KeyCode::Up | KeyCode::Char('k') => self.hex.move_by(-line, size),
            KeyCode::Down | KeyCode::Char('j') => self.hex.move_by(line, size),
            KeyCode::PageUp => self.hex.move_by(-page, size),
            KeyCode::PageDown => self.hex.move_by(page, size),
            KeyCode::Home => {
                let width = u64::try_from(line).unwrap_or(16).max(1);
                self.hex.set_cursor(cursor - cursor % width, size);
            }
            KeyCode::End => {
                let width = u64::try_from(line).unwrap_or(16).max(1);
                self.hex
                    .set_cursor(cursor - cursor % width + width - 1, size);
            }
            KeyCode::Char('g') => self.hex.set_cursor(0, size),
            KeyCode::Char('G') => self.hex.set_cursor(size, size),
            _ => return,
        }
        // the tree follows the byte: the innermost field that holds it
        if let Some(path) = self.tree.find_byte(self.hex.cursor())
            && path.as_slice() != self.tree.selected_path()
        {
            self.tree.select_path(&path);
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

    /// A viewer for `data` parsed with `schema`.
    pub(crate) fn app_for(schema: &str, data: &[u8]) -> (App, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let schema_path = dir.path().join("t.yaml");
        std::fs::write(&schema_path, schema).unwrap();
        let file = dir.path().join("t.bin");
        std::fs::write(&file, data).unwrap();
        let loaded = load(&Options {
            file: file.clone(),
            type_or_schema: schema_path.to_string_lossy().into_owned(),
            include_paths: Vec::new(),
            color: false,
        })
        .unwrap();
        (App::new(file, loaded), dir)
    }

    #[test]
    fn tab_moves_the_keys_between_the_tree_and_the_bytes() {
        let (mut app, _dir) = app();
        assert_eq!(app.focus, Focus::Tree);
        key(&mut app, KeyCode::Tab);
        assert_eq!(app.focus, Focus::Hex);
        key(&mut app, KeyCode::Tab);
        assert_eq!(app.focus, Focus::Tree);
        key(&mut app, KeyCode::BackTab);
        assert_eq!(app.focus, Focus::Hex);
        assert!(!app.quit);
        // q still leaves from either pane
        key(&mut app, KeyCode::Char('q'));
        assert!(app.quit);
    }

    #[test]
    fn moving_in_the_bytes_selects_the_innermost_field_of_the_byte() {
        let (mut app, _dir) = app(); // a = byte 0, b = bytes 1..3
        key(&mut app, KeyCode::Tab);
        assert_eq!(app.hex.cursor(), 0);
        key(&mut app, KeyCode::Right);
        assert_eq!(
            (app.hex.cursor(), app.tree.selected().name.as_str()),
            (1, "b")
        );
        key(&mut app, KeyCode::Char('l'));
        assert_eq!(
            (app.hex.cursor(), app.tree.selected().name.as_str()),
            (2, "b")
        );
        key(&mut app, KeyCode::Right); // the end of the file: stays on the last byte
        assert_eq!(app.hex.cursor(), 2);
        key(&mut app, KeyCode::Char('h'));
        key(&mut app, KeyCode::Left);
        assert_eq!(
            (app.hex.cursor(), app.tree.selected().name.as_str()),
            (0, "a")
        );
        key(&mut app, KeyCode::Char('G'));
        assert_eq!(app.hex.cursor(), 2);
        key(&mut app, KeyCode::Char('g'));
        assert_eq!(
            (app.hex.cursor(), app.tree.selected().name.as_str()),
            (0, "a")
        );
        // a line down in a file of one line is the last byte; the line keys stay in the file
        key(&mut app, KeyCode::Down);
        assert_eq!(app.hex.cursor(), 2);
        key(&mut app, KeyCode::Up);
        key(&mut app, KeyCode::PageDown);
        key(&mut app, KeyCode::PageUp);
        key(&mut app, KeyCode::End);
        key(&mut app, KeyCode::Home);
        assert!(app.hex.cursor() <= 2);
    }

    #[test]
    fn moving_in_the_tree_scrolls_the_bytes_to_the_field() {
        // 160 bytes of padding, a marker byte, then a two-byte field
        let schema = "id: t\nseq:\n  - id: pad\n    size: 160\n  - id: marker\n    type: u1\n  - id: tail\n    type: u2le\n";
        let (mut app, _dir) = app_for(schema, &[0; 163]);
        key(&mut app, KeyCode::Down); // pad
        assert_eq!(app.hex.cursor(), 0);
        key(&mut app, KeyCode::Down); // marker
        assert_eq!(app.tree.selected().name, "marker");
        assert_eq!(app.hex.cursor(), 160);
        key(&mut app, KeyCode::Down); // tail
        assert_eq!(app.hex.cursor(), 161);
        key(&mut app, KeyCode::Char('g')); // the root again: the start of the file
        assert_eq!(app.hex.cursor(), 0);
        // going to the bytes and back changes nothing
        key(&mut app, KeyCode::Char('G'));
        key(&mut app, KeyCode::Tab);
        assert_eq!(app.hex.cursor(), 161);
        key(&mut app, KeyCode::Tab);
        assert_eq!(app.tree.selected().name, "tail");
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
