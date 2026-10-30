//! Syntax highlighting of a schema: which parts of each line are keys, strings,
//! numbers and so on, for the pager that reads a schema.
//!
//! A schema is YAML (or a Kaitai Struct `.ksy`). This is a scanner of the subset
//! that schemas use, a line at a time with a little state (a block scalar `|` or
//! `>`, a quoted string that goes on to the next line), and not a YAML parser:
//! text that it does not understand is left plain, never changed or lost. It
//! works on text that is already safe to show (see [`crate::format::safe`]), and
//! gives byte ranges of it, so the caller can cut a line to a width first.

use std::ops::Range;

/// What a piece of a line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A key.
    Key,
    /// A key at the left edge: `meta`, `seq`, `types`, `instances`, `enums`.
    Section,
    /// The value of a `type`: the name of a type.
    Type,
    /// The value of an `enum`: the name of an enum.
    Enum,
    /// A quoted string, and the text of a block scalar.
    String,
    /// A number: decimal, `0x…`, `0b…`, `0o…`, with `_` and a fraction.
    Number,
    /// `true`, `false` and `null`.
    Literal,
    /// A comment, from `#` to the end of the line.
    Comment,
    /// A list dash, a colon, a bracket, a comma, a block scalar indicator.
    Punct,
}

/// The pieces of one line that are not plain, in order, as byte ranges.
pub type Marks = Vec<(Range<usize>, Kind)>;

/// The pieces of every line of `lines`.
#[must_use]
pub fn scan(lines: &[String]) -> Vec<Marks> {
    let mut scanner = Scanner::default();
    lines.iter().map(|line| scanner.line(line)).collect()
}

/// What goes from one line to the next.
#[derive(Default)]
struct Scanner {
    /// A block scalar (`|` or `>`) is being read: its lines are indented more
    /// than this column.
    block: Option<usize>,
    /// A quoted string goes on: the quote that will close it.
    quote: Option<char>,
}

impl Scanner {
    fn line(&mut self, line: &str) -> Marks {
        let mut marks = Marks::new();
        let indent = line.len() - line.trim_start_matches(' ').len();
        let blank = line.trim().is_empty();

        if let Some(column) = self.block {
            if blank || indent > column {
                if !blank {
                    marks.push((indent..line.len(), Kind::String));
                }
                return marks;
            }
            self.block = None;
        }
        if let Some(quote) = self.quote {
            let end = close_of(line, 0, quote);
            marks.push((0..end.unwrap_or(line.len()), Kind::String));
            match end {
                Some(end) => {
                    self.quote = None;
                    self.rest(line, end, &mut marks);
                }
                None => return marks,
            }
            return marks;
        }

        let mut at = indent;
        let mut column = indent;
        let mut keyed_at_edge = true;
        // list dashes: `- `, also `- - x`
        loop {
            let rest = tail(line, at);
            if rest == "-" || rest.starts_with("- ") {
                marks.push((at..at + 1, Kind::Punct));
                at += 1;
                let spaces = tail(line, at).len() - tail(line, at).trim_start_matches(' ').len();
                at += spaces;
                column = at;
                keyed_at_edge = false;
            } else {
                break;
            }
        }
        let rest = tail(line, at);
        if rest.is_empty() {
            return marks;
        }
        if rest.starts_with('#') {
            marks.push((at..line.len(), Kind::Comment));
            return marks;
        }
        match key_end(rest) {
            Some((name_end, colon)) => {
                let name = head(rest, name_end);
                let kind = if keyed_at_edge && indent == 0 {
                    Kind::Section
                } else {
                    Kind::Key
                };
                marks.push((at..at + name_end, kind));
                marks.push((at + colon..at + colon + 1, Kind::Punct));
                let value_at = at + colon + 1;
                let bare = name.trim_matches(['"', '\'']);
                self.value(line, value_at, bare, column, &mut marks);
            }
            None => self.value(line, at, "", column, &mut marks),
        }
        marks
    }

    /// The value that starts at or after `from`, of the key `key`.
    fn value(&mut self, line: &str, from: usize, key: &str, column: usize, marks: &mut Marks) {
        let start =
            from + (tail(line, from).len() - tail(line, from).trim_start_matches(' ').len());
        let rest = tail(line, start);
        let Some(first) = rest.chars().next() else {
            return;
        };
        match first {
            '#' => marks.push((start..line.len(), Kind::Comment)),
            '|' | '>' => {
                let end = start + rest.find(' ').unwrap_or(rest.len());
                marks.push((start..end, Kind::Punct));
                self.block = Some(column);
                self.rest(line, end, marks);
            }
            '"' | '\'' => {
                if let Some(end) = close_of(line, start + 1, first) {
                    marks.push((start..end, Kind::String));
                    self.rest(line, end, marks);
                } else {
                    marks.push((start..line.len(), Kind::String));
                    self.quote = Some(first);
                }
            }
            '[' | '{' => flow(line, start, marks),
            _ => {
                let end = plain_end(line, start);
                let text = part(line, start, end).trim_end();
                let kind = scalar_kind(text, key);
                if let Some(kind) = kind {
                    marks.push((start..start + text.len(), kind));
                }
                self.rest(line, start + text.len(), marks);
            }
        }
    }

    /// What can follow a value on the line: a comment.
    #[allow(
        clippy::unused_self,
        reason = "kept with the other steps of the scanner"
    )]
    fn rest(&self, line: &str, from: usize, marks: &mut Marks) {
        let after = tail(line, from);
        let skipped = after.len() - after.trim_start_matches(' ').len();
        if tail(after, skipped).starts_with('#') {
            marks.push((from + skipped..line.len(), Kind::Comment));
        }
    }
}

/// Where the quote `quote` that opened before `from` is closed on the line: the
/// byte after the closing quote.
fn close_of(line: &str, from: usize, quote: char) -> Option<usize> {
    let mut chars = tail(line, from).char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if quote == '"' && c == '\\' {
            chars.next();
        } else if c == quote {
            if quote == '\'' && chars.peek().is_some_and(|&(_, next)| next == '\'') {
                chars.next();
            } else {
                return Some(from + i + 1);
            }
        }
    }
    None
}

/// For a line that starts with a key (`name: value` or `name:`), the end of the
/// name and the place of the colon.
fn key_end(rest: &str) -> Option<(usize, usize)> {
    if let Some(quote) = rest.chars().next().filter(|c| matches!(c, '"' | '\'')) {
        let end = close_of(rest, 1, quote)?;
        let after = tail(rest, end).trim_start_matches(' ');
        let colon = rest.len() - after.len();
        return (after.starts_with(':') && (after.len() == 1 || tail(after, 1).starts_with(' ')))
            .then_some((end, colon));
    }
    let mut previous_space = false;
    for (i, c) in rest.char_indices() {
        match c {
            ':' => {
                let after = tail(rest, i + 1);
                if after.is_empty() || after.starts_with(' ') {
                    return Some((head(rest, i).trim_end().len(), i));
                }
            }
            '#' if previous_space => return None,
            '[' | '{' | '"' | '\'' if i == 0 => return None,
            _ => {}
        }
        previous_space = c == ' ';
    }
    None
}

/// The end of a plain scalar that starts at `from`: before ` #`, or the end.
fn plain_end(line: &str, from: usize) -> usize {
    let mut previous_space = false;
    for (i, c) in tail(line, from).char_indices() {
        if c == '#' && previous_space {
            return from + i;
        }
        previous_space = c == ' ';
    }
    line.len()
}

/// What a plain scalar is, if it is something to colour. `key` is the key it
/// is the value of.
fn scalar_kind(text: &str, key: &str) -> Option<Kind> {
    if text.is_empty() {
        return None;
    }
    if matches!(
        text,
        "true" | "false" | "True" | "False" | "null" | "Null" | "~"
    ) {
        return Some(Kind::Literal);
    }
    if is_number(text) {
        return Some(Kind::Number);
    }
    match key {
        "type" => Some(Kind::Type),
        "enum" => Some(Kind::Enum),
        _ => None,
    }
}

/// Whether the text is a number as a schema writes one: `12`, `-3`, `0x4d_5a`,
/// `0b101`, `0o17`, `1.5`, `1e3`.
fn is_number(text: &str) -> bool {
    let body = text.strip_prefix(['-', '+']).unwrap_or(text);
    if body.is_empty() {
        return false;
    }
    for (prefix, digit) in [
        ("0x", char::is_ascii_hexdigit as fn(&char) -> bool),
        ("0X", char::is_ascii_hexdigit),
        ("0b", |c: &char| matches!(c, '0' | '1')),
        ("0o", |c: &char| matches!(c, '0'..='7')),
    ] {
        if let Some(digits) = body.strip_prefix(prefix) {
            return !digits.is_empty() && digits.chars().all(|c| c == '_' || digit(&c));
        }
    }
    let mut seen_digit = false;
    let mut seen_dot = false;
    let mut seen_exp = false;
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '0'..='9' => seen_digit = true,
            '_' => {}
            '.' if !seen_dot && !seen_exp => seen_dot = true,
            'e' | 'E' if seen_digit && !seen_exp => {
                seen_exp = true;
                if matches!(chars.peek(), Some('+' | '-')) {
                    chars.next();
                }
            }
            _ => return false,
        }
    }
    seen_digit
}

/// A flow list or mapping on one line (`[1, 2]`, `{a: 1}`) that starts at `from`.
fn flow(line: &str, from: usize, marks: &mut Marks) {
    let bytes = line.as_bytes();
    let mut i = from;
    while i < line.len() {
        let c = tail(line, i).chars().next().unwrap_or(' ');
        match c {
            '[' | ']' | '{' | '}' | ',' | ':' => {
                marks.push((i..i + 1, Kind::Punct));
                i += 1;
            }
            '"' | '\'' => {
                let end = close_of(line, i + 1, c).unwrap_or(line.len());
                let kind = if tail(line, end).trim_start_matches(' ').starts_with(':') {
                    Kind::Key
                } else {
                    Kind::String
                };
                marks.push((i..end, kind));
                i = end;
            }
            '#' if i == 0 || bytes.get(i - 1) == Some(&b' ') => {
                marks.push((i..line.len(), Kind::Comment));
                return;
            }
            ' ' => i += 1,
            _ => {
                let mut end = i;
                for (j, d) in tail(line, i).char_indices() {
                    if matches!(d, ',' | ']' | '}' | ' ') {
                        break;
                    }
                    if d == ':' && tail(line, i + j + 1).starts_with([' ', ',', ']', '}']) {
                        break;
                    }
                    end = i + j + d.len_utf8();
                }
                if end == i {
                    end = i + c.len_utf8();
                }
                let word = part(line, i, end);
                let followed_by_colon = tail(line, end).starts_with(':');
                let kind = if followed_by_colon {
                    Some(Kind::Key)
                } else {
                    scalar_kind(word, "")
                };
                if let Some(kind) = kind {
                    marks.push((i..end, kind));
                }
                i = end;
            }
        }
    }
}

/// `text` from byte `from` on; empty when `from` is not a place in it.
fn tail(text: &str, from: usize) -> &str {
    text.get(from..).unwrap_or_default()
}

/// `text` before byte `to`; empty when `to` is not a place in it.
fn head(text: &str, to: usize) -> &str {
    text.get(..to).unwrap_or_default()
}

/// `text` from byte `from` to byte `to`; empty when they are not places in it.
fn part(text: &str, from: usize, to: usize) -> &str {
    text.get(from..to).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::string_slice,
        clippy::indexing_slicing,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;

    /// The marked pieces of a line as (text, kind).
    fn pieces(line: &str) -> Vec<(String, Kind)> {
        scan(&[line.to_owned()])
            .remove(0)
            .into_iter()
            .map(|(range, kind)| (line[range].to_owned(), kind))
            .collect()
    }

    fn lines(text: &str) -> Vec<Vec<(String, Kind)>> {
        let rows: Vec<String> = text.lines().map(str::to_owned).collect();
        scan(&rows)
            .into_iter()
            .zip(&rows)
            .map(|(marks, row)| {
                marks
                    .into_iter()
                    .map(|(range, kind)| (row[range].to_owned(), kind))
                    .collect()
            })
            .collect()
    }

    fn piece(text: &str, kind: Kind) -> (String, Kind) {
        (text.to_owned(), kind)
    }

    #[test]
    fn a_section_a_key_and_its_values() {
        assert_eq!(
            pieces("seq:"),
            [piece("seq", Kind::Section), piece(":", Kind::Punct)]
        );
        assert_eq!(
            pieces("  - id: size"),
            [
                piece("-", Kind::Punct),
                piece("id", Kind::Key),
                piece(":", Kind::Punct)
            ]
        );
        assert_eq!(
            pieces("    type: u2le"),
            [
                piece("type", Kind::Key),
                piece(":", Kind::Punct),
                piece("u2le", Kind::Type)
            ]
        );
        assert_eq!(
            pieces("    enum: kind_t"),
            [
                piece("enum", Kind::Key),
                piece(":", Kind::Punct),
                piece("kind_t", Kind::Enum)
            ]
        );
    }

    #[test]
    fn numbers_booleans_nulls_and_strings() {
        let kinds = |line: &str| pieces(line).last().map(|p| p.1);
        assert_eq!(kinds("size: 12"), Some(Kind::Number));
        assert_eq!(kinds("size: -3"), Some(Kind::Number));
        assert_eq!(kinds("magic: 0x4d_5a"), Some(Kind::Number));
        assert_eq!(kinds("b: 0b1010"), Some(Kind::Number));
        assert_eq!(kinds("o: 0o17"), Some(Kind::Number));
        assert_eq!(kinds("f: 1.5e3"), Some(Kind::Number));
        assert_eq!(kinds("if: true"), Some(Kind::Literal));
        assert_eq!(kinds("x: null"), Some(Kind::Literal));
        assert_eq!(kinds("contents: \"DOE\""), Some(Kind::String));
        assert_eq!(kinds("contents: 'it''s'"), Some(Kind::String));
        // an expression is not coloured as anything
        assert_eq!(pieces("size: len_x * 2").len(), 2);
        assert_eq!(pieces("size: 0xzz").len(), 2);
    }

    #[test]
    fn comments_whole_and_after_a_value() {
        assert_eq!(
            pieces("# the header"),
            [piece("# the header", Kind::Comment)]
        );
        assert_eq!(
            pieces("  size: 4  # bytes"),
            [
                piece("size", Kind::Key),
                piece(":", Kind::Punct),
                piece("4", Kind::Number),
                piece("# bytes", Kind::Comment)
            ]
        );
        // a # inside a word or a string is not a comment
        assert_eq!(pieces("name: a#b").len(), 2);
        assert_eq!(pieces("name: \"a # b\"").last().unwrap().1, Kind::String);
    }

    #[test]
    fn a_block_scalar_is_its_lines_until_the_indent_goes_back() {
        let rows = lines("doc: |\n  First line.\n\n  Second: not a key.\nseq: []");
        assert_eq!(rows[0].last().unwrap(), &piece("|", Kind::Punct));
        assert_eq!(rows[1], [piece("First line.", Kind::String)]);
        assert_eq!(rows[2], Vec::new());
        assert_eq!(rows[3], [piece("Second: not a key.", Kind::String)]);
        assert_eq!(rows[4][0], piece("seq", Kind::Section));
    }

    #[test]
    fn a_block_scalar_in_a_list_item_ends_at_the_next_key() {
        let rows = lines("  - id: a\n    doc: >-\n      text\n    size: 2");
        assert_eq!(rows[2], [piece("text", Kind::String)]);
        assert_eq!(rows[3][0], piece("size", Kind::Key));
    }

    #[test]
    fn a_quoted_string_may_go_on_to_the_next_line() {
        let rows = lines("doc: \"one\n  two\" # end\nnext: 1");
        assert_eq!(rows[0].last().unwrap().1, Kind::String);
        assert_eq!(rows[1][0], piece("  two\"", Kind::String));
        assert_eq!(rows[1].last().unwrap(), &piece("# end", Kind::Comment));
        assert_eq!(rows[2][0], piece("next", Kind::Section));
    }

    #[test]
    fn an_unterminated_quote_does_not_lose_text() {
        let line = "x: \"never closed";
        let marks = &scan(&[line.to_owned()])[0];
        // everything stays inside the line
        assert!(marks.iter().all(|(range, _)| range.end <= line.len()));
    }

    #[test]
    fn flow_lists_and_mappings() {
        assert_eq!(
            pieces("  contents: [0x50, 0x4b, \"PK\"]"),
            [
                piece("contents", Kind::Key),
                piece(":", Kind::Punct),
                piece("[", Kind::Punct),
                piece("0x50", Kind::Number),
                piece(",", Kind::Punct),
                piece("0x4b", Kind::Number),
                piece(",", Kind::Punct),
                piece("\"PK\"", Kind::String),
                piece("]", Kind::Punct),
            ]
        );
        let mapping = pieces("cases: {1: a, \"x\": true}");
        assert!(mapping.contains(&piece("1", Kind::Key)));
        assert!(mapping.contains(&piece("\"x\"", Kind::Key)));
        assert!(mapping.contains(&piece("true", Kind::Literal)));
    }

    #[test]
    fn a_quoted_key_and_a_key_with_a_colon_in_its_value() {
        assert_eq!(
            pieces("  \"0x01\": header")[0],
            piece("\"0x01\"", Kind::Key)
        );
        let url = pieces("  doc-ref: https://example.org/x");
        assert_eq!(url[0], piece("doc-ref", Kind::Key));
        assert_eq!(url.len(), 2);
    }

    #[test]
    fn text_the_scanner_does_not_understand_is_left_alone() {
        for line in ["", "    ", "- ", "just words here", "{{{", "a: [", "- - x"] {
            let marks = &scan(&[line.to_owned()])[0];
            for (range, _) in marks {
                assert!(range.end <= line.len() && line.is_char_boundary(range.start));
                assert!(line.is_char_boundary(range.end));
            }
        }
        assert_eq!(pieces("just words here"), Vec::new());
    }

    #[test]
    fn ranges_are_in_order_and_do_not_overlap_on_multibyte_text() {
        let line = "  - id: näme  # ünï";
        let marks = &scan(&[line.to_owned()])[0];
        let mut last = 0;
        for (range, _) in marks {
            assert!(range.start >= last, "{marks:?}");
            assert!(line.is_char_boundary(range.start) && line.is_char_boundary(range.end));
            last = range.end;
        }
    }
}
