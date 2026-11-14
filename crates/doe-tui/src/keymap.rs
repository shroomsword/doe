//! The keys of the viewer, as data: what each one does, in which pane. The same
//! table is used to handle a key and to write the help, so a key cannot be
//! bound and not be in the help.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// What a key can do (when no prompt is open).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Leave.
    Quit,
    /// Move the keys between the tree and the bytes.
    ToggleFocus,
    /// Show or hide the inspector.
    ToggleInspector,
    /// Switch the tree pane between the fields and the files of an archive.
    Files,
    /// Files: the next algorithm of the checksum column (MD5, SHA-1, off).
    Checksums,
    /// Files: the previous algorithm of the checksum column.
    ChecksumsBack,
    /// Files: copy the checksum of the selected file.
    CopyChecksum,
    /// Files: preview the selected file, as text with its syntax coloured.
    Preview,
    /// Colour the bytes by the field they belong to, or not.
    ByteColours,
    /// Show the description of each field after its value, or not.
    Descriptions,
    /// While building: make the fields of the selected bytes (or the selected field) a new type of the schema.
    GroupFields,
    /// While building: take the type of the selected field apart: its fields take the field's place.
    UngroupField,
    /// While building, in the tree: add a field into the selected type, or where the schema ends.
    TreeAddField,
    /// Make the focused pane fill the screen, or put both panes back.
    Fullscreen,
    /// Show the next layer of a file that holds another file (a `.tar.gz` is gzip, then the tar it holds).
    Layers,
    /// Show the help.
    Help,
    /// Show or hide the message about a parse that failed.
    ToggleReport,
    /// Read the file with the change that the message about a failed parse suggests.
    TryFix,
    /// Take the mouse, or give it back to the terminal.
    ToggleMouse,
    /// Read the schema and the file again.
    Reload,
    /// Ask for text to search for.
    Search,
    /// The next match.
    SearchNext,
    /// The previous match.
    SearchPrevious,
    /// Ask for an offset.
    Goto,
    /// Jump to the value that the selected field places or sizes.
    FollowPointer,
    /// Go back to the place before the last jump.
    JumpBack,
    /// Go forward again after going back.
    JumpForward,
    /// List the schemas and their types.
    Schemas,
    /// Browse the types that are available on the type paths.
    Types,
    /// Browse the colour themes and choose one.
    Themes,
    /// Write all integers in the next radix.
    Radix,
    /// Choose a radix for the selected integer field.
    FieldRadix,
    /// Ask how to copy the selected field: its path, value, JSON, bytes, place or schema.
    CopyAs,
    /// Tree: one row up.
    TreeUp,
    /// Tree: one row down.
    TreeDown,
    /// Tree: a page up.
    TreePageUp,
    /// Tree: a page down.
    TreePageDown,
    /// Tree: the first row.
    TreeHome,
    /// Tree: the last row.
    TreeEnd,
    /// Tree: open a node or go into it.
    TreeRight,
    /// Tree: close a node or go to its parent.
    TreeLeft,
    /// Tree: open or close.
    TreeToggle,
    /// Tree: the next field at the same level.
    NextSibling,
    /// Tree: the previous field at the same level.
    PreviousSibling,
    /// Bytes: one byte back.
    HexLeft,
    /// Bytes: one byte on.
    HexRight,
    /// Bytes: a line up.
    HexUp,
    /// Bytes: a line down.
    HexDown,
    /// Bytes: a page up.
    HexPageUp,
    /// Bytes: a page down.
    HexPageDown,
    /// Bytes: the start of the line.
    HexLineStart,
    /// Bytes: the end of the line.
    HexLineEnd,
    /// Bytes: the first byte of the file.
    HexStart,
    /// Bytes: the last byte of the file.
    HexEnd,
    /// Bytes: move by a bit and not by a byte, or back.
    HexBits,
    /// Bytes: the next field that holds the byte under the cursor.
    HexNextField,
    /// Bytes: the previous field that holds the byte under the cursor.
    HexPrevField,
    /// Bytes: start a selection of bytes, or end it.
    HexSelect,
    /// Bytes: copy the selected bytes, as hex.
    HexCopy,
    /// Bytes (building a schema): add a field where the schema ends.
    HexAddField,
    /// Bytes (building a schema): add a field at the cursor, with a gap before it.
    HexAddFieldHere,
    /// Building a schema: take the last change back.
    Undo,
    /// Building a schema: do again what was taken back.
    Redo,
    /// Building a schema: remove the selected field.
    RemoveField,
    /// Building a schema: change the kind of the selected field.
    ChangeKind,
    /// Building a schema: change the byte order of the whole schema.
    ToggleByteOrder,
    /// Building a schema: save it.
    Save,
    /// Show or hide the row of keys under the panes.
    ToggleFooter,
    /// In a comparison: go to the next difference.
    NextDifference,
    /// In a comparison: go to the previous difference.
    PreviousDifference,
    /// In a comparison: leave out the fields that are the same, or show them again.
    OnlyDifferences,
    /// In a comparison: make the other file the current one, or go back to the first.
    OtherFile,
    /// In a comparison: the two trees side by side, or the tree of the first file with marks.
    CompareView,
}

/// Where a binding applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// In both panes.
    Anywhere,
    /// When the tree has the keys.
    Tree,
    /// When the bytes have the keys.
    Bytes,
}

impl Scope {
    /// The heading of the scope in the help.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Scope::Anywhere => "Anywhere",
            Scope::Tree => "In the tree",
            Scope::Bytes => "In the bytes",
        }
    }
}

/// A key and the modifiers it needs (`Shift` is not looked at: `G` is `G`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    /// The key.
    pub code: KeyCode,
    /// `Ctrl` or nothing.
    pub ctrl: bool,
    /// `Alt` or nothing.
    pub alt: bool,
}

const fn plain(code: KeyCode) -> Key {
    Key {
        code,
        ctrl: false,
        alt: false,
    }
}

const fn ctrl(code: KeyCode) -> Key {
    Key {
        code,
        ctrl: true,
        alt: false,
    }
}

const fn alt(code: KeyCode) -> Key {
    Key {
        code,
        ctrl: false,
        alt: true,
    }
}

const fn ch(c: char) -> Key {
    plain(KeyCode::Char(c))
}

/// One line of the table.
#[derive(Debug, Clone, Copy)]
pub struct Binding {
    /// The keys that do it.
    pub keys: &'static [Key],
    /// Where.
    pub scope: Scope,
    /// What it does.
    pub action: Action,
    /// The line in the help.
    pub help: &'static str,
}

/// Every key of the viewer. The first binding for a key in the pane that has
/// the keys is used, then the ones for both panes.
pub const BINDINGS: &[Binding] = &[
    Binding {
        keys: &[ch('q'), plain(KeyCode::Esc), ctrl(KeyCode::Char('c'))],
        scope: Scope::Anywhere,
        action: Action::Quit,
        help: "leave (Esc cancels a prompt, a search or this help first)",
    },
    Binding {
        keys: &[plain(KeyCode::Tab), plain(KeyCode::BackTab)],
        scope: Scope::Anywhere,
        action: Action::ToggleFocus,
        help: "move the keys between the tree and the bytes",
    },
    Binding {
        keys: &[ch('i')],
        scope: Scope::Anywhere,
        action: Action::ToggleInspector,
        help: "show or hide the inspector (the selected bytes as numbers and text)",
    },
    Binding {
        keys: &[ch('F')],
        scope: Scope::Anywhere,
        action: Action::Files,
        help: "switch the tree between the fields and the files of an archive (when the schema says it is a file system)",
    },
    Binding {
        keys: &[ch('D')],
        scope: Scope::Anywhere,
        action: Action::Descriptions,
        help: "show the description (doc:) of each field after its value, or not (the inspector, i, has the whole text of the selected field)",
    },
    Binding {
        keys: &[ch('f')],
        scope: Scope::Anywhere,
        action: Action::Fullscreen,
        help: "make the focused pane fill the screen, or put both panes back (Esc puts them back too, before it leaves; Tab then moves to the other pane, which fills the screen)",
    },
    Binding {
        keys: &[ch('Z')],
        scope: Scope::Anywhere,
        action: Action::GroupFields,
        help: "with --build: make the fields of the selected bytes (or the selected field) a new type of the schema, which other fields can have; it asks for the name",
    },
    Binding {
        keys: &[ch('O')],
        scope: Scope::Anywhere,
        action: Action::UngroupField,
        help: "with --build: take the type of the selected field apart (a type of the schema that no other field has): its fields take the place of the field",
    },
    Binding {
        keys: &[ch('L')],
        scope: Scope::Anywhere,
        action: Action::Layers,
        help: "show the next layer of a file that holds another file (a .tar.gz is the gzip file, then the tar it holds), and the file itself after the last",
    },
    Binding {
        keys: &[ch('B')],
        scope: Scope::Anywhere,
        action: Action::ByteColours,
        help: "colour the bytes by the field they belong to, in the colour of its value in the tree, or not (the selection is as it was)",
    },
    Binding {
        keys: &[ch('/')],
        scope: Scope::Anywhere,
        action: Action::Search,
        help: "search the names, types and values of the fields",
    },
    Binding {
        keys: &[ch('n')],
        scope: Scope::Anywhere,
        action: Action::SearchNext,
        help: "go to the next match",
    },
    Binding {
        keys: &[ch('N')],
        scope: Scope::Anywhere,
        action: Action::SearchPrevious,
        help: "go to the previous match",
    },
    Binding {
        keys: &[ch('J')],
        scope: Scope::Anywhere,
        action: Action::FollowPointer,
        help: "jump to what the selected field points at: the value its pos: places or its size: measures (a number that is an offset is a guess; Ctrl-O comes back)",
    },
    Binding {
        keys: &[ctrl(KeyCode::Char('o')), alt(KeyCode::Left)],
        scope: Scope::Anywhere,
        action: Action::JumpBack,
        help: "go back to where you were before the last jump (g); the places are kept for the session",
    },
    Binding {
        keys: &[ctrl(KeyCode::Char('n')), alt(KeyCode::Right)],
        scope: Scope::Anywhere,
        action: Action::JumpForward,
        help: "go forward again after going back",
    },
    Binding {
        keys: &[ch('g')],
        scope: Scope::Anywhere,
        action: Action::Goto,
        help: "go to an offset (decimal or 0x…) and select the field there",
    },
    Binding {
        keys: &[ch('s')],
        scope: Scope::Anywhere,
        action: Action::Schemas,
        help: "list the schemas and types (Enter reads a schema, or chooses the type to parse)",
    },
    Binding {
        keys: &[ch('T')],
        scope: Scope::Anywhere,
        action: Action::Themes,
        help: "browse the colour themes (each is shown as you move; Enter uses and saves it)",
    },
    Binding {
        keys: &[ch('t')],
        scope: Scope::Anywhere,
        action: Action::Types,
        help: "browse the types on the type paths (/ filters, v reads one, Enter parses with it)",
    },
    Binding {
        keys: &[ch('x')],
        scope: Scope::Anywhere,
        action: Action::Radix,
        help: "write the integers in the next radix: decimal, hex, binary, octal",
    },
    Binding {
        keys: &[ch('X')],
        scope: Scope::Anywhere,
        action: Action::FieldRadix,
        help: "choose a radix for the selected integer field (every element, and after a reload)",
    },
    Binding {
        keys: &[ch('Y')],
        scope: Scope::Anywhere,
        action: Action::CopyAs,
        help: "copy the selected field as…: p path, v value, j JSON, x hex, X hex without spaces, c C array, o offset and length, s schema field (to the clipboard of the terminal, where it allows it)",
    },
    Binding {
        keys: &[ch('e')],
        scope: Scope::Anywhere,
        action: Action::ToggleReport,
        help: "show or hide the message about a parse that failed",
    },
    Binding {
        keys: &[ch('>')],
        scope: Scope::Anywhere,
        action: Action::NextDifference,
        help: "when comparing two files (--compare): the next difference: in the tree the next field that differs, in the bytes the next run of bytes that differ",
    },
    Binding {
        keys: &[ch('<')],
        scope: Scope::Anywhere,
        action: Action::PreviousDifference,
        help: "when comparing two files: the previous difference",
    },
    Binding {
        keys: &[ch('=')],
        scope: Scope::Anywhere,
        action: Action::OnlyDifferences,
        help: "when comparing two files: leave out the fields that are the same, or show them again",
    },
    Binding {
        keys: &[ch('V')],
        scope: Scope::Anywhere,
        action: Action::OtherFile,
        help: "when comparing two files: make the other file the one shown (its tree and its bytes, at the same field), or go back to the first",
    },
    Binding {
        keys: &[ch('|')],
        scope: Scope::Anywhere,
        action: Action::CompareView,
        help: "when comparing two files: the two trees side by side, or the tree of the first file with marks and what only the second has",
    },
    Binding {
        keys: &[ch('K')],
        scope: Scope::Anywhere,
        action: Action::ToggleFooter,
        help: "show or hide the row of keys that matter now, under the panes (--no-footer and footer = false in the config file start with it hidden)",
    },
    Binding {
        keys: &[ch('z')],
        scope: Scope::Anywhere,
        action: Action::ToggleMouse,
        help: "give the mouse back to the terminal (to select and copy text with its own selection), or take it again; clicks, the wheel and drags are the viewer's while it has it",
    },
    Binding {
        keys: &[ch('o')],
        scope: Scope::Anywhere,
        action: Action::TryFix,
        help: "try what the message about a failed parse suggests (the other byte order): the file is read that way, the schema is not changed",
    },
    Binding {
        keys: &[ch('u')],
        scope: Scope::Anywhere,
        action: Action::Undo,
        help: "with --build: take the last change to the schema back",
    },
    Binding {
        keys: &[ctrl(KeyCode::Char('r'))],
        scope: Scope::Anywhere,
        action: Action::Redo,
        help: "with --build: do it again",
    },
    Binding {
        keys: &[ch('d')],
        scope: Scope::Anywhere,
        action: Action::RemoveField,
        help: "with --build: remove the selected field",
    },
    Binding {
        keys: &[ch('c')],
        scope: Scope::Anywhere,
        action: Action::ChangeKind,
        help: "with --build: change the kind of the selected field",
    },
    Binding {
        keys: &[ch('w')],
        scope: Scope::Anywhere,
        action: Action::Save,
        help: "with --build: save the schema to its file",
    },
    Binding {
        keys: &[ch('E')],
        scope: Scope::Anywhere,
        action: Action::ToggleByteOrder,
        help: "with --build: little or big endian for the schema",
    },
    Binding {
        keys: &[ch('r')],
        scope: Scope::Anywhere,
        action: Action::Reload,
        help: "read the schema and the file again (--build: rename the field)",
    },
    Binding {
        keys: &[ch('?')],
        scope: Scope::Anywhere,
        action: Action::Help,
        help: "show this help (/ searches it, the arrow keys scroll it)",
    },
    Binding {
        keys: &[plain(KeyCode::Up), ch('k')],
        scope: Scope::Tree,
        action: Action::TreeUp,
        help: "up one row",
    },
    Binding {
        keys: &[plain(KeyCode::Down), ch('j')],
        scope: Scope::Tree,
        action: Action::TreeDown,
        help: "down one row",
    },
    Binding {
        keys: &[plain(KeyCode::PageUp)],
        scope: Scope::Tree,
        action: Action::TreePageUp,
        help: "up a page",
    },
    Binding {
        keys: &[plain(KeyCode::PageDown)],
        scope: Scope::Tree,
        action: Action::TreePageDown,
        help: "down a page",
    },
    Binding {
        keys: &[plain(KeyCode::Home)],
        scope: Scope::Tree,
        action: Action::TreeHome,
        help: "the first row",
    },
    Binding {
        keys: &[plain(KeyCode::End), ch('G')],
        scope: Scope::Tree,
        action: Action::TreeEnd,
        help: "the last row",
    },
    Binding {
        keys: &[plain(KeyCode::Right), ch('l')],
        scope: Scope::Tree,
        action: Action::TreeRight,
        help: "open a node, or go into it (on \"N more…\" show more)",
    },
    Binding {
        keys: &[ch('a')],
        scope: Scope::Tree,
        action: Action::TreeAddField,
        help: "with --build: add a field into the selected field's type when that is a type of the schema, else where the schema ends",
    },
    Binding {
        keys: &[plain(KeyCode::Left), ch('h')],
        scope: Scope::Tree,
        action: Action::TreeLeft,
        help: "close a node, or go to its parent",
    },
    Binding {
        keys: &[plain(KeyCode::Enter), ch(' ')],
        scope: Scope::Tree,
        action: Action::TreeToggle,
        help: "open or close a node (in the files: Enter on a file previews it)",
    },
    Binding {
        keys: &[ch('m')],
        scope: Scope::Tree,
        action: Action::Checksums,
        help: "in the files: show the MD5 of each file, then the SHA-1, then hide them (checksums for comparing files, not a security measure; … while it is made, — for a file that has none)",
    },
    Binding {
        keys: &[ch('M')],
        scope: Scope::Tree,
        action: Action::ChecksumsBack,
        help: "in the files: the previous algorithm of the checksums (SHA-1, MD5, off)",
    },
    Binding {
        keys: &[ch('y')],
        scope: Scope::Tree,
        action: Action::CopyChecksum,
        help: "in the files: copy the checksum of the selected file to the clipboard of the terminal, where it allows it",
    },
    Binding {
        keys: &[ch('p')],
        scope: Scope::Tree,
        action: Action::Preview,
        help: "in the files: preview the selected file as text with its syntax coloured (↑↓ PgUp PgDn scroll, s saves the entry, Esc goes back)",
    },
    Binding {
        keys: &[ch(']')],
        scope: Scope::Tree,
        action: Action::NextSibling,
        help: "the next field at the same level",
    },
    Binding {
        keys: &[ch('[')],
        scope: Scope::Tree,
        action: Action::PreviousSibling,
        help: "the previous field at the same level",
    },
    Binding {
        keys: &[plain(KeyCode::Left), ch('h')],
        scope: Scope::Bytes,
        action: Action::HexLeft,
        help: "back one byte",
    },
    Binding {
        keys: &[plain(KeyCode::Right), ch('l')],
        scope: Scope::Bytes,
        action: Action::HexRight,
        help: "on one byte",
    },
    Binding {
        keys: &[plain(KeyCode::Up), ch('k')],
        scope: Scope::Bytes,
        action: Action::HexUp,
        help: "up one line",
    },
    Binding {
        keys: &[plain(KeyCode::Down), ch('j')],
        scope: Scope::Bytes,
        action: Action::HexDown,
        help: "down one line",
    },
    Binding {
        keys: &[plain(KeyCode::PageUp)],
        scope: Scope::Bytes,
        action: Action::HexPageUp,
        help: "up a page",
    },
    Binding {
        keys: &[plain(KeyCode::PageDown)],
        scope: Scope::Bytes,
        action: Action::HexPageDown,
        help: "down a page",
    },
    Binding {
        keys: &[plain(KeyCode::Home)],
        scope: Scope::Bytes,
        action: Action::HexLineStart,
        help: "the start of the line",
    },
    Binding {
        keys: &[plain(KeyCode::End)],
        scope: Scope::Bytes,
        action: Action::HexLineEnd,
        help: "the end of the line",
    },
    Binding {
        keys: &[ch('b')],
        scope: Scope::Bytes,
        action: Action::HexBits,
        help: "move by bits and not bytes (again: back)",
    },
    Binding {
        keys: &[ch('v')],
        scope: Scope::Bytes,
        action: Action::HexSelect,
        help: "select bytes: move to extend, v or q to end",
    },
    Binding {
        keys: &[ch('y')],
        scope: Scope::Bytes,
        action: Action::HexCopy,
        help: "copy the selected bytes as hex",
    },
    Binding {
        keys: &[ch('a')],
        scope: Scope::Bytes,
        action: Action::HexAddField,
        help: "with --build: add a field where the schema ends",
    },
    Binding {
        keys: &[ch('A')],
        scope: Scope::Bytes,
        action: Action::HexAddFieldHere,
        help: "with --build: add a field at the cursor",
    },
    Binding {
        keys: &[ch('.')],
        scope: Scope::Bytes,
        action: Action::HexNextField,
        help: "the next field that holds this byte",
    },
    Binding {
        keys: &[ch(',')],
        scope: Scope::Bytes,
        action: Action::HexPrevField,
        help: "the previous field that holds this byte",
    },
    Binding {
        keys: &[ctrl(KeyCode::Home)],
        scope: Scope::Bytes,
        action: Action::HexStart,
        help: "the first byte of the file",
    },
    Binding {
        keys: &[ctrl(KeyCode::End), ch('G')],
        scope: Scope::Bytes,
        action: Action::HexEnd,
        help: "the last byte of the file",
    },
];

fn matches(key: Key, event: &KeyEvent) -> bool {
    key.code == event.code
        && key.ctrl == event.modifiers.contains(KeyModifiers::CONTROL)
        && key.alt == event.modifiers.contains(KeyModifiers::ALT)
}

/// What `event` does when `pane` has the keys (`pane` is `Tree` or `Bytes`).
#[must_use]
pub fn lookup(pane: Scope, event: &KeyEvent) -> Option<Action> {
    let find = |scope: Scope| {
        BINDINGS
            .iter()
            .filter(|b| b.scope == scope)
            .find(|b| b.keys.iter().any(|k| matches(*k, event)))
            .map(|b| b.action)
    };
    find(pane).or_else(|| find(Scope::Anywhere))
}

/// A key as the help writes it.
#[must_use]
pub fn label(key: Key) -> String {
    let name = match key.code {
        KeyCode::Char(' ') => "Space".to_owned(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Esc => "Esc".to_owned(),
        KeyCode::Tab => "Tab".to_owned(),
        KeyCode::BackTab => "Shift-Tab".to_owned(),
        KeyCode::Enter => "Enter".to_owned(),
        KeyCode::Up => "↑".to_owned(),
        KeyCode::Down => "↓".to_owned(),
        KeyCode::Left => "←".to_owned(),
        KeyCode::Right => "→".to_owned(),
        KeyCode::PageUp => "PgUp".to_owned(),
        KeyCode::PageDown => "PgDn".to_owned(),
        KeyCode::Home => "Home".to_owned(),
        KeyCode::End => "End".to_owned(),
        other => format!("{other:?}"),
    };
    if key.alt {
        format!("Alt-{name}")
    } else if key.ctrl {
        let name = if name.len() == 1 {
            name.to_uppercase()
        } else {
            name
        };
        format!("Ctrl-{name}")
    } else {
        name
    }
}

/// The help: `(heading, [(keys, what it does)])` for each scope.
#[must_use]
pub fn help() -> Vec<(&'static str, Vec<(String, &'static str)>)> {
    [Scope::Anywhere, Scope::Tree, Scope::Bytes]
        .into_iter()
        .map(|scope| {
            let lines = BINDINGS
                .iter()
                .filter(|b| b.scope == scope)
                .map(|b| {
                    (
                        b.keys
                            .iter()
                            .map(|k| label(*k))
                            .collect::<Vec<_>>()
                            .join(" "),
                        b.help,
                    )
                })
                .collect();
            (scope.title(), lines)
        })
        .collect()
}

/// The prompt's own keys, also in the help: they are handled by the prompt, not the table.
pub const PROMPT_HELP: &[(&str, &str)] = &[
    ("Enter", "run the search or the jump"),
    ("Esc", "cancel"),
    (
        "← → Home End",
        "move in the text (also Ctrl-A, Ctrl-E, Ctrl-B, Ctrl-F)",
    ),
    (
        "Backspace Delete",
        "delete (Ctrl-W a word, Ctrl-U to the start, Ctrl-K to the end)",
    ),
    ("Ctrl-← Ctrl-→", "move by a word"),
    ("paste", "inserts the text (one line)"),
];

// ── The footer: the keys that matter now ───────────────────────────────────────────────────────────────────────

/// Where the viewer is, as far as the keys that matter are concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Situation {
    /// Nothing can be done but leave and read the help: the file is being parsed, or nothing could be parsed.
    Waiting,
    /// A line is being typed (a search, a jump).
    Prompt,
    /// A list, a dialog, the preview of a file of an archive or the help has the screen (most of them cover the footer,
    /// and have their own keys on their border).
    Dialog,
    /// The tree of fields or the bytes have the keys.
    Panes,
}

/// What the footer depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "the switches that decide which keys matter, one field each"
)]
pub struct FooterContext {
    /// Where the viewer is.
    pub situation: Situation,
    /// The pane that has the keys.
    pub pane: Scope,
    /// Whether the files of an archive are shown in the place of the tree of fields.
    pub files_shown: bool,
    /// Whether the file is an archive (so that `F` has something to switch to).
    pub archive: bool,
    /// The label of the algorithm of the checksum column, when it is shown.
    pub checksums: Option<&'static str>,
    /// Whether a schema is being built.
    pub building: bool,
    /// Whether the parse failed and a change was found that helps.
    pub fix: bool,
    /// Whether a pane fills the screen.
    pub fullscreen: bool,
    /// Whether a second file is being compared with the first.
    pub comparing: bool,
}

/// A part of the footer: the keys, what they do, and the action that a click on it presses (none for the keys of a
/// preview or a prompt, which are not in the table of bindings).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FooterPart {
    /// The keys, as the help writes them (`↑↓`, `Tab`, `/`).
    pub keys: String,
    /// What they do, in a word or two.
    pub label: String,
    /// What a click presses.
    pub action: Option<Action>,
}

impl FooterPart {
    /// The width of the part as it is drawn: `keys label`.
    #[must_use]
    pub fn width(&self) -> usize {
        crate::format::width(&self.keys) + 1 + crate::format::width(&self.label)
    }
}

/// A part of a context: the actions whose first keys are shown together, and what they do.
type Entry = (&'static [Action], &'static str);

const TREE_PARTS: &[Entry] = &[
    (&[Action::TreeUp, Action::TreeDown], "move"),
    (&[Action::TreeRight], "open"),
    (&[Action::Search], "search"),
    (&[Action::CopyAs], "copy as"),
    (&[Action::Goto], "goto"),
    (&[Action::FollowPointer], "follow"),
    (&[Action::ToggleFocus], "bytes"),
];

const FILES_PARTS: &[Entry] = &[
    (&[Action::TreeUp, Action::TreeDown], "move"),
    (&[Action::TreeRight], "open"),
    (&[Action::TreeToggle], "preview"),
    (&[Action::CopyChecksum], "copy sum"),
    (&[Action::Search], "search"),
    (&[Action::ToggleFocus], "bytes"),
];

const BYTES_PARTS: &[Entry] = &[
    (
        &[
            Action::HexLeft,
            Action::HexRight,
            Action::HexUp,
            Action::HexDown,
        ],
        "move",
    ),
    (&[Action::HexSelect], "select"),
    (&[Action::HexCopy], "copy"),
    (&[Action::Goto], "goto"),
    (&[Action::HexBits], "bits"),
    (&[Action::ToggleFocus], "tree"),
];

/// The first key of `action` in `pane`, or in both, as the help writes it.
#[must_use]
pub fn first_key(pane: Scope, action: Action) -> Option<String> {
    [pane, Scope::Anywhere].into_iter().find_map(|scope| {
        BINDINGS
            .iter()
            .find(|b| b.scope == scope && b.action == action)
            .and_then(|b| b.keys.first())
            .map(|k| label(*k))
    })
}

fn bound(pane: Scope, entry: &Entry) -> Option<FooterPart> {
    let (actions, text) = entry;
    let keys: Option<String> = actions.iter().map(|a| first_key(pane, *a)).collect();
    Some(FooterPart {
        keys: keys?,
        label: (*text).to_owned(),
        action: actions.first().copied(),
    })
}

fn literal(keys: &str, label: &str) -> FooterPart {
    FooterPart {
        keys: keys.to_owned(),
        label: label.to_owned(),
        action: None,
    }
}

/// The footer for a context: the few keys that matter, most useful first, the last one always the help. Each key is the
/// first one of its action in [`BINDINGS`], so the footer cannot say a key that is not bound.
#[must_use]
pub fn footer(ctx: &FooterContext) -> Vec<FooterPart> {
    let help = || bound(ctx.pane, &(&[Action::Help], "all keys"));
    let quit = || bound(ctx.pane, &(&[Action::Quit], "quit"));
    let mut parts: Vec<FooterPart> = match ctx.situation {
        Situation::Waiting => Vec::new(),
        Situation::Prompt => {
            return vec![literal("Enter", "accept"), literal("Esc", "cancel")];
        }
        Situation::Dialog => {
            return vec![literal("Esc", "close")];
        }
        Situation::Panes => {
            let mut parts = Vec::new();
            if ctx.fullscreen {
                parts.extend(bound(ctx.pane, &(&[Action::Fullscreen], "restore")));
            }
            if ctx.fix {
                parts.extend(bound(ctx.pane, &(&[Action::TryFix], "try the fix")));
            }
            if ctx.comparing {
                parts.extend(bound(ctx.pane, &(&[Action::NextDifference], "next diff")));
                parts.extend(bound(ctx.pane, &(&[Action::OtherFile], "other file")));
            }
            if ctx.building {
                let add: &'static [Action] = if ctx.pane == Scope::Bytes {
                    &[Action::HexAddField]
                } else {
                    &[Action::TreeAddField]
                };
                for entry in [
                    (add, "add field"),
                    (&[Action::Undo][..], "undo"),
                    (&[Action::Save][..], "save"),
                ] {
                    parts.extend(bound(ctx.pane, &entry));
                }
            }
            let list = match (ctx.pane, ctx.files_shown) {
                (Scope::Bytes, _) => BYTES_PARTS,
                (_, true) => FILES_PARTS,
                (_, false) => TREE_PARTS,
            };
            parts.extend(list.iter().filter_map(|e| bound(ctx.pane, e)));
            // the other view of an archive
            if ctx.archive {
                let label = if ctx.files_shown { "fields" } else { "files" };
                parts.extend(bound(ctx.pane, &(&[Action::Files], label)));
            }
            // the checksum column is named, and `m` goes through the algorithms
            if ctx.files_shown && ctx.pane != Scope::Bytes {
                let text = ctx.checksums.unwrap_or("checksums");
                parts.extend(bound(ctx.pane, &(&[Action::Checksums], text)));
            }
            parts
        }
    };
    parts.extend(quit());
    parts.extend(help());
    parts
}

/// The parts that fit in `width` cells, as the footer is drawn (a space before, ` · ` between): parts are dropped from
/// the end, but the last two (leaving, and the help) stay.
#[must_use]
pub fn fit(mut parts: Vec<FooterPart>, width: usize) -> Vec<FooterPart> {
    let total = |parts: &[FooterPart]| {
        1 + parts.iter().map(FooterPart::width).sum::<usize>()
            + FOOTER_SEPARATOR.chars().count() * parts.len().saturating_sub(1)
    };
    while parts.len() > 2 && total(&parts) > width {
        let at = parts.len() - 3;
        parts.remove(at);
    }
    parts
}

/// What separates the parts of the footer.
pub const FOOTER_SEPARATOR: &str = " · ";

// ── The which-key pop-up ────────────────────────────────────────────────────────────────────────────────────────

/// A key that takes another key, and what the other key can be.
#[derive(Debug, Clone, Copy)]
pub struct Chord {
    /// The action of the first key.
    pub prefix: Action,
    /// The second keys and what each does, in the order of the box.
    pub choices: &'static [(char, &'static str)],
}

/// The keys that take another key.
pub const CHORDS: &[Chord] = &[Chord {
    prefix: Action::CopyAs,
    choices: &[
        ('p', "path"),
        ('v', "value"),
        ('j', "JSON"),
        ('x', "hex"),
        ('X', "hex, no spaces"),
        ('c', "C array"),
        ('o', "offset+length"),
        ('s', "schema field"),
    ],
}];

/// The chord that `prefix` starts, if it starts one.
#[must_use]
pub fn chord(prefix: Action) -> Option<&'static Chord> {
    CHORDS.iter().find(|c| c.prefix == prefix)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use std::fmt::Write as _;

    use super::*;

    fn event(code: KeyCode, ctrl: bool) -> KeyEvent {
        KeyEvent::new(
            code,
            if ctrl {
                KeyModifiers::CONTROL
            } else {
                KeyModifiers::NONE
            },
        )
    }

    /// Every key that can be typed, with and without Ctrl.
    fn all_keys() -> Vec<KeyEvent> {
        let mut keys = Vec::new();
        let mut codes: Vec<KeyCode> = (0x20_u8..0x7f)
            .map(|b| KeyCode::Char(char::from(b)))
            .collect();
        codes.extend([
            KeyCode::Esc,
            KeyCode::Tab,
            KeyCode::BackTab,
            KeyCode::Enter,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::Home,
            KeyCode::End,
            KeyCode::Backspace,
            KeyCode::Delete,
            KeyCode::Insert,
            KeyCode::F(1),
        ]);
        for code in codes {
            keys.push(event(code, false));
            keys.push(event(code, true));
        }
        keys
    }

    fn help_text() -> String {
        let mut text = String::new();
        for (title, lines) in help() {
            text.push_str(title);
            for (keys, what) in lines {
                let _ = write!(text, "\n{keys}  {what}"); // writing to a String cannot fail
            }
        }
        for (keys, what) in PROMPT_HELP {
            let _ = write!(text, "\n{keys}  {what}"); // writing to a String cannot fail
        }
        text
    }

    /// The test that the help is complete: every key that does something in a
    /// pane is written in the help, under that pane's heading or "Anywhere".
    #[test]
    fn every_bound_key_is_in_the_help() {
        let help = help();
        for pane in [Scope::Tree, Scope::Bytes] {
            for event in all_keys() {
                let Some(action) = lookup(pane, &event) else {
                    continue;
                };
                let key = Key {
                    code: event.code,
                    ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
                    alt: event.modifiers.contains(KeyModifiers::ALT),
                };
                let name = label(key);
                let listed = help
                    .iter()
                    .filter(|(title, _)| {
                        *title == pane.title() || *title == Scope::Anywhere.title()
                    })
                    .flat_map(|(_, lines)| lines.iter())
                    .any(|(keys, _)| keys.split(' ').any(|k| k == name));
                assert!(
                    listed,
                    "{name} does {action:?} in {pane:?} but is not in the help"
                );
            }
        }
    }

    #[test]
    fn every_action_has_a_key_and_every_binding_a_line() {
        for b in BINDINGS {
            assert!(!b.keys.is_empty(), "{:?}", b.action);
            assert!(!b.help.is_empty(), "{:?}", b.action);
        }
        // no two bindings of the same scope share a key
        for (i, a) in BINDINGS.iter().enumerate() {
            for b in &BINDINGS[i + 1..] {
                if a.scope != b.scope {
                    continue;
                }
                for key in a.keys {
                    assert!(
                        !b.keys.contains(key),
                        "{} is bound twice in {:?}",
                        label(*key),
                        a.scope
                    );
                }
            }
        }
        // the actions all appear
        let text = help_text();
        assert!(
            text.contains("go to an offset")
                && text.contains("search")
                && text.contains("next field")
        );
    }

    #[test]
    fn the_pane_decides_and_ctrl_matters() {
        assert_eq!(
            lookup(Scope::Tree, &event(KeyCode::Home, false)),
            Some(Action::TreeHome)
        );
        assert_eq!(
            lookup(Scope::Bytes, &event(KeyCode::Home, false)),
            Some(Action::HexLineStart)
        );
        assert_eq!(
            lookup(Scope::Bytes, &event(KeyCode::Home, true)),
            Some(Action::HexStart)
        );
        assert_eq!(lookup(Scope::Tree, &event(KeyCode::Home, true)), None);
        assert_eq!(
            lookup(Scope::Tree, &event(KeyCode::Char('c'), true)),
            Some(Action::Quit)
        );
        assert_eq!(lookup(Scope::Tree, &event(KeyCode::Char('Q'), false)), None);
        assert_eq!(
            lookup(Scope::Bytes, &event(KeyCode::Char('g'), false)),
            Some(Action::Goto)
        );
        assert_eq!(
            lookup(Scope::Tree, &event(KeyCode::Char('G'), false)),
            Some(Action::TreeEnd)
        );
        assert_eq!(
            lookup(Scope::Bytes, &event(KeyCode::Char('G'), false)),
            Some(Action::HexEnd)
        );
        assert_eq!(
            lookup(Scope::Tree, &event(KeyCode::Char('?'), false)),
            Some(Action::Help)
        );
        // shift does not change a key
        let shifted = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
        assert_eq!(lookup(Scope::Tree, &shifted), Some(Action::TreeEnd));
    }

    /// Every context that the footer is made for.
    fn contexts() -> Vec<FooterContext> {
        let mut all = Vec::new();
        for pane in [Scope::Tree, Scope::Bytes] {
            for files_shown in [false, true] {
                for archive in [false, true] {
                    for building in [false, true] {
                        for fix in [false, true] {
                            for checksums in [None, Some("SHA-256")] {
                                all.push(FooterContext {
                                    situation: Situation::Panes,
                                    pane,
                                    files_shown: files_shown && archive,
                                    archive,
                                    checksums,
                                    building,
                                    fix,
                                    fullscreen: !fix && building,
                                    comparing: fix || checksums.is_some(),
                                });
                            }
                        }
                    }
                }
            }
        }
        for situation in [Situation::Waiting, Situation::Prompt, Situation::Dialog] {
            all.push(FooterContext {
                situation,
                pane: Scope::Tree,
                files_shown: false,
                archive: false,
                checksums: None,
                building: false,
                fix: false,
                fullscreen: false,
                comparing: false,
            });
        }
        all
    }

    #[test]
    fn every_part_of_a_footer_is_bound_in_the_table_of_keys() {
        for ctx in contexts() {
            let parts = footer(&ctx);
            for part in &parts {
                match part.action {
                    Some(action) => {
                        // the keys that are shown are the first keys of the actions that the context reads
                        let first = first_key(ctx.pane, action).unwrap_or_else(|| {
                            panic!("{action:?} is not bound for {:?}: {ctx:?}", ctx.pane)
                        });
                        assert!(part.keys.starts_with(&first), "{part:?} in {ctx:?}");
                    }
                    None => assert!(
                        matches!(ctx.situation, Situation::Prompt | Situation::Dialog),
                        "only the keys of a prompt or a dialog are not in the table: {part:?}"
                    ),
                }
            }
            // the lists lose no part for want of a binding
            if ctx.situation == Situation::Panes {
                let list = match (ctx.pane, ctx.files_shown) {
                    (Scope::Bytes, _) => BYTES_PARTS,
                    (_, true) => FILES_PARTS,
                    (_, false) => TREE_PARTS,
                };
                for entry in list {
                    assert!(bound(ctx.pane, entry).is_some(), "{entry:?} in {ctx:?}");
                }
            }
        }
    }

    #[test]
    fn a_footer_is_cut_to_the_width_and_keeps_the_help() {
        for ctx in contexts() {
            for width in [80, 60, 40, 24] {
                let parts = fit(footer(&ctx), width);
                let text: usize = 1
                    + parts.iter().map(FooterPart::width).sum::<usize>()
                    + FOOTER_SEPARATOR.chars().count() * (parts.len() - 1);
                if ctx.situation == Situation::Panes {
                    assert_eq!(
                        parts.last().map(|p| p.label.as_str()),
                        Some("all keys"),
                        "{ctx:?}"
                    );
                    // two parts are the least: the one before the help, and the help
                    assert!(
                        text <= width || parts.len() == 2,
                        "{text} > {width}: {parts:?}"
                    );
                }
            }
        }
        // the first parts are the ones that stay
        let ctx = contexts()
            .into_iter()
            .find(|c| {
                c.situation == Situation::Panes
                    && c.pane == Scope::Tree
                    && !c.building
                    && !c.fix
                    && !c.archive
            })
            .unwrap();
        let narrow = fit(footer(&ctx), 30);
        assert_eq!(narrow.first().map(|p| p.label.as_str()), Some("move"));
    }

    #[test]
    fn the_footer_of_each_context_is_what_the_issue_says() {
        let text = |ctx: &FooterContext| {
            fit(footer(ctx), 200)
                .iter()
                .map(|p| format!("{} {}", p.keys, p.label))
                .collect::<Vec<_>>()
                .join(" · ")
        };
        let mut ctx = FooterContext {
            situation: Situation::Panes,
            pane: Scope::Tree,
            files_shown: false,
            archive: false,
            checksums: None,
            building: false,
            fix: false,
            fullscreen: false,
            comparing: false,
        };
        assert_eq!(
            text(&ctx),
            "↑↓ move · → open · / search · Y copy as · g goto · J follow · Tab bytes · q quit · ? all keys"
        );
        ctx.pane = Scope::Bytes;
        assert_eq!(
            text(&ctx),
            "←→↑↓ move · v select · y copy · g goto · b bits · Tab tree · q quit · ? all keys"
        );
        ctx.pane = Scope::Tree;
        ctx.archive = true;
        ctx.files_shown = true;
        ctx.checksums = Some("MD5");
        assert_eq!(
            text(&ctx),
            "↑↓ move · → open · Enter preview · y copy sum · / search · Tab bytes · F fields · m MD5 · q quit · ? all keys"
        );
        ctx.archive = false;
        ctx.files_shown = false;
        ctx.building = true;
        ctx.fix = true;
        assert!(text(&ctx).starts_with("o try the fix · a add field · u undo · w save · ↑↓ move"));
        ctx.fix = false;
        ctx.building = false;
        ctx.comparing = true;
        assert!(
            text(&ctx).starts_with("> next diff · V other file · ↑↓ move"),
            "{}",
            text(&ctx)
        );
        ctx.comparing = false;
        ctx.fullscreen = true;
        assert!(
            text(&ctx).starts_with("f restore · ↑↓ move"),
            "{}",
            text(&ctx)
        );
    }

    #[test]
    fn the_chords_have_a_letter_for_each_choice_and_no_letter_twice() {
        for chord in CHORDS {
            let mut seen = std::collections::HashSet::new();
            for (letter, label) in chord.choices {
                assert!(seen.insert(*letter), "{letter} twice in {:?}", chord.prefix);
                assert_ne!(*label, "");
            }
            assert!(
                BINDINGS.iter().any(|b| b.action == chord.prefix),
                "the first key of {:?} is not bound",
                chord.prefix
            );
        }
        assert_eq!(chord(Action::CopyAs).map(|c| c.choices.len()), Some(8));
        assert!(chord(Action::Goto).is_none());
    }
}
