//! The preview of a file of an archive: its text, highlighted in the language its name (or a `#!` line) tells, shown over
//! the viewer with line numbers and scrolled by the keys. It is read-only; `s` saves the whole entry to a file.
//!
//! The text is the first [`PREVIEW_BYTES`] of the entry, if it is text (valid UTF-8, no control characters other than
//! white space). A line longer than [`LINE_LIMIT`] characters is cut, a tab is four columns at most, and every line is
//! made safe to show (see [`crate::format::safe`]) before the grammar sees it, so that the marks are byte ranges of the
//! text that is drawn.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::code::{self, Marks};
use crate::{format, languages};

/// The most bytes of an entry that the preview reads: more is cut, with a mark.
pub const PREVIEW_BYTES: usize = 100_000;

/// The longest line the preview draws, in characters: a longer one is cut, with a mark.
pub const LINE_LIMIT: usize = 2000;

/// What a key did to the preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// It stays open.
    Keep,
    /// Close it.
    Close,
    /// Ask where to save the entry.
    Save,
}

/// An open preview.
#[derive(Debug, Clone)]
pub struct Preview {
    /// The path of the entry in the archive.
    pub path: String,
    /// The last part of the path, for the name of the file that is saved.
    pub name: String,
    /// The bytes of the entry in the input: first, and the byte after the last.
    pub data: (u64, u64),
    /// The language it is coloured as (an id of [`crate::languages`]), if it has one with a grammar.
    pub language: Option<&'static str>,
    /// The lines, safe to show.
    pub lines: Vec<String>,
    /// The pieces of each line that are coloured.
    pub marks: Vec<Marks>,
    /// Whether the entry is longer than what is shown.
    pub cut: bool,
    /// The first line that is shown.
    pub top: usize,
    /// How many lines the last drawing had room for (set by the drawing).
    pub rows: usize,
}

impl Preview {
    /// The preview of an entry whose first bytes are `bytes`.
    ///
    /// # Errors
    ///
    /// The words to say when the entry is not text.
    pub fn new(
        path: String,
        name: String,
        data: (u64, u64),
        bytes: &[u8],
    ) -> Result<Preview, String> {
        let total = data.1.saturating_sub(data.0);
        let cut = total > bytes.len() as u64;
        let text = text_of(bytes, cut).ok_or_else(|| format!("{total} bytes (not text)"))?;
        let language = languages::detect(&name, text);
        let mut lines: Vec<String> = text
            .split('\n')
            .map(|line| shown(line.strip_suffix('\r').unwrap_or(line)))
            .collect();
        // the text ends with a line end: that is not a line of its own
        if lines.len() > 1 && lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        let marks = language
            .and_then(|id| code::highlight(id, &lines))
            .unwrap_or_else(|| vec![Marks::new(); lines.len()]);
        let mut marks = marks;
        if cut {
            lines.push("…".to_owned());
            marks.push(Marks::new());
        }
        Ok(Preview {
            path,
            name,
            data,
            language,
            lines,
            marks,
            cut,
            top: 0,
            rows: 1,
        })
    }

    /// The name of the language, for the title: `plain` when there is none or the grammars have none for it.
    #[must_use]
    pub fn language_name(&self) -> &'static str {
        self.language
            .filter(|id| languages::grammar(id).is_some())
            .and_then(languages::name)
            .unwrap_or("plain")
    }

    /// The largest `top`: the last line at the bottom.
    fn last_top(&self) -> usize {
        self.lines.len().saturating_sub(self.rows.max(1))
    }

    /// A key while the preview is open.
    pub fn key(&mut self, key: &KeyEvent) -> Outcome {
        let page = self.rows.saturating_sub(1).max(1);
        let last = self.last_top();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Outcome::Close,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Outcome::Close;
            }
            KeyCode::Char('s') => return Outcome::Save,
            KeyCode::Up | KeyCode::Char('k') => self.top = self.top.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.top = (self.top + 1).min(last),
            KeyCode::PageUp => self.top = self.top.saturating_sub(page),
            KeyCode::PageDown | KeyCode::Char(' ') => self.top = (self.top + page).min(last),
            KeyCode::Home | KeyCode::Char('g') => self.top = 0,
            KeyCode::End | KeyCode::Char('G') => self.top = last,
            _ => {}
        }
        self.top = self.top.min(last);
        Outcome::Keep
    }
}

/// The text of `bytes`, if they are text. When the bytes are the start of a longer entry (`cut`) the cut may be inside a
/// character, which is dropped.
fn text_of(bytes: &[u8], cut: bool) -> Option<&str> {
    let drop_most = if cut { 3 } else { 0 };
    for drop in 0..=drop_most.min(bytes.len()) {
        let Some(part) = bytes.get(..bytes.len() - drop) else {
            continue;
        };
        if let Ok(text) = std::str::from_utf8(part) {
            let plain = text
                .chars()
                .all(|c| !c.is_control() || matches!(c, '\t' | '\n' | '\r'));
            return plain.then_some(text);
        }
    }
    None
}

/// A line made to be drawn: tabs to the next multiple of four columns, a long line cut, and no character that would
/// act on the terminal.
fn shown(line: &str) -> String {
    let mut out = String::new();
    let mut column = 0;
    for c in line.chars() {
        if column >= LINE_LIMIT {
            out.push('…');
            break;
        }
        if c == '\t' {
            let spaces = 4 - column % 4;
            out.extend(std::iter::repeat_n(' ', spaces));
            column += spaces;
        } else {
            out.push(c);
            column += 1;
        }
    }
    format::safe(&out)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::format_collect,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;

    fn preview(name: &str, text: &str) -> Preview {
        Preview::new(
            "dir/".to_owned() + name,
            name.to_owned(),
            (0, text.len() as u64),
            text.as_bytes(),
        )
        .expect("text")
    }

    #[test]
    fn a_text_entry_has_lines_marks_and_the_language_of_its_name() {
        let p = preview("main.rs", "fn main() {\n\tlet n = 1;\r\n}\n");
        assert_eq!(p.lines, ["fn main() {", "    let n = 1;", "}"]);
        assert_eq!(p.language, Some("rust"));
        assert_eq!(p.language_name(), "Rust");
        assert!(p.marks[0].iter().any(|(_, t)| *t == code::Token::Keyword));
        assert_eq!(p.marks.len(), p.lines.len());
        assert!(!p.cut);
    }

    #[test]
    fn a_shebang_names_the_language_and_an_unknown_file_is_plain() {
        let p = preview("run", "#!/usr/bin/env python3\nprint(1) # hi\n");
        assert_eq!(p.language_name(), "Python");
        assert!(p.marks[1].iter().any(|(_, t)| *t == code::Token::Comment));
        let plain = preview("notes.xyz", "fn main() { let x = 1; }\n");
        assert_eq!(plain.language_name(), "plain");
        assert!(plain.marks.iter().all(Vec::is_empty));
        // a language the grammars do not have is plain too, and says so
        let toml = preview("a.toml", "a = 1\n");
        assert_eq!(toml.language, Some("toml"));
        assert_eq!(toml.language_name(), "plain");
    }

    #[test]
    fn binary_is_not_previewed_and_control_characters_are_not_drawn() {
        let error = Preview::new("a".into(), "a".into(), (0, 5), &[0, 1, 2, 3, 0xff]).unwrap_err();
        assert_eq!(error, "5 bytes (not text)");
        assert!(Preview::new("a".into(), "a".into(), (0, 3), b"a\x1bb").is_err());
        // white space is text; a bidirectional control is made visible
        let p = preview("a.txt", "a\u{202e}b\n");
        assert_eq!(p.lines, ["a\\u{202e}b"]);
    }

    #[test]
    fn a_long_line_is_cut_and_an_entry_over_the_limit_ends_with_a_mark() {
        let long = format!("{}\nshort\n", "a".repeat(LINE_LIMIT + 50));
        let p = preview("x.txt", &long);
        assert_eq!(p.lines[0], format!("{}…", "a".repeat(LINE_LIMIT)));
        assert_eq!(p.lines[1], "short");
        // the entry is longer than the bytes that were read, and the cut is inside 'é'
        let bytes = "abcé".as_bytes();
        let p = Preview::new("a".into(), "a.txt".into(), (0, 100), &bytes[..4]).unwrap();
        assert_eq!(p.lines, ["abc", "…"]);
        assert!(p.cut);
        // but a whole entry that ends inside a character is not text
        assert!(Preview::new("a".into(), "a.txt".into(), (0, 4), &bytes[..4]).is_err());
    }

    #[test]
    fn the_keys_scroll_between_the_first_line_and_the_last_screen() {
        let text: String = (0..50).map(|i| format!("line {i}\n")).collect();
        let mut p = preview("a.txt", &text);
        p.rows = 10;
        let press =
            |p: &mut Preview, code: KeyCode| p.key(&KeyEvent::new(code, KeyModifiers::NONE));
        assert_eq!(press(&mut p, KeyCode::Down), Outcome::Keep);
        assert_eq!(p.top, 1);
        press(&mut p, KeyCode::PageDown);
        assert_eq!(p.top, 10);
        press(&mut p, KeyCode::End);
        assert_eq!(p.top, 40);
        press(&mut p, KeyCode::Down);
        assert_eq!(p.top, 40);
        press(&mut p, KeyCode::PageUp);
        assert_eq!(p.top, 31);
        press(&mut p, KeyCode::Home);
        assert_eq!(p.top, 0);
        press(&mut p, KeyCode::Up);
        assert_eq!(p.top, 0);
        assert_eq!(press(&mut p, KeyCode::Char('s')), Outcome::Save);
        assert_eq!(press(&mut p, KeyCode::Esc), Outcome::Close);
        assert_eq!(press(&mut p, KeyCode::Char('q')), Outcome::Close);
    }
}
