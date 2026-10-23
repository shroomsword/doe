//! Drawing the state of the viewer.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, Focus};
use crate::format;
use crate::hex_view;
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

    /// A byte that is wholly in the selected field.
    #[must_use]
    pub fn hex_whole(self) -> Style {
        self.colored(Color::Cyan).add_modifier(Modifier::REVERSED)
    }

    /// A byte that is partly in the selected field (the other bits are a neighbour's).
    #[must_use]
    pub fn hex_part(self) -> Style {
        self.colored(Color::Cyan).add_modifier(Modifier::UNDERLINED)
    }

    /// The byte the cursor is on, in the hex pane when it has the focus.
    #[must_use]
    pub fn hex_cursor(self) -> Style {
        let style = if self.color {
            Style::default().bg(Color::Yellow).fg(Color::Black)
        } else {
            Style::default()
        };
        style.add_modifier(Modifier::REVERSED | Modifier::BOLD | Modifier::UNDERLINED)
    }

    /// The border of a pane: bold when it has the focus.
    #[must_use]
    pub fn border(self, focused: bool) -> Style {
        if focused {
            self.colored(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().add_modifier(Modifier::DIM)
        }
    }

    /// The selected row: reversed, which shows without any colour.
    #[must_use]
    pub fn selected(self) -> Style {
        Style::default().add_modifier(Modifier::REVERSED)
    }
}

/// The narrowest terminal that has the tree and the bytes side by side.
const SIDE_BY_SIDE: u16 = 90;
/// The shortest terminal that stacks them.
const STACKED: u16 = 14;
/// The narrowest terminal that is drawn at all.
const SMALLEST: u16 = 8;

/// Draw the viewer: the tree of what was read, in a pane with the type in its
/// title and how the parse went, and next to it (or below, on a narrow
/// terminal) the bytes of the file with the selected field marked.
pub fn draw(frame: &mut Frame<'_>, app: &mut App, theme: &Theme) {
    let area = frame.area();
    if area.width < SMALLEST || area.height < 3 {
        return;
    }
    let (tree_area, hex_area) = if area.width >= SIDE_BY_SIDE {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)])
                .areas(area);
        (left, Some(right))
    } else if area.height >= STACKED {
        let [top, bottom] =
            Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);
        (top, Some(bottom))
    } else {
        (area, None)
    };

    // The pane that has the keys is the one with the hex pane when the other
    // is not drawn.
    let focus = if hex_area.is_some() {
        app.focus
    } else {
        Focus::Tree
    };

    let status = match &app.error {
        None => Span::styled(" parsed ", theme.good()),
        Some(_) => Span::styled(" the parse failed ", theme.bad()),
    };
    let hint = if hex_area.is_some() {
        " q quit · Tab switch pane "
    } else {
        " q quit "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border(focus == Focus::Tree))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format::truncate(&format::safe(&app.type_name), 40),
                theme.title(),
            ),
            Span::raw(" "),
        ]))
        .title(Line::from(status).alignment(Alignment::Right))
        .title_bottom(Line::from(Span::styled(hint, theme.dim())));
    let inner = block.inner(tree_area);
    frame.render_widget(block, tree_area);
    tree_view::render(frame, inner, &app.tree, &mut app.view, theme);

    if let Some(hex_area) = hex_area {
        let size = app.bytes.as_ref().map_or(0, crate::bytes::FileBytes::size);
        let note = format::truncate(
            &hex_view::describe(app.tree.selected(), hex_view::offset_digits(size)),
            usize::from(hex_area.width.saturating_sub(4)),
        );
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border(focus == Focus::Hex))
            .title(Line::from(vec![
                Span::raw(" "),
                Span::styled(note, theme.title()),
                Span::raw(" "),
            ]))
            .title_bottom(Line::from(Span::styled(
                format!(" {size} bytes "),
                theme.dim(),
            )));
        let inner = block.inner(hex_area);
        frame.render_widget(block, hex_area);
        match app.bytes.as_mut() {
            Some(bytes) => hex_view::render(
                frame,
                inner,
                bytes,
                &mut app.hex,
                Some(app.tree.selected()),
                focus == Focus::Hex,
                theme,
            ),
            None => frame.render_widget(Paragraph::new("could not open the file"), inner),
        }
    }
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
    use crate::app::Focus;
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

    fn rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
        terminal
            .backend()
            .to_string()
            .lines()
            .map(|l| l.trim_matches('"').to_owned())
            .collect()
    }

    #[test]
    fn the_tree_and_the_bytes_are_side_by_side_on_a_wide_terminal() {
        let (text, terminal) = screen(100, 8, false);
        let lines = rows(&terminal);
        // the tree on the left, the bytes on the right, on the same rows
        let first = &lines[1];
        assert!(first.contains("▾ t"), "{text}");
        assert!(first.contains("00000000  07 34 12"), "{text}");
        assert!(first.find("▾ t").unwrap() < first.find("00000000").unwrap());
        // the title of the bytes says what is selected: the root, all three bytes
        assert!(
            lines[0].contains("t: bytes 0x00000000..0x00000002"),
            "{text}"
        );
        assert!(lines.last().unwrap().contains("3 bytes"), "{text}");
        assert!(lines.last().unwrap().contains("Tab switch pane"), "{text}");
    }

    #[test]
    fn the_bytes_are_below_the_tree_on_a_narrow_terminal_and_hidden_on_a_small_one() {
        let (_, terminal) = screen(60, 20, false);
        let lines = rows(&terminal);
        let tree = lines.iter().position(|l| l.contains("▾ t")).unwrap();
        let bytes = lines
            .iter()
            .position(|l| l.contains("00000000  07 34 12"))
            .unwrap();
        assert!(tree < bytes, "{lines:?}");
        let (_, terminal) = screen(60, 8, false);
        assert!(!terminal.backend().to_string().contains("00000000"));
        assert!(terminal.backend().to_string().contains("▾ t"));
    }

    #[test]
    fn the_pane_with_the_keys_has_the_bold_border() {
        let (mut app, _dir) = app();
        let theme = Theme::new(false);
        let mut terminal = Terminal::new(TestBackend::new(100, 8)).unwrap();
        terminal.draw(|f| draw(f, &mut app, &theme)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        // the top left corner is the tree's, the corner of the bytes is at 45%
        let tree_corner = buffer[(0, 0)].modifier;
        let hex_corner = buffer[(45, 0)].modifier;
        assert!(tree_corner.contains(Modifier::BOLD) && !tree_corner.contains(Modifier::DIM));
        assert!(hex_corner.contains(Modifier::DIM));
        app.focus = Focus::Hex;
        terminal.draw(|f| draw(f, &mut app, &theme)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert!(buffer[(0, 0)].modifier.contains(Modifier::DIM));
        assert!(buffer[(45, 0)].modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn the_selected_field_is_marked_in_the_bytes_and_the_cursor_shows_in_the_pane_with_the_keys() {
        let (mut app, _dir) = app();
        app.tree.select_path(&[1]); // b: bytes 1 and 2
        let theme = Theme::new(false);
        let mut terminal = Terminal::new(TestBackend::new(100, 8)).unwrap();
        terminal.draw(|f| draw(f, &mut app, &theme)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let at = |needle: &str| -> u16 {
            let line: String = (0..buffer.area.width)
                .map(|x| buffer[(x, 1)].symbol().to_owned())
                .collect();
            let chars: Vec<char> = line.chars().collect();
            let target: Vec<char> = needle.chars().collect();
            let at = chars
                .windows(target.len())
                .position(|w| w == target.as_slice())
                .unwrap();
            u16::try_from(at).unwrap()
        };
        let x07 = at("07 34 12");
        assert!(
            !buffer[(x07, 1)].modifier.contains(Modifier::REVERSED),
            "byte 0 is not b"
        );
        assert!(
            buffer[(x07 + 3, 1)].modifier.contains(Modifier::REVERSED),
            "byte 1 is"
        );
        assert!(
            buffer[(x07 + 6, 1)].modifier.contains(Modifier::REVERSED),
            "byte 2 is"
        );
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
