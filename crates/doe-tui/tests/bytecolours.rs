//! The bytes pane draws each byte in the colour of the field it belongs to (the colour its value has in the tree), and the
//! selection is as it was. No terminal.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::panic,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::Options;
use doe_tui::app::App;
use doe_tui::load::load;
use doe_tui::theme::{self, Depth, Palette, contrast};
use doe_tui::tree::Shown;
use doe_tui::ui::{Theme, draw};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier, Style};

const SCHEMA: &str = "meta:
  id: example
  endian: le
seq:
  - id: magic
    contents: \"DOE\"
  - id: count
    type: u1
  - id: entries
    type: entry
    repeat: expr
    repeat-expr: count
types:
  entry:
    seq:
      - id: kind
        type: u1
        enum: kinds
      - id: len
        type: u1
      - id: text
        type: str
        size: len
        encoding: ASCII
enums:
  kinds:
    1: greeting
    2: farewell
";

/// "DOE", 2, then a greeting "hello" and a farewell "bye".
const BYTES: [u8; 16] = [
    b'D', b'O', b'E', 2, 1, 5, b'h', b'e', b'l', b'l', b'o', 2, 3, b'b', b'y', b'e',
];

fn app(dir: &Path) -> App {
    std::fs::write(dir.join("example.yaml"), SCHEMA).unwrap();
    std::fs::write(dir.join("example.bin"), BYTES).unwrap();
    let options = Options {
        file: dir.join("example.bin"),
        type_or_schema: dir.join("example.yaml").to_string_lossy().into_owned(),
        include_paths: Vec::new(),
        color: true,
        watch: false,
        theme: None,
        theme_dir: None,
        config_file: None,
        outer_file: None,
        theme_on_command_line: false,
        transparent: None,
        mouse: true,
        radix: None,
    };
    let mut app = App::new(options.file.clone(), load(&options).unwrap());
    app.options = Some(options);
    app
}

fn key(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn draw_with(app: &mut App, theme: &Theme) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(200, 14)).unwrap();
    terminal.draw(|f| draw(f, app, theme)).unwrap();
    terminal.backend().buffer().clone()
}

/// The style of the cell of the hex of byte `i` of the first line of the bytes (16 to a line is not assumed: the line that
/// has the text `44 4f 45` is looked for).
fn hex_cell(buffer: &Buffer, i: usize) -> Style {
    for y in 0..buffer.area.height {
        let row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        if let Some(at) = row.find("44 4f 45") {
            let x0 = row[..at].chars().count();
            // a wider gap after the eighth byte
            let x = x0 + 3 * i + usize::from(i >= 8);
            let cell = &buffer[(u16::try_from(x).unwrap(), y)];
            return cell.style();
        }
    }
    panic!("the bytes are not on the screen");
}

fn fg(style: Style) -> Option<Color> {
    style.fg
}

/// No colour of its own: the default one, which a cell reports as Reset.
fn is_plain(color: Option<Color>) -> bool {
    matches!(color, None | Some(Color::Reset))
}

#[test]
fn every_byte_has_the_colour_its_field_has_in_the_tree() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    let theme = Theme::new(true);
    // the first row of the tree (the whole file) is selected at first: select count, so that the other bytes are plain
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    let buffer = draw_with(&mut app, &theme);
    let value = |shown: &Shown| theme.value(shown).fg;
    // magic is a run of bytes, count a number, kind an enum, len a number, text a string
    let bytes = Shown::Bytes {
        len: 3,
        preview: vec![],
    };
    let number = Shown::UInt(2);
    let enumeration = Shown::Enum {
        value: 1,
        name: Some("greeting".to_owned()),
    };
    let string = Shown::Str {
        text: "hello".to_owned(),
        cut: false,
    };
    assert_eq!(fg(hex_cell(&buffer, 0)), value(&bytes));
    assert_eq!(fg(hex_cell(&buffer, 4)), value(&enumeration));
    assert_eq!(fg(hex_cell(&buffer, 5)), value(&number));
    assert_eq!(fg(hex_cell(&buffer, 6)), value(&string));
    assert_eq!(fg(hex_cell(&buffer, 10)), value(&string));
    assert_eq!(fg(hex_cell(&buffer, 11)), value(&enumeration));
    assert_eq!(fg(hex_cell(&buffer, 15)), value(&string));
    // the kinds are different colours
    let colours: std::collections::HashSet<_> = [0, 4, 5, 6]
        .iter()
        .map(|i| format!("{:?}", fg(hex_cell(&buffer, *i))))
        .collect();
    assert_eq!(colours.len(), 4);
    // the selected byte is the selection's, not the colour of its kind
    assert_ne!(fg(hex_cell(&buffer, 3)), value(&number));
}

#[test]
fn b_turns_the_colours_off_and_on_and_no_colour_has_none() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    let theme = Theme::new(true);
    // count is selected, so that the bytes around it are plain when the colours are off
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert!(!is_plain(fg(hex_cell(&draw_with(&mut app, &theme), 4))));
    key(&mut app, 'B');
    assert!(!app.byte_colours);
    assert!(
        app.notice.as_deref().unwrap().contains("not coloured"),
        "{:?}",
        app.notice
    );
    let off = draw_with(&mut app, &theme);
    assert!(
        (0..16)
            .filter(|i| *i != 3)
            .all(|i| is_plain(fg(hex_cell(&off, i))))
    );
    key(&mut app, 'B');
    assert!(!is_plain(fg(hex_cell(&draw_with(&mut app, &theme), 4))));
    // with no colour (NO_COLOR) nothing is added
    let none = draw_with(&mut app, &Theme::new(false));
    assert!((0..16).all(|i| is_plain(fg(hex_cell(&none, i)))));
}

#[test]
fn the_selection_is_as_it_was_over_the_colours() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    let theme = Theme::new(true);
    // the tree has the root, magic, count: Down twice selects count
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.tree.selected().name, "count");
    let coloured = hex_cell(&draw_with(&mut app, &theme), 3);
    assert!(
        coloured.add_modifier.contains(Modifier::REVERSED),
        "{coloured:?}"
    );
    app.byte_colours = false;
    let plain = hex_cell(&draw_with(&mut app, &theme), 3);
    // with the terminal's own colours the selected byte is drawn exactly as without the colours
    assert_eq!(coloured, plain);
    // and the byte next to it still has its colour
    app.byte_colours = true;
    assert!(!is_plain(fg(hex_cell(&draw_with(&mut app, &theme), 4))));
}

#[test]
fn in_a_theme_the_selected_bytes_keep_the_selection_background_and_a_readable_colour_of_their_kind()
{
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.tree.selected().name, "count");
    let mut moved = 0;
    let mut checked = 0;
    for colours in theme::builtin() {
        let palette = Palette::new(&colours.colors, Depth::True);
        let themed = Theme::themed(palette);
        let buffer = draw_with(&mut app, &themed);
        let selected = hex_cell(&buffer, 3);
        assert_eq!(
            selected.bg,
            Some(palette.selection),
            "{}: the selection background",
            colours.id
        );
        let text = selected.fg.unwrap();
        assert!(
            contrast(text, palette.selection).unwrap() >= doe_tui::theme::SELECTED_CONTRAST,
            "{}: {text:?} on {:?}",
            colours.id,
            palette.selection
        );
        // a byte that is not selected has the plain colour of its kind
        let other = hex_cell(&buffer, 6);
        assert_eq!(
            other.fg,
            themed
                .value(&Shown::Str {
                    text: String::new(),
                    cut: false
                })
                .fg,
            "{}",
            colours.id
        );
        if text != palette.number {
            moved += 1;
        }
        checked += 1;
    }
    assert!(checked > 20);
    // most colours are readable on the selection as they are
    assert!(
        moved < checked / 2,
        "{moved} of {checked} themes move the colour of a number"
    );
}

#[test]
fn the_cursor_byte_and_the_range_of_v_keep_their_own_styles_over_the_colours() {
    use doe_tui::hex_view::hex_line_with;
    use doe_tui::kinds::{ByteKinds, Kind, Leaf};
    let kinds = ByteKinds::from_leaves(vec![Leaf {
        first: 0,
        end: 5,
        kind: Some(Kind::Number),
        priority: 0,
    }]);
    let theme = Theme::new(true);
    let line = hex_line_with(
        0,
        &[1, 2, 3, 4, 5],
        5,
        8,
        None,
        Some(1),
        false,
        Some((3, 4)),
        Some(&kinds),
        &theme,
    );
    // the spans: offset, gap, then each byte with a gap after it
    let bytes: Vec<_> = line
        .spans
        .iter()
        .filter(|s| s.content.len() == 2 && s.content.chars().all(|c| c.is_ascii_hexdigit()))
        .collect();
    assert_eq!(bytes.len(), 5);
    let number = theme.value(&Shown::UInt(0)).fg;
    // byte 0 has the colour of its kind
    assert_eq!(bytes[0].style.fg, number);
    // the cursor (byte 1) is the cursor, not the kind
    assert_eq!(bytes[1].style, theme.hex_cursor());
    // byte 2 is a plain number, bytes 3 and 4 are in the range of v: reverse and italic, with no colour of their own
    assert_eq!(bytes[2].style.fg, number);
    for byte in &bytes[3..] {
        assert!(
            byte.style
                .add_modifier
                .contains(Modifier::REVERSED | Modifier::ITALIC)
        );
        assert_ne!(byte.style.fg, number);
    }
}
