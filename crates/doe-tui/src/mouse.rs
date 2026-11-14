//! Where things are on the screen, for the mouse: what the last drawing put where, and which row, byte or pane a
//! cell belongs to.
//!
//! The drawing knows every rectangle, so it records them (see [`Hits`]) and nothing is worked out twice: a layout that
//! changes is followed for free. The functions here are pure, so they are tested without a terminal.

use ratatui::layout::Rect;

use crate::app::Focus;
use crate::keymap::Action;

/// The rows of a list (the tree, or the files) as they were last drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowsHit {
    /// Where the rows are, without the scrollbar.
    pub area: Rect,
    /// The index of the first row shown.
    pub first: usize,
    /// How many rows there are.
    pub len: usize,
    /// Whether the rows are the entries of an archive and not the tree of fields.
    pub files: bool,
}

/// The bytes as they were last drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HexHit {
    /// Where the lines are.
    pub area: Rect,
    /// The offset of the first line.
    pub top: u64,
    /// Bytes per line.
    pub per_line: usize,
    /// The digits of the offset column.
    pub digits: usize,
    /// The size of the file.
    pub size: u64,
}

/// Where the last drawing put what.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hits {
    /// The pane of the tree, with its border.
    pub tree_pane: Option<Rect>,
    /// The pane of the bytes, with its border.
    pub hex_pane: Option<Rect>,
    /// The rows of the tree or of the files.
    pub rows: Option<RowsHit>,
    /// The lines of bytes.
    pub hex: Option<HexHit>,
    /// The parts of the key hint on the border of the tree pane, each with the key that a click on it presses.
    pub hints: Vec<(Rect, Action)>,
}

/// What is under a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A row of the tree, or of the files, and how many cells from its left edge the cell is.
    Row {
        /// The index of the row.
        index: usize,
        /// The cell, counted from the left edge of the row.
        column: u16,
        /// Whether the rows are the entries of an archive.
        files: bool,
    },
    /// A byte of the file.
    Byte(u64),
    /// A part of the key hint: a click on it presses its key.
    Hint(Action),
    /// A pane, where nothing more exact is under the cell.
    Pane(Focus),
}

fn inside(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

impl Hits {
    /// What is under the cell at `column`, `row` (from the top left of the screen).
    #[must_use]
    pub fn target(&self, column: u16, row: u16) -> Option<Target> {
        if let Some((_, action)) = self.hints.iter().find(|(r, _)| inside(*r, column, row)) {
            return Some(Target::Hint(*action));
        }
        if let Some(rows) = self.rows.filter(|r| inside(r.area, column, row)) {
            let index = rows.first + usize::from(row - rows.area.y);
            if index < rows.len {
                return Some(Target::Row {
                    index,
                    column: column - rows.area.x,
                    files: rows.files,
                });
            }
        }
        if let Some(hex) = self.hex.filter(|h| inside(h.area, column, row))
            && let Some(offset) = hex.byte_at(column, row)
        {
            return Some(Target::Byte(offset));
        }
        if self.tree_pane.is_some_and(|p| inside(p, column, row)) {
            return Some(Target::Pane(Focus::Tree));
        }
        if self.hex_pane.is_some_and(|p| inside(p, column, row)) {
            return Some(Target::Pane(Focus::Hex));
        }
        None
    }

    /// The pane that is under the cell, if any (for the wheel).
    #[must_use]
    pub fn pane_at(&self, column: u16, row: u16) -> Option<Focus> {
        if self.tree_pane.is_some_and(|p| inside(p, column, row)) {
            Some(Focus::Tree)
        } else if self.hex_pane.is_some_and(|p| inside(p, column, row)) {
            Some(Focus::Hex)
        } else {
            None
        }
    }
}

impl HexHit {
    /// The byte that the cell at `column`, `row` shows, in the hex part or in the text part; the gaps between bytes
    /// belong to the byte before them. `None` on the offset column, past the last byte of a short line and below the
    /// end of the file.
    #[must_use]
    pub fn byte_at(&self, column: u16, row: u16) -> Option<u64> {
        if !inside(self.area, column, row) || self.per_line == 0 {
            return None;
        }
        let line = u64::from(row - self.area.y);
        let first = self
            .top
            .checked_add(line.checked_mul(self.per_line as u64)?)?;
        let x = usize::from(column - self.area.x);
        // `offset  hex hex … hex  text`: the bytes are in groups of eight, a group has one more cell after it
        let hex_start = self.digits + 2;
        let n = self.per_line;
        let hex_width = 3 * n - 1 + (n - 1) / 8;
        let index = if x < hex_start {
            return None;
        } else if x < hex_start + hex_width {
            let at = x - hex_start;
            // byte `i` starts at `3 i + i / 8`; the cells up to the next byte are its own
            (0..n).rev().find(|i| 3 * i + i / 8 <= at)?
        } else {
            let text_start = hex_start + hex_width + 2;
            let at = x.checked_sub(text_start)?;
            if at >= n {
                return None;
            }
            at
        };
        let offset = first.checked_add(index as u64)?;
        (offset < self.size).then_some(offset)
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;

    fn hex() -> HexHit {
        // 8 digits, 16 bytes a line, from (10, 2)
        HexHit {
            area: Rect::new(10, 2, 80, 5),
            top: 0x100,
            per_line: 16,
            digits: 8,
            size: 0x1a4,
        }
    }

    #[test]
    fn the_hex_part_and_the_text_part_give_the_byte() {
        let h = hex();
        // the offset column and the two cells after it show no byte
        assert_eq!(h.byte_at(10, 2), None);
        assert_eq!(h.byte_at(19, 2), None);
        // the first byte starts after 8 digits and 2 spaces: its two digits and the space after
        assert_eq!(h.byte_at(20, 2), Some(0x100));
        assert_eq!(h.byte_at(21, 2), Some(0x100));
        assert_eq!(h.byte_at(22, 2), Some(0x100));
        assert_eq!(h.byte_at(23, 2), Some(0x101));
        // the eighth byte is followed by a wider gap; the ninth begins after it
        assert_eq!(h.byte_at(20 + 3 * 7, 2), Some(0x107));
        assert_eq!(h.byte_at(20 + 3 * 7 + 3, 2), Some(0x107));
        assert_eq!(h.byte_at(20 + 3 * 8 + 1, 2), Some(0x108));
        // the text part, after the 16 bytes (48 cells) and two spaces
        let text = 20 + 3 * 16 - 1 + 1 + 2;
        assert_eq!(h.byte_at(text, 2), Some(0x100));
        assert_eq!(h.byte_at(text + 15, 2), Some(0x10f));
        assert_eq!(h.byte_at(text + 16, 2), None);
        // the next line
        assert_eq!(h.byte_at(20, 3), Some(0x110));
    }

    #[test]
    fn a_cell_past_the_end_of_the_file_or_outside_the_lines_gives_no_byte() {
        let h = hex();
        // 0x1a4 bytes: the line at 0x1a0 has four
        assert_eq!(h.byte_at(20 + 3 * 3, 8), None, "below the area");
        let h = HexHit {
            area: Rect::new(10, 2, 80, 20),
            ..h
        };
        assert_eq!(h.byte_at(20 + 3 * 3, 12), Some(0x1a3));
        assert_eq!(h.byte_at(20 + 3 * 4, 12), None, "after the last byte");
        assert_eq!(h.byte_at(200, 0), None, "outside");
    }

    #[test]
    fn a_narrow_line_of_eight_has_no_extra_gap() {
        let h = HexHit {
            area: Rect::new(0, 0, 60, 3),
            top: 0,
            per_line: 8,
            digits: 8,
            size: 100,
        };
        assert_eq!(h.byte_at(10 + 3 * 7, 0), Some(7));
        assert_eq!(h.byte_at(10 + 3 * 8 - 2, 0), Some(7));
        assert_eq!(h.byte_at(10 + 3 * 8, 0), None, "the gap before the text");
        // the text part follows the 8 bytes (23 cells) and two spaces
        assert_eq!(h.byte_at(10 + 23 + 2, 0), Some(0));
        assert_eq!(h.byte_at(10 + 23 + 2 + 7, 0), Some(7));
    }

    #[test]
    fn a_row_is_found_from_the_first_one_shown_and_not_past_the_last() {
        let hits = Hits {
            tree_pane: Some(Rect::new(0, 0, 40, 12)),
            hex_pane: Some(Rect::new(40, 0, 40, 12)),
            rows: Some(RowsHit {
                area: Rect::new(1, 1, 38, 10),
                first: 5,
                len: 8,
                files: false,
            }),
            hex: None,
            hints: Vec::new(),
        };
        assert_eq!(
            hits.target(4, 1),
            Some(Target::Row {
                index: 5,
                column: 3,
                files: false
            })
        );
        assert_eq!(
            hits.target(1, 3),
            Some(Target::Row {
                index: 7,
                column: 0,
                files: false
            })
        );
        // rows 5 to 12 are there: the cell for the ninth is empty, and is the pane
        assert_eq!(hits.target(1, 9), Some(Target::Pane(Focus::Tree)));
        // the border of the tree pane, and the other pane
        assert_eq!(hits.target(0, 0), Some(Target::Pane(Focus::Tree)));
        assert_eq!(hits.target(50, 5), Some(Target::Pane(Focus::Hex)));
        assert_eq!(hits.target(90, 5), None);
        assert_eq!(hits.pane_at(50, 5), Some(Focus::Hex));
    }
}
