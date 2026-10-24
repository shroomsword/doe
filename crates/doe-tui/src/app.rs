//! The state of the viewer and what a key does to it. Nothing here touches a
//! terminal, so it is tested by calling it.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use doe::DoeError;

use crate::bytes::FileBytes;
use crate::hex_view::{HexState, Selection};
use crate::keymap::{self, Action, Scope};
use crate::load::{LoadFailure, Loaded};
use crate::prompt::{Kind, LineEditor, Outcome as PromptOutcome};
use crate::report::Report;
use crate::search::{Needle, Outcome as SearchOutcome, Search};
use crate::tree::TreeModel;
use crate::tree_view::TreeView;

/// How many fields a search looks at between two redraws of the screen.
pub const SEARCH_STEP: usize = 5_000;

/// The pane that keys go to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The tree of fields.
    Tree,
    /// The bytes.
    Hex,
}

/// The line that is being typed at the bottom of the screen.
#[derive(Debug, Clone)]
pub struct Prompt {
    /// What it is for.
    pub kind: Kind,
    /// The text and the cursor.
    pub line: LineEditor,
}

/// The viewer.
#[derive(Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each flag is a separate thing that is shown or not; they are not a state machine"
)]
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
    /// Whether the inspector shows the selected bytes in other forms.
    pub inspector: bool,
    /// Whether the help is shown.
    pub help: bool,
    /// What went wrong, if something did: a parse that stopped, or nothing parsed at all.
    pub report: Option<Report>,
    /// Whether the message about a parse that failed is shown.
    pub show_report: bool,
    /// The line being typed, if any.
    pub prompt: Option<Prompt>,
    /// The search that is going on, if any (it is advanced by [`App::search_step`]).
    pub search: Option<Search>,
    /// What was last searched for, for `n` and `N`.
    pub last_query: Option<String>,
    /// A message for the status bar, until the next key.
    pub notice: Option<String>,
    /// Whether the person asked to leave.
    pub quit: bool,
}

/// An offset as it is typed: decimal or `0x` hex, `_` allowed between digits.
///
/// # Errors
///
/// What is wrong with the text.
pub fn parse_offset(text: &str) -> Result<u64, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("type an offset: a number, or 0x… for hex".to_owned());
    }
    let (digits, radix) = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => (hex, 16),
        None => (text, 10),
    };
    let cleaned: String = digits.chars().filter(|&c| c != '_').collect();
    if cleaned.is_empty() || cleaned.starts_with('-') || cleaned.starts_with('+') {
        return Err(format!("not an offset: {}", crate::format::safe(text)));
    }
    u64::from_str_radix(&cleaned, radix).map_err(|e| {
        if matches!(e.kind(), std::num::IntErrorKind::PosOverflow) {
            format!("{} is too large for an offset", crate::format::safe(text))
        } else {
            format!("not an offset: {}", crate::format::safe(text))
        }
    })
}

impl App {
    /// A viewer for a parsed file. If the parse stopped, the field where it did
    /// is selected and its bytes are shown.
    #[must_use]
    pub fn new(file: PathBuf, loaded: Loaded) -> Self {
        let mut app = App {
            bytes: FileBytes::open(&file).ok(),
            hex: HexState::new(),
            focus: Focus::Tree,
            inspector: false,
            help: false,
            report: None,
            show_report: true,
            prompt: None,
            search: None,
            last_query: None,
            notice: None,
            file,
            type_name: loaded.type_name,
            size: loaded.size,
            error: loaded.error,
            tree: TreeModel::new(loaded.trace, loaded.value),
            view: TreeView::new(),
            quit: false,
        };
        if let Some(error) = &app.error {
            let path = app.tree.failed_path().unwrap_or_default();
            let names = app.tree.names_to(&path);
            // the type the field is in: the nearest one above it that has a name
            let in_type = (0..path.len()).rev().find_map(|depth| {
                app.tree
                    .node(path.get(..depth)?)
                    .and_then(|n| n.type_name.clone())
            });
            app.report = Some(Report::from_parse(error, &names, in_type.as_deref()));
            app.tree.select_path(&path);
            app.show_selected_field();
        }
        app
    }

    /// A viewer for a file that could not be parsed at all: it shows what went
    /// wrong and where, and the bytes of the file if it can be read.
    #[must_use]
    pub fn broken(
        file: PathBuf,
        failure: &LoadFailure,
        schema_file: Option<&std::path::Path>,
    ) -> Self {
        App {
            bytes: FileBytes::open(&file).ok(),
            hex: HexState::new(),
            focus: Focus::Tree,
            inspector: false,
            help: false,
            report: Some(Report::from_failure(failure, schema_file)),
            show_report: true,
            prompt: None,
            search: None,
            last_query: None,
            notice: None,
            file,
            type_name: failure.type_name.clone(),
            size: failure.size,
            error: None,
            tree: TreeModel::new(
                doe::parser::trace::TraceNode::named(&failure.type_name),
                None,
            ),
            view: TreeView::new(),
            quit: false,
        }
    }

    /// What is said after the viewer has been left, if the last parse failed:
    /// the message, and that the command exits with a failure.
    #[must_use]
    pub fn finish_message(&self) -> Option<String> {
        let report = self.report.as_ref()?;
        Some(format!(
            "{}: {}",
            crate::format::safe(&self.file.display().to_string()),
            report.one_line()
        ))
    }

    /// Whether a search is going on (the event loop then advances it between keys).
    #[must_use]
    pub fn searching(&self) -> bool {
        self.search.is_some()
    }

    /// What a key does. The keys are in [`keymap::BINDINGS`] and in the help
    /// (`?`); this decides who gets a key:
    ///
    /// - a search that is going on is stopped by any key (`Esc` and Ctrl-C
    ///   only stop it);
    /// - the help is closed by any key;
    /// - a prompt takes the key as text or as an edit of it;
    /// - otherwise the pane that has the keys, then the keys for both.
    pub fn on_key(&mut self, key: KeyEvent) {
        self.notice = None;
        if self.search.take().is_some() {
            let stops = key.code == KeyCode::Esc
                || (key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL));
            self.notice = Some("search stopped".to_owned());
            if stops {
                return;
            }
        }
        if self.help {
            self.help = false;
            return;
        }
        if self.prompt.is_some() {
            self.prompt_key(&key);
            return;
        }
        let pane = match self.focus {
            Focus::Tree => Scope::Tree,
            Focus::Hex => Scope::Bytes,
        };
        if let Some(action) = keymap::lookup(pane, &key) {
            self.act(action);
        }
    }

    /// Text that is pasted: it goes into the prompt, if there is one.
    pub fn on_paste(&mut self, text: &str) {
        if let Some(prompt) = &mut self.prompt {
            prompt.line.insert_str(text);
        }
    }

    fn prompt_key(&mut self, key: &KeyEvent) {
        let Some(prompt) = self.prompt.as_mut() else {
            return;
        };
        match prompt.line.key(key) {
            PromptOutcome::Editing => {}
            PromptOutcome::Cancel => self.prompt = None,
            PromptOutcome::Submit => {
                let Some(prompt) = self.prompt.take() else {
                    return;
                };
                match prompt.kind {
                    Kind::Search => self.start_search(prompt.line.text(), true),
                    Kind::Goto => self.goto(prompt.line.text()),
                }
            }
        }
    }

    fn open_prompt(&mut self, kind: Kind) {
        self.prompt = Some(Prompt {
            kind,
            line: LineEditor::new(),
        });
    }

    /// Start a search for `query` from the selected field (the last query if it is empty).
    fn start_search(&mut self, query: &str, forward: bool) {
        let query = if query.is_empty() {
            self.last_query.clone().unwrap_or_default()
        } else {
            query.to_owned()
        };
        let Some(needle) = Needle::new(&query) else {
            self.notice = Some("nothing to search for".to_owned());
            return;
        };
        self.last_query = Some(query);
        self.search = Some(Search::new(
            needle,
            forward,
            self.tree.selected_path().to_vec(),
        ));
    }

    /// Look at the next [`SEARCH_STEP`] fields of the search that is going on.
    pub fn search_step(&mut self) {
        let Some(mut search) = self.search.take() else {
            return;
        };
        match search.step(&self.tree, SEARCH_STEP) {
            SearchOutcome::Running => self.search = Some(search),
            SearchOutcome::Found(path) => {
                self.tree.select_path(&path);
                self.show_selected_field();
                if search.wrapped() {
                    self.notice = Some("search wrapped around".to_owned());
                }
            }
            SearchOutcome::NotFound => {
                self.notice = Some(format!(
                    "not found: {}",
                    crate::format::safe(search.needle().query())
                ));
            }
        }
    }

    /// Go to the offset typed in `text`: the bytes show it and the tree selects the
    /// innermost field that holds it.
    fn goto(&mut self, text: &str) {
        let offset = match parse_offset(text) {
            Ok(offset) => offset,
            Err(message) => {
                self.notice = Some(message);
                return;
            }
        };
        let size = self.bytes.as_ref().map_or(0, FileBytes::size);
        if offset >= size {
            self.notice = Some(format!(
                "offset {offset:#x} is past the end of the file ({size} bytes)"
            ));
            return;
        }
        match self.tree.find_byte(offset) {
            Some(path) => self.tree.select_path(&path),
            None => {
                self.notice = Some(format!(
                    "no field holds byte {offset:#x} (it is after what was parsed)"
                ));
            }
        }
        self.hex.reveal(offset, size);
    }

    fn act(&mut self, action: Action) {
        match action {
            Action::Quit => self.quit = true,
            Action::ToggleFocus => {
                self.focus = match self.focus {
                    Focus::Tree => Focus::Hex,
                    Focus::Hex => Focus::Tree,
                };
                if self.focus == Focus::Hex {
                    // the cursor starts on the field that is selected
                    self.show_selected_field();
                }
            }
            Action::ToggleInspector => self.inspector = !self.inspector,
            Action::Help => self.help = true,
            Action::ToggleReport => self.show_report = !self.show_report,
            Action::Search => self.open_prompt(Kind::Search),
            Action::SearchNext | Action::SearchPrevious => match self.last_query.clone() {
                Some(query) => self.start_search(&query, action == Action::SearchNext),
                None => self.notice = Some("nothing searched for yet: press /".to_owned()),
            },
            Action::Goto => self.open_prompt(Kind::Goto),
            Action::HexLeft
            | Action::HexRight
            | Action::HexUp
            | Action::HexDown
            | Action::HexPageUp
            | Action::HexPageDown
            | Action::HexLineStart
            | Action::HexLineEnd
            | Action::HexStart
            | Action::HexEnd => self.hex_action(action),
            tree_action => {
                let before = self.tree.selected_path().to_vec();
                self.tree_action(tree_action);
                if self.tree.selected_path() != before.as_slice() {
                    self.show_selected_field();
                }
            }
        }
    }

    /// Scroll the bytes to the selected field, and put the cursor at its start.
    pub fn show_selected_field(&mut self) {
        let size = self.bytes.as_ref().map_or(0, FileBytes::size);
        let row = self.tree.selected();
        if let Some(selection) = Selection::of(row) {
            self.hex.reveal(selection.first_byte(), size);
        } else if let Some(span) = row.span.filter(|_| !row.processed) {
            // a field that failed has no bits, but it has a place
            self.hex.reveal(span.start / 8, size);
        }
    }

    fn tree_action(&mut self, action: Action) {
        let tree = &mut self.tree;
        match action {
            Action::TreeUp => tree.up(),
            Action::TreeDown => tree.down(),
            Action::TreePageUp => tree.page_up(self.view.page()),
            Action::TreePageDown => tree.page_down(self.view.page()),
            Action::TreeHome => tree.home(),
            Action::TreeEnd => tree.end(),
            Action::TreeRight => {
                if tree.selected().openable && !tree.selected().open {
                    tree.expand();
                } else if tree.selected().openable {
                    tree.first_child();
                } else {
                    // the "more" row shows the next piece
                    tree.expand();
                }
            }
            Action::TreeLeft => {
                if tree.selected().open {
                    tree.collapse();
                } else {
                    tree.parent();
                }
            }
            Action::TreeToggle => tree.toggle(),
            Action::NextSibling => tree.next_sibling(),
            Action::PreviousSibling => tree.prev_sibling(),
            _ => {}
        }
    }

    fn hex_action(&mut self, action: Action) {
        let size = self.bytes.as_ref().map_or(0, FileBytes::size);
        let line = i64::try_from(self.hex.bytes_per_line()).unwrap_or(16);
        let page = line.saturating_mul(i64::try_from(self.hex.page_lines()).unwrap_or(16));
        let cursor = self.hex.cursor();
        let width = u64::try_from(line).unwrap_or(16).max(1);
        match action {
            Action::HexLeft => self.hex.move_by(-1, size),
            Action::HexRight => self.hex.move_by(1, size),
            Action::HexUp => self.hex.move_by(-line, size),
            Action::HexDown => self.hex.move_by(line, size),
            Action::HexPageUp => self.hex.move_by(-page, size),
            Action::HexPageDown => self.hex.move_by(page, size),
            Action::HexLineStart => self.hex.set_cursor(cursor - cursor % width, size),
            Action::HexLineEnd => self
                .hex
                .set_cursor(cursor - cursor % width + width - 1, size),
            Action::HexStart => self.hex.set_cursor(0, size),
            Action::HexEnd => self.hex.set_cursor(size, size),
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

    /// A key without modifiers, for other tests.
    pub(crate) fn press_key(app: &mut App, code: KeyCode) {
        key(app, code);
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

    /// Fields to search: a name, a count, and three items with an enum and a label.
    const REC: &str = "id: rec
seq:
  - id: name
    type: str
    size: 5
    encoding: UTF-8
  - id: Count
    type: u1
  - id: items
    type: item
    repeat: expr
    repeat-expr: 3
types:
  item:
    seq:
      - id: kind
        type: u1
        enum: kinds
      - id: label
        type: str
        size: 2
        encoding: UTF-8
enums:
  kinds:
    1: alpha
    2: beta
";

    fn rec_data() -> Vec<u8> {
        let mut data = b"Hello".to_vec();
        data.push(7);
        for (kind, label) in [(1_u8, "aa"), (2, "bB"), (1, "cc")] {
            data.push(kind);
            data.extend(label.as_bytes());
        }
        data
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            key(app, KeyCode::Char(c));
        }
    }

    /// Run the search that is going on to its end, as the event loop would.
    fn finish_search(app: &mut App) {
        let mut steps = 0;
        while app.searching() {
            app.search_step();
            steps += 1;
            assert!(steps < 100, "the search never ends");
        }
    }

    fn selected(app: &App) -> Vec<String> {
        app.tree.names_to(app.tree.selected_path())
    }

    #[test]
    fn offsets_are_decimal_or_hex() {
        assert_eq!(parse_offset("0"), Ok(0));
        assert_eq!(parse_offset(" 42 "), Ok(42));
        assert_eq!(parse_offset("0x2a"), Ok(42));
        assert_eq!(parse_offset("0X2A"), Ok(42));
        assert_eq!(parse_offset("1_000"), Ok(1000));
        assert_eq!(parse_offset("0xffff_ffff_ffff_ffff"), Ok(u64::MAX));
        for bad in ["", "  ", "abc", "0x", "-5", "+5", "0xg", "1.5", "0b11"] {
            assert!(parse_offset(bad).is_err(), "{bad:?}");
        }
        assert!(
            parse_offset("18446744073709551616")
                .unwrap_err()
                .contains("too large")
        );
        // what is wrong is said without printing raw text
        assert!(!parse_offset("a\x1bb").unwrap_err().contains('\x1b'));
    }

    #[test]
    fn searching_with_the_prompt_selects_the_match_and_shows_its_bytes() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        key(&mut app, KeyCode::Char('/'));
        assert!(app.prompt.is_some());
        type_text(&mut app, "beta");
        key(&mut app, KeyCode::Enter);
        assert!(app.prompt.is_none() && app.searching());
        finish_search(&mut app);
        assert_eq!(selected(&app), ["rec", "items", "[1]", "kind"]);
        // the bytes follow: kind of the second item is byte 9
        assert_eq!(app.hex.cursor(), 9);
        assert_eq!(app.last_query.as_deref(), Some("beta"));
        // n goes on, round to the same one (it is the only match), and says so
        key(&mut app, KeyCode::Char('n'));
        finish_search(&mut app);
        assert_eq!(selected(&app), ["rec", "items", "[1]", "kind"]);
        assert_eq!(app.notice.as_deref(), Some("search wrapped around"));
    }

    #[test]
    fn n_and_capital_n_walk_through_the_matches_both_ways() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        key(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "label");
        key(&mut app, KeyCode::Enter);
        finish_search(&mut app);
        assert_eq!(selected(&app), ["rec", "items", "[0]", "label"]);
        key(&mut app, KeyCode::Char('n'));
        finish_search(&mut app);
        assert_eq!(selected(&app), ["rec", "items", "[1]", "label"]);
        key(&mut app, KeyCode::Char('n'));
        finish_search(&mut app);
        assert_eq!(selected(&app), ["rec", "items", "[2]", "label"]);
        key(&mut app, KeyCode::Char('N'));
        finish_search(&mut app);
        assert_eq!(selected(&app), ["rec", "items", "[1]", "label"]);
        // round the other way
        key(&mut app, KeyCode::Char('N'));
        finish_search(&mut app);
        key(&mut app, KeyCode::Char('N'));
        finish_search(&mut app);
        assert_eq!(selected(&app), ["rec", "items", "[2]", "label"]);
        assert_eq!(app.notice.as_deref(), Some("search wrapped around"));
    }

    #[test]
    fn a_search_with_no_match_says_so_and_changes_nothing() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        key(&mut app, KeyCode::Down);
        let before = selected(&app);
        key(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "no such thing");
        key(&mut app, KeyCode::Enter);
        finish_search(&mut app);
        assert_eq!(app.notice.as_deref(), Some("not found: no such thing"));
        assert_eq!(selected(&app), before);
        // case: a capital in the query makes it exact
        key(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "HELLO");
        key(&mut app, KeyCode::Enter);
        finish_search(&mut app);
        assert!(app.notice.as_deref().unwrap().starts_with("not found"));
        key(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "hello");
        key(&mut app, KeyCode::Enter);
        finish_search(&mut app);
        assert_eq!(selected(&app), ["rec", "name"]);
    }

    #[test]
    fn an_empty_query_repeats_the_last_one_and_n_needs_one() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        key(&mut app, KeyCode::Char('n'));
        assert_eq!(
            app.notice.as_deref(),
            Some("nothing searched for yet: press /")
        );
        key(&mut app, KeyCode::Char('/'));
        key(&mut app, KeyCode::Enter); // nothing typed, nothing before
        assert_eq!(app.notice.as_deref(), Some("nothing to search for"));
        key(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "kind");
        key(&mut app, KeyCode::Enter);
        finish_search(&mut app);
        let first = selected(&app);
        key(&mut app, KeyCode::Char('/'));
        key(&mut app, KeyCode::Enter); // the same query again, from here
        finish_search(&mut app);
        assert_ne!(selected(&app), first);
    }

    #[test]
    fn a_key_stops_a_search_that_is_going_on_and_is_then_used() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        app.search = Some(Search::new(Needle::new("beta").unwrap(), true, Vec::new()));
        assert!(app.searching());
        key(&mut app, KeyCode::Down); // stops it, and moves
        assert!(!app.searching());
        assert_eq!(app.notice, Some("search stopped".to_owned()));
        assert_eq!(selected(&app), ["rec", "name"]);
        // Esc and Ctrl-C only stop it: they do not leave
        app.search = Some(Search::new(Needle::new("beta").unwrap(), true, Vec::new()));
        key(&mut app, KeyCode::Esc);
        assert!(!app.searching() && !app.quit);
        app.search = Some(Search::new(Needle::new("beta").unwrap(), true, Vec::new()));
        press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(!app.searching() && !app.quit);
        key(&mut app, KeyCode::Esc);
        assert!(app.quit);
    }

    #[test]
    fn goto_selects_the_innermost_field_at_an_offset() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        // 0x7 is the first byte of the label of the first item
        key(&mut app, KeyCode::Char('g'));
        type_text(&mut app, "0x7");
        key(&mut app, KeyCode::Enter);
        assert_eq!(selected(&app), ["rec", "items", "[0]", "label"]);
        assert_eq!(app.hex.cursor(), 7);
        // decimal; a byte of the last label
        key(&mut app, KeyCode::Char('g'));
        type_text(&mut app, "13");
        key(&mut app, KeyCode::Enter);
        assert_eq!(selected(&app), ["rec", "items", "[2]", "label"]);
        assert_eq!(app.hex.cursor(), 13);
        key(&mut app, KeyCode::Char('g'));
        type_text(&mut app, "0");
        key(&mut app, KeyCode::Enter);
        assert_eq!(selected(&app), ["rec", "name"]);
        assert!(app.notice.is_none());
    }

    #[test]
    fn goto_out_of_the_file_or_with_a_bad_offset_says_so_and_stays() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        key(&mut app, KeyCode::Down);
        let before = (selected(&app), app.hex.cursor());
        for (typed, said) in [
            ("15", "offset 0xf is past the end of the file (15 bytes)"),
            (
                "0xffff",
                "offset 0xffff is past the end of the file (15 bytes)",
            ),
            ("nope", "not an offset: nope"),
            ("", "type an offset: a number, or 0x… for hex"),
            ("-1", "not an offset: -1"),
        ] {
            key(&mut app, KeyCode::Char('g'));
            type_text(&mut app, typed);
            key(&mut app, KeyCode::Enter);
            assert_eq!(app.notice.as_deref(), Some(said));
            assert_eq!((selected(&app), app.hex.cursor()), before);
        }
    }

    #[test]
    fn goto_in_a_gap_between_fields_selects_the_type_that_holds_it_or_nothing() {
        // `inner` is 4 bytes but its fields read 2: bytes 2 and 3 are in no field of it
        let schema = "id: gap\nseq:\n  - id: inner\n    type: pair\n    size: 4\ntypes:\n  pair:\n    seq:\n      - id: x\n        type: u1\n      - id: y\n        type: u1\n";
        let (mut app, _dir) = app_for(schema, &[1, 2, 3, 4]);
        key(&mut app, KeyCode::Char('g'));
        type_text(&mut app, "3");
        key(&mut app, KeyCode::Enter);
        assert_eq!(
            selected(&app),
            ["gap", "inner"],
            "the type that holds the byte"
        );
        assert_eq!(app.hex.cursor(), 3);
        // bytes after what the type reads: no field holds them
        let schema = "id: short\nseq:\n  - id: a\n    type: u2le\n";
        let (mut app, _dir) = app_for(schema, &[1, 2, 3, 4, 5]);
        key(&mut app, KeyCode::Char('g'));
        type_text(&mut app, "4");
        key(&mut app, KeyCode::Enter);
        assert_eq!(
            app.notice.as_deref(),
            Some("no field holds byte 0x4 (it is after what was parsed)")
        );
        assert_eq!(selected(&app), ["short"], "the selection is as it was");
        assert_eq!(app.hex.cursor(), 4, "the bytes show it");
    }

    #[test]
    fn the_prompt_takes_keys_as_text_and_pastes_and_cancels() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        key(&mut app, KeyCode::Char('/'));
        // keys that do things elsewhere are text here
        type_text(&mut app, "qig?/");
        assert!(!app.quit && !app.inspector && !app.help);
        assert_eq!(app.prompt.as_ref().unwrap().line.text(), "qig?/");
        key(&mut app, KeyCode::Left);
        key(&mut app, KeyCode::Backspace);
        assert_eq!(app.prompt.as_ref().unwrap().line.text(), "qig/");
        app.on_paste("beta\nsecond line");
        assert_eq!(app.prompt.as_ref().unwrap().line.text(), "qigbeta/");
        // Esc leaves the prompt, not the viewer
        key(&mut app, KeyCode::Esc);
        assert!(app.prompt.is_none() && !app.quit);
        // a paste with no prompt goes nowhere
        app.on_paste("beta");
        assert!(app.prompt.is_none());
        // the keys of the tree work again
        key(&mut app, KeyCode::Down);
        assert_eq!(selected(&app), ["rec", "name"]);
    }

    #[test]
    fn question_mark_shows_the_help_and_the_next_key_closes_it_and_does_nothing_else() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        key(&mut app, KeyCode::Char('?'));
        assert!(app.help);
        key(&mut app, KeyCode::Char('q'));
        assert!(
            !app.help && !app.quit,
            "the key closed the help and was not used"
        );
        key(&mut app, KeyCode::Char('q'));
        assert!(app.quit);
    }

    #[test]
    fn brackets_go_to_the_next_and_previous_field_at_the_same_level() {
        let (mut app, _dir) = app_for(REC, &rec_data());
        key(&mut app, KeyCode::Down); // name
        key(&mut app, KeyCode::Char(']'));
        assert_eq!(selected(&app), ["rec", "Count"]);
        key(&mut app, KeyCode::Char(']'));
        assert_eq!(selected(&app), ["rec", "items"]);
        key(&mut app, KeyCode::Char(']'));
        assert_eq!(selected(&app), ["rec", "items"], "no field after the last");
        key(&mut app, KeyCode::Char('['));
        key(&mut app, KeyCode::Char('['));
        assert_eq!(selected(&app), ["rec", "name"]);
        // the bytes follow
        key(&mut app, KeyCode::Char(']'));
        assert_eq!(app.hex.cursor(), 5);
    }

    /// A parse that fails in a type inside a type: `body` needs three bytes and has two.
    const NESTED_FAIL: &str = "id: rec
seq:
  - id: head
    type: u1
  - id: body
    type: pair
types:
  pair:
    seq:
      - id: x
        type: u1
      - id: y
        type: u2le
";

    /// A parse that fails in the third element of a repeated field.
    const REPEATED_FAIL: &str = "id: rec
seq:
  - id: count
    type: u1
  - id: items
    type: u2le
    repeat: expr
    repeat-expr: count
";

    #[test]
    fn a_parse_that_fails_in_a_nested_type_selects_the_field_and_shows_its_bytes() {
        let (app, _dir) = app_for(NESTED_FAIL, &[1, 5, 9]);
        assert!(app.error.is_some());
        let report = app.report.as_ref().unwrap();
        assert!(!report.fatal);
        assert_eq!(
            report.headline,
            "the parse failed at rec/body/y (in rec::pair)"
        );
        assert_ne!(report.causes.len(), 0);
        // the failed field is selected, marked, and the bytes are at it
        assert_eq!(selected(&app), ["rec", "body", "y"]);
        assert!(app.tree.selected().failed);
        assert_eq!(app.hex.cursor(), 2);
        // what was read before is in the tree
        assert!(
            app.tree
                .rows()
                .iter()
                .any(|r| r.name == "x" && r.shown.is_some())
        );
        assert!(app.show_report);
    }

    #[test]
    fn a_parse_that_fails_in_a_repeated_field_selects_the_element() {
        let (app, _dir) = app_for(REPEATED_FAIL, &[3, 1, 0, 2, 0, 9]);
        let report = app.report.as_ref().unwrap();
        assert_eq!(
            report.headline,
            "the parse failed at rec/items/[2] (in rec)"
        );
        assert_eq!(selected(&app), ["rec", "items", "[2]"]);
        assert!(app.tree.selected().failed);
        assert_eq!(app.hex.cursor(), 5);
        // the two elements that were read are there
        assert!(app.tree.rows().iter().any(|r| r.name == "[1]"));
    }

    #[test]
    fn a_parse_that_works_has_no_report_and_the_viewer_ends_well() {
        let (app, _dir) = app();
        assert!(app.report.is_none());
        assert_eq!(app.finish_message(), None);
    }

    #[test]
    fn what_is_said_when_the_viewer_is_left_names_the_file_and_the_error() {
        let (app, dir) = app_for(NESTED_FAIL, &[1, 5, 9]);
        let message = app.finish_message().unwrap();
        let file = dir.path().join("t.bin").display().to_string();
        assert!(
            message.starts_with(&format!(
                "{file}: the parse failed at rec/body/y (in rec::pair): "
            )),
            "{message}"
        );
    }

    #[test]
    fn e_shows_and_hides_the_message() {
        let (mut app, _dir) = app_for(NESTED_FAIL, &[1, 5, 9]);
        assert!(app.show_report);
        key(&mut app, KeyCode::Char('e'));
        assert!(!app.show_report);
        key(&mut app, KeyCode::Char('e'));
        assert!(app.show_report);
    }

    fn broken_app(schema: &str, data: Option<&[u8]>) -> (App, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let schema_path = dir.path().join("t.yaml");
        std::fs::write(&schema_path, schema).unwrap();
        let file = dir.path().join("t.bin");
        if let Some(data) = data {
            std::fs::write(&file, data).unwrap();
        }
        let options = Options {
            file: file.clone(),
            type_or_schema: schema_path.to_string_lossy().into_owned(),
            include_paths: vec![dir.path().to_path_buf()],
            color: false,
        };
        let failure = crate::load::try_load(&options).unwrap_err();
        let schema = crate::load::schema_file(&options);
        (App::broken(file, &failure, schema.as_deref()), dir)
    }

    #[test]
    fn a_schema_that_does_not_compile_leaves_the_viewer_open_with_the_error_and_the_bytes() {
        let (mut app, _dir) = broken_app(
            "id: bad\nseq:\n  - id: a\n    type: nope\n",
            Some(&[1, 2, 3]),
        );
        let report = app.report.as_ref().unwrap();
        assert!(report.fatal);
        assert_eq!(report.place.as_ref().unwrap().line, 4);
        assert!(
            app.finish_message()
                .unwrap()
                .contains("the schema bad cannot be used")
        );
        // the viewer works: the bytes can be moved in, the keys are handled, q leaves
        key(&mut app, KeyCode::Tab);
        key(&mut app, KeyCode::Right);
        assert_eq!(app.hex.cursor(), 1);
        key(&mut app, KeyCode::Char('/'));
        key(&mut app, KeyCode::Esc);
        assert!(!app.quit);
        key(&mut app, KeyCode::Char('q'));
        assert!(app.quit);
    }

    #[test]
    fn a_missing_file_is_a_report_and_the_viewer_has_no_bytes() {
        let (mut app, _dir) = broken_app("id: ok\nseq:\n  - id: a\n    type: u1\n", None);
        assert!(app.report.as_ref().unwrap().fatal);
        assert!(app.bytes.is_none());
        assert!(
            app.finish_message()
                .unwrap()
                .contains("the file cannot be read")
        );
        // every key is safe without a file
        for code in [
            KeyCode::Down,
            KeyCode::Tab,
            KeyCode::Right,
            KeyCode::End,
            KeyCode::Char('i'),
            KeyCode::Char('n'),
        ] {
            key(&mut app, code);
        }
        for text in ["0", "5"] {
            key(&mut app, KeyCode::Char('g'));
            type_text(&mut app, text);
            key(&mut app, KeyCode::Enter);
        }
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
        press(&mut app, KeyCode::Home, KeyModifiers::CONTROL);
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
        key(&mut app, KeyCode::Home); // the root again: the start of the file
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
        key(&mut app, KeyCode::Home);
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
