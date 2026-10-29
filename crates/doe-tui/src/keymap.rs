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
    /// Show the help.
    Help,
    /// Show or hide the message about a parse that failed.
    ToggleReport,
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
    /// List the schemas and their types.
    Schemas,
    /// Browse the types that are available on the type paths.
    Types,
    /// Write all integers in the next radix.
    Radix,
    /// Choose a radix for the selected integer field.
    FieldRadix,
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
}

const fn plain(code: KeyCode) -> Key {
    Key { code, ctrl: false }
}

const fn ctrl(code: KeyCode) -> Key {
    Key { code, ctrl: true }
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
        keys: &[ch('e')],
        scope: Scope::Anywhere,
        action: Action::ToggleReport,
        help: "show or hide the message about a parse that failed",
    },
    Binding {
        keys: &[ch('r')],
        scope: Scope::Anywhere,
        action: Action::Reload,
        help: "read the schema and the file again, and keep the place",
    },
    Binding {
        keys: &[ch('?')],
        scope: Scope::Anywhere,
        action: Action::Help,
        help: "show this help",
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
        keys: &[plain(KeyCode::Left), ch('h')],
        scope: Scope::Tree,
        action: Action::TreeLeft,
        help: "close a node, or go to its parent",
    },
    Binding {
        keys: &[plain(KeyCode::Enter), ch(' ')],
        scope: Scope::Tree,
        action: Action::TreeToggle,
        help: "open or close a node",
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
    key.code == event.code && key.ctrl == event.modifiers.contains(KeyModifiers::CONTROL)
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
    if key.ctrl {
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
        assert_eq!(lookup(Scope::Tree, &event(KeyCode::Char('c'), false)), None);
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
}
