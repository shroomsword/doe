//! Drawing the state of the viewer.

use ratatui::Frame;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::App;

/// The styles of the viewer. Without colour they are the plain terminal
/// colours with bold and reverse only, which is what `NO_COLOR` asks for.
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

    /// A parse that failed.
    #[must_use]
    pub fn bad(self) -> Style {
        self.colored(Color::Red).add_modifier(Modifier::BOLD)
    }

    /// Hints.
    #[must_use]
    pub fn dim(self) -> Style {
        Style::default().add_modifier(Modifier::DIM)
    }
}

/// Draw the viewer: for now the name of the type, how the parse went, and how to leave.
pub fn draw(frame: &mut Frame<'_>, app: &App, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" doe ", theme.title()));
    let inner = block.inner(frame.area());
    frame.render_widget(block, frame.area());

    let result = match &app.error {
        None => Span::styled("parsed", theme.good()),
        Some(error) => Span::styled(format!("the parse failed: {error}"), theme.bad()),
    };
    let lines = vec![
        Line::from(vec![
            Span::raw("type  "),
            Span::styled(app.type_name.clone(), theme.title()),
        ]),
        Line::from(vec![
            Span::raw("file  "),
            Span::raw(app.file.display().to_string()),
        ]),
        Line::from(vec![
            Span::raw("size  "),
            Span::raw(format!("{} bytes", app.size)),
        ]),
        Line::from(vec![Span::raw("      "), result]),
        Line::raw(""),
        Line::from(Span::styled("q to quit", theme.dim())),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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
        let (app, _dir) = app();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| draw(f, &app, &Theme::new(color)))
            .unwrap();
        let text = terminal.backend().to_string();
        (text, terminal)
    }

    #[test]
    fn the_type_the_file_and_the_way_out_are_shown() {
        let (text, _) = screen(60, 10, true);
        assert!(text.contains("type  t"), "{text}");
        assert!(text.contains("size  3 bytes"), "{text}");
        assert!(text.contains("parsed"), "{text}");
        assert!(text.contains("q to quit"), "{text}");
    }

    #[test]
    fn a_tiny_terminal_is_drawn_without_panicking() {
        for (w, h) in [(1, 1), (5, 2), (20, 3)] {
            screen(w, h, true);
        }
    }

    #[test]
    fn without_colour_no_cell_has_one() {
        let (_, terminal) = screen(60, 10, false);
        let buffer = terminal.backend().buffer();
        assert!(
            buffer
                .content()
                .iter()
                .all(|cell| cell.fg == Color::Reset && cell.bg == Color::Reset)
        );
        let (_, terminal) = screen(60, 10, true);
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
