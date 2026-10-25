//! Drawing the rows of the tree model: indentation, an open/closed marker, the
//! name, the type and the value, cut to the cells there are.

use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};

use crate::format;
use crate::tree::{Row, RowKind, TreeModel};
use crate::ui::Theme;

/// Where the view is in the rows, kept between drawings.
#[derive(Debug, Default, Clone, Copy)]
pub struct TreeView {
    /// The index of the first row that is shown.
    top: usize,
    /// How many rows fitted the last time it was drawn.
    height: usize,
}

impl TreeView {
    /// A view at the top.
    #[must_use]
    pub fn new() -> Self {
        TreeView::default()
    }

    /// How many rows fitted the last time (for the page keys).
    #[must_use]
    pub fn page(&self) -> usize {
        self.height.max(1)
    }

    /// The index of the first row shown.
    #[must_use]
    pub fn top(&self) -> usize {
        self.top
    }

    /// Scroll as little as needed for `selected` to be in view in `height` rows,
    /// and not past the end of `len` rows.
    pub fn follow(&mut self, selected: usize, len: usize, height: usize) {
        self.height = height;
        if height == 0 {
            return;
        }
        if selected < self.top {
            self.top = selected;
        } else if selected >= self.top + height {
            self.top = selected + 1 - height;
        }
        self.top = self.top.min(len.saturating_sub(height));
    }
}

/// A piece of a line and how it is drawn.
type Piece = (String, Style);

/// The pieces of a row, before they are cut to the width.
fn pieces(row: &Row, theme: Theme) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    // the first cell says that the parse failed here
    out.push(if row.failed {
        ("✗".to_owned(), theme.failed())
    } else {
        (" ".to_owned(), Style::default())
    });
    out.push(("  ".repeat(row.depth), Style::default()));
    if matches!(row.kind, RowKind::More { .. } | RowKind::Earlier { .. }) {
        out.push(("  ".to_owned(), Style::default()));
        out.push((format::safe(&row.name), theme.dim()));
        return out;
    }
    let marker = match (row.openable, row.open) {
        (false, _) => "  ",
        (true, true) => "▾ ",
        (true, false) => "▸ ",
    };
    out.push((marker.to_owned(), theme.marker()));
    let name_style = if row.instance {
        theme.instance()
    } else {
        theme.name()
    };
    out.push((format::safe(&row.name), name_style));
    // the root is named after its type: said once
    if let Some(type_name) = row.type_name.as_ref().filter(|t| **t != row.name) {
        out.push((format!("  {}", format::safe(type_name)), theme.type_name()));
    }
    if let Some(shown) = &row.shown {
        out.push((" = ".to_owned(), theme.dim()));
        out.push((format::shown(shown), theme.value(shown)));
    }
    if row.instance {
        // an instance is not in the sequence of the type: dimmed all over
        for piece in &mut out {
            piece.1 = theme.faded(piece.1);
        }
    }
    out
}

/// A row as a line `width` cells wide: cut with `…` if it is longer, and
/// filled with blanks (so a selected row is highlighted to the edge).
#[must_use]
pub fn row_line(row: &Row, selected: bool, width: usize, theme: &Theme) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut left = width;
    for (text, style) in pieces(row, *theme) {
        if left == 0 {
            break;
        }
        let cells = format::width(&text);
        if cells <= left {
            left -= cells;
            spans.push(Span::styled(text, style));
        } else {
            spans.push(Span::styled(format::truncate(&text, left), style));
            left = 0;
        }
    }
    let line = Line::from(spans);
    let used = line.width();
    let mut line = line;
    if used < width {
        line.spans.push(Span::raw(" ".repeat(width - used)));
    }
    if selected {
        line = line.patch_style(theme.selected());
    }
    line
}

/// Draw the rows that are in view in `area`, and a scrollbar if they do not all fit.
pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    model: &TreeModel,
    view: &mut TreeView,
    theme: &Theme,
) {
    let rows = model.rows();
    view.follow(model.selected_index(), rows.len(), usize::from(area.height));
    if area.width == 0 || area.height == 0 {
        return;
    }
    let scrollbar = rows.len() > usize::from(area.height) && area.width > 2;
    let width = usize::from(area.width) - usize::from(scrollbar);
    let lines: Vec<Line<'static>> = rows
        .iter()
        .enumerate()
        .skip(view.top)
        .take(usize::from(area.height))
        .map(|(index, row)| row_line(row, index == model.selected_index(), width, theme))
        .collect();
    let body = Rect {
        width: u16::try_from(width).unwrap_or(area.width),
        ..area
    };
    frame.render_widget(ratatui::widgets::Paragraph::new(lines), body);
    if scrollbar {
        let mut state = ScrollbarState::new(rows.len().saturating_sub(usize::from(area.height)))
            .position(view.top);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .style(theme.dim()),
            area.inner(Margin {
                vertical: 0,
                horizontal: 0,
            }),
            &mut state,
        );
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
    use ratatui::buffer::Buffer;
    use ratatui::style::Modifier;

    use super::*;
    use crate::tree::tests::model;

    /// One field of each kind of value, a type inside, an instance and an enum.
    const KINDS: &str = "id: kinds
seq:
  - id: small
    type: u1
  - id: big
    type: u2le
  - id: neg
    type: s2le
  - id: ratio
    type: f4le
  - id: flag
    type: b1
  - id: pad
    type: b7
  - id: kind
    type: u1
    enum: kinds
  - id: other
    type: u1
    enum: kinds
  - id: title
    type: str
    size: 5
    encoding: UTF-8
  - id: blob
    size: 3
  - id: sub
    type: inner
types:
  inner:
    seq:
      - id: x
        type: u1
enums:
  kinds:
    1: png
instances:
  twice:
    value: small * 2
";

    fn kinds_bytes() -> Vec<u8> {
        let mut bytes = vec![
            7, 0x2c, 0x01, 0xfb, 0xff, 0x00, 0x00, 0xc0, 0x3f, 0x80, 1, 9,
        ];
        bytes.extend("ab日".as_bytes());
        bytes.extend([1, 2, 3, 4]);
        bytes
    }

    fn open_all(tree: &mut TreeModel) {
        tree.select_path(&[10, 0]);
        tree.home();
    }

    fn text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn lines(tree: &TreeModel, width: usize, theme: Theme) -> Vec<String> {
        tree.rows()
            .iter()
            .enumerate()
            .map(|(i, row)| text(&row_line(row, i == tree.selected_index(), width, &theme)))
            .collect()
    }

    fn kinds() -> TreeModel {
        let (mut tree, error) = model(KINDS, &kinds_bytes());
        assert_eq!(error, None);
        open_all(&mut tree);
        tree
    }

    #[test]
    fn each_kind_of_value_at_a_wide_width() {
        let theme = Theme::new(false);
        let got = lines(&kinds(), 60, theme);
        let want = [
            " ▾ kinds",
            "     small = 7",
            "     big = 300 (0x12c)",
            "     neg = -5 (-0x5)",
            "     ratio = 1.5",
            "     flag = true",
            "     pad = 0",
            "     kind = png (1)",
            "     other = 9 (no name)",
            "     title = \"ab日\"",
            "     blob = <3 bytes> 01 02 03",
            "   ▾ sub  kinds::inner",
            "       x = 4",
            "     twice = 14",
        ];
        let want: Vec<String> = want
            .iter()
            .map(|l| format!("{l}{}", " ".repeat(60 - crate::format::width(l))))
            .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn the_same_at_a_narrow_width_is_cut_with_an_ellipsis() {
        let theme = Theme::new(false);
        let got = lines(&kinds(), 22, theme);
        for line in &got {
            assert_eq!(crate::format::width(line), 22, "{line:?}");
        }
        assert_eq!(got[2], "     big = 300 (0x12c)");
        assert_eq!(got[10], "     blob = <3 bytes>…");
        assert_eq!(got[11], "   ▾ sub  kinds::inner");
        assert_eq!(got[9].trim_end(), "     title = \"ab日\"");
        // at the extreme, only the start of the row is left
        for line in lines(&kinds(), 3, theme) {
            assert_eq!(crate::format::width(&line), 3);
        }
        assert!(lines(&kinds(), 0, theme).iter().all(String::is_empty));
    }

    #[test]
    fn a_selected_row_is_reversed_and_an_instance_is_dimmed() {
        let theme = Theme::new(true);
        let tree = kinds();
        let selected = row_line(&tree.rows()[0], true, 30, &theme);
        assert!(selected.style.add_modifier.contains(Modifier::REVERSED));
        let plain = row_line(&tree.rows()[1], false, 30, &theme);
        assert!(!plain.style.add_modifier.contains(Modifier::REVERSED));
        let instance = tree.rows().iter().find(|r| r.instance).unwrap();
        let line = row_line(instance, false, 40, &theme);
        assert!(
            line.spans
                .iter()
                .take(5)
                .all(|s| s.style.add_modifier.contains(Modifier::DIM)),
            "{line:?}"
        );
    }

    #[test]
    fn the_failed_row_is_marked_and_what_was_read_before_is_shown() {
        // two items are announced, one and a half are there
        let schema = "id: t\nseq:\n  - id: count\n    type: u1\n  - id: a\n    type: u1\n  - id: b\n    type: u2le\n";
        let (mut tree, error) = model(schema, &[2, 9, 1]);
        assert!(error.is_some());
        let theme = Theme::new(false);
        let got = lines(&tree, 40, theme);
        assert!(got.iter().any(|l| l.starts_with('✗')), "{got:?}");
        assert!(got.iter().any(|l| l.contains("a = 9")), "{got:?}");
        // the failed row is marked by more than colour: the mark is text
        assert!(!lines(&kinds(), 40, theme).iter().any(|l| l.contains('✗')));
        let failed = got.iter().find(|l| l.starts_with('✗')).unwrap();
        assert!(failed.contains('b'), "{failed:?}");
        tree.home();
    }

    #[test]
    fn strings_with_wide_combining_and_control_characters_are_safe() {
        let schema =
            "id: s\nseq:\n  - id: text\n    type: str\n    size-eos: true\n    encoding: UTF-8\n";
        let theme = Theme::new(false);
        for (input, expect) in [
            ("日本語".as_bytes().to_vec(), "text = \"日本語\""),
            ("e\u{301}x".as_bytes().to_vec(), "text = \"e\u{301}x\""),
            (
                b"a\x1b[31mred\x07\x00\r\n".to_vec(),
                "text = \"a\\u{1b}[31mred\\u{7}\\u{0}\\r\\n\"",
            ),
            (
                "\u{85}\u{202e}".as_bytes().to_vec(),
                "text = \"\\u{85}\\u{202e}\"",
            ),
        ] {
            let (tree, _) = model(schema, &input);
            let got = lines(&tree, 70, theme);
            assert!(got[1].contains(expect), "{got:?} for {expect}");
            assert!(
                !got.iter().any(|l| l.chars().any(char::is_control)),
                "{got:?}"
            );
        }
    }

    #[test]
    fn a_name_from_a_schema_cannot_send_escape_codes_either() {
        let schema = "id: \"x\\e[2J\"\nseq:\n  - id: a\n    type: u1\n";
        // the id is not valid, or is escaped when drawn: either way no control character is drawn
        let (tree, _) = model(schema, &[1]);
        let theme = Theme::new(false);
        assert!(
            !lines(&tree, 40, theme)
                .iter()
                .any(|l| l.chars().any(char::is_control))
        );
    }

    fn drawn(tree: &TreeModel, view: &mut TreeView, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| render(frame, frame.area(), tree, view, &Theme::new(false)))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn row_text(buffer: &Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol().to_owned())
            .collect()
    }

    #[test]
    fn the_view_follows_the_selection_and_has_a_scrollbar_when_rows_do_not_fit() {
        let mut tree = kinds();
        let mut view = TreeView::new();
        let buffer = drawn(&tree, &mut view, 40, 5);
        assert_eq!(view.top(), 0);
        assert!(row_text(&buffer, 0).starts_with(" ▾ kinds"));
        // a scrollbar on the right edge
        assert!((0..5).any(|y| buffer[(39, y)].symbol() != " "));

        tree.end();
        let buffer = drawn(&tree, &mut view, 40, 5);
        assert_eq!(view.top(), tree.rows().len() - 5);
        assert!(row_text(&buffer, 4).contains("twice"));
        // the selected row is the last one drawn
        assert_eq!(tree.selected_index(), tree.rows().len() - 1);

        tree.home();
        drawn(&tree, &mut view, 40, 5);
        assert_eq!(view.top(), 0);
        // all rows fit: no scrollbar
        let buffer = drawn(&tree, &mut view, 40, 30);
        assert!((0..30).all(|y| buffer[(39, y)].symbol() == " "));
    }

    #[test]
    fn resizing_down_to_nothing_does_not_panic() {
        let mut tree = kinds();
        let mut view = TreeView::new();
        tree.end();
        for (w, h) in [
            (80, 24),
            (40, 5),
            (10, 3),
            (3, 3),
            (2, 2),
            (1, 1),
            (1, 50),
            (50, 1),
            (80, 24),
        ] {
            drawn(&tree, &mut view, w, h);
        }
        // the view comes back to the selected row
        let buffer = drawn(&tree, &mut view, 40, 6);
        assert!((0..6).any(|y| row_text(&buffer, y).contains("twice")));
    }

    #[test]
    fn a_more_row_is_drawn_dim() {
        let count = 300;
        let (mut tree, _) = model(
            &format!(
                "id: big\nseq:\n  - id: items\n    type: u1\n    repeat: expr\n    repeat-expr: {count}\n"
            ),
            &vec![1_u8; count],
        );
        tree.down();
        tree.expand();
        let theme = Theme::new(true);
        let last = tree.rows().last().unwrap();
        let line = row_line(last, false, 40, &theme);
        assert!(text(&line).contains("100 more…"), "{}", text(&line));
        assert!(
            line.spans
                .iter()
                .any(|s| s.style.add_modifier.contains(Modifier::DIM))
        );
    }
}
