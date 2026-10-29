//! Every key of the viewer is in the README: the help screen is made from the
//! same table as the key handling (`keymap::BINDINGS`), so this checks the
//! README against both.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use crossterm::event::KeyCode;
use doe_tui::keymap::{BINDINGS, Key};

const README: &str = include_str!("../../../README.md");

/// The parts of the README about the viewer: its section, and the one about building a schema in it.
fn viewer_section() -> String {
    let part = |heading: &str| {
        let (_, from_heading) = README.split_once(heading).unwrap();
        from_heading
            .split_once("\n### ")
            .map_or(from_heading, |(section, _)| section)
            .to_owned()
    };
    part("### Terminal viewer") + &part("### Building a schema")
}

/// How the README writes a key: a character in backticks, a named key as its name.
fn written(key: Key) -> Vec<String> {
    let name = match key.code {
        KeyCode::Char(c) if key.ctrl => return vec![format!("Ctrl-{}", c.to_ascii_uppercase())],
        KeyCode::Char(' ') => return vec!["`Space`".to_owned()],
        KeyCode::Char(c) => return vec![format!("`{c}`")],
        KeyCode::Esc => "Esc",
        KeyCode::Tab | KeyCode::BackTab => "Tab",
        KeyCode::Enter => "Enter",
        KeyCode::Up => "↑",
        KeyCode::Down => "↓",
        KeyCode::Left => "←",
        KeyCode::Right => "→",
        KeyCode::PageUp => "PgUp",
        KeyCode::PageDown => "PgDn",
        KeyCode::Home => "Home",
        KeyCode::End => "End",
        KeyCode::Backspace => "Backspace",
        KeyCode::Delete => "Delete",
        other => panic!("{other:?} has no name in the test"),
    };
    if key.ctrl {
        vec![format!("Ctrl-{name}")]
    } else {
        vec![name.to_owned()]
    }
}

#[test]
fn the_readme_documents_every_key_of_the_help_screen() {
    let section = viewer_section();
    let mut missing = Vec::new();
    for binding in BINDINGS {
        for &key in binding.keys {
            for text in written(key) {
                if !section.contains(&text) {
                    missing.push(format!("{text} ({})", binding.help));
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "keys that are in the help and not in the README's terminal viewer section:\n{}",
        missing.join("\n")
    );
}

#[test]
fn the_readme_documents_the_keys_of_the_prompt() {
    let section = viewer_section();
    for key in [
        "Backspace",
        "Delete",
        "Ctrl-W",
        "Ctrl-U",
        "Ctrl-K",
        "Enter",
        "Esc",
    ] {
        assert!(section.contains(key), "{key}");
    }
}
