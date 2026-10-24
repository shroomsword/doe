//! The hex pane: offsets, hex and text of the bytes of the file, with the bytes
//! of the selected field marked down to the bit. Only the lines in view are
//! read from the file.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::bytes::FileBytes;
use crate::tree::Row;
use crate::ui::Theme;

/// The widest line is 32 bytes; the choices of bytes per line, widest first.
const LINE_CHOICES: [usize; 6] = [32, 16, 8, 4, 2, 1];

/// Whether a byte is in the selected range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cover {
    /// Not in it.
    No,
    /// All 8 bits are.
    Whole,
    /// Some of the bits are; the others belong to a neighbouring field.
    Part,
}

/// The bits of the selected field, from `start` up to `end` (not included).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// The first bit, from the start of the file.
    pub start: u64,
    /// The bit after the last one.
    pub end: u64,
}

impl Selection {
    /// The selection of a row: none for an instance that is computed, a field
    /// that is empty, or one read from bytes that a transform made (its
    /// offsets are not the file's).
    #[must_use]
    pub fn of(row: &Row) -> Option<Selection> {
        let span = row.span?;
        if row.processed || span.is_empty() {
            return None;
        }
        Some(Selection {
            start: span.start,
            end: span.end,
        })
    }

    /// How much of the byte at `offset` is selected.
    #[must_use]
    pub fn cover(&self, offset: u64) -> Cover {
        let first = offset.saturating_mul(8);
        let last = first.saturating_add(8);
        if self.end <= first || self.start >= last {
            Cover::No
        } else if self.start <= first && self.end >= last {
            Cover::Whole
        } else {
            Cover::Part
        }
    }

    /// The offset of the first byte.
    #[must_use]
    pub fn first_byte(&self) -> u64 {
        self.start / 8
    }
}

/// Where the hex pane is, between drawings.
#[derive(Debug, Clone, Copy)]
pub struct HexState {
    /// An offset on the first line shown.
    top: u64,
    /// The byte the cursor is on (it is in the pane when the pane has the focus).
    cursor: u64,
    /// Bytes per line, and lines, as of the last drawing.
    bytes_per_line: usize,
    lines: usize,
}

impl Default for HexState {
    fn default() -> Self {
        HexState {
            top: 0,
            cursor: 0,
            bytes_per_line: 16,
            lines: 16,
        }
    }
}

impl HexState {
    /// A pane at the start of the file.
    #[must_use]
    pub fn new() -> Self {
        HexState::default()
    }

    /// The byte the cursor is on.
    #[must_use]
    pub fn cursor(&self) -> u64 {
        self.cursor
    }

    /// An offset on the first line shown.
    #[must_use]
    pub fn top(&self) -> u64 {
        self.line_start(self.top)
    }

    /// Bytes per line, as of the last drawing.
    #[must_use]
    pub fn bytes_per_line(&self) -> usize {
        self.bytes_per_line
    }

    /// Lines shown, as of the last drawing (for the page keys).
    #[must_use]
    pub fn page_lines(&self) -> usize {
        self.lines.max(1)
    }

    /// The offset of the first byte of the line that holds `offset`.
    fn line_start(&self, offset: u64) -> u64 {
        let width = self.bytes_per_line.max(1) as u64;
        offset - offset % width
    }

    /// Put the cursor on `offset` (the last byte of a file of `size` at most)
    /// and scroll as little as needed to see it.
    pub fn set_cursor(&mut self, offset: u64, size: u64) {
        self.cursor = offset.min(size.saturating_sub(1));
        self.follow_cursor();
    }

    /// Move the cursor by `delta` bytes, staying in the file.
    pub fn move_by(&mut self, delta: i64, size: u64) {
        let moved = if delta < 0 {
            self.cursor.saturating_sub(delta.unsigned_abs())
        } else {
            self.cursor.saturating_add(delta.unsigned_abs())
        };
        self.set_cursor(moved, size);
    }

    /// Scroll as little as needed for the cursor to be in view.
    pub fn follow_cursor(&mut self) {
        let cursor_line = self.line_start(self.cursor);
        let top = self.line_start(self.top);
        let bytes = self.bytes_per_line.max(1) as u64;
        let lines = self.lines.max(1) as u64;
        if cursor_line < top {
            self.top = cursor_line;
        } else if cursor_line >= top + lines * bytes {
            self.top = cursor_line + bytes - lines * bytes;
        }
    }

    /// Show the byte at `offset`: nothing changes if it is in view, else it is
    /// put in the middle. The cursor goes to it.
    pub fn reveal(&mut self, offset: u64, size: u64) {
        self.cursor = offset.min(size.saturating_sub(1));
        let line = self.line_start(self.cursor);
        let top = self.line_start(self.top);
        let bytes = self.bytes_per_line.max(1) as u64;
        let lines = self.lines.max(1) as u64;
        // a margin of a line at each end, so that the field is not at the edge
        let margin = u64::from(lines > 4);
        if line < top + margin * bytes || line >= top + (lines - margin) * bytes {
            self.top = line.saturating_sub((lines / 2) * bytes);
        }
    }
}

/// How many bytes per line fit in `width` cells with an offset column of
/// `digits` digits: `offset  hex hex … hex  text`, hex in groups of 8.
#[must_use]
pub fn bytes_per_line(width: usize, digits: usize) -> usize {
    for n in LINE_CHOICES {
        let needed = digits + 2 + n * 3 + (n / 8).saturating_sub(1) + 1 + n;
        if needed <= width {
            return n;
        }
    }
    1
}

/// The digits of the offset column: 8, or 12 for a file past 4 GiB.
#[must_use]
pub fn offset_digits(size: u64) -> usize {
    if size > u64::from(u32::MAX) { 12 } else { 8 }
}

/// What the pane's title says about the selected field.
#[must_use]
pub fn describe(row: &Row, digits: usize) -> String {
    let name = crate::format::safe(&row.name);
    if row.processed {
        return format!("{name}: not bytes of the file (made by process:)");
    }
    let Some(span) = row.span else {
        if row.failed {
            return format!("{name}: the parse failed here");
        }
        return format!("{name}: computed, not in the file");
    };
    if row.failed && span.is_empty() {
        return format!(
            "{name}: the parse failed here, at byte {:#0w$x}",
            span.start / 8,
            w = digits + 2
        );
    }
    if span.is_empty() {
        return format!("{name}: no bytes");
    }
    let first = span.start / 8;
    let last = (span.end - 1) / 8;
    let aligned = span.start % 8 == 0 && span.end % 8 == 0;
    let bytes = if first == last {
        format!("byte {first:#0w$x}", w = digits + 2)
    } else {
        format!("bytes {first:#0w$x}..{last:#0w$x}", w = digits + 2)
    };
    if aligned {
        format!("{name}: {bytes}")
    } else {
        format!("{name}: {bytes}, bits {}..{}", span.start, span.end)
    }
}

fn ascii(byte: u8) -> char {
    if (0x20..0x7f).contains(&byte) {
        char::from(byte)
    } else {
        '.'
    }
}

/// What a byte looks like, by how it is selected and whether the cursor is on it.
fn byte_style(cover: Cover, cursor: bool, theme: Theme) -> Style {
    let mut style = match cover {
        Cover::No => Style::default(),
        Cover::Whole => theme.hex_whole(),
        Cover::Part => theme.hex_part(),
    };
    if cursor {
        style = style.patch(theme.hex_cursor());
    }
    style
}

/// One line of the pane: `offset`, then the bytes of `data` (fewer than
/// `per_line` at the end of the file) as hex and as text.
#[must_use]
pub fn hex_line(
    offset: u64,
    data: &[u8],
    per_line: usize,
    digits: usize,
    selection: Option<&Selection>,
    cursor: Option<u64>,
    theme: &Theme,
) -> Line<'static> {
    let cover = |index: usize| -> Cover {
        let at = offset + index as u64;
        selection.map_or(Cover::No, |s| s.cover(at))
    };
    let mut spans = vec![
        Span::styled(format!("{offset:0digits$x}"), theme.dim()),
        Span::raw("  "),
    ];
    for index in 0..per_line {
        let byte = data.get(index);
        let at = offset + index as u64;
        let here = cover(index);
        let style = byte_style(here, cursor == Some(at) && byte.is_some(), *theme);
        spans.push(match byte {
            Some(b) => Span::styled(format!("{b:02x}"), style),
            None => Span::raw("  "),
        });
        if index + 1 < per_line {
            // a selection is continuous across the space between its bytes
            let next = cover(index + 1);
            let joined =
                here == Cover::Whole && next == Cover::Whole && data.get(index + 1).is_some();
            let gap = if (index + 1) % 8 == 0 { "  " } else { " " };
            spans.push(if joined {
                Span::styled(gap, theme.hex_whole())
            } else {
                Span::raw(gap)
            });
        }
    }
    spans.push(Span::raw("  "));
    for index in 0..per_line {
        match data.get(index) {
            Some(&b) => {
                let at = offset + index as u64;
                let style = byte_style(cover(index), cursor == Some(at), *theme);
                spans.push(Span::styled(ascii(b).to_string(), style));
            }
            None => spans.push(Span::raw(" ")),
        }
    }
    Line::from(spans)
}

/// Draw the lines that are in `area`, reading only their bytes from `bytes`.
pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    bytes: &mut FileBytes,
    state: &mut HexState,
    selected: Option<&Row>,
    focused: bool,
    theme: &Theme,
) {
    let size = bytes.size();
    let digits = offset_digits(size);
    let per_line = bytes_per_line(usize::from(area.width), digits);
    // a change of width keeps the first line where it was, to the line
    state.bytes_per_line = per_line;
    state.lines = usize::from(area.height);
    state.top = state.line_start(state.top);
    state.follow_cursor();
    if area.width == 0 || area.height == 0 {
        return;
    }

    let selection = selected.and_then(Selection::of);
    let mut lines = Vec::new();
    for line in 0..u64::from(area.height) {
        let offset = state.top.saturating_add(line * per_line as u64);
        if offset >= size.max(1) && line > 0 {
            break;
        }
        let data = bytes.window(offset, per_line).to_vec();
        lines.push(hex_line(
            offset,
            &data,
            per_line,
            digits,
            selection.as_ref(),
            focused.then_some(state.cursor),
            theme,
        ));
    }
    if let Some(error) = bytes.error() {
        lines.push(Line::from(Span::styled(
            format!("could not read the file: {}", crate::format::safe(error)),
            theme.bad(),
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);
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
    use crate::Options;
    use crate::load::load;
    use crate::tree::TreeModel;

    /// A model, the bytes of its file, and the directory that holds it.
    fn setup(schema: &str, data: &[u8]) -> (TreeModel, FileBytes, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let schema_path = dir.path().join("t.yaml");
        std::fs::write(&schema_path, schema).unwrap();
        let file = dir.path().join("t.bin");
        std::fs::write(&file, data).unwrap();
        let loaded = load(&Options {
            file: file.clone(),
            type_or_schema: schema_path.to_string_lossy().into_owned(),
            include_paths: Vec::new(),
            color: false,
        })
        .unwrap();
        let bytes = FileBytes::open(&file).unwrap();
        (TreeModel::new(loaded.trace, loaded.value), bytes, dir)
    }

    fn text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    /// Which bytes of a line are drawn reversed (whole) or underlined (part).
    fn marks(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .filter(|s| s.content.len() == 2 && s.content.chars().all(|c| c.is_ascii_hexdigit()))
            .map(|s| {
                if s.style.add_modifier.contains(Modifier::REVERSED) {
                    'W'
                } else if s.style.add_modifier.contains(Modifier::UNDERLINED) {
                    'p'
                } else {
                    '.'
                }
            })
            .collect()
    }

    fn drawn(
        bytes: &mut FileBytes,
        state: &mut HexState,
        row: Option<&Row>,
        focused: bool,
        w: u16,
        h: u16,
    ) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal
            .draw(|f| render(f, f.area(), bytes, state, row, focused, &Theme::new(false)))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn row_text(buffer: &Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol().to_owned())
            .collect::<String>()
            .trim_end()
            .to_owned()
    }

    #[test]
    fn a_line_has_the_offset_the_hex_and_the_text() {
        let line = hex_line(
            0x10,
            b"Hello, doe!\x00\x01",
            16,
            8,
            None,
            None,
            &Theme::new(false),
        );
        assert_eq!(
            text(&line),
            "00000010  48 65 6c 6c 6f 2c 20 64  6f 65 21 00 01           Hello, doe!..   "
        );
        // the last line of a file has blanks in place of the missing bytes
        let short = hex_line(0, &[0x41, 0x42], 8, 8, None, None, &Theme::new(false));
        assert_eq!(text(&short), "00000000  41 42                    AB      ");
    }

    #[test]
    fn the_bits_of_a_selection_say_whole_or_part() {
        // bits 4..12: the low half of byte 0 and the high half of byte 1... (bits count from the start)
        let sel = Selection { start: 4, end: 12 };
        assert_eq!(sel.cover(0), Cover::Part);
        assert_eq!(sel.cover(1), Cover::Part);
        assert_eq!(sel.cover(2), Cover::No);
        let whole = Selection { start: 8, end: 24 };
        assert_eq!(
            (
                whole.cover(0),
                whole.cover(1),
                whole.cover(2),
                whole.cover(3)
            ),
            (Cover::No, Cover::Whole, Cover::Whole, Cover::No)
        );
        let one = Selection { start: 3, end: 4 };
        assert_eq!(one.cover(0), Cover::Part);
        assert_eq!(Selection { start: 0, end: 8 }.cover(0), Cover::Whole);
        assert_eq!(whole.first_byte(), 1);
    }

    const NESTED: &str = "id: t
seq:
  - id: head
    type: u1
  - id: inner
    type: pair
  - id: tail
    type: u1
types:
  pair:
    seq:
      - id: x
        type: u1
      - id: y
        type: u2le
";

    #[test]
    fn a_nested_selection_marks_the_bytes_of_the_type_and_of_a_field_inside() {
        let (mut tree, mut file, _dir) = setup(NESTED, &[1, 2, 3, 4, 5]);
        let theme = Theme::new(false);
        let data = file.window(0, 16).to_vec();
        tree.select_path(&[1]); // inner: bytes 1..4
        let sel = Selection::of(tree.selected()).unwrap();
        assert_eq!((sel.start, sel.end), (8, 32));
        let line = hex_line(0, &data, 8, 8, Some(&sel), None, &theme);
        assert_eq!(marks(&line), ".WWW.");
        tree.select_path(&[1, 1]); // inner.y: bytes 2..4
        let sel = Selection::of(tree.selected()).unwrap();
        let line = hex_line(0, &data, 8, 8, Some(&sel), None, &theme);
        assert_eq!(marks(&line), "..WW.");
    }

    #[test]
    fn a_bit_field_marks_only_part_of_its_byte() {
        let schema = "id: bits\nseq:\n  - id: high\n    type: b4\n  - id: low\n    type: b4\n  - id: next\n    type: u1\n";
        let (mut tree, mut file, _dir) = setup(schema, &[0xab, 0xcd]);
        let theme = Theme::new(false);
        let data = file.window(0, 16).to_vec();
        tree.select_path(&[0]);
        let sel = Selection::of(tree.selected()).unwrap();
        assert_eq!((sel.start, sel.end), (0, 4));
        assert_eq!(
            marks(&hex_line(0, &data, 4, 8, Some(&sel), None, &theme)),
            "p."
        );
        // the neighbour in the same byte is part of the same byte too
        tree.select_path(&[1]);
        let sel = Selection::of(tree.selected()).unwrap();
        assert_eq!(
            marks(&hex_line(0, &data, 4, 8, Some(&sel), None, &theme)),
            "p."
        );
        // and the title says which bits
        assert_eq!(
            describe(tree.selected(), 8),
            "low: byte 0x00000000, bits 4..8"
        );
        tree.select_path(&[2]);
        assert_eq!(describe(tree.selected(), 8), "next: byte 0x00000001");
    }

    #[test]
    fn a_field_placed_with_pos_is_marked_where_it_is() {
        let schema = "id: placed\nseq:\n  - id: offset\n    type: u1\ninstances:\n  later:\n    pos: offset\n    type: u2le\n";
        let (mut tree, mut file, _dir) = setup(schema, &[4, 0, 0, 0, 0x34, 0x12]);
        let theme = Theme::new(false);
        let data = file.window(0, 16).to_vec();
        tree.select_path(&[1]); // the instance, at byte 4
        assert!(tree.selected().instance);
        let sel = Selection::of(tree.selected()).unwrap();
        assert_eq!((sel.start, sel.end), (32, 48));
        assert_eq!(
            marks(&hex_line(0, &data, 8, 8, Some(&sel), None, &theme)),
            "....WW"
        );
        assert_eq!(
            describe(tree.selected(), 8),
            "later: bytes 0x00000004..0x00000005"
        );
    }

    #[test]
    fn a_processed_field_says_that_it_is_not_bytes_of_the_file() {
        let schema = "id: p\nseq:\n  - id: len\n    type: u1\n  - id: packed\n    size: 2\n    process: xor(0x55)\n    type: inside\ntypes:\n  inside:\n    seq:\n      - id: a\n        type: u1\n      - id: b\n        type: u1\n";
        let (mut tree, mut file, _dir) = setup(schema, &[2, 0x54, 0x57]);
        let theme = Theme::new(false);
        let data = file.window(0, 8).to_vec();
        tree.select_path(&[1]); // the field with the process: its offsets are the file's
        assert!(!tree.selected().processed);
        let sel = Selection::of(tree.selected()).unwrap();
        assert_eq!(
            marks(&hex_line(0, &data, 4, 8, Some(&sel), None, &theme)),
            ".WW"
        );
        // what is read from its result is in other bytes: no marks, and a note
        tree.select_path(&[1, 0]);
        assert!(tree.selected().processed, "{:?}", tree.selected());
        assert_eq!(Selection::of(tree.selected()), None);
        assert_eq!(
            describe(tree.selected(), 8),
            "a: not bytes of the file (made by process:)"
        );
        assert_eq!(marks(&hex_line(0, &data, 4, 8, None, None, &theme)), "...");
    }

    #[test]
    fn bytes_per_line_adapt_to_the_width() {
        assert_eq!(bytes_per_line(200, 8), 32);
        assert_eq!(bytes_per_line(76, 8), 16);
        assert_eq!(bytes_per_line(75, 8), 8);
        assert_eq!(bytes_per_line(43, 8), 8);
        assert_eq!(bytes_per_line(42, 8), 4);
        assert_eq!(bytes_per_line(0, 8), 1);
        assert_eq!(bytes_per_line(30, 12), 2);
        assert_eq!(offset_digits((1 << 32) - 1), 8);
        assert_eq!(offset_digits(1 << 32), 12);
    }

    #[test]
    fn the_pane_reads_only_the_lines_in_view_and_scrolls_with_the_cursor() {
        let data: Vec<u8> = (0..=255).cycle().take(4096).collect();
        let (_tree, mut file, _dir) = setup("id: s\nseq:\n  - id: a\n    type: u1\n", &data);
        let mut state = HexState::new();
        let buffer = drawn(&mut file, &mut state, None, true, 80, 4);
        assert_eq!(state.bytes_per_line(), 16);
        assert!(row_text(&buffer, 0).starts_with("00000000  00 01 02"));
        assert!(row_text(&buffer, 3).starts_with("00000030"));
        // the cursor moves past the bottom: the pane scrolls by as little as it must
        state.move_by(16 * 5, 4096);
        let buffer = drawn(&mut file, &mut state, None, true, 80, 4);
        assert!(
            row_text(&buffer, 3).starts_with("00000050"),
            "{}",
            row_text(&buffer, 3)
        );
        assert!(row_text(&buffer, 0).starts_with("00000020"));
        // back to the start, and past the ends
        state.move_by(-100_000, 4096);
        assert_eq!(state.cursor(), 0);
        state.move_by(1_000_000, 4096);
        assert_eq!(state.cursor(), 4095);
        let buffer = drawn(&mut file, &mut state, None, true, 80, 4);
        assert!(row_text(&buffer, 3).starts_with("00000ff0"));
    }

    #[test]
    fn revealing_a_field_puts_it_in_the_middle_only_when_it_was_out_of_view() {
        let data = vec![0_u8; 4096];
        let (_tree, mut file, _dir) = setup("id: s\nseq:\n  - id: a\n    type: u1\n", &data);
        let mut state = HexState::new();
        drawn(&mut file, &mut state, None, false, 80, 8);
        state.reveal(0x20, 4096);
        drawn(&mut file, &mut state, None, false, 80, 8);
        let first = row_text(&drawn(&mut file, &mut state, None, false, 80, 8), 0);
        assert!(first.starts_with("00000000"), "in view already: {first}");
        state.reveal(0x800, 4096);
        let buffer = drawn(&mut file, &mut state, None, false, 80, 8);
        let shown: Vec<String> = (0..8).map(|y| row_text(&buffer, y)).collect();
        assert!(shown.iter().any(|l| l.starts_with("00000800")), "{shown:?}");
        assert!(!shown[0].starts_with("00000800"), "{shown:?}");
        assert_eq!(state.cursor(), 0x800);
    }

    #[test]
    fn tiny_panes_and_resizes_do_not_panic() {
        let data: Vec<u8> = (0..200).collect();
        let (tree, mut file, _dir) = setup("id: s\nseq:\n  - id: a\n    type: u1\n", &data);
        let mut state = HexState::new();
        state.move_by(150, 200);
        for (w, h) in [
            (80, 24),
            (30, 5),
            (12, 3),
            (3, 3),
            (1, 1),
            (1, 30),
            (100, 1),
            (80, 24),
        ] {
            drawn(&mut file, &mut state, Some(tree.selected()), true, w, h);
        }
        // an empty file
        let (_t, mut empty, _d) = setup("id: e\nseq: []\n", &[]);
        let mut state = HexState::new();
        drawn(&mut empty, &mut state, None, true, 60, 4);
    }
}
