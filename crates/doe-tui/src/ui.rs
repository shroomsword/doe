//! Drawing the state of the viewer.

use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders};

use crate::app::App;
use crate::format;
use crate::tree::Shown;
use crate::tree_view;

/// The styles of the viewer. Without colour they are the plain terminal
/// colours with bold, dim and reverse only, which is what `NO_COLOR` asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    color: bool,
}

impl Theme {
    /// A theme with or without colour.
    #[must_use]
    pub fn new(color: bool) -> Self {
        Theme { color }
    }

    fn colored(self, color: Color) -> Style {
        if self.color {
            Style::default().fg(color)
        } else {
            Style::default()
        }
    }

    /// Titles and the type name.
    #[must_use]
    pub fn title(self) -> Style {
        self.colored(Color::Cyan).add_modifier(Modifier::BOLD)
    }

    /// A parse that worked.
    #[must_use]
    pub fn good(self) -> Style {
        self.colored(Color::Green)
    }

    /// A parse that failed, and the row where it failed.
    #[must_use]
    pub fn bad(self) -> Style {
        self.colored(Color::Red).add_modifier(Modifier::BOLD)
    }

    /// The mark and the name of the row that failed.
    #[must_use]
    pub fn failed(self) -> Style {
        self.bad()
    }

    /// Hints, and what is secondary.
    #[must_use]
    pub fn dim(self) -> Style {
        Style::default().add_modifier(Modifier::DIM)
    }

    /// The open/closed marker of a row.
    #[must_use]
    pub fn marker(self) -> Style {
        self.dim()
    }

    /// The name of a field.
    #[must_use]
    pub fn name(self) -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    /// The name of an instance (the row is dimmed as a whole as well).
    #[must_use]
    pub fn instance(self) -> Style {
        Style::default().add_modifier(Modifier::ITALIC)
    }

    /// The type of a field.
    #[must_use]
    pub fn type_name(self) -> Style {
        self.colored(Color::Blue)
    }

    /// A value, by its kind.
    #[must_use]
    pub fn value(self, shown: &Shown) -> Style {
        match shown {
            Shown::UInt(_) | Shown::SInt(_) | Shown::Float(_) => self.colored(Color::Yellow),
            Shown::Enum { .. } => self.colored(Color::Magenta),
            Shown::Bool(_) => self.colored(Color::Cyan),
            Shown::Str { .. } => self.colored(Color::Green),
            Shown::Bytes { .. } => self.colored(Color::Red),
        }
    }

    /// The selected row: reversed, which shows without any colour.
    #[must_use]
    pub fn selected(self) -> Style {
        Style::default().add_modifier(Modifier::REVERSED)
    }
}

/// Draw the viewer: the tree of what was read in a pane with the type in its
/// title, and how the parse went.
pub fn draw(frame: &mut Frame<'_>, app: &mut App, theme: &Theme) {
    let status = match &app.error {
        None => Span::styled(" parsed ", theme.good()),
        Some(_) => Span::styled(" the parse failed ", theme.bad()),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format::truncate(&format::safe(&app.type_name), 40),
                theme.title(),
            ),
            Span::raw(" "),
        ]))
        .title(Line::from(status).alignment(Alignment::Right))
        .title_bottom(Line::from(Span::styled(" q quit ", theme.dim())));
    let inner = block.inner(frame.area());
    frame.render_widget(block, frame.area());
    tree_view::render(frame, inner, &app.tree, &mut app.view, theme);
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::app::tests::app;

    fn screen(width: u16, height: u16, color: bool) -> (String, Terminal<TestBackend>) {
        let (mut app, _dir) = app();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| draw(f, &mut app, &Theme::new(color)))
            .unwrap();
        let text = terminal.backend().to_string();
        (text, terminal)
    }

    #[test]
    fn the_title_has_the_type_and_the_parse() {
        let (text, _) = screen(40, 8, true);
        let first = text.lines().next().unwrap();
        assert!(first.contains(" t "), "{text}");
        assert!(first.contains("parsed"), "{text}");
        assert!(text.contains("q quit"), "{text}");
    }

    #[test]
    fn a_tiny_terminal_is_drawn_without_panicking() {
        for (w, h) in [(1, 1), (2, 2), (5, 2), (20, 3), (3, 40), (80, 1)] {
            screen(w, h, true);
        }
    }

    #[test]
    fn without_colour_no_cell_has_one() {
        let (_, terminal) = screen(40, 8, false);
        let buffer = terminal.backend().buffer();
        assert!(
            buffer
                .content()
                .iter()
                .all(|cell| cell.fg == Color::Reset && cell.bg == Color::Reset)
        );
        let (_, terminal) = screen(40, 8, true);
        assert!(
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .any(|cell| cell.fg != Color::Reset)
        );
    }
}
