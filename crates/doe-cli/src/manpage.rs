//! The manual pages, made from the README and the clap definition.
//!
//! The README is the one place the documentation is written. `doe(1)` is the
//! command: the part clap knows (synopsis and options) with the sections of
//! the README that are about running it. `doe-schema(5)` is the schema format.
//! A section of the README that is in neither list below is a test failure, so
//! that new documentation is not left out without somebody deciding it.

use std::fmt::Write as _;

use clap::CommandFactory;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::cli::Cli;

const README: &str = include_str!("../../../README.md");

/// A manual page that can be printed.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    /// `doe(1)`: the command.
    Doe,
    /// `doe-schema(5)`: the schema format.
    Schema,
}

/// Where a section of the README (a `##` heading) goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Home {
    Doe,
    Schema,
    /// Not in a manual page, and why.
    Left(&'static str),
}

/// Every `##` section of the README, in the page it belongs to.
const SECTIONS: [(&str, Home); 12] = [
    (
        "Installation",
        Home::Left("about getting doe, not using it"),
    ),
    ("Quick start", Home::Doe),
    ("Type library", Home::Doe),
    ("Schema format", Home::Schema),
    ("Kaitai Struct compatibility", Home::Schema),
    ("Output", Home::Doe),
    (
        "CLI reference",
        Home::Doe, // without the `--help` listing, which is the OPTIONS section
    ),
    ("Configuration", Home::Doe),
    ("Memory use", Home::Doe),
    ("Fuzzing", Home::Left("for people who work on doe")),
    (
        "Using as a library",
        Home::Left("Rust API, not the command"),
    ),
    ("License", Home::Doe),
];

/// A `##` section of the README.
#[derive(Debug)]
struct Section<'a> {
    title: &'a str,
    body: &'a str,
}

/// The text before the first `##` heading, and the sections after it.
fn split_readme(readme: &str) -> (&str, Vec<Section<'_>>) {
    let mut starts: Vec<(usize, usize, &str)> = Vec::new();
    let mut heading: Option<(usize, usize)> = None;
    for (event, range) in Parser::new_ext(readme, Options::empty()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading {
                level: HeadingLevel::H2,
                ..
            }) => heading = Some((range.start, range.end)),
            Event::End(TagEnd::Heading(HeadingLevel::H2)) => {
                if let Some((start, end)) = heading.take() {
                    let line = readme.get(start..end).unwrap_or_default();
                    let title = line.trim_start_matches('#').trim();
                    starts.push((start, end, title));
                }
            }
            _ => {}
        }
    }
    let intro_end = starts.first().map_or(readme.len(), |first| first.0);
    let intro = readme.get(..intro_end).unwrap_or_default();
    let sections = starts
        .iter()
        .enumerate()
        .map(|(index, &(_, end, title))| {
            let next = starts
                .get(index + 1)
                .map_or(readme.len(), |following| following.0);
            Section {
                title,
                body: readme.get(end..next).unwrap_or_default(),
            }
        })
        .collect();
    (intro, sections)
}

/// The text of a manual page.
pub(crate) fn render(page: Page) -> anyhow::Result<Vec<u8>> {
    let (intro, sections) = split_readme(README);
    let mut out = Vec::new();
    match page {
        Page::Doe => {
            let man = clap_mangen::Man::new(Cli::command());
            man.render_title(&mut out)?;
            man.render_name_section(&mut out)?;
            man.render_synopsis_section(&mut out)?;
            man.render_description_section(&mut out)?;
            out.extend_from_slice(roff(&intro_text(intro), false).as_bytes());
            man.render_options_section(&mut out)?;
            for section in sections_of(&sections, Home::Doe) {
                out.extend_from_slice(section_roff(section).as_bytes());
            }
            out.extend_from_slice(b".SH SEE ALSO\n.BR doe\\-schema (5)\n");
            man.render_version_section(&mut out)?;
        }
        Page::Schema => {
            let version = env!("CARGO_PKG_VERSION");
            let mut text = String::new();
            let _ = write!(
                text,
                ".TH doe\\-schema 5 \"doe {version}\"\n\
                 .SH NAME\n\
                 doe\\-schema \\- the schema format read by doe\n\
                 .SH DESCRIPTION\n\
                 This page describes the schema that \\fBdoe\\fR(1) parses a file with: a YAML \
                 file, or a Kaitai Struct \\fB.ksy\\fR definition.\n"
            );
            out.extend_from_slice(text.as_bytes());
            for section in sections_of(&sections, Home::Schema) {
                out.extend_from_slice(section_roff(section).as_bytes());
            }
            out.extend_from_slice(b".SH SEE ALSO\n.BR doe (1)\n");
        }
    }
    Ok(out)
}

fn sections_of<'a>(
    sections: &'a [Section<'a>],
    home: Home,
) -> impl Iterator<Item = &'a Section<'a>> {
    sections.iter().filter(move |section| {
        SECTIONS
            .iter()
            .any(|(title, place)| *title == section.title && *place == home)
    })
}

/// The part of the README before the first section that is about the program:
/// its description, without the title and the badges.
fn intro_text(intro: &str) -> String {
    let body = intro
        .lines()
        .skip_while(|line| {
            line.starts_with('#') || line.trim().is_empty() || line.starts_with("[![")
        })
        .collect::<Vec<_>>()
        .join("\n");
    // The first paragraph is the tagline, which the page has as its NAME.
    match body.split_once("\n\n") {
        Some((tagline, rest)) if !tagline.contains(". ") => rest.to_owned(),
        _ => body,
    }
}

/// A section as roff: its heading, and its body, from the first sub-heading
/// on for the CLI reference, whose start is the listing of the options.
fn section_roff(section: &Section<'_>) -> String {
    let mut text = String::new();
    let _ = writeln!(text, ".SH \"{}\"", section.title.to_uppercase());
    text.push_str(&roff(section.body, section.title == "CLI reference"));
    text
}

/// The inline state of the Markdown being written as roff.
struct Writer {
    out: String,
    /// Whether the next character starts a line of the roff source.
    line_start: bool,
    /// The fonts that are on, innermost last: `B` or `I`.
    fonts: Vec<char>,
    /// The lists that are open: the number of the next item of an ordered one.
    lists: Vec<Option<u64>>,
    /// Whether nothing has been written in the item that was just started.
    item_fresh: bool,
    /// Events are dropped while this is above zero (an image).
    skipping: usize,
    /// Where the text of the heading being read goes.
    heading: Option<String>,
    /// Where the text of the link being read starts in `out`, and its address.
    links: Vec<(usize, String)>,
    /// Whether a cell has been written in the table row being read.
    cell_in_row: bool,
}

impl Writer {
    fn new() -> Self {
        Writer {
            out: String::new(),
            line_start: true,
            fonts: Vec::new(),
            lists: Vec::new(),
            item_fresh: false,
            skipping: 0,
            heading: None,
            links: Vec::new(),
            cell_in_row: false,
        }
    }

    fn newline(&mut self) {
        if !self.line_start {
            self.out.push('\n');
            self.line_start = true;
        }
    }

    /// Write a request (a line that starts with a dot).
    fn request(&mut self, line: &str) {
        self.newline();
        self.out.push_str(line);
        self.out.push('\n');
        self.line_start = true;
    }

    /// Write text, escaped, so that it is what it says in the page.
    fn text(&mut self, text: &str) {
        for c in text.chars() {
            match c {
                '\n' => {
                    self.out.push('\n');
                    self.line_start = true;
                    continue;
                }
                '\\' => self.out.push_str("\\e"),
                '-' => self.out.push_str("\\-"),
                '\'' => self.out.push_str("\\(aq"),
                '`' => self.out.push_str("\\(ga"),
                '\u{2014}' => self.out.push_str("\\(em"),
                '\u{2013}' => self.out.push_str("\\(en"),
                '\u{2026}' => self.out.push_str("..."),
                '\u{2192}' => self.out.push_str("\\(->"),
                '\u{00d7}' => self.out.push_str("\\(mu"),
                '.' if self.line_start => self.out.push_str("\\&."),
                other => self.out.push(other),
            }
            self.line_start = false;
        }
    }

    /// Write an address, which may be broken after a slash.
    fn address(&mut self, address: &str) {
        let mut parts = address.split_inclusive('/').peekable();
        while let Some(part) = parts.next() {
            self.text(part);
            if parts.peek().is_some() {
                self.out.push_str("\\:");
            }
        }
    }

    fn font(&mut self, font: char) {
        self.fonts.push(font);
        let _ = write!(self.out, "\\f{font}");
        self.line_start = false;
    }

    fn end_font(&mut self) {
        self.fonts.pop();
        let back = self.fonts.last().copied().unwrap_or('R');
        let _ = write!(self.out, "\\f{back}");
        self.line_start = false;
    }

    /// Start a block that is not text of the item that was just started.
    fn block(&mut self, top_level: &str, in_list: &str) {
        if self.lists.is_empty() {
            self.request(top_level);
        } else if self.item_fresh {
            self.newline();
        } else {
            self.request(in_list);
        }
        self.item_fresh = false;
    }
}

/// Markdown as roff. `from_subheading` leaves out what comes before the first
/// sub-heading.
fn roff(markdown: &str, from_subheading: bool) -> String {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let mut w = Writer::new();
    let mut started = !from_subheading;
    for event in Parser::new_ext(markdown, options) {
        if !started {
            if matches!(
                event,
                Event::Start(Tag::Heading {
                    level: HeadingLevel::H3,
                    ..
                })
            ) {
                started = true;
            } else {
                continue;
            }
        }
        handle(&mut w, event);
    }
    w.newline();
    w.out
}

fn handle(w: &mut Writer, event: Event<'_>) {
    if w.skipping > 0 {
        match event {
            Event::Start(Tag::Image { .. }) => w.skipping += 1,
            Event::End(TagEnd::Image) => w.skipping -= 1,
            _ => {}
        }
        return;
    }
    match event {
        Event::Start(tag) => start(w, tag),
        Event::End(tag) => end(w, tag),
        Event::Text(text) => {
            if let Some(heading) = w.heading.as_mut() {
                heading.push_str(&text);
            } else {
                w.item_fresh = false;
                w.text(&text);
            }
        }
        Event::Code(code) => {
            if let Some(heading) = w.heading.as_mut() {
                heading.push_str(&code);
            } else {
                w.item_fresh = false;
                w.font('B');
                w.text(&code);
                w.end_font();
            }
        }
        Event::SoftBreak => {
            if let Some(heading) = w.heading.as_mut() {
                heading.push(' ');
            } else {
                w.text("\n");
            }
        }
        Event::HardBreak => w.request(".br"),
        // Raw HTML, rules, footnotes and the rest have no place on a page.
        _ => {}
    }
}

fn start(w: &mut Writer, tag: Tag<'_>) {
    match tag {
        Tag::Paragraph => w.block(".PP", ".IP \"\" 3"),
        Tag::Heading { .. } => {
            w.heading = Some(String::new());
        }
        Tag::BlockQuote(_) => {
            w.request(".RS 3");
        }
        Tag::CodeBlock(_) => {
            if w.lists.is_empty() {
                w.request(".PP");
                w.request(".RS 4");
            } else if w.item_fresh {
                w.newline();
            } else {
                w.request(".IP \"\" 3");
            }
            w.item_fresh = false;
            w.request(".nf");
        }
        Tag::List(first) => {
            if !w.lists.is_empty() {
                w.newline();
                w.request(".RS 3");
            }
            w.lists.push(first);
        }
        Tag::Item => {
            let tag = match w.lists.last_mut() {
                Some(Some(number)) => {
                    let tag = format!(".IP \"{number}.\" 4");
                    *number += 1;
                    tag
                }
                _ => ".IP \\(bu 3".to_owned(),
            };
            w.request(&tag);
            w.item_fresh = true;
        }
        Tag::Emphasis => {
            w.item_fresh = false;
            w.font('I');
        }
        Tag::Strong => {
            w.item_fresh = false;
            w.font('B');
        }
        Tag::Link { dest_url, .. } => {
            w.item_fresh = false;
            w.links.push((w.out.len(), dest_url.to_string()));
        }
        Tag::Image { .. } => w.skipping = 1,
        Tag::Table(columns) => {
            w.request(".PP");
            // Cells are narrow; justified, a long word cannot be fitted.
            w.request(".na");
            w.request(".TS");
            w.request("allbox;");
            let spec = vec!["lx"; columns.len().max(1)].join(" ");
            w.request(&format!("{spec}."));
        }
        Tag::TableHead | Tag::TableRow => {
            w.cell_in_row = false;
        }
        Tag::TableCell => {
            if w.cell_in_row {
                w.out.push('\t');
            }
            w.cell_in_row = true;
            w.out.push_str("T{\n");
            w.line_start = true;
        }
        _ => {}
    }
}

fn end(w: &mut Writer, tag: TagEnd) {
    match tag {
        TagEnd::Paragraph | TagEnd::Item => w.newline(),
        TagEnd::Heading(_) => {
            let title = w.heading.take().unwrap_or_default();
            let mut escaped = Writer::new();
            escaped.text(&title);
            let line = format!(".SS \"{}\"", escaped.out.replace('"', "\\(dq"));
            w.request(&line);
        }
        TagEnd::BlockQuote(_) => {
            w.request(".RE");
        }
        TagEnd::CodeBlock => {
            w.request(".fi");
            if w.lists.is_empty() {
                w.request(".RE");
            }
        }
        TagEnd::List(_) => {
            w.lists.pop();
            if !w.lists.is_empty() {
                w.request(".RE");
            }
        }
        TagEnd::Emphasis | TagEnd::Strong => w.end_font(),
        TagEnd::Link => {
            if let Some((start, dest)) = w.links.pop() {
                let written = w.out.len() > start;
                let same = w.out.get(start..).is_some_and(|text| text == dest);
                if written && !same && !dest.is_empty() && !dest.starts_with('#') {
                    w.text(" (");
                    w.font('I');
                    w.address(&dest);
                    w.end_font();
                    w.text(")");
                }
            }
        }
        TagEnd::TableCell => {
            w.newline();
            w.out.push_str("T}");
            w.line_start = false;
        }
        TagEnd::TableHead | TagEnd::TableRow => {
            w.out.push('\n');
            w.line_start = true;
        }
        TagEnd::Table => {
            w.request(".TE");
            w.request(".ad");
        }
        _ => {}
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests may panic"
)]
mod tests {
    use super::*;

    #[test]
    fn every_readme_section_is_in_a_page_or_left_out_on_purpose() {
        let (_, sections) = split_readme(README);
        let titles: Vec<&str> = sections.iter().map(|section| section.title).collect();
        let listed: Vec<&str> = SECTIONS.iter().map(|(title, _)| *title).collect();
        for title in &titles {
            assert!(
                listed.contains(title),
                "the README section '{title}' is in no manual page: add it to SECTIONS \
                 in manpage.rs, in a page or as left out, with the reason"
            );
        }
        for title in &listed {
            assert!(
                titles.contains(title),
                "SECTIONS lists '{title}', which is not a section of the README"
            );
        }
        assert_eq!(titles, listed, "SECTIONS keeps the order of the README");
    }

    #[test]
    fn the_sections_split_where_the_headings_are() {
        let (intro, sections) =
            split_readme("# t\n\nintro\n\n## A\n\nx\n\n```\n## not\n```\n\n## B\n\ny\n");
        assert!(intro.contains("intro"));
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].title, "A");
        assert!(sections[0].body.contains("## not"));
        assert_eq!(sections[1].title, "B");
    }

    fn convert(markdown: &str) -> String {
        roff(markdown, false)
    }

    #[test]
    fn text_is_escaped_so_it_reads_the_same_in_the_page() {
        let out = convert("a-b \\ it's `x-y` — z\n\n.dot start\n");
        assert!(
            out.contains("a\\-b \\e it\\(aqs \\fBx\\-y\\fR \\(em z"),
            "{out}"
        );
        assert!(out.contains("\\&.dot start"), "{out}");
    }

    #[test]
    fn a_line_that_starts_with_a_dot_or_quote_is_not_a_request() {
        let out = convert("first\n.second\n'third\n");
        for line in out.lines().skip(1) {
            assert!(!line.starts_with('.') && !line.starts_with('\''), "{out}");
        }
    }

    #[test]
    fn lists_nest_and_number() {
        let out = convert("- one\n  - inner\n- two\n\n1. first\n2. second\n");
        assert!(out.contains(".IP \\(bu 3\none"), "{out}");
        assert!(out.contains(".RS 3\n.IP \\(bu 3\ninner\n.RE"), "{out}");
        assert!(out.contains(".IP \"1.\" 4\nfirst"), "{out}");
        assert!(out.contains(".IP \"2.\" 4\nsecond"), "{out}");
    }

    #[test]
    fn code_blocks_keep_their_lines() {
        let out = convert("```yaml\nid: t\n  - a: 'b'\n.x: \\n\n```\n");
        assert!(
            out.contains(".nf\nid: t\n  \\- a: \\(aqb\\(aq\n\\&.x: \\en\n.fi"),
            "{out}"
        );
    }

    #[test]
    fn links_show_their_address_and_images_nothing() {
        let out = convert("[text](https://example.com/a) and [![img](b.svg)](https://x.y)\n");
        assert!(
            out.contains("text (\\fIhttps:/\\:/\\:example.com/\\:a\\fR)"),
            "{out}"
        );
        assert!(!out.contains("x.y"), "{out}");
        assert!(!out.contains("img"), "{out}");
    }

    #[test]
    fn links_inside_the_readme_show_only_their_text() {
        let out = convert("see [Bit fields](#bit-fields) now\n");
        assert!(out.contains("see Bit fields now"), "{out}");
    }

    #[test]
    fn tables_use_tbl() {
        let out = convert("| a | b |\n|---|---|\n| 1 | `2` |\n");
        assert!(
            out.contains(
                ".TS\nallbox;\nlx lx.\nT{\na\nT}\tT{\nb\nT}\nT{\n1\nT}\tT{\n\\fB2\\fR\nT}\n.TE"
            ),
            "{out}"
        );
    }

    #[test]
    fn headings_below_the_section_are_subsections() {
        let out = convert("### The `x` part\n\ntext\n");
        assert!(
            out.contains(".SS \"The x part\"\n.PP\ntext")
                || out.contains(".SS \"The x part\"\ntext"),
            "{out}"
        );
    }

    #[test]
    fn the_pages_have_their_sections() {
        let doe = String::from_utf8(render(Page::Doe).unwrap()).unwrap();
        assert!(doe.contains(".SH \"TYPE LIBRARY\""), "{doe}");
        assert!(doe.contains(".SH \"OUTPUT\""), "{doe}");
        assert!(!doe.contains(".SH \"SCHEMA FORMAT\""));
        assert!(!doe.contains(".SH \"INSTALLATION\""));
        let schema = String::from_utf8(render(Page::Schema).unwrap()).unwrap();
        assert!(schema.contains(".TH doe\\-schema 5"), "{schema}");
        assert!(schema.contains(".SH \"SCHEMA FORMAT\""));
        assert!(schema.contains(".SH \"KAITAI STRUCT COMPATIBILITY\""));
        assert!(!schema.contains(".SH \"OUTPUT\""));
    }
}
