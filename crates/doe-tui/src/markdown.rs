//! A Markdown entry of an archive, rendered for the terminal: headings, paragraphs with emphasis, lists (nested, ordered,
//! task lists), block quotes, code blocks (coloured by the language of the fence), tables, links and rules.
//!
//! The text is from an untrusted archive, so nothing in it acts: raw HTML is shown as the text it is, a link is its text
//! and its target written out, an image is its alt text, and the nesting, the number of blocks and the cells of tables
//! are limited. [`parse`] makes the document once, as blocks of styled text; [`layout`] makes the lines for a width
//! (wrapping the paragraphs), so that a resize only lays the same document out again.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::code::{self, Token};
use crate::{format, languages, preview};

/// The most blocks that are made; the rest is not shown.
pub const MAX_BLOCKS: usize = 20_000;
/// The most cells of tables.
pub const MAX_CELLS: usize = 4_000;
/// The deepest nesting of lists and quotes that has its own indent.
pub const MAX_DEPTH: usize = 12;
/// The widest a column of a table is drawn.
const MAX_CELL_WIDTH: usize = 40;

/// How a piece of text is drawn (the theme gives each a style: [`crate::ui::Theme::markdown`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// Plain text.
    Plain,
    /// `**strong**`.
    Strong,
    /// `*emphasis*`.
    Emphasis,
    /// `~~gone~~`.
    Strike,
    /// `` `code` `` and the text of a code block that has no language.
    Code,
    /// A heading of that level.
    Heading(u8),
    /// The text of a link or the target of an image.
    Link,
    /// The bars of a block quote, bullets and the marks around things.
    Dim,
    /// Code coloured as the language says.
    Token(Token),
}

/// A piece of a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// The text, safe to show.
    pub text: String,
    /// How it is drawn.
    pub style: Style,
}

fn span(text: impl Into<String>, style: Style) -> Span {
    Span {
        text: text.into(),
        style,
    }
}

/// What a block is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A paragraph, heading or list item text: wrapped to the width. The marker (`• `, `3. `, `☑ `) starts its first line.
    Text {
        /// The marker, if the block starts a list item.
        marker: Option<String>,
        /// The text.
        spans: Vec<Span>,
    },
    /// Lines that are not wrapped: a code block.
    Pre(Vec<Vec<Span>>),
    /// A table: the cells of the rows, the first row the header.
    Table(Vec<Vec<String>>),
    /// A horizontal rule.
    Rule,
    /// A note of the renderer (the file is too large to show all of it).
    Note(String),
}

/// A block, and where it is: how many quotes it is in and how far its list items indent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// The block quotes it is inside.
    pub quote: usize,
    /// The spaces of indent that lists give it.
    pub indent: usize,
    /// What it is.
    pub kind: Kind,
}

/// A rendered document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Doc {
    /// The blocks, in order.
    pub blocks: Vec<Block>,
    /// Whether the file was too large to render all of it.
    pub truncated: bool,
}

/// The id of the language that the first word of a code fence names, as `languageOfFence` does in the web IDE.
#[must_use]
pub fn language_of_fence(info: &str) -> Option<&'static str> {
    let word = info
        .split_whitespace()
        .next()?
        .trim_start_matches('.')
        .to_lowercase();
    if let Some((id, _, _)) = languages::LANGUAGES.iter().find(|(id, _, _)| *id == word) {
        return Some(id);
    }
    languages::EXTENSIONS
        .iter()
        .find(|(extension, _)| *extension == word)
        .map(|(_, id)| *id)
}

/// The spans of a line of code with the marks the grammar gave it.
fn coloured(line: &str, marks: &[(std::ops::Range<usize>, Token)]) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut at = 0;
    for (range, token) in marks {
        if range.start > at
            && let Some(plain) = line.get(at..range.start)
        {
            spans.push(span(plain, Style::Plain));
        }
        if let Some(piece) = line.get(range.start.max(at)..range.end) {
            spans.push(span(piece, Style::Token(*token)));
        }
        at = range.end.max(at);
    }
    if let Some(rest) = line.get(at..)
        && !rest.is_empty()
    {
        spans.push(span(rest, Style::Plain));
    }
    spans
}

/// What the list the parser is in numbers its items with.
enum Counter {
    Bullet,
    Number(u64),
}

/// The state of the walk through the events.
struct Walk {
    blocks: Vec<Block>,
    cells: usize,
    truncated: bool,
    quote: usize,
    lists: Vec<Counter>,
    /// The marker of the item whose first block has not been written yet.
    marker: Option<String>,
    styles: Vec<Style>,
    spans: Vec<Span>,
    heading: Option<u8>,
    code: Option<(Option<&'static str>, String)>,
    link: Option<String>,
    image: Option<(String, String)>,
    table: Option<Vec<Vec<String>>>,
    row: Vec<String>,
    cell: String,
    in_cell: bool,
}

impl Walk {
    fn indent(&self) -> usize {
        self.lists.len().saturating_sub(1).min(MAX_DEPTH) * 2
    }

    fn push(&mut self, kind: Kind) {
        if self.blocks.len() >= MAX_BLOCKS {
            self.truncated = true;
            return;
        }
        let indent = self.indent();
        self.blocks.push(Block {
            quote: self.quote.min(MAX_DEPTH),
            indent,
            kind,
        });
    }

    fn style(&self) -> Style {
        self.heading
            .map(Style::Heading)
            .or_else(|| self.styles.last().copied())
            .unwrap_or(Style::Plain)
    }

    fn text(&mut self, text: &str) {
        let safe = format::safe(&text.replace('\t', "    "));
        if self.in_cell {
            self.cell.push_str(&safe);
        } else if let Some((_, alt)) = self.image.as_mut() {
            alt.push_str(&safe);
        } else if let Some((_, body)) = self.code.as_mut() {
            body.push_str(text);
        } else {
            let style = self.style();
            self.spans.push(span(safe, style));
        }
    }

    /// End the text that was being collected as a block of its own.
    fn flush_text(&mut self) {
        if self.spans.is_empty() && self.marker.is_none() {
            return;
        }
        let spans = std::mem::take(&mut self.spans);
        let marker = self.marker.take();
        self.push(Kind::Text { marker, spans });
    }

    fn end_code(&mut self) {
        let Some((language, body)) = self.code.take() else {
            return;
        };
        let lines: Vec<String> = body
            .trim_end_matches('\n')
            .split('\n')
            .map(preview::shown)
            .collect();
        let marks = language.and_then(|id| code::highlight(id, &lines));
        let rows = lines
            .iter()
            .enumerate()
            .map(|(i, line)| match marks.as_ref().and_then(|m| m.get(i)) {
                Some(marks) => coloured(line, marks),
                None => vec![span(line.clone(), Style::Code)],
            })
            .collect();
        self.push(Kind::Pre(rows));
    }
}

/// Make the document of `text`.
#[must_use]
pub fn parse(text: &str) -> Doc {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut walk = Walk {
        blocks: Vec::new(),
        cells: 0,
        truncated: false,
        quote: 0,
        lists: Vec::new(),
        marker: None,
        styles: Vec::new(),
        spans: Vec::new(),
        heading: None,
        code: None,
        link: None,
        image: None,
        table: None,
        row: Vec::new(),
        cell: String::new(),
        in_cell: false,
    };
    for event in Parser::new_ext(text, options) {
        if walk.truncated {
            break;
        }
        match event {
            Event::Start(tag) => start(&mut walk, tag),
            Event::End(tag) => end(&mut walk, tag),
            Event::Text(t) => walk.text(&t),
            Event::Code(t) => {
                if walk.in_cell {
                    walk.cell.push_str(&format::safe(&t));
                } else {
                    walk.spans.push(span(format::safe(&t), Style::Code));
                }
            }
            // raw HTML is text
            Event::Html(t) | Event::InlineHtml(t) => {
                for (i, line) in t.trim_end_matches('\n').split('\n').enumerate() {
                    if i > 0 {
                        walk.flush_text();
                    }
                    walk.spans.push(span(format::safe(line), Style::Dim));
                }
            }
            Event::SoftBreak => walk.text(" "),
            Event::HardBreak => walk.flush_text(),
            Event::Rule => walk.push(Kind::Rule),
            Event::TaskListMarker(done) => {
                walk.marker = Some(if done { "☑ " } else { "☐ " }.to_owned());
            }
            _ => {}
        }
    }
    walk.flush_text();
    if walk.truncated {
        walk.blocks.push(Block {
            quote: 0,
            indent: 0,
            kind: Kind::Note(
                "… the rest is not shown: this file is too large to render (r shows the source)"
                    .to_owned(),
            ),
        });
    }
    Doc {
        blocks: walk.blocks,
        truncated: walk.truncated,
    }
}

fn start(walk: &mut Walk, tag: Tag<'_>) {
    match tag {
        Tag::Paragraph | Tag::Item => {
            if matches!(tag, Tag::Item) {
                let marker = match walk.lists.last_mut() {
                    Some(Counter::Number(n)) => {
                        let marker = format!("{n}. ");
                        *n += 1;
                        marker
                    }
                    _ => "• ".to_owned(),
                };
                walk.marker = Some(marker);
            }
        }
        Tag::Heading { level, .. } => {
            walk.flush_text();
            walk.heading = Some(match level {
                HeadingLevel::H1 => 1,
                HeadingLevel::H2 => 2,
                HeadingLevel::H3 => 3,
                HeadingLevel::H4 => 4,
                HeadingLevel::H5 => 5,
                HeadingLevel::H6 => 6,
            });
        }
        Tag::BlockQuote(_) => {
            walk.flush_text();
            walk.quote += 1;
        }
        Tag::CodeBlock(kind) => {
            walk.flush_text();
            let language = match kind {
                CodeBlockKind::Fenced(info) => language_of_fence(&info),
                CodeBlockKind::Indented => None,
            };
            walk.code = Some((language, String::new()));
        }
        Tag::List(first) => {
            walk.flush_text();
            walk.lists
                .push(first.map_or(Counter::Bullet, Counter::Number));
        }
        Tag::Emphasis => walk.styles.push(Style::Emphasis),
        Tag::Strong => walk.styles.push(Style::Strong),
        Tag::Strikethrough => walk.styles.push(Style::Strike),
        Tag::Link { dest_url, .. } => {
            walk.link = Some(format::safe(&dest_url));
            walk.styles.push(Style::Link);
        }
        Tag::Image { dest_url, .. } => walk.image = Some((format::safe(&dest_url), String::new())),
        Tag::Table(_) => {
            walk.flush_text();
            walk.table = Some(Vec::new());
        }
        Tag::TableRow | Tag::TableHead => walk.row = Vec::new(),
        Tag::TableCell => {
            walk.in_cell = true;
            walk.cell = String::new();
        }
        _ => {}
    }
}

fn end(walk: &mut Walk, tag: TagEnd) {
    match tag {
        TagEnd::Paragraph | TagEnd::Item => walk.flush_text(),
        TagEnd::Heading(_) => {
            walk.flush_text();
            walk.heading = None;
        }
        TagEnd::BlockQuote(_) => {
            walk.flush_text();
            walk.quote = walk.quote.saturating_sub(1);
        }
        TagEnd::CodeBlock => walk.end_code(),
        TagEnd::List(_) => {
            walk.flush_text();
            walk.lists.pop();
        }
        TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
            walk.styles.pop();
        }
        TagEnd::Link => {
            walk.styles.pop();
            if let Some(url) = walk.link.take() {
                if walk.in_cell {
                    walk.cell.push_str(" <");
                    walk.cell.push_str(&url);
                    walk.cell.push('>');
                } else if !url.is_empty() {
                    walk.spans.push(span(format!(" <{url}>"), Style::Dim));
                }
            }
        }
        TagEnd::Image => {
            if let Some((url, alt)) = walk.image.take() {
                let label = if alt.is_empty() { url.clone() } else { alt };
                let text = format!("[image: {label}]");
                if walk.in_cell {
                    walk.cell.push_str(&text);
                } else {
                    walk.spans.push(span(text, Style::Dim));
                }
            }
        }
        TagEnd::TableCell => {
            walk.in_cell = false;
            walk.cells += 1;
            if walk.cells > MAX_CELLS {
                walk.truncated = true;
                return;
            }
            let cell = std::mem::take(&mut walk.cell);
            walk.row.push(cell);
        }
        TagEnd::TableRow | TagEnd::TableHead => {
            let row = std::mem::take(&mut walk.row);
            if let Some(table) = walk.table.as_mut() {
                table.push(row);
            }
        }
        TagEnd::Table => {
            if let Some(rows) = walk.table.take() {
                walk.push(Kind::Table(rows));
            }
        }
        _ => {}
    }
}

/// A line of the rendering.
pub type Line = Vec<Span>;

/// The lines of a document laid out for a pane `width` cells wide.
#[must_use]
pub fn layout(doc: &Doc, width: usize) -> Vec<Line> {
    let width = width.max(8);
    let mut lines: Vec<Line> = Vec::new();
    let mut previous_quote = None;
    for block in &doc.blocks {
        // a blank line between blocks, except between the lines of one list and one quote
        if !lines.is_empty() {
            let tight = matches!(
                &block.kind,
                Kind::Text {
                    marker: Some(_),
                    ..
                }
            ) && block.indent > 0;
            let same_quote = previous_quote == Some(block.quote) && block.quote > 0;
            if !tight || same_quote {
                lines.push(bars(block.quote, 0));
            }
        }
        previous_quote = Some(block.quote);
        let lead = |first: bool, marker: &str| -> Line {
            let mut out = bars(block.quote, block.indent);
            if first {
                out.push(span(marker.to_owned(), Style::Dim));
            } else {
                out.push(span(" ".repeat(format::width(marker)), Style::Plain));
            }
            out
        };
        match &block.kind {
            Kind::Text { marker, spans } => {
                let marker = marker.clone().unwrap_or_default();
                let room = width
                    .saturating_sub(block.quote * 2 + block.indent + format::width(&marker))
                    .max(4);
                for (i, words) in wrap(spans, room).into_iter().enumerate() {
                    let mut line = lead(i == 0, &marker);
                    line.extend(words);
                    lines.push(line);
                }
            }
            Kind::Pre(rows) => {
                for row in rows {
                    let mut line = bars(block.quote, block.indent + 2);
                    line.extend(row.iter().cloned());
                    lines.push(cut(line, width));
                }
            }
            Kind::Table(rows) => {
                for line in table_lines(rows) {
                    let mut out = bars(block.quote, block.indent);
                    out.push(span(line.0, line.1));
                    lines.push(cut(out, width));
                }
            }
            Kind::Rule => {
                let mut line = bars(block.quote, block.indent);
                let rest = width.saturating_sub(block.quote * 2 + block.indent);
                line.push(span("─".repeat(rest.min(60)), Style::Dim));
                lines.push(line);
            }
            Kind::Note(text) => lines.push(vec![span(text.clone(), Style::Dim)]),
        }
    }
    // no blank line at the top or the end
    while lines.last().is_some_and(|l| {
        l.iter().all(|s| {
            s.text.trim().is_empty()
                || s.style == Style::Dim && s.text.chars().all(|c| c == '│' || c == ' ')
        })
    }) {
        lines.pop();
    }
    lines
}

/// The bars of the quotes a block is in and its indent.
fn bars(quote: usize, indent: usize) -> Line {
    let mut out = Vec::new();
    if quote > 0 {
        out.push(span("│ ".repeat(quote), Style::Dim));
    }
    if indent > 0 {
        out.push(span(" ".repeat(indent), Style::Plain));
    }
    out
}

/// A line cut to `width` cells (the rest is not shown; the last cell is `…`).
fn cut(line: Line, width: usize) -> Line {
    let mut out = Vec::new();
    let mut used = 0;
    for piece in line {
        let w = format::width(&piece.text);
        if used + w <= width {
            used += w;
            out.push(piece);
            continue;
        }
        let room = width.saturating_sub(used + 1);
        let mut text = String::new();
        let mut taken = 0;
        for c in piece.text.chars() {
            let cw = format::width(&c.to_string());
            if taken + cw > room {
                break;
            }
            text.push(c);
            taken += cw;
        }
        text.push('…');
        out.push(span(text, piece.style));
        return out;
    }
    out
}

/// The spans broken into lines of at most `room` cells, at spaces; a word longer than a line is broken where it must be.
fn wrap(spans: &[Span], room: usize) -> Vec<Line> {
    let mut lines: Vec<Line> = vec![Vec::new()];
    let mut used = 0;
    for piece in spans {
        // words with the space that follows them, as pieces of the same style
        let mut words: Vec<(String, bool)> = Vec::new();
        let mut word = String::new();
        for c in piece.text.chars() {
            if c == ' ' {
                words.push((std::mem::take(&mut word), true));
            } else {
                word.push(c);
            }
        }
        if !word.is_empty() {
            words.push((word, false));
        }
        for (word, space) in words {
            let mut text = word;
            let mut w = format::width(&text);
            // a word that does not fit on a line of its own is broken
            while w > room {
                if used > 0 {
                    lines.push(Vec::new());
                    used = 0;
                }
                let mut head = String::new();
                let mut taken = 0;
                let mut rest = String::new();
                for c in text.chars() {
                    let cw = format::width(&c.to_string());
                    if rest.is_empty() && taken + cw <= room {
                        head.push(c);
                        taken += cw;
                    } else {
                        rest.push(c);
                    }
                }
                if head.is_empty() {
                    break;
                }
                if let Some(line) = lines.last_mut() {
                    line.push(span(head, piece.style));
                }
                lines.push(Vec::new());
                text = rest;
                w = format::width(&text);
            }
            let need = w + usize::from(space);
            if used > 0 && used + w > room {
                lines.push(Vec::new());
                used = 0;
            }
            if let Some(line) = lines.last_mut() {
                let mut shown = text;
                if space {
                    shown.push(' ');
                }
                if !shown.is_empty() {
                    line.push(span(shown, piece.style));
                }
            }
            used += need;
        }
    }
    // a space at the end of a line is not drawn
    for line in &mut lines {
        if let Some(last) = line.last_mut() {
            last.text = last.text.trim_end().to_owned();
        }
        line.retain(|s| !s.text.is_empty());
    }
    lines.retain(|line| !line.is_empty());
    if lines.is_empty() {
        lines.push(Vec::new());
    }
    lines
}

/// The lines of a table drawn with box characters: `(text, style)`, the header in the style of strong text.
fn table_lines(rows: &[Vec<String>]) -> Vec<(String, Style)> {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return Vec::new();
    }
    let mut widths = vec![1; columns];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if let Some(w) = widths.get_mut(i) {
                *w = (*w).max(format::width(cell).min(MAX_CELL_WIDTH));
            }
        }
    }
    let rule = |left: &str, mid: &str, right: &str| -> String {
        let parts: Vec<String> = widths.iter().map(|w| "─".repeat(w + 2)).collect();
        format!("{left}{}{right}", parts.join(mid))
    };
    let mut out = vec![(rule("┌", "┬", "┐"), Style::Dim)];
    for (r, row) in rows.iter().enumerate() {
        let mut line = String::from("│");
        for (i, w) in widths.iter().enumerate() {
            let cell = row.get(i).map_or("", String::as_str);
            let text = format::truncate(cell, *w);
            let pad = w.saturating_sub(format::width(&text));
            line.push(' ');
            line.push_str(&text);
            line.push_str(&" ".repeat(pad));
            line.push_str(" │");
        }
        out.push((line, if r == 0 { Style::Strong } else { Style::Plain }));
        if r == 0 && rows.len() > 1 {
            out.push((rule("├", "┼", "┤"), Style::Dim));
        }
    }
    out.push((rule("└", "┴", "┘"), Style::Dim));
    out
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::format_collect,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]
    use super::*;

    fn text_of(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect())
            .collect()
    }

    #[test]
    fn the_constructs_are_laid_out_as_lines() {
        let doc = parse(
            "# Title\n\nSome *emphasis* and **strong** and `code` and a [link](https://example.test/a).\n\n- one\n  - nested\n- [x] done\n\n3. three\n4. four\n\n> quoted\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n---\n\n![alt text](https://example.test/i.png)\n",
        );
        let lines = text_of(&layout(&doc, 80));
        let all = lines.join("\n");
        assert!(all.contains("Title"), "{all}");
        assert!(
            all.contains("Some emphasis and strong and code and a link <https://example.test/a>."),
            "{all}"
        );
        assert!(
            all.contains("• one") && all.contains("  • nested") && all.contains("☑ done"),
            "{all}"
        );
        assert!(all.contains("3. three") && all.contains("4. four"), "{all}");
        assert!(all.contains("│ quoted"), "{all}");
        assert!(
            all.contains("│ a │ b │")
                && all.contains("│ 1 │ 2 │")
                && all.contains("┌")
                && all.contains("└"),
            "{all}"
        );
        assert!(all.contains("──────"), "{all}");
        assert!(all.contains("[image: alt text]"), "{all}");
        // the styles
        let h = layout(&doc, 80);
        assert!(
            h[0].iter()
                .any(|s| s.style == Style::Heading(1) && s.text == "Title")
        );
    }

    #[test]
    fn a_code_block_is_coloured_by_the_language_of_its_fence() {
        let doc = parse("```rust\nfn main() { let n = 42; }\n```\n\n```\nno language\n```\n");
        let lines = layout(&doc, 80);
        let rust = &lines[0];
        assert!(
            rust.iter()
                .any(|s| matches!(s.style, Style::Token(Token::Keyword)) && s.text == "fn"),
            "{rust:?}"
        );
        assert!(
            rust.iter()
                .any(|s| matches!(s.style, Style::Token(Token::Number)) && s.text == "42")
        );
        let plain = lines
            .iter()
            .find(|l| l.iter().any(|s| s.text == "no language"))
            .unwrap();
        assert!(plain.iter().any(|s| s.style == Style::Code));
        assert_eq!(language_of_fence("js"), Some("javascript"));
        assert_eq!(language_of_fence("RS title=x"), Some("rust"));
        assert_eq!(language_of_fence("klingon"), None);
    }

    #[test]
    fn a_paragraph_wraps_to_the_width_and_wraps_again_when_it_is_narrower() {
        let doc = parse("one two three four five six seven eight nine ten\n");
        let wide = text_of(&layout(&doc, 80));
        assert_eq!(wide, ["one two three four five six seven eight nine ten"]);
        let narrow = text_of(&layout(&doc, 20));
        assert!(narrow.len() >= 3, "{narrow:?}");
        assert!(narrow.iter().all(|l| format::width(l) <= 20), "{narrow:?}");
        assert_eq!(
            narrow.join(" "),
            "one two three four five six seven eight nine ten"
        );
        // a word longer than the line is broken
        let long = text_of(&layout(&parse(&"x".repeat(50)), 20));
        assert!(
            long.len() >= 3 && long.iter().all(|l| format::width(l) <= 20),
            "{long:?}"
        );
    }

    #[test]
    fn raw_html_and_odd_links_are_text() {
        let doc = parse(
            "<script>alert(1)</script>\n\n<img src=\"https://evil.test/x.png\"> and [click](javascript:alert(4)) ![p](https://evil.test/p.png)\n",
        );
        let all = text_of(&layout(&doc, 120)).join("\n");
        assert!(all.contains("<script>alert(1)</script>"), "{all}");
        assert!(
            all.contains("<img src=\"https://evil.test/x.png\">"),
            "{all}"
        );
        assert!(all.contains("click <javascript:alert(4)>"), "{all}");
        assert!(all.contains("[image: p]"), "{all}");
    }

    #[test]
    fn control_characters_are_made_visible() {
        let doc = parse("a\u{202e}b \u{1b}[31mred\n");
        let all = text_of(&layout(&doc, 80)).join("\n");
        assert!(
            !all.contains('\u{202e}') && !all.contains('\u{1b}'),
            "{all:?}"
        );
    }

    #[test]
    fn deep_nesting_many_blocks_and_many_cells_are_capped() {
        let quotes = parse(&format!("{}deep\n", "> ".repeat(500)));
        let lines = layout(&quotes, 80);
        assert!(
            lines
                .iter()
                .all(|l| l.iter().map(|s| format::width(&s.text)).sum::<usize>() <= 80)
        );
        let lists: String = (0..200)
            .map(|i| format!("{}- item {i}\n", "  ".repeat(i.min(40))))
            .collect();
        let lines = layout(&parse(&lists), 80);
        assert!(text_of(&lines).join("\n").contains("item 199"));
        let many: String = "line\n\n".repeat(MAX_BLOCKS + 10);
        let doc = parse(&many);
        assert!(doc.truncated);
        assert!(doc.blocks.len() <= MAX_BLOCKS + 1);
        assert!(
            text_of(&layout(&doc, 80))
                .last()
                .unwrap()
                .contains("too large to render")
        );
        let table = format!("| a | b |\n|---|---|\n{}", "| 1 | 2 |\n".repeat(MAX_CELLS));
        assert!(parse(&table).truncated);
    }

    #[test]
    fn a_table_with_a_long_cell_is_cut_and_a_narrow_pane_cuts_the_line() {
        let doc = parse(&format!("| h |\n|---|\n| {} |\n", "w".repeat(100)));
        let lines = text_of(&layout(&doc, 30));
        assert!(lines.iter().all(|l| format::width(l) <= 30), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains('…')), "{lines:?}");
    }
}
