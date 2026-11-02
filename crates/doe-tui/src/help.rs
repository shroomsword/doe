//! The help over the screen: scrolled with the arrow keys, searched with `/`.
//!
//! The text is made by [`crate::ui::help_lines`]; this keeps what the viewer
//! needs to know between keys (how far it is scrolled, the search) and what a
//! key does while the help is up.

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::ui::{Theme, help_lines};

/// The state of the help while it is shown.
#[derive(Debug, Default)]
pub struct HelpView {
    /// The first line shown.
    pub scroll: usize,
    /// What is searched for (any case); empty when there is no search.
    pub query: String,
    /// Whether keys are being typed into the search.
    pub typing: bool,
    /// The match that is current, as an index into [`HelpView::matches`].
    pub at: usize,
    /// How many lines the box showed last time, for the keys that move by a page.
    pub rows: Cell<u16>,
}

/// What a key did to the help.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The help stays.
    Stay,
    /// The help is closed.
    Close,
}

/// The text of every line of the help.
#[must_use]
pub fn plain_lines() -> Vec<String> {
    help_lines(Theme::new(false))
        .iter()
        .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect()
}

impl HelpView {
    /// The lines (as indexes) that have the query, in order.
    #[must_use]
    pub fn matches(&self) -> Vec<usize> {
        if self.query.is_empty() {
            return Vec::new();
        }
        let needle = self.query.to_lowercase();
        plain_lines()
            .iter()
            .enumerate()
            .filter(|(_, line)| line.to_lowercase().contains(&needle))
            .map(|(i, _)| i)
            .collect()
    }

    /// The line of the current match.
    #[must_use]
    pub fn current_line(&self) -> Option<usize> {
        let matches = self.matches();
        matches.get(self.at % matches.len().max(1)).copied()
    }

    fn max_scroll(&self) -> usize {
        plain_lines()
            .len()
            .saturating_sub(usize::from(self.rows.get()))
    }

    fn scroll_to_match(&mut self) {
        if let Some(line) = self.current_line() {
            self.scroll = line.saturating_sub(1).min(self.max_scroll());
        }
    }

    fn step(&mut self, forward: bool) {
        let count = self.matches().len();
        if count == 0 {
            return;
        }
        self.at = if forward {
            (self.at + 1) % count
        } else {
            (self.at + count - 1) % count
        };
        self.scroll_to_match();
    }

    fn scroll_by(&mut self, down: bool, by: usize) {
        self.scroll = if down {
            (self.scroll + by).min(self.max_scroll())
        } else {
            self.scroll.saturating_sub(by)
        };
    }

    /// What a key does while the help is up: type into the search, move
    /// through it, scroll, or close the help (any other key does).
    pub fn key(&mut self, key: &KeyEvent) -> Outcome {
        if self.typing {
            match key.code {
                KeyCode::Esc => {
                    self.typing = false;
                    self.query.clear();
                }
                KeyCode::Enter => self.typing = false,
                KeyCode::Backspace => {
                    self.query.pop();
                    self.at = 0;
                    self.scroll_to_match();
                }
                KeyCode::Char(c)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.query.push(c);
                    self.at = 0;
                    self.scroll_to_match();
                }
                _ => {}
            }
            return Outcome::Stay;
        }
        let page = usize::from(self.rows.get()).saturating_sub(1).max(1);
        match key.code {
            KeyCode::Char('/') => {
                self.typing = true;
                self.query.clear();
                self.at = 0;
            }
            KeyCode::Char('n') if !self.query.is_empty() => self.step(true),
            KeyCode::Char('N') if !self.query.is_empty() => self.step(false),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_by(true, 1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll_by(false, 1),
            KeyCode::PageDown | KeyCode::Char(' ') => self.scroll_by(true, page),
            KeyCode::PageUp => self.scroll_by(false, page),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = self.max_scroll(),
            KeyCode::Esc if !self.query.is_empty() => self.query.clear(),
            _ => return Outcome::Close,
        }
        Outcome::Stay
    }

    /// Back to the state of a help that was just opened.
    pub fn reset(&mut self) {
        self.scroll = 0;
        self.query.clear();
        self.typing = false;
        self.at = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(view: &mut HelpView, code: KeyCode) -> Outcome {
        view.key(&KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn type_text(view: &mut HelpView, text: &str) {
        for c in text.chars() {
            press(view, KeyCode::Char(c));
        }
    }

    #[test]
    fn slash_types_a_query_and_enter_keeps_it() {
        let mut view = HelpView::default();
        view.rows.set(10);
        assert_eq!(press(&mut view, KeyCode::Char('/')), Outcome::Stay);
        assert!(view.typing);
        type_text(&mut view, "offset");
        assert_eq!(view.query, "offset");
        let count = view.matches().len();
        assert!(count >= 1, "the help mentions an offset");
        // the view went to the first match
        assert_eq!(
            view.scroll,
            view.matches()[0].saturating_sub(1).min(view.max_scroll())
        );
        press(&mut view, KeyCode::Enter);
        assert!(!view.typing && view.query == "offset");
        // n and N go round the matches
        for _ in 0..count {
            press(&mut view, KeyCode::Char('n'));
        }
        assert_eq!(view.at, count % count);
        press(&mut view, KeyCode::Char('N'));
        assert_eq!(view.at, count - 1);
    }

    #[test]
    fn the_search_ignores_case_and_a_text_that_is_not_there_matches_nothing() {
        let mut view = HelpView::default();
        press(&mut view, KeyCode::Char('/'));
        type_text(&mut view, "GO TO AN OFFSET");
        assert_ne!(view.matches(), Vec::<usize>::new());
        press(&mut view, KeyCode::Esc);
        assert!(
            view.query.is_empty() && !view.typing,
            "Esc in the box clears it"
        );
        press(&mut view, KeyCode::Char('/'));
        type_text(&mut view, "zzzzqqqq");
        assert_eq!(view.matches(), Vec::<usize>::new());
        assert_eq!(view.current_line(), None);
    }

    #[test]
    fn esc_clears_the_query_first_and_then_closes_and_other_keys_close() {
        let mut view = HelpView::default();
        press(&mut view, KeyCode::Char('/'));
        type_text(&mut view, "tree");
        press(&mut view, KeyCode::Enter);
        assert_eq!(press(&mut view, KeyCode::Esc), Outcome::Stay);
        assert_eq!(view.query, "");
        assert_eq!(press(&mut view, KeyCode::Esc), Outcome::Close);
        // n with no search is just a key
        assert_eq!(press(&mut view, KeyCode::Char('n')), Outcome::Close);
        assert_eq!(press(&mut view, KeyCode::Char('q')), Outcome::Close);
    }

    #[test]
    fn the_arrow_keys_scroll_within_the_text() {
        let mut view = HelpView::default();
        view.rows.set(10);
        press(&mut view, KeyCode::Down);
        press(&mut view, KeyCode::Char('j'));
        assert_eq!(view.scroll, 2);
        press(&mut view, KeyCode::Up);
        assert_eq!(view.scroll, 1);
        press(&mut view, KeyCode::End);
        assert_eq!(view.scroll, plain_lines().len() - 10);
        press(&mut view, KeyCode::Down);
        assert_eq!(view.scroll, plain_lines().len() - 10, "not past the end");
        press(&mut view, KeyCode::Home);
        assert_eq!(view.scroll, 0);
    }
}
