//! Drawing the state of the viewer.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::{App, Focus};
use crate::format;
use crate::hex_view;
use crate::inspect;
use crate::status;
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

    /// The status bar: reversed, like the selection.
    #[must_use]
    pub fn status(self) -> Style {
        Style::default().add_modifier(Modifier::REVERSED)
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

/// The status bar needs a line of its own below this height.
const STATUS_MIN_HEIGHT: u16 = 6;
/// The lines of an inspector that is shown below the bytes (more than this, and it takes the pane).
const INSPECTOR_MAX_LINES: u16 = 12;

/// Draw the viewer: the tree of what was read, in a pane with the type in its
/// title and how the parse went, and next to it (or below, on a narrow
/// terminal) the bytes of the file with the selected field marked; below
/// them the inspector, if it is on; and at the bottom the status bar.
pub fn draw(frame: &mut Frame<'_>, app: &mut App, theme: &Theme) {
    let mut area = frame.area();
    if area.width < SMALLEST || area.height < 3 {
        return;
    }
    if area.height >= STATUS_MIN_HEIGHT {
        let [rest, bar] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
        status_bar(frame, bar, app, *theme);
        area = rest;
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
        " q quit · Tab switch pane · i inspector "
    } else {
        " q quit · i inspector "
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
    // With no room for the bytes, the inspector takes the place of the tree.
    if app.inspector && hex_area.is_none() {
        inspector(frame, inner, app, *theme);
    } else {
        tree_view::render(frame, inner, &app.tree, &mut app.view, theme);
    }

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
        let (bytes_area, inspector_area) = if app.inspector {
            let lines = u16::try_from(inspector_lines(app).len()).unwrap_or(u16::MAX);
            let want = lines.clamp(3, INSPECTOR_MAX_LINES) + 1;
            if inner.height >= want + 5 {
                let [top, bottom] =
                    Layout::vertical([Constraint::Min(0), Constraint::Length(want)]).areas(inner);
                (Some(top), Some(bottom))
            } else {
                (None, Some(inner))
            }
        } else {
            (Some(inner), None)
        };
        if let Some(bytes_area) = bytes_area {
            match app.bytes.as_mut() {
                Some(bytes) => hex_view::render(
                    frame,
                    bytes_area,
                    bytes,
                    &mut app.hex,
                    Some(app.tree.selected()),
                    focus == Focus::Hex,
                    theme,
                ),
                None => frame.render_widget(Paragraph::new("could not open the file"), bytes_area),
            }
        }
        if let Some(area) = inspector_area {
            inspector(frame, area, app, *theme);
        }
    }
}

/// The bar at the bottom: the field on the left, the file on the right.
fn status_bar(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let names = app.tree.names_to(app.tree.selected_path());
    let file_name = app.file.file_name().map_or_else(
        || app.file.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let (left, right) = status::bar(app.tree.selected(), &names, &file_name, app.size);
    let width = usize::from(area.width);
    let right = format::truncate(&right, width / 2);
    let room = width.saturating_sub(format::width(&right) + 2);
    let left = format::truncate(&left, room);
    let gap = width.saturating_sub(format::width(&left) + format::width(&right) + 1);
    let line = Line::from(vec![
        Span::raw(" "),
        Span::raw(left),
        Span::raw(" ".repeat(gap)),
        Span::raw(right),
    ])
    .style(theme.status());
    frame.render_widget(Paragraph::new(line), area);
}

/// The lines of the inspector for the selected field, as text.
fn inspector_lines(app: &mut App) -> Vec<(String, String)> {
    let row = app.tree.selected().clone();
    let target = match inspect::target(&row) {
        Ok(target) => target,
        Err(reason) => return vec![(String::new(), reason.message().to_owned())],
    };
    let (first, total) = inspect::byte_range(&target);
    let Some(bytes) = app.bytes.as_mut() else {
        return vec![(String::new(), "The file cannot be read.".to_owned())];
    };
    let data = bytes
        .window(
            first,
            usize::try_from(total)
                .unwrap_or(usize::MAX)
                .min(inspect::TEXT_BYTES),
        )
        .to_vec();
    if data.is_empty() {
        return vec![(
            String::new(),
            "No bytes: the field is past the end of the file.".to_owned(),
        )];
    }
    inspect::lines(
        &data,
        total,
        target.start - first * 8,
        target.end - first * 8,
    )
    .into_iter()
    .map(|(label, text)| (label.to_owned(), text))
    .collect()
}

/// Draw the inspector in `area`: a title line, then the lines.
fn inspector(frame: &mut Frame<'_>, area: Rect, app: &mut App, theme: Theme) {
    let lines = inspector_lines(app);
    let width = usize::from(area.width);
    let mut out = vec![Line::from(Span::styled(
        format::truncate("── inspector ", width),
        theme.dim(),
    ))];
    for (label, text) in lines
        .into_iter()
        .take(usize::from(area.height.saturating_sub(1)))
    {
        if label.is_empty() {
            // a message: it wraps, it is not cut
            out.push(Line::from(Span::raw(text)));
            continue;
        }
        let label_width = 12;
        let shown = format::truncate(&text, width.saturating_sub(label_width + 1));
        out.push(Line::from(vec![
            Span::styled(format!("{label:<label_width$}"), theme.dim()),
            Span::raw(" "),
            Span::raw(shown),
        ]));
    }
    frame.render_widget(Paragraph::new(out).wrap(Wrap { trim: false }), area);
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
        // the bar of the status is the last line; the bottoms of the panes are above it
        let bottom = &lines[lines.len() - 2];
        assert!(bottom.contains("3 bytes"), "{text}");
        assert!(bottom.contains("Tab switch pane"), "{text}");
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

    const RECORD: &str = "id: rec
seq:
  - id: one
    type: u1
  - id: two
    type: u2le
  - id: four
    type: u4le
  - id: eight
    type: u8le
  - id: text
    size: 24
  - id: inner
    type: pair
  - id: flags
    type: b4
  - id: low
    type: b4
types:
  pair:
    seq:
      - id: x
        type: u1
      - id: y
        type: u2le
instances:
  twice:
    value: one * 2
";

    fn record() -> Vec<u8> {
        let mut data = vec![0x41, 0x34, 0x12];
        data.extend(1.5_f32.to_le_bytes());
        data.extend(1.25_f64.to_le_bytes());
        data.extend(b"hello, binary world! ...");
        data.extend([9, 0x78, 0x56]);
        data.push(0xab);
        data
    }

    /// The screen of a viewer for the record, with the inspector on and the field at `path` selected.
    fn inspected(path: &[usize], width: u16, height: u16) -> Vec<String> {
        let (mut app, _dir) = crate::app::tests::app_for(RECORD, &record());
        app.tree.select_path(path);
        app.inspector = true;
        let theme = Theme::new(false);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw(f, &mut app, &theme)).unwrap();
        rows(&terminal)
    }

    fn has(lines: &[String], needle: &str) -> bool {
        lines.iter().any(|l| l.contains(needle))
    }

    #[test]
    fn the_status_bar_has_the_path_the_type_the_place_and_the_file() {
        let lines = inspected(&[5, 1], 120, 30); // inner/y
        let bar = lines.last().unwrap();
        assert!(
            bar.starts_with(" rec/inner/y · unsigned integer · @0x28 (40) · 2 bytes (16 bits)"),
            "{bar}"
        );
        assert!(
            bar.trim_end_matches('"').ends_with("t.bin · 43 bytes"),
            "{bar}"
        );
        // a type, a bit field, an instance
        let bar = inspected(&[5], 120, 30).last().unwrap().clone();
        assert!(
            bar.starts_with(" rec/inner · rec::pair · @0x27 (39) · 3 bytes (24 bits)"),
            "{bar}"
        );
        let bar = inspected(&[7], 120, 30).last().unwrap().clone();
        assert!(
            bar.contains("rec/low · unsigned integer · bit 4 of byte 0x2a · 4 bits"),
            "{bar}"
        );
        let bar = inspected(&[8], 120, 30).last().unwrap().clone();
        assert!(
            bar.contains("rec/twice · unsigned integer · computed, not in the file"),
            "{bar}"
        );
        // the root
        let bar = inspected(&[], 120, 30).last().unwrap().clone();
        assert!(
            bar.starts_with(" rec · rec · @0x0 (0) · 43 bytes (344 bits)"),
            "{bar}"
        );
        // it is cut to the width, and the file stays
        let bar = inspected(&[5, 1], 40, 30).last().unwrap().clone();
        assert!(bar.contains('…') && bar.contains("t.bin"), "{bar}");
    }

    #[test]
    fn the_inspector_for_fields_of_1_2_4_and_8_bytes_and_a_longer_one() {
        let one = inspected(&[0], 120, 30);
        assert!(has(&one, "── inspector"));
        assert!(has(&one, "bytes        41 (1 byte)"), "{one:?}");
        assert!(has(&one, "unsigned LE  65 (0x41) 01000001"));
        assert!(has(&one, "signed LE    65"));
        assert!(has(&one, "ASCII        \"A\""));

        let two = inspected(&[1], 120, 30);
        assert!(
            has(&two, "unsigned LE  4660 (0x1234) 0001001000110100"),
            "{two:?}"
        );
        assert!(has(&two, "unsigned BE  13330 (0x3412)"));
        assert!(has(&two, "UTF-16 LE"));

        let four = inspected(&[2], 120, 30);
        assert!(has(&four, "float LE     1.5"), "{four:?}");
        assert!(has(&four, "unsigned LE  1069547520 (0x3fc00000)"));

        let eight = inspected(&[3], 120, 30);
        assert!(has(&eight, "float LE     1.25"), "{eight:?}");
        assert!(!has(&eight, "unsigned LE  1.25"));

        let long = inspected(&[4], 120, 30);
        assert!(
            has(&long, "bytes        68 65 6c 6c 6f 2c 20 62 … (24 bytes)"),
            "{long:?}"
        );
        assert!(
            has(&long, "UTF-8        \"hello, binary world! ...\""),
            "{long:?}"
        );
        assert!(!has(&long, "unsigned LE"));
    }

    #[test]
    fn the_inspector_for_a_bit_field_says_which_bits() {
        let lines = inspected(&[7], 120, 30); // the low half of 0xab
        assert!(
            has(&lines, "bits         4 bits = 11 (0xb, 0b1011)"),
            "{lines:?}"
        );
        assert!(has(&lines, "ab (1 byte)"));
    }

    #[test]
    fn the_inspector_says_why_there_are_no_bytes_for_an_instance_or_a_processed_field() {
        let lines = inspected(&[8], 120, 30); // twice: computed
        assert!(
            has(
                &lines,
                "No bytes of the file: this value is computed by an expression."
            ),
            "{lines:?}"
        );
        assert!(!has(&lines, "unsigned LE"));

        let schema = "id: p\nseq:\n  - id: len\n    type: u1\n  - id: packed\n    size: 2\n    process: xor(0x55)\n    type: inside\ntypes:\n  inside:\n    seq:\n      - id: a\n        type: u1\n      - id: b\n        type: u1\n";
        let (mut app, _dir) = crate::app::tests::app_for(schema, &[2, 0x54, 0x57]);
        app.tree.select_path(&[1, 0]);
        app.inspector = true;
        let theme = Theme::new(false);
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
        terminal.draw(|f| draw(f, &mut app, &theme)).unwrap();
        let lines = rows(&terminal);
        // the message wraps in the narrow pane: its two ends are on the screen
        assert!(
            has(&lines, "No bytes of the file: this value is read"),
            "{lines:?}"
        );
        assert!(has(&lines, "process:."), "{lines:?}");
        assert!(!has(&lines, "unsigned LE"));
        // the status bar says the same
        assert!(
            lines
                .last()
                .unwrap()
                .contains("not in the file (made by process:)"),
            "{lines:?}"
        );
    }

    #[test]
    fn the_inspector_takes_the_pane_when_there_is_little_room_and_is_off_by_default() {
        let (mut app, _dir) = crate::app::tests::app_for(RECORD, &record());
        let theme = Theme::new(false);
        let mut terminal = Terminal::new(TestBackend::new(100, 10)).unwrap();
        terminal.draw(|f| draw(f, &mut app, &theme)).unwrap();
        assert!(!has(&rows(&terminal), "── inspector"), "off by default");
        crate::app::tests::press_key(&mut app, crossterm::event::KeyCode::Char('i'));
        assert!(app.inspector);
        terminal.draw(|f| draw(f, &mut app, &theme)).unwrap();
        let lines = rows(&terminal);
        assert!(has(&lines, "── inspector"), "{lines:?}");
        assert!(
            !has(&lines, "00000000  41"),
            "it took the place of the bytes: {lines:?}"
        );
        crate::app::tests::press_key(&mut app, crossterm::event::KeyCode::Char('i'));
        assert!(!app.inspector);
        // and on a terminal with room, the bytes and the inspector are both there
        let mut terminal = Terminal::new(TestBackend::new(120, 34)).unwrap();
        app.inspector = true;
        terminal.draw(|f| draw(f, &mut app, &theme)).unwrap();
        let lines = rows(&terminal);
        assert!(
            has(&lines, "── inspector") && has(&lines, "00000000  41"),
            "{lines:?}"
        );
    }

    #[test]
    fn small_terminals_with_the_inspector_and_the_bar_do_not_panic() {
        for (w, h) in [
            (120, 34),
            (100, 8),
            (60, 20),
            (30, 7),
            (8, 6),
            (9, 3),
            (1, 1),
        ] {
            let _ = inspected(&[2], w, h);
        }
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
