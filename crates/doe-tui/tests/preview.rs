//! The preview of a file of an archive in the terminal viewer: Enter or `p` on a file of the Files view, highlighting by
//! the name or a `#!` line, scrolling, the limits, and `s` to save the entry. No terminal.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::panic,
    clippy::format_collect,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::Options;
use doe_tui::app::App;
use doe_tui::load::load;
use doe_tui::preview::{LINE_LIMIT, PREVIEW_BYTES};
use doe_tui::ui::{Theme, draw};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;

/// A tar (ustar) of regular files.
fn tar(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, content) in files {
        let mut header = [0_u8; 512];
        let mut put =
            |at: usize, text: &str| header[at..at + text.len()].copy_from_slice(text.as_bytes());
        put(0, name);
        put(100, "0000644\0");
        put(108, "0000000\0");
        put(116, "0000000\0");
        put(124, &format!("{:011o}\0", content.len()));
        put(136, "00000000000\0");
        put(148, "        ");
        put(156, "0");
        put(257, "ustar\0");
        put(263, "00");
        let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
        header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(content);
        out.resize(out.len().div_ceil(512) * 512, 0);
    }
    out.resize(out.len() + 1024, 0);
    out
}

fn app_of(dir: &Path, files: &[(&str, &[u8])]) -> App {
    let file = dir.join("a.tar");
    std::fs::write(&file, tar(files)).unwrap();
    let options = Options {
        file: file.clone(),
        type_or_schema: "tar".to_owned(),
        include_paths: vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("../../types")],
        color: true,
        watch: false,
        theme: None,
        theme_dir: None,
        config_file: None,
        outer_file: None,
        theme_on_command_line: false,
        transparent: None,
        radix: None,
    };
    let mut app = App::new(file, load(&options).unwrap());
    app.options = Some(options);
    app
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn key(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

fn draw_buffer(app: &mut App, color: bool, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| draw(f, app, &Theme::new(color))).unwrap();
    terminal.backend().buffer().clone()
}

fn rows(buffer: &Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

/// The foreground colour of the text `needle` in the first row that has it.
fn colour_of(buffer: &Buffer, needle: &str) -> Color {
    for (y, row) in rows(buffer).iter().enumerate() {
        if let Some(at) = row.find(needle) {
            let x = row[..at].chars().count();
            return buffer[(u16::try_from(x).unwrap(), u16::try_from(y).unwrap())].fg;
        }
    }
    panic!("{needle:?} is not on the screen: {:#?}", rows(buffer));
}

/// Select the file `name` of the (flat) archive and open its preview with Enter.
fn select(app: &mut App, name: &str) {
    press(app, KeyCode::Home);
    for _ in 0..20 {
        if app.files.as_ref().unwrap().selected_path() == name {
            return;
        }
        press(app, KeyCode::Down);
    }
    panic!("{name} is not in the files");
}

const RUST: &str = "fn main() {\n\t// say hello\n\tlet n = 42;\n\tprintln!(\"hello\");\n}\n";

#[test]
fn enter_previews_a_rust_file_with_line_numbers_and_the_colours_of_its_tokens() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_of(
        dir.path(),
        &[("main.rs", RUST.as_bytes()), ("notes.xyz", b"fn x() {}\n")],
    );
    select(&mut app, "main.rs");
    press(&mut app, KeyCode::Enter);
    assert!(app.preview.is_some());
    let buffer = draw_buffer(&mut app, true, 100, 20);
    let lines = rows(&buffer);
    let text = lines.join("\n");
    assert!(text.contains("main.rs · Rust"), "{text}");
    // line numbers, the bar, and the text with the tab as spaces
    assert!(
        lines.iter().any(|l| l.contains("1 │ fn main() {")),
        "{text}"
    );
    assert!(
        lines.iter().any(|l| l.contains("3 │     let n = 42;")),
        "{text}"
    );
    assert!(text.contains("s saves the entry"), "{text}");
    // the tokens have colours of their own, and plain text has none
    let keyword = colour_of(&buffer, "fn main");
    let number = colour_of(&buffer, "42");
    let string = colour_of(&buffer, "\"hello\"");
    assert_ne!(keyword, Color::Reset);
    assert_ne!(number, Color::Reset);
    assert_ne!(string, Color::Reset);
    assert!(keyword != number && number != string && keyword != string);
    // the same text with no colour has none
    let plain = draw_buffer(&mut app, false, 100, 20);
    assert_eq!(colour_of(&plain, "42"), Color::Reset);
    // Esc goes back to the files, and does not leave
    press(&mut app, KeyCode::Esc);
    assert!(app.preview.is_none() && !app.quit);
    assert!(app.files_shown());
}

#[test]
fn p_previews_too_and_an_unknown_file_is_plain_and_a_shebang_names_the_language() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_of(
        dir.path(),
        &[
            ("notes.xyz", b"fn main() { let x = 1; }\n"),
            ("run", b"#!/usr/bin/env python3\nprint(1)  # hi\n"),
            ("Makefile", b"all:\n\techo hi # note\n"),
        ],
    );
    select(&mut app, "notes.xyz");
    key(&mut app, 'p');
    let buffer = draw_buffer(&mut app, true, 100, 20);
    assert!(rows(&buffer).join("\n").contains("notes.xyz · plain"));
    assert_eq!(colour_of(&buffer, "fn main"), Color::Reset);
    key(&mut app, 'q');
    assert!(app.preview.is_none() && !app.quit);
    select(&mut app, "run");
    key(&mut app, 'p');
    let buffer = draw_buffer(&mut app, true, 100, 20);
    assert!(rows(&buffer).join("\n").contains("run · Python"));
    // the number is coloured (a comment is dim, which is not a colour)
    assert_ne!(colour_of(&buffer, "1)"), Color::Reset);
    key(&mut app, 'q');
    select(&mut app, "Makefile");
    press(&mut app, KeyCode::Enter);
    let buffer = draw_buffer(&mut app, true, 100, 20);
    assert!(rows(&buffer).join("\n").contains("Makefile · Shell"));
}

#[test]
fn binary_and_empty_entries_are_not_previewed_and_say_why() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_of(
        dir.path(),
        &[("blob.bin", &[0, 1, 2, 3, 0xff]), ("empty.txt", b"")],
    );
    select(&mut app, "blob.bin");
    press(&mut app, KeyCode::Enter);
    assert!(app.preview.is_none());
    assert_eq!(app.notice.as_deref(), Some("5 bytes (not text)"));
    select(&mut app, "empty.txt");
    key(&mut app, 'p');
    // an empty entry has no contents to show (or an empty preview): never a panic, and the viewer is still up
    let _ = draw_buffer(&mut app, true, 100, 20);
    assert!(!app.quit);
}

#[test]
fn a_long_file_scrolls_by_the_arrow_keys_pages_home_and_end() {
    let dir = tempfile::tempdir().unwrap();
    let text: String = (1..=100).map(|i| format!("line {i}\n")).collect();
    let mut app = app_of(dir.path(), &[("long.txt", text.as_bytes())]);
    select(&mut app, "long.txt");
    press(&mut app, KeyCode::Enter);
    let first = rows(&draw_buffer(&mut app, false, 100, 20)).join("\n");
    assert!(
        first.contains("  1 │ line 1") && !first.contains("line 30"),
        "{first}"
    );
    press(&mut app, KeyCode::Down);
    let screen = rows(&draw_buffer(&mut app, false, 100, 20)).join("\n");
    assert!(
        !screen.contains("  1 │ line 1\n") && screen.contains("  2 │ line 2"),
        "{screen}"
    );
    press(&mut app, KeyCode::PageDown);
    let screen = rows(&draw_buffer(&mut app, false, 100, 20)).join("\n");
    assert!(
        screen.contains("line 19") && !screen.contains("  2 │ line 2\n"),
        "{screen}"
    );
    press(&mut app, KeyCode::End);
    let screen = rows(&draw_buffer(&mut app, false, 100, 20)).join("\n");
    assert!(screen.contains("100 │ line 100"), "{screen}");
    press(&mut app, KeyCode::Home);
    let screen = rows(&draw_buffer(&mut app, false, 100, 20)).join("\n");
    assert!(screen.contains("  1 │ line 1"), "{screen}");
}

#[test]
fn an_entry_over_the_limit_ends_with_a_mark_and_a_long_line_is_cut() {
    let dir = tempfile::tempdir().unwrap();
    let big: String = "let x = 1; // line\n".repeat(PREVIEW_BYTES / 19 + 100);
    let minified = format!("{}\nend\n", "x".repeat(LINE_LIMIT * 3));
    let mut app = app_of(
        dir.path(),
        &[("big.rs", big.as_bytes()), ("min.js", minified.as_bytes())],
    );
    select(&mut app, "big.rs");
    press(&mut app, KeyCode::Enter);
    let preview = app.preview.as_ref().unwrap();
    assert!(preview.cut);
    assert_eq!(preview.lines.last().map(String::as_str), Some("…"));
    assert!(preview.lines.len() < big.lines().count());
    press(&mut app, KeyCode::Esc);
    select(&mut app, "min.js");
    press(&mut app, KeyCode::Enter);
    let preview = app.preview.as_ref().unwrap();
    assert_eq!(preview.lines[0], format!("{}…", "x".repeat(LINE_LIMIT)));
    assert_eq!(preview.lines[1], "end");
    // and it is drawn in a narrow terminal without stalling or panicking
    let _ = draw_buffer(&mut app, true, 40, 10);
}

#[test]
fn s_saves_the_whole_entry_and_never_over_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let big: String = "let x = 1; // line\n".repeat(PREVIEW_BYTES / 19 + 100);
    let mut app = app_of(dir.path(), &[("big.rs", big.as_bytes())]);
    select(&mut app, "big.rs");
    press(&mut app, KeyCode::Enter);
    assert!(app.preview.as_ref().unwrap().cut);
    key(&mut app, 's');
    let prompt = app.prompt.as_ref().unwrap();
    // the prompt starts with the name of the entry
    assert_eq!(prompt.line.text(), "big.rs");
    let target = dir.path().join("saved.rs");
    for _ in 0.."big.rs".len() {
        press(&mut app, KeyCode::Backspace);
    }
    for c in target.to_string_lossy().chars() {
        key(&mut app, c);
    }
    let screen = rows(&draw_buffer(&mut app, false, 120, 20)).join("\n");
    assert!(screen.contains("save the entry as"), "{screen}");
    press(&mut app, KeyCode::Enter);
    // the whole entry, not the part that is shown
    assert_eq!(std::fs::read(&target).unwrap(), big.as_bytes());
    assert!(
        app.notice
            .as_deref()
            .unwrap()
            .starts_with(&format!("saved {} bytes", big.len())),
        "{:?}",
        app.notice
    );
    // the preview is still open; saving again to the same file is refused
    assert!(app.preview.is_some());
    key(&mut app, 's');
    for _ in 0.."big.rs".len() {
        press(&mut app, KeyCode::Backspace);
    }
    for c in target.to_string_lossy().chars() {
        key(&mut app, c);
    }
    press(&mut app, KeyCode::Enter);
    assert!(
        app.notice.as_deref().unwrap().contains("is there already"),
        "{:?}",
        app.notice
    );
    assert_eq!(std::fs::read(&target).unwrap(), big.as_bytes());
    // Esc in the prompt only closes the prompt
    key(&mut app, 's');
    press(&mut app, KeyCode::Esc);
    assert!(app.prompt.is_none() && app.preview.is_some());
}

#[test]
fn p_without_a_file_selected_or_without_files_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_of(dir.path(), &[("d/x.txt", b"x\n")]);
    // the directory is selected first
    key(&mut app, 'p');
    assert!(app.preview.is_none());
    assert!(
        app.notice.as_deref().unwrap().contains("directory"),
        "{:?}",
        app.notice
    );
    // in the tree of fields the preview is in the files
    key(&mut app, 'F');
    key(&mut app, 'p');
    assert!(
        app.notice.as_deref().unwrap().contains("press F"),
        "{:?}",
        app.notice
    );
}

const README: &str = "# Title

Some *emphasis* and **strong** words and a [link](https://example.test/a) that is long enough to wrap in a narrow pane of the viewer.

- one
  - nested
- [x] done

> quoted

| a | b |
|---|---|
| 1 | 2 |

```rust
fn main() { let n = 42; }
```
";

#[test]
fn r_renders_a_markdown_entry_and_back_and_the_choice_is_kept_for_the_next_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_of(
        dir.path(),
        &[
            ("README.md", README.as_bytes()),
            (
                "other.md",
                b"# Other
",
            ),
            ("main.rs", RUST.as_bytes()),
        ],
    );
    select(&mut app, "README.md");
    press(&mut app, KeyCode::Enter);
    // the source first, highlighted, with line numbers
    let lines = rows(&draw_buffer(&mut app, true, 100, 30)).join(
        "
",
    );
    assert!(
        lines.contains("README.md · Markdown") && lines.contains("1 │ # Title"),
        "{lines}"
    );
    assert!(lines.contains("r source/rendered"), "{lines}");
    key(&mut app, 'r');
    let buffer = draw_buffer(&mut app, true, 100, 30);
    let lines = rows(&buffer).join(
        "
",
    );
    assert!(lines.contains("(rendered)"), "{lines}");
    // no source marks: the heading without its #, the bullets, the table box, the quote bar and the link target
    assert!(
        lines.contains("Title") && !lines.contains("# Title"),
        "{lines}"
    );
    assert!(
        lines.contains("• one") && lines.contains("  • nested") && lines.contains("☑ done"),
        "{lines}"
    );
    assert!(
        lines.contains("│ quoted") && lines.contains("│ a │ b │"),
        "{lines}"
    );
    assert!(lines.contains("link <https://example.test/a>"), "{lines}");
    assert!(
        !lines.contains("**strong**") && !lines.contains("*emphasis*"),
        "{lines}"
    );
    // the code block is coloured by its fence (with the terminal's own 16 colours), and the heading has its style
    assert_ne!(colour_of(&buffer, "fn"), Color::Reset);
    assert_ne!(colour_of(&buffer, "42"), Color::Reset);
    assert_ne!(colour_of(&buffer, "Title"), Color::Reset);
    // with no colour the text is the same and nothing is coloured
    let plain = draw_buffer(&mut app, false, 100, 30);
    assert_eq!(colour_of(&plain, "42"), Color::Reset);
    // r again: the source
    key(&mut app, 'r');
    assert!(
        rows(&draw_buffer(&mut app, true, 100, 30))
            .join(
                "
"
            )
            .contains("1 │ # Title")
    );
    // v does it too, and the choice is kept for the next Markdown entry, not for a file that is not Markdown
    key(&mut app, 'v');
    press(&mut app, KeyCode::Esc);
    select(&mut app, "other.md");
    press(&mut app, KeyCode::Enter);
    assert!(app.preview.as_ref().unwrap().rendered);
    assert!(
        rows(&draw_buffer(&mut app, true, 100, 30))
            .join(
                "
"
            )
            .contains("(rendered)")
    );
    press(&mut app, KeyCode::Esc);
    select(&mut app, "main.rs");
    press(&mut app, KeyCode::Enter);
    assert!(!app.preview.as_ref().unwrap().rendered);
    // r on a file that is not Markdown does nothing
    key(&mut app, 'r');
    assert!(!app.preview.as_ref().unwrap().rendered);
}

#[test]
fn the_rendering_wraps_again_when_the_terminal_is_resized_and_scrolls_within_its_own_lines() {
    let dir = tempfile::tempdir().unwrap();
    let words: String = (0..80).map(|i| format!("word{i} ")).collect();
    let text = format!(
        "# H

{words}
"
    );
    let mut app = app_of(dir.path(), &[("a.md", text.as_bytes())]);
    select(&mut app, "a.md");
    press(&mut app, KeyCode::Enter);
    key(&mut app, 'r');
    let wide = rows(&draw_buffer(&mut app, false, 120, 12));
    let narrow = rows(&draw_buffer(&mut app, false, 40, 12));
    let longest = |lines: &[String]| lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    assert!(longest(&narrow) <= 40, "{narrow:#?}");
    // the same words, on more lines: the last word is not on the first screen of the narrow one
    assert!(
        rows(&draw_buffer(&mut app, false, 120, 12))
            .join(
                "
"
            )
            .contains("word79"),
        "{wide:#?}"
    );
    assert!(
        !narrow
            .join(
                "
"
            )
            .contains("word79"),
        "{narrow:#?}"
    );
    // End goes to the last line of the rendering, whatever the width
    let _ = draw_buffer(&mut app, false, 40, 12);
    press(&mut app, KeyCode::End);
    let end = rows(&draw_buffer(&mut app, false, 40, 12)).join(
        "
",
    );
    assert!(end.contains("word79"), "{end}");
    // and back to the source, from the top
    key(&mut app, 'r');
    assert_eq!(app.preview.as_ref().unwrap().top, 0);
}

#[test]
fn raw_html_in_a_markdown_entry_is_text_in_the_rendering() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_of(
        dir.path(),
        &[(
            "evil.md",
            b"<script>alert(1)</script>

[click](javascript:alert(4)) ![p](https://evil.test/p.png)
",
        )],
    );
    select(&mut app, "evil.md");
    press(&mut app, KeyCode::Enter);
    key(&mut app, 'r');
    let lines = rows(&draw_buffer(&mut app, false, 100, 12)).join(
        "
",
    );
    assert!(lines.contains("<script>alert(1)</script>"), "{lines}");
    assert!(
        lines.contains("click <javascript:alert(4)>") && lines.contains("[image: p]"),
        "{lines}"
    );
}
