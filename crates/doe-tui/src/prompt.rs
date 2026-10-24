//! The line that is typed at the bottom of the screen (a search, an offset): a
//! text with a cursor, edited the way a shell line is.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::format;

/// The longest text a prompt keeps, in characters.
const MAX_CHARS: usize = 4096;

/// What a prompt is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A search for text in the fields.
    Search,
    /// An offset to go to.
    Goto,
}

impl Kind {
    /// The text before what is typed.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Kind::Search => "/",
            Kind::Goto => "goto offset: ",
        }
    }
}

/// What a key did to the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// It is still being typed.
    Editing,
    /// Enter: the text is done.
    Submit,
    /// Esc: forget it.
    Cancel,
}

/// A line of text and the place of the cursor (in characters, not bytes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineEditor {
    text: String,
    /// The cursor, from 0 to the number of characters.
    cursor: usize,
}

impl LineEditor {
    /// An empty line.
    #[must_use]
    pub fn new() -> Self {
        LineEditor::default()
    }

    /// A line with `text` in it and the cursor after it.
    #[must_use]
    pub fn with_text(text: &str) -> Self {
        let mut line = LineEditor::new();
        line.insert_str(text);
        line
    }

    /// The text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Where the cursor is, in characters.
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Where the cursor is, in cells from the start of the text.
    #[must_use]
    pub fn cursor_cells(&self) -> usize {
        let before: String = self.text.chars().take(self.cursor).collect();
        format::width(&before)
    }

    fn chars(&self) -> usize {
        self.text.chars().count()
    }

    fn byte_of(&self, chars: usize) -> usize {
        self.text
            .char_indices()
            .nth(chars)
            .map_or(self.text.len(), |(i, _)| i)
    }

    /// Put a character at the cursor. Control characters are not text.
    pub fn insert_char(&mut self, c: char) {
        if c.is_control() || self.chars() >= MAX_CHARS {
            return;
        }
        let at = self.byte_of(self.cursor);
        self.text.insert(at, c);
        self.cursor += 1;
    }

    /// Put text at the cursor, as when it is pasted: a new line ends it (the
    /// prompt is one line) and other control characters are left out.
    pub fn insert_str(&mut self, text: &str) {
        for c in text.chars() {
            if c == '\n' || c == '\r' {
                break;
            }
            if c == '\t' {
                self.insert_char(' ');
            } else {
                self.insert_char(c);
            }
        }
    }

    /// Delete the character before the cursor.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = self.byte_of(self.cursor - 1);
        let end = self.byte_of(self.cursor);
        self.text.replace_range(start..end, "");
        self.cursor -= 1;
    }

    /// Delete the character at the cursor.
    pub fn delete(&mut self) {
        if self.cursor >= self.chars() {
            return;
        }
        let start = self.byte_of(self.cursor);
        let end = self.byte_of(self.cursor + 1);
        self.text.replace_range(start..end, "");
    }

    /// Move the cursor one character back.
    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Move the cursor one character on.
    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.chars());
    }

    /// Move the cursor to the start of the line.
    pub fn home(&mut self) {
        self.cursor = 0;
    }

    /// Move the cursor to the end of the line.
    pub fn end(&mut self) {
        self.cursor = self.chars();
    }

    /// The character index of the start of the word before the cursor.
    fn word_start(&self) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let mut at = self.cursor;
        while at > 0 && chars.get(at - 1).is_some_and(|c| c.is_whitespace()) {
            at -= 1;
        }
        while at > 0 && chars.get(at - 1).is_some_and(|c| !c.is_whitespace()) {
            at -= 1;
        }
        at
    }

    /// The character index of the end of the word after the cursor.
    fn word_end(&self) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let mut at = self.cursor;
        while chars.get(at).is_some_and(|c| c.is_whitespace()) {
            at += 1;
        }
        while chars.get(at).is_some_and(|c| !c.is_whitespace()) {
            at += 1;
        }
        at
    }

    /// Move the cursor to the start of the word before it.
    pub fn word_left(&mut self) {
        self.cursor = self.word_start();
    }

    /// Move the cursor to the end of the word after it.
    pub fn word_right(&mut self) {
        self.cursor = self.word_end();
    }

    /// Delete the word before the cursor.
    pub fn delete_word(&mut self) {
        let start = self.word_start();
        let (a, b) = (self.byte_of(start), self.byte_of(self.cursor));
        self.text.replace_range(a..b, "");
        self.cursor = start;
    }

    /// Delete from the start of the line to the cursor.
    pub fn delete_to_start(&mut self) {
        let end = self.byte_of(self.cursor);
        self.text.replace_range(..end, "");
        self.cursor = 0;
    }

    /// Delete from the cursor to the end of the line.
    pub fn delete_to_end(&mut self) {
        let start = self.byte_of(self.cursor);
        self.text.truncate(start);
    }

    /// Handle a key.
    pub fn key(&mut self, event: &KeyEvent) -> Outcome {
        let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
        match event.code {
            KeyCode::Enter => return Outcome::Submit,
            KeyCode::Esc => return Outcome::Cancel,
            KeyCode::Char('c' | 'g') if ctrl => return Outcome::Cancel,
            KeyCode::Char('a') if ctrl => self.home(),
            KeyCode::Char('e') if ctrl => self.end(),
            KeyCode::Char('b') if ctrl => self.left(),
            KeyCode::Char('f') if ctrl => self.right(),
            KeyCode::Char('w') if ctrl => self.delete_word(),
            KeyCode::Char('u') if ctrl => self.delete_to_start(),
            KeyCode::Char('k') if ctrl => self.delete_to_end(),
            KeyCode::Char('d') if ctrl => self.delete(),
            KeyCode::Left if ctrl => self.word_left(),
            KeyCode::Right if ctrl => self.word_right(),
            KeyCode::Left => self.left(),
            KeyCode::Right => self.right(),
            KeyCode::Home => self.home(),
            KeyCode::End => self.end(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Char(c) if !ctrl => self.insert_char(c),
            _ => {}
        }
        Outcome::Editing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(line: &mut LineEditor, text: &str) {
        for c in text.chars() {
            line.key(&KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
    }

    fn press(line: &mut LineEditor, code: KeyCode) -> Outcome {
        line.key(&KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn ctrl(line: &mut LineEditor, c: char) -> Outcome {
        line.key(&KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
    }

    #[test]
    fn typing_and_moving_and_deleting() {
        let mut line = LineEditor::new();
        typed(&mut line, "hello");
        assert_eq!((line.text(), line.cursor()), ("hello", 5));
        press(&mut line, KeyCode::Left);
        press(&mut line, KeyCode::Left);
        typed(&mut line, "X");
        assert_eq!((line.text(), line.cursor()), ("helXlo", 4));
        press(&mut line, KeyCode::Backspace);
        assert_eq!((line.text(), line.cursor()), ("hello", 3));
        press(&mut line, KeyCode::Delete);
        assert_eq!((line.text(), line.cursor()), ("helo", 3));
        press(&mut line, KeyCode::Home);
        press(&mut line, KeyCode::Backspace); // nothing before the cursor
        assert_eq!((line.text(), line.cursor()), ("helo", 0));
        press(&mut line, KeyCode::End);
        press(&mut line, KeyCode::Delete); // nothing after
        press(&mut line, KeyCode::Right);
        assert_eq!((line.text(), line.cursor()), ("helo", 4));
        press(&mut line, KeyCode::Left);
        assert_eq!(line.cursor(), 3);
        // empty line: nothing panics
        let mut empty = LineEditor::new();
        for code in [
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Backspace,
            KeyCode::Delete,
            KeyCode::Home,
            KeyCode::End,
        ] {
            press(&mut empty, code);
        }
        assert_eq!((empty.text(), empty.cursor()), ("", 0));
    }

    #[test]
    fn emacs_keys_and_words() {
        let mut line = LineEditor::with_text("one two  three");
        assert_eq!(line.cursor(), 14);
        ctrl(&mut line, 'w'); // "three"
        assert_eq!(line.text(), "one two  ");
        ctrl(&mut line, 'w'); // "two  "
        assert_eq!(line.text(), "one ");
        ctrl(&mut line, 'a');
        assert_eq!(line.cursor(), 0);
        line.key(&KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL));
        assert_eq!(line.cursor(), 3);
        line.key(&KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL));
        assert_eq!(line.cursor(), 4);
        line.key(&KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL));
        assert_eq!(line.cursor(), 0);
        let mut line = LineEditor::with_text("abcdef");
        press(&mut line, KeyCode::Left);
        press(&mut line, KeyCode::Left);
        ctrl(&mut line, 'k');
        assert_eq!(line.text(), "abcd");
        ctrl(&mut line, 'u');
        assert_eq!((line.text(), line.cursor()), ("", 0));
        let mut line = LineEditor::with_text("xyz");
        ctrl(&mut line, 'a');
        ctrl(&mut line, 'd');
        ctrl(&mut line, 'f');
        ctrl(&mut line, 'b');
        ctrl(&mut line, 'e');
        assert_eq!((line.text(), line.cursor()), ("yz", 2));
    }

    #[test]
    fn enter_submits_and_escape_cancels() {
        let mut line = LineEditor::with_text("x");
        assert_eq!(press(&mut line, KeyCode::Enter), Outcome::Submit);
        assert_eq!(press(&mut line, KeyCode::Esc), Outcome::Cancel);
        assert_eq!(ctrl(&mut line, 'c'), Outcome::Cancel);
        assert_eq!(press(&mut line, KeyCode::Tab), Outcome::Editing);
        assert_eq!(line.text(), "x");
    }

    #[test]
    fn pasting_inserts_at_the_cursor_one_line_without_control_characters() {
        let mut line = LineEditor::with_text("ab");
        press(&mut line, KeyCode::Left);
        line.insert_str("12\t3\nignored");
        assert_eq!((line.text(), line.cursor()), ("a12 3b", 5));
        let mut line = LineEditor::new();
        line.insert_str("a\x1b[2Jb\x07c");
        assert_eq!(
            line.text(),
            "a[2Jbc",
            "no escape or bell gets into the text"
        );
        line.insert_str("\r\nrest");
        assert_eq!(line.text(), "a[2Jbc");
    }

    #[test]
    fn wide_and_combining_characters_are_one_character_each() {
        let mut line = LineEditor::with_text("日本e\u{301}");
        assert_eq!(line.cursor(), 4); // 日 本 e ◌́
        assert_eq!(line.cursor_cells(), 5); // 2 + 2 + 1 + 0
        press(&mut line, KeyCode::Backspace); // the combining mark only
        assert_eq!(line.text(), "日本e");
        press(&mut line, KeyCode::Left);
        press(&mut line, KeyCode::Left);
        assert_eq!(line.cursor_cells(), 2);
        press(&mut line, KeyCode::Delete);
        assert_eq!(line.text(), "日e");
    }

    #[test]
    fn the_text_has_a_limit() {
        let mut line = LineEditor::new();
        line.insert_str(&"x".repeat(10_000));
        assert_eq!(line.text().len(), 4096);
    }
}
