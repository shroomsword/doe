//! Drawing the state of the viewer.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::{App, Focus, Prompt};
use crate::format;
use crate::hex_view;
use crate::inspect;
use crate::keymap;
use crate::report::Report;
use crate::status;
use crate::theme::Palette;
use crate::tree::Shown;
use crate::tree_view;

/// The styles of the viewer. Without a theme they are the colours of the
/// terminal itself (its own palette, so it matches whatever the person has
/// chosen for it), and without colour they are bold, dim and reverse only, which
/// is what `NO_COLOR` asks for. With a [`Palette`] from a theme they are the
/// colours of the theme, and the background of the whole screen is the theme's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    color: bool,
    palette: Option<Palette>,
}

impl Theme {
    /// The terminal's own colours, or none.
    #[must_use]
    pub fn new(color: bool) -> Self {
        Theme {
            color,
            palette: None,
        }
    }

    /// The colours of a theme.
    #[must_use]
    pub fn themed(palette: Palette) -> Self {
        Theme {
            color: true,
            palette: Some(palette),
        }
    }

    /// The palette of the theme, if there is one.
    #[must_use]
    pub fn palette(self) -> Option<Palette> {
        self.palette
    }

    fn colored(self, color: Color) -> Style {
        if self.color {
            Style::default().fg(color)
        } else {
            Style::default()
        }
    }

    /// The colour to use for something of the terminal's palette, or the one of the theme.
    fn pick(self, themed: impl FnOnce(Palette) -> Color, terminal: Color) -> Style {
        match self.palette {
            Some(palette) => Style::default().fg(themed(palette)),
            None => self.colored(terminal),
        }
    }

    /// What the whole screen is drawn on: the background and the text of the theme.
    #[must_use]
    pub fn base(self) -> Style {
        match self.palette {
            Some(p) => Style::default().fg(p.foreground).bg(p.background),
            None => Style::default(),
        }
    }

    /// Titles and the type name.
    #[must_use]
    pub fn title(self) -> Style {
        self.pick(|p| p.accent, Color::Cyan)
            .add_modifier(Modifier::BOLD)
    }

    /// A parse that worked.
    #[must_use]
    pub fn good(self) -> Style {
        self.pick(|p| p.string, Color::Green)
    }

    /// A parse that failed, and the row where it failed.
    #[must_use]
    pub fn bad(self) -> Style {
        self.pick(|p| p.error, Color::Red)
            .add_modifier(Modifier::BOLD)
    }

    /// The mark and the name of the row that failed.
    #[must_use]
    pub fn failed(self) -> Style {
        self.bad()
    }

    /// Hints, and what is secondary.
    #[must_use]
    pub fn dim(self) -> Style {
        match self.palette {
            Some(p) => Style::default().fg(p.muted),
            None => Style::default().add_modifier(Modifier::DIM),
        }
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
        match self.palette {
            Some(p) => Style::default()
                .fg(p.instance)
                .add_modifier(Modifier::ITALIC),
            None => Style::default().add_modifier(Modifier::ITALIC),
        }
    }

    /// What is drawn over a piece of a row that is an instance: dimmer.
    #[must_use]
    pub fn faded(self, style: Style) -> Style {
        match self.palette {
            Some(p) => style.fg(p.instance),
            None => style.add_modifier(Modifier::DIM),
        }
    }

    /// The type of a field.
    #[must_use]
    pub fn type_name(self) -> Style {
        self.pick(|p| p.type_, Color::Blue)
    }

    /// A value, by its kind.
    #[must_use]
    pub fn value(self, shown: &Shown) -> Style {
        match shown {
            Shown::UInt(_) | Shown::SInt(_) | Shown::Float(_) => {
                self.pick(|p| p.number, Color::Yellow)
            }
            Shown::Enum { .. } => self.pick(|p| p.enum_, Color::Magenta),
            Shown::Bool(_) => self.pick(|p| p.boolean, Color::Cyan),
            Shown::Str { .. } => self.pick(|p| p.string, Color::Green),
            Shown::Bytes { .. } => self.pick(|p| p.bytes, Color::Red),
        }
    }

    /// A byte that is wholly in the selected field.
    #[must_use]
    pub fn hex_whole(self) -> Style {
        match self.palette {
            Some(p) => Style::default()
                .fg(p.foreground)
                .bg(p.selection)
                .add_modifier(Modifier::BOLD),
            None => self.colored(Color::Cyan).add_modifier(Modifier::REVERSED),
        }
    }

    /// A byte that is partly in the selected field (the other bits are a neighbour's).
    #[must_use]
    pub fn hex_part(self) -> Style {
        match self.palette {
            Some(p) => Style::default()
                .fg(p.foreground)
                .bg(p.selection_part)
                .add_modifier(Modifier::UNDERLINED),
            None => self.colored(Color::Cyan).add_modifier(Modifier::UNDERLINED),
        }
    }

    /// The byte the cursor is on, in the hex pane when it has the focus.
    #[must_use]
    pub fn hex_cursor(self) -> Style {
        let style = match self.palette {
            Some(p) => Style::default().bg(p.accent).fg(p.background),
            None if self.color => Style::default().bg(Color::Yellow).fg(Color::Black),
            None => Style::default(),
        };
        let modifiers = if self.palette.is_some() {
            Modifier::BOLD | Modifier::UNDERLINED
        } else {
            Modifier::REVERSED | Modifier::BOLD | Modifier::UNDERLINED
        };
        style.add_modifier(modifiers)
    }

    /// The border of a pane: bold when it has the focus.
    #[must_use]
    pub fn border(self, focused: bool) -> Style {
        match (self.palette, focused) {
            (Some(p), true) => Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
            (Some(p), false) => Style::default().fg(p.line),
            (None, true) => self.colored(Color::Cyan).add_modifier(Modifier::BOLD),
            (None, false) => Style::default().add_modifier(Modifier::DIM),
        }
    }

    /// The status bar: reversed, like the selection.
    #[must_use]
    pub fn status(self) -> Style {
        match self.palette {
            Some(p) => Style::default().fg(p.foreground).bg(p.panel),
            None => Style::default().add_modifier(Modifier::REVERSED),
        }
    }

    /// The selected row: reversed, which shows without any colour.
    #[must_use]
    pub fn selected(self) -> Style {
        match self.palette {
            Some(p) => Style::default().bg(p.selection),
            None => Style::default().add_modifier(Modifier::REVERSED),
        }
    }

    /// Something that is odd and not wrong.
    #[must_use]
    pub fn warning(self) -> Style {
        self.pick(|p| p.warning, Color::Yellow)
    }

    /// The offsets in the bytes.
    #[must_use]
    pub fn offset(self) -> Style {
        match self.palette {
            Some(p) => Style::default().fg(p.offset),
            None => self.dim(),
        }
    }
}

/// Blank `area` for something drawn over the screen: with the background of the theme, if it has one.
fn clear(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    frame.render_widget(Clear, area);
    if theme.palette().is_some() {
        frame.render_widget(Block::default().style(theme.base()), area);
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
    // the background of a theme is under everything
    if theme.palette().is_some() {
        frame.render_widget(Block::default().style(theme.base()), area);
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

    let fatal = app.report.as_ref().is_some_and(|r| r.fatal);
    let status = if app.loading {
        Span::styled(" parsing… ", theme.title())
    } else if fatal {
        Span::styled(" cannot be parsed ", theme.bad())
    } else {
        match (&app.error, &app.report) {
            (Some(_), _) => Span::styled(" the parse failed ", theme.bad()),
            (None, Some(_)) => Span::styled(" could not read again ", theme.bad()),
            (None, None) => Span::styled(" parsed ", theme.good()),
        }
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
    // What went wrong is where the tree is (nothing was parsed), or under it.
    let mut tree_inner = inner;
    if app.loading {
        // the file is still being parsed: there is no tree yet
        let seconds = app.loading_since.map_or(0, |t| t.elapsed().as_secs());
        let text = vec![
            Line::from(Span::styled(
                format!(
                    "parsing {} as {}… {seconds} s",
                    format::safe(&app.file.display().to_string()),
                    format::safe(&app.type_name)
                ),
                theme.title(),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "the tree comes when it is done; q leaves",
                theme.dim(),
            )),
        ];
        frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), inner);
        tree_inner = Rect { height: 0, ..inner };
    } else if let Some(report) = &app.report {
        let lines = report_lines(report, *theme);
        if report.fatal {
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
            tree_inner = Rect { height: 0, ..inner };
        } else if app.show_report && inner.height >= REPORT_MIN_HEIGHT {
            let rows = estimate_rows(&lines, inner.width)
                .min(REPORT_MAX_ROWS)
                .min(inner.height / 2);
            let [top, bottom] =
                Layout::vertical([Constraint::Min(0), Constraint::Length(rows + 1)]).areas(inner);
            let mut panel = vec![Line::from(Span::styled(
                format::truncate("── error (e hides) ", usize::from(bottom.width)),
                theme.bad(),
            ))];
            panel.extend(lines);
            frame.render_widget(Paragraph::new(panel).wrap(Wrap { trim: false }), bottom);
            tree_inner = top;
        }
    }
    // With no room for the bytes, the inspector takes the place of the tree.
    if fatal || app.loading {
        // the report, or the message that it is being read, is all there is to see in this pane
    } else if app.inspector && hex_area.is_none() {
        inspector(frame, tree_inner, app, *theme);
    } else {
        tree_view::render(frame, tree_inner, &app.tree, &mut app.view, theme);
    }

    if let Some(hex_area) = hex_area {
        let size = app.bytes.as_ref().map_or(0, crate::bytes::FileBytes::size);
        let described = if app.loading {
            "the file is being parsed".to_owned()
        } else {
            hex_view::describe(app.tree.selected(), hex_view::offset_digits(size))
        };
        let note = format::truncate(&described, usize::from(hex_area.width.saturating_sub(4)));
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

    // A notice shows on the last line while there is no bar for it.
    if area_has_no_bar(frame.area())
        && let Some(notice) = &app.notice
    {
        let row = Rect {
            y: frame.area().bottom().saturating_sub(1),
            height: 1,
            ..frame.area()
        };
        clear(frame, row, *theme);
        frame.render_widget(
            Paragraph::new(format::truncate(notice, usize::from(row.width))).style(theme.status()),
            row,
        );
    }
    if let Some(prompt) = &app.prompt {
        prompt_line(frame, prompt, *theme);
    }
    if app.help {
        help(frame, *theme);
    }
}

/// Whether the screen is too short for the status bar.
fn area_has_no_bar(area: Rect) -> bool {
    area.height < STATUS_MIN_HEIGHT
}

/// The line being typed, on the last row of the screen, with the cursor in it.
fn prompt_line(frame: &mut Frame<'_>, prompt: &Prompt, theme: Theme) {
    let area = frame.area();
    if area.height == 0 || area.width == 0 {
        return;
    }
    let row = Rect {
        y: area.bottom() - 1,
        height: 1,
        ..area
    };
    let label = prompt.kind.label();
    let width = usize::from(row.width);
    // keep the cursor in view: scroll the text left when the line is longer than the row
    let room = width.saturating_sub(format::width(label) + 1).max(1);
    let cursor = prompt.line.cursor_cells();
    let skip_cells = cursor.saturating_sub(room.saturating_sub(1));
    let mut shown = String::new();
    let mut used = 0;
    let mut skipped = 0;
    for c in prompt.line.text().chars() {
        let w = format::width(c.encode_utf8(&mut [0; 4]));
        if skipped < skip_cells {
            skipped += w;
            continue;
        }
        if used + w > room {
            break;
        }
        shown.push(c);
        used += w;
    }
    clear(frame, row, theme);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(label, theme.title()),
            Span::raw(shown),
        ])),
        row,
    );
    let x = row.x + u16::try_from(format::width(label) + cursor - skipped.min(cursor)).unwrap_or(0);
    frame.set_cursor_position((x.min(row.right().saturating_sub(1)), row.y));
}

/// The help: every key, in a box over the screen.
fn help(frame: &mut Frame<'_>, theme: Theme) {
    let screen = frame.area();
    if screen.width < 10 || screen.height < 4 {
        return;
    }
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (title, bindings) in keymap::help() {
        lines.push(Line::from(Span::styled(title, theme.title())));
        for (keys, what) in bindings {
            lines.push(Line::from(vec![
                Span::styled(format!("  {keys:<14}"), theme.name()),
                Span::raw(what),
            ]));
        }
        lines.push(Line::raw(""));
    }
    lines.push(Line::from(Span::styled(
        "In a prompt (/ and g)",
        theme.title(),
    )));
    for (keys, what) in keymap::PROMPT_HELP {
        lines.push(Line::from(vec![
            Span::styled(format!("  {keys:<14}"), theme.name()),
            Span::raw(*what),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "any key closes this help",
        theme.dim(),
    )));

    let wanted = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let width = screen.width.min(100);
    let height = screen.height.min(wanted);
    let area = Rect {
        x: screen.x + (screen.width - width) / 2,
        y: screen.y + (screen.height - height) / 2,
        width,
        height,
    };
    clear(frame, area, theme);
    let truncated = usize::from(height.saturating_sub(2)) < lines.len();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border(true))
        .title(Span::styled(" keys ", theme.title()))
        .title_bottom(Line::from(Span::styled(
            if truncated {
                " the terminal is too small for all of the help "
            } else {
                " any key closes "
            },
            theme.dim(),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// The shortest pane that has the message about a failed parse under the tree.
const REPORT_MIN_HEIGHT: u16 = 10;
/// The most rows of that message.
const REPORT_MAX_ROWS: u16 = 8;

/// The lines of a report: what happened, what caused it, and where in the schema.
fn report_lines(report: &Report, theme: Theme) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        report.headline.clone(),
        theme.bad(),
    ))];
    for (i, cause) in report.causes.iter().enumerate() {
        let text = if i == 0 {
            cause.clone()
        } else {
            format!("caused by: {cause}")
        };
        lines.push(Line::from(Span::raw(text)));
    }
    if let Some(place) = &report.place {
        lines.push(Line::from(Span::styled(
            format!("{}:{}:{}", place.file, place.line, place.column),
            theme.title(),
        )));
        lines.push(Line::from(Span::raw(place.source_line.clone())));
        lines.push(Line::from(Span::styled(
            format!("{}^", " ".repeat(place.caret_cells)),
            theme.bad(),
        )));
    }
    if report.fatal {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "the bytes of the file are shown if it can be read; q leaves",
            theme.dim(),
        )));
    }
    lines
}

/// How many rows `lines` take when they wrap at `width` cells.
fn estimate_rows(lines: &[Line<'_>], width: u16) -> u16 {
    let width = usize::from(width.max(1));
    let rows: usize = lines.iter().map(|l| l.width().div_ceil(width).max(1)).sum();
    u16::try_from(rows).unwrap_or(u16::MAX)
}

/// The bar at the bottom: the field on the left, the file on the right.
fn status_bar(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let names = app.tree.names_to(app.tree.selected_path());
    let file_name = app.file.file_name().map_or_else(
        || app.file.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let (mut left, right) = status::bar(app.tree.selected(), &names, &file_name, app.size);
    if let Some(search) = &app.search {
        left = format!(
            "searching for \"{}\"… {} fields looked at (any key stops)",
            format::safe(search.needle().query()),
            search.visited()
        );
    } else if let Some(notice) = &app.notice {
        left.clone_from(notice);
    }
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

    fn draw_app(app: &mut App, width: u16, height: u16) -> (Vec<String>, Terminal<TestBackend>) {
        let theme = Theme::new(false);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw(f, app, &theme)).unwrap();
        let lines = rows(&terminal);
        (lines, terminal)
    }

    #[test]
    fn the_prompt_is_the_last_line_with_the_cursor_in_it() {
        let (mut app, _dir) = app();
        app.prompt = Some(Prompt {
            kind: crate::prompt::Kind::Search,
            line: crate::prompt::LineEditor::with_text("abc"),
        });
        let (lines, mut terminal) = draw_app(&mut app, 100, 10);
        assert!(lines.last().unwrap().starts_with("/abc"), "{lines:?}");
        let cursor = terminal.get_cursor_position().unwrap();
        assert_eq!((cursor.x, cursor.y), (4, 9));

        // the cursor is where it is in the text, wide characters count two cells
        let mut line = crate::prompt::LineEditor::with_text("日本");
        line.left();
        app.prompt = Some(Prompt {
            kind: crate::prompt::Kind::Goto,
            line,
        });
        let (lines, mut terminal) = draw_app(&mut app, 100, 10);
        assert!(
            lines.last().unwrap().starts_with("goto offset: 日本"),
            "{lines:?}"
        );
        let cursor = terminal.get_cursor_position().unwrap();
        assert_eq!(cursor.x, 13 + 2);

        // a line longer than the screen scrolls, and the cursor stays on the screen
        app.prompt = Some(Prompt {
            kind: crate::prompt::Kind::Search,
            line: crate::prompt::LineEditor::with_text(&"x".repeat(200)),
        });
        let (lines, mut terminal) = draw_app(&mut app, 40, 10);
        assert!(lines.last().unwrap().starts_with("/xxx"));
        let cursor = terminal.get_cursor_position().unwrap();
        assert!(cursor.x < 40 && cursor.y == 9, "{cursor:?}");

        // no prompt: no cursor to show, and the bar is back
        app.prompt = None;
        let (lines, _) = draw_app(&mut app, 100, 10);
        assert!(lines.last().unwrap().contains("t.bin"), "{lines:?}");
    }

    #[test]
    fn the_prompt_and_the_notice_are_there_on_a_screen_too_short_for_the_bar() {
        let (mut app, _dir) = app();
        app.notice = Some("not found: zzz".to_owned());
        let (lines, _) = draw_app(&mut app, 60, 5);
        assert!(
            lines.last().unwrap().contains("not found: zzz"),
            "{lines:?}"
        );
        app.prompt = Some(Prompt {
            kind: crate::prompt::Kind::Goto,
            line: crate::prompt::LineEditor::with_text("0x10"),
        });
        let (lines, _) = draw_app(&mut app, 60, 5);
        assert!(
            lines.last().unwrap().starts_with("goto offset: 0x10"),
            "{lines:?}"
        );
    }

    #[test]
    fn the_bar_shows_a_notice_and_the_progress_of_a_search() {
        let (mut app, _dir) = app();
        app.notice = Some("search wrapped around".to_owned());
        let (lines, _) = draw_app(&mut app, 100, 10);
        assert!(
            lines.last().unwrap().starts_with(" search wrapped around"),
            "{lines:?}"
        );
        app.notice = None;
        app.search = Some(crate::search::Search::new(
            crate::search::Needle::new("beta").unwrap(),
            true,
            Vec::new(),
        ));
        let (lines, _) = draw_app(&mut app, 100, 10);
        assert!(
            lines
                .last()
                .unwrap()
                .contains("searching for \"beta\"… 0 fields looked at (any key stops)"),
            "{lines:?}"
        );
    }

    #[test]
    fn the_help_lists_the_keys_and_fits_or_says_that_it_does_not() {
        let (mut app, _dir) = app();
        app.help = true;
        let (lines, _) = draw_app(&mut app, 100, 50);
        for needle in [
            "keys",
            "Anywhere",
            "In the tree",
            "In the bytes",
            "In a prompt",
            "Ctrl-C",
            "go to an offset",
            "search the names",
            "any key closes",
        ] {
            assert!(
                has(&lines, needle),
                "{needle} is not in the help: {lines:?}"
            );
        }
        // every line of the table is on the screen of a tall terminal
        for (_, bindings) in crate::keymap::help() {
            for (keys, what) in bindings {
                assert!(has(&lines, &format!("{keys:<14}{what}")), "{keys}: {what}");
            }
        }
        let (lines, _) = draw_app(&mut app, 60, 12);
        assert!(has(&lines, "too small for all of the help"), "{lines:?}");
        for (w, h) in [(9, 4), (10, 3), (3, 3), (1, 1), (100, 4)] {
            let _ = draw_app(&mut app, w, h);
        }
    }

    #[test]
    fn a_parse_that_fails_in_a_nested_type_is_shown_with_the_failed_row_and_the_message() {
        let schema = "id: rec\nseq:\n  - id: head\n    type: u1\n  - id: body\n    type: pair\ntypes:\n  pair:\n    seq:\n      - id: x\n        type: u1\n      - id: y\n        type: u2le\n";
        let (mut app, _dir) = crate::app::tests::app_for(schema, &[1, 5, 9]);
        let (lines, _) = draw_app(&mut app, 100, 16);
        // the tree up to the failure, the failed row marked and selected
        let failed = lines
            .iter()
            .find(|l| l.contains('✗'))
            .expect("a failed row");
        assert!(failed.contains('y'), "{failed}");
        assert!(has(&lines, "x = 5"), "what was read before it: {lines:?}");
        // the message under the tree, with the field and the type
        assert!(has(&lines, "── error (e hides)"), "{lines:?}");
        // (the headline wraps in the narrow pane)
        assert!(has(&lines, "the parse failed at rec/body/y"), "{lines:?}");
        assert!(has(&lines, "rec::pair)"), "{lines:?}");
        assert!(
            has(&lines, "buffer overrun"),
            "the message of the error: {lines:?}"
        );
        assert!(
            has(
                &lines,
                " rec/body/y · not read · the parse failed here, at byte 0x2"
            ),
            "the bar: {lines:?}"
        );
        // the bytes are at the field, and the bar says where
        assert!(has(&lines, "the parse failed"), "the title says so");
        assert!(
            lines[0].contains("y: the parse failed here, at byte 0x00000002"),
            "the title of the bytes names the field: {lines:?}"
        );
        // e hides the message, and the tree has the room back
        app.show_report = false;
        let (lines, _) = draw_app(&mut app, 100, 16);
        assert!(!has(&lines, "── error"), "{lines:?}");
    }

    #[test]
    fn a_parse_that_fails_in_a_repeated_field() {
        let schema = "id: rec\nseq:\n  - id: count\n    type: u1\n  - id: items\n    type: u2le\n    repeat: expr\n    repeat-expr: count\n";
        let (mut app, _dir) = crate::app::tests::app_for(schema, &[3, 1, 0, 2, 0, 9]);
        let (lines, _) = draw_app(&mut app, 100, 16);
        assert!(
            has(&lines, "the parse failed at rec/items/[2]"),
            "{lines:?}"
        );
        let marked: Vec<&String> = lines.iter().filter(|l| l.contains('✗')).collect();
        assert_eq!(marked.len(), 1, "{lines:?}");
        assert!(marked[0].contains("[2]"));
        assert!(
            has(&lines, "[1] = 2"),
            "the elements before are there: {lines:?}"
        );
    }

    #[test]
    fn a_schema_that_does_not_compile_is_shown_with_its_place_and_the_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let schema = dir.path().join("t.yaml");
        std::fs::write(&schema, "id: bad\nseq:\n  - id: a\n    type: nope\n").unwrap();
        let file = dir.path().join("t.bin");
        std::fs::write(&file, [1, 2, 3]).unwrap();
        let options = crate::Options {
            file: file.clone(),
            type_or_schema: schema.to_string_lossy().into_owned(),
            include_paths: Vec::new(),
            color: false,
            watch: false,
            theme: None,
            theme_dir: None,
        };
        let failure = crate::load::try_load(&options).unwrap_err();
        let mut app = App::broken(file, &failure, Some(&schema));
        let (lines, _) = draw_app(&mut app, 110, 18);
        assert!(has(&lines, "cannot be parsed"), "{lines:?}");
        assert!(has(&lines, "the schema bad cannot be used"), "{lines:?}");
        assert!(has(&lines, "nope"), "the error says which type: {lines:?}");
        assert!(lines.iter().any(|l| l.contains("t.yaml:4:11")), "{lines:?}");
        // the line of the schema and a caret under the place
        let source = lines
            .iter()
            .position(|l| l.contains("    type: nope"))
            .expect("the line");
        let caret = &lines[source + 1];
        assert!(caret.contains('^'), "{lines:?}");
        let column = |line: &str, needle: char| line.chars().position(|c| c == needle).unwrap();
        assert_eq!(
            column(caret, '^'),
            column(&lines[source], 'n'),
            "the caret is under `nope`"
        );
        // there is no tree, and the bytes of the file are there
        assert!(!has(&lines, "▾"));
        assert!(has(&lines, "00000000  01 02 03"), "{lines:?}");
        assert!(has(&lines, "the bytes of the file are shown"));
    }

    #[test]
    fn a_missing_file_and_an_unknown_type_are_shown() {
        let dir = tempfile::tempdir().unwrap();
        let schema = dir.path().join("t.yaml");
        std::fs::write(&schema, "id: ok\nseq:\n  - id: a\n    type: u1\n").unwrap();
        let options = crate::Options {
            file: dir.path().join("missing.bin"),
            type_or_schema: schema.to_string_lossy().into_owned(),
            include_paths: Vec::new(),
            color: false,
            watch: false,
            theme: None,
            theme_dir: None,
        };
        let failure = crate::load::try_load(&options).unwrap_err();
        let mut app = App::broken(options.file.clone(), &failure, Some(&schema));
        let (lines, _) = draw_app(&mut app, 110, 18);
        assert!(has(&lines, "the file cannot be read"), "{lines:?}");
        assert!(has(&lines, "could not read file"), "{lines:?}");
        assert!(
            has(&lines, "No such file") || has(&lines, "caused by"),
            "{lines:?}"
        );
        assert!(
            has(&lines, "could not open the file"),
            "no bytes to show: {lines:?}"
        );

        let unknown = crate::Options {
            type_or_schema: "no_such_type".to_owned(),
            ..options
        };
        let failure = crate::load::try_load(&unknown).unwrap_err();
        let mut app = App::broken(unknown.file.clone(), &failure, None);
        let (lines, _) = draw_app(&mut app, 110, 18);
        assert!(
            has(&lines, "the schema no_such_type cannot be used"),
            "{lines:?}"
        );
        assert!(has(&lines, "schema not found"), "{lines:?}");
    }

    #[test]
    fn a_report_is_drawn_on_small_screens_without_panicking() {
        let schema = "id: rec\nseq:\n  - id: head\n    type: u1\n  - id: body\n    type: u2le\n";
        for (w, h) in [(100, 16), (60, 12), (40, 10), (30, 6), (12, 4), (8, 3)] {
            let (mut app, _dir) = crate::app::tests::app_for(schema, &[1]);
            let _ = draw_app(&mut app, w, h);
            let dir = tempfile::tempdir().unwrap();
            let options = crate::Options {
                file: dir.path().join("nothing"),
                type_or_schema: "nothing".to_owned(),
                include_paths: Vec::new(),
                color: false,
                watch: false,
                theme: None,
                theme_dir: None,
            };
            let failure = crate::load::try_load(&options).unwrap_err();
            let mut app = App::broken(options.file.clone(), &failure, None);
            let _ = draw_app(&mut app, w, h);
        }
    }

    #[test]
    fn while_the_file_is_parsed_the_screen_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("big.bin");
        std::fs::write(&file, [1, 2, 3]).unwrap();
        let mut app = App::loading(file, "my_format");
        let (lines, _) = draw_app(&mut app, 100, 12);
        assert!(has(&lines, "parsing"), "{lines:?}");
        assert!(has(&lines, "my_format… 0 s"), "{lines:?}");
        assert!(has(&lines, "big.bin"), "{lines:?}");
        assert!(has(&lines, "the file is being parsed"), "{lines:?}");
        assert!(has(&lines, "q leaves"), "{lines:?}");
        // the bytes are there already: the file is read as they are shown, not parsed
        assert!(has(&lines, "00000000  01 02 03"), "{lines:?}");
        assert!(!has(&lines, "▾"), "there is no tree yet");
        for (w, h) in [(30, 8), (12, 4), (8, 3)] {
            let _ = draw_app(&mut app, w, h);
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
