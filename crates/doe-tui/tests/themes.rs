//! The built-in themes: the table is well formed, every theme is readable, the
//! web IDE's file is made from the table, the README says where the themes come
//! from, and the viewer draws with the colours of a theme and no others.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use doe_tui::Options;
use doe_tui::app::App;
use doe_tui::load::load;
use doe_tui::theme::{self, ColorTheme, Depth, Palette, Rgb, Variant};
use doe_tui::ui::{Theme, draw};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;

const README: &str = include_str!("../../../README.md");

/// The least ratio between the text and the background (WCAG AA for text).
const AA: f64 = 4.5;

fn find(id: &str) -> &'static ColorTheme {
    theme::builtin()
        .iter()
        .find(|t| t.id == id)
        .unwrap_or_else(|| panic!("no theme {id}"))
}

#[test]
fn the_table_has_a_dozen_themes_and_every_theme_says_where_it_is_from() {
    let themes = theme::builtin();
    assert!(themes.len() >= 12, "{}", themes.len());
    let mut ids = HashSet::new();
    for t in themes {
        assert!(ids.insert(t.id.as_str()), "{} twice", t.id);
        assert!(
            !t.id.is_empty()
                && t.id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{:?} is not an id",
            t.id
        );
        assert_ne!(t.name, "");
        for (what, value) in [
            ("author", &t.author),
            ("license", &t.license),
            ("source", &t.source),
        ] {
            assert!(
                value.as_deref().is_some_and(|v| !v.is_empty()),
                "{} has no {what}",
                t.id
            );
        }
        // only licences that allow the theme to be redistributed
        assert_eq!(t.license.as_deref(), Some("MIT"), "{}", t.id);
    }
    let light = themes
        .iter()
        .filter(|t| t.variant == Variant::Light)
        .count();
    assert!(light >= 4 && light < themes.len(), "{light} light themes");
}

#[test]
fn the_variant_says_which_way_the_background_goes() {
    for t in theme::builtin() {
        let dark = t.colors.background.luminance() < t.colors.foreground.luminance();
        assert_eq!(dark, t.variant == Variant::Dark, "{}", t.id);
    }
}

#[test]
fn the_text_and_the_error_colour_are_readable_on_the_background_in_every_theme() {
    for t in theme::builtin() {
        let c = &t.colors;
        for (what, color) in [("foreground", c.foreground), ("error", c.error)] {
            let ratio = color.contrast(c.background);
            assert!(
                ratio >= AA,
                "{}: {what} {} on {} is {ratio:.2}:1, below {AA}:1",
                t.id,
                color.hex(),
                c.background.hex()
            );
        }
    }
}

#[test]
fn the_text_is_readable_on_the_selection_too() {
    for t in theme::builtin() {
        let c = &t.colors;
        for (what, on) in [
            ("selection", c.selection),
            ("selection_part", c.selection_part),
        ] {
            let ratio = c.foreground.contrast(on);
            assert!(
                ratio >= AA,
                "{}: foreground on {what} is {ratio:.2}:1",
                t.id
            );
        }
    }
}

#[test]
fn the_high_contrast_themes_are_seven_to_one() {
    for id in ["high-contrast", "high-contrast-light"] {
        let c = &find(id).colors;
        for (name, color) in c.named() {
            if matches!(
                name,
                "background" | "line" | "panel" | "selection" | "selection_part"
            ) {
                continue;
            }
            assert!(
                color.contrast(c.background) >= 7.0,
                "{id}: {name} is {:.2}:1",
                color.contrast(c.background)
            );
        }
    }
}

#[test]
fn colours_are_read_and_the_contrast_is_the_one_of_wcag() {
    assert_eq!(Rgb::parse("#0a1B2c").unwrap(), Rgb(10, 27, 44));
    assert_eq!(Rgb(10, 27, 44).hex(), "#0a1b2c");
    for bad in [
        "", "#fff", "123456", "#12345g", "#1234567", "red", "# 12345",
    ] {
        assert!(Rgb::parse(bad).is_err(), "{bad:?}");
    }
    let (black, white) = (Rgb(0, 0, 0), Rgb(255, 255, 255));
    assert!((black.contrast(white) - 21.0).abs() < 1e-9);
    assert!((white.contrast(white) - 1.0).abs() < 1e-9);
    // a known pair: #767676 on white is the least grey that is 4.5:1
    assert!(Rgb(0x76, 0x76, 0x76).contrast(white) >= 4.5);
    assert!(Rgb(0x77, 0x77, 0x77).contrast(white) < 4.5);
}

#[test]
fn a_name_is_found_however_it_is_written() {
    for name in ["nord", "Nord", " NORD "] {
        assert_eq!(theme::resolve(name, None).unwrap().id, "nord");
    }
    for name in ["Solarized Dark", "solarized_dark", "solarized-dark"] {
        assert_eq!(theme::resolve(name, None).unwrap().id, "solarized-dark");
    }
}

#[test]
fn an_unknown_theme_is_an_error_that_lists_the_ones_there_are() {
    let error = theme::resolve("nope", None).unwrap_err().to_string();
    assert!(error.contains("nope"), "{error}");
    for t in theme::builtin() {
        assert!(error.contains(&t.id), "{} is not in {error}", t.id);
    }
}

fn user_theme(name: &str) -> String {
    let mut text = format!("name = \"{name}\"\nvariant = \"light\"\n[colors]\n");
    for (key, color) in find("doe-light").colors.named() {
        writeln!(text, "{key} = \"{}\"", color.hex()).unwrap();
    }
    text
}

#[test]
fn a_theme_is_found_in_the_directory_of_the_person_and_by_path() {
    let dir = tempfile::tempdir().unwrap();
    let themes = dir.path().join("themes");
    std::fs::create_dir(&themes).unwrap();
    std::fs::write(themes.join("mine.toml"), user_theme("Mine")).unwrap();
    let mine = theme::resolve("mine", Some(&themes)).unwrap();
    assert_eq!(mine.name, "Mine");
    assert_eq!(mine.variant, Variant::Light);
    assert_eq!(mine.colors, find("doe-light").colors);

    let elsewhere = dir.path().join("elsewhere.toml");
    std::fs::write(&elsewhere, user_theme("Elsewhere")).unwrap();
    let by_path = theme::resolve(elsewhere.to_str().unwrap(), None).unwrap();
    assert_eq!(by_path.name, "Elsewhere");

    // a built-in theme is not hidden by a file
    std::fs::write(themes.join("nord.toml"), user_theme("Not Nord")).unwrap();
    assert_eq!(theme::resolve("nord", Some(&themes)).unwrap().name, "Nord");

    assert_eq!(
        theme::user_dir(Some(&dir.path().join("config.toml"))).unwrap(),
        themes
    );
    assert_eq!(theme::user_dir(None), None);
}

#[test]
fn a_theme_file_that_is_not_usable_says_what_is_wrong() {
    let dir = tempfile::tempdir().unwrap();
    let write = |text: &str| -> PathBuf {
        let path = dir.path().join("t.toml");
        std::fs::write(&path, text).unwrap();
        path
    };
    let bad = |text: &str| -> String {
        let path = write(text);
        format!(
            "{:#}",
            theme::resolve(path.to_str().unwrap(), None).unwrap_err()
        )
    };
    let full = user_theme("T");
    // a missing colour
    let without = full.replace("accent = ", "# accent = ");
    assert!(bad(&without).contains("accent"), "{}", bad(&without));
    // a colour that is not one
    let wrong = full.replace("\nbackground = \"#ffffff\"", "\nbackground = \"white\"");
    assert!(bad(&wrong).contains("white"), "{}", bad(&wrong));
    // a colour that is not in the table
    let extra = format!("{full}sparkle = \"#ffffff\"\n");
    assert!(bad(&extra).contains("sparkle"), "{}", bad(&extra));
    // not TOML
    assert!(bad("{{{").contains("t.toml"));
}

#[test]
fn the_depth_of_the_terminal_is_what_the_environment_says() {
    use Depth::{Basic, Indexed, True};
    assert_eq!(Depth::from_env(Some("truecolor"), Some("xterm")), True);
    assert_eq!(Depth::from_env(Some("24bit"), None), True);
    assert_eq!(Depth::from_env(Some("TrueColor"), None), True);
    assert_eq!(Depth::from_env(None, Some("xterm-direct")), True);
    assert_eq!(Depth::from_env(None, Some("xterm-256color")), Indexed);
    assert_eq!(Depth::from_env(Some(""), Some("screen-256color")), Indexed);
    assert_eq!(Depth::from_env(None, Some("xterm")), Basic);
    assert_eq!(Depth::from_env(None, Some("linux")), Basic);
    assert_eq!(Depth::from_env(None, None), Basic);
}

#[test]
fn colours_are_brought_down_to_the_nearest_the_terminal_has() {
    let nord = &find("nord").colors;
    assert_eq!(Depth::True.color(Rgb(1, 2, 3)), Color::Rgb(1, 2, 3));
    // 256 colours: the cube and the greys
    assert_eq!(Depth::Indexed.color(Rgb(255, 0, 0)), Color::Indexed(196));
    assert_eq!(Depth::Indexed.color(Rgb(0, 0, 0)), Color::Indexed(16));
    assert_eq!(
        Depth::Indexed.color(Rgb(255, 255, 255)),
        Color::Indexed(231)
    );
    assert_eq!(
        Depth::Indexed.color(Rgb(128, 128, 128)),
        Color::Indexed(244)
    );
    // 16 colours
    assert_eq!(Depth::Basic.color(Rgb(0, 0, 0)), Color::Black);
    assert_eq!(Depth::Basic.color(Rgb(255, 255, 255)), Color::White);
    assert_eq!(Depth::Basic.color(Rgb(250, 90, 90)), Color::LightRed);
    // the background of a dark theme stays dark, and the text stays light
    for depth in [Depth::Indexed, Depth::Basic] {
        let p = Palette::new(nord, depth);
        assert!(
            matches!(p.background, Color::Indexed(16..=19 | 232..=240) | Color::Black | Color::DarkGray),
            "{:?}",
            p.background
        );
        assert_ne!(p.background, p.foreground);
    }
}

// ── the web IDE's file ───────────────────────────────────────────────────────

fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// `web/src/themes.generated.ts`, as the table says it should be.
fn generated() -> String {
    let mut out = String::new();
    out.push_str(
        "// Generated from crates/doe-tui/themes/themes.toml by\n\
         //   DOE_UPDATE_GENERATED=1 cargo test -p doe-tui --test themes\n\
         // Do not edit: a test fails when this file is not what the table makes.\n\n\
         export interface ThemeColors {\n",
    );
    let first = theme::builtin().first().unwrap();
    for (name, _) in first.colors.named() {
        writeln!(out, "  {name}: string;").unwrap();
    }
    out.push_str(
        "}\n\n\
         export interface ThemeInfo {\n  id: string;\n  name: string;\n  variant: 'dark' | 'light';\n  \
         author: string;\n  license: string;\n  source: string;\n  note?: string;\n  colors: ThemeColors;\n}\n\n\
         export const THEMES: readonly ThemeInfo[] = [\n",
    );
    for t in theme::builtin() {
        out.push_str("  {\n");
        writeln!(out, "    id: {},", quoted(&t.id)).unwrap();
        writeln!(out, "    name: {},", quoted(&t.name)).unwrap();
        writeln!(out, "    variant: {},", quoted(t.variant.name())).unwrap();
        writeln!(out, "    author: {},", quoted(t.author.as_deref().unwrap())).unwrap();
        writeln!(
            out,
            "    license: {},",
            quoted(t.license.as_deref().unwrap())
        )
        .unwrap();
        writeln!(out, "    source: {},", quoted(t.source.as_deref().unwrap())).unwrap();
        if let Some(note) = &t.note {
            writeln!(out, "    note: {},", quoted(note)).unwrap();
        }
        out.push_str("    colors: {\n");
        for (name, color) in t.colors.named() {
            writeln!(out, "      {name}: '{}',", color.hex()).unwrap();
        }
        out.push_str("    },\n  },\n");
    }
    out.push_str("];\n");
    out
}

#[test]
fn the_web_ides_file_is_what_the_table_makes() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/src/themes.generated.ts");
    let expected = generated();
    if std::env::var_os("DOE_UPDATE_GENERATED").is_some() {
        std::fs::write(&path, &expected).unwrap();
        return;
    }
    // git may have checked the file out with the line ends of the platform
    let current = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        current == expected,
        "web/src/themes.generated.ts is not what crates/doe-tui/themes/themes.toml makes: \
         run `DOE_UPDATE_GENERATED=1 cargo test -p doe-tui --test themes`"
    );
}

#[test]
fn the_readme_names_every_theme_with_its_author_and_licence() {
    for t in theme::builtin() {
        let row = README
            .lines()
            .find(|line| line.starts_with('|') && line.contains(&format!("`{}`", t.id)))
            .unwrap_or_else(|| panic!("the README has no row for {}", t.id));
        assert!(row.contains(&t.name), "{}: {row}", t.id);
        assert!(
            row.contains(t.author.as_deref().unwrap()),
            "{}: {row}",
            t.id
        );
        assert!(
            row.contains(t.license.as_deref().unwrap()),
            "{}: {row}",
            t.id
        );
    }
}

// ── the viewer draws with the colours of the theme and no others ─────────────

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// The colours of every cell of a screen.
fn colours(terminal: &Terminal<TestBackend>) -> HashSet<(String, Color)> {
    let buffer = terminal.backend().buffer();
    let mut seen = HashSet::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            seen.insert(("foreground".to_owned(), cell.fg));
            seen.insert(("background".to_owned(), cell.bg));
            seen.insert(("underline".to_owned(), cell.underline_color));
        }
    }
    seen
}

fn the_screens_of(id: &str, depth: Depth) {
    let dir = tempfile::tempdir().unwrap();
    let schema = dir.path().join("t.yaml");
    std::fs::write(
        &schema,
        "id: t\nmeta:\n  endian: le\nseq:\n  - id: magic\n    contents: [0x44, 0x4f]\n  - id: n\n    type: u2\n  - id: flags\n    type: u1\n  - id: name\n    type: strz\n    encoding: ASCII\n  - id: kind\n    type: u1\n    enum: kinds\n  - id: blob\n    size: 4\n  - id: items\n    type: u1\n    repeat: expr\n    repeat-expr: 3\ninstances:\n  twice:\n    value: n * 2\nenums:\n  kinds:\n    1: text\n",
    )
    .unwrap();
    let file = dir.path().join("t.bin");
    std::fs::write(&file, b"DO\x07\x00\x01hi\x00\x01abcd\x01\x02\x03").unwrap();
    let options = Options {
        file: file.clone(),
        type_or_schema: schema.to_string_lossy().into_owned(),
        include_paths: vec![],
        color: true,
        watch: false,
        theme: Some(id.to_owned()),
        theme_dir: None,
        radix: None,
    };
    let loaded = load(&options).unwrap();
    let styles = options.theme(depth).unwrap();
    let palette = styles.palette().unwrap();
    let colors = &find(id).colors;
    let allowed: HashSet<Color> = {
        let p = Palette::new(colors, depth);
        [
            p.background,
            p.foreground,
            p.muted,
            p.line,
            p.panel,
            p.accent,
            p.selection,
            p.selection_part,
            p.error,
            p.warning,
            p.number,
            p.string,
            p.bytes,
            p.enum_,
            p.boolean,
            p.offset,
            p.type_,
            p.instance,
        ]
        .into_iter()
        .collect()
    };
    assert_eq!(palette.background, Palette::new(colors, depth).background);

    let mut app = App::new(file, loaded);
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    let mut seen: HashSet<(String, Color)> = HashSet::new();
    let mut shoot = |app: &mut App, terminal: &mut Terminal<TestBackend>| {
        terminal.draw(|f| draw(f, app, &styles)).unwrap();
        seen.extend(colours(terminal));
    };
    shoot(&mut app, &mut terminal);
    // down the tree, with the bytes following, then the bytes, the inspector,
    // a search with its prompt, and the help
    for _ in 0..4 {
        press(&mut app, KeyCode::Down);
        shoot(&mut app, &mut terminal);
    }
    press(&mut app, KeyCode::Tab);
    for _ in 0..3 {
        press(&mut app, KeyCode::Right);
        shoot(&mut app, &mut terminal);
    }
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Char('i'));
    shoot(&mut app, &mut terminal);
    press(&mut app, KeyCode::Char('i'));
    press(&mut app, KeyCode::Char('/'));
    press(&mut app, KeyCode::Char('h'));
    shoot(&mut app, &mut terminal);
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('g'));
    press(&mut app, KeyCode::Char('x'));
    press(&mut app, KeyCode::Enter);
    shoot(&mut app, &mut terminal);
    press(&mut app, KeyCode::Char('?'));
    shoot(&mut app, &mut terminal);

    for (what, color) in &seen {
        assert!(
            allowed.contains(color) || (what == "underline" && *color == Color::Reset),
            "{id} at {depth}: a cell has {color:?} as its {what}, which is not a colour of the theme"
        );
    }
    // and the theme is really there
    assert!(seen.contains(&("background".to_owned(), palette.background)));
    assert!(seen.contains(&("foreground".to_owned(), palette.foreground)));
    assert!(
        seen.contains(&("foreground".to_owned(), palette.number)),
        "numbers"
    );
    assert!(
        seen.contains(&("foreground".to_owned(), palette.string)),
        "strings"
    );
    assert!(
        seen.contains(&("background".to_owned(), palette.selection)),
        "the selection"
    );
}

#[test]
fn the_viewer_draws_with_the_colours_of_every_theme_and_no_others() {
    for t in theme::builtin() {
        the_screens_of(&t.id, Depth::True);
    }
}

#[test]
fn it_draws_with_the_colours_a_smaller_terminal_has() {
    for id in ["nord", "solarized-light", "high-contrast"] {
        the_screens_of(id, Depth::Indexed);
        the_screens_of(id, Depth::Basic);
    }
}

#[test]
fn without_colour_a_theme_is_not_used() {
    let mut options = Options {
        file: PathBuf::from("f"),
        type_or_schema: "t".to_owned(),
        include_paths: vec![],
        color: false,
        watch: false,
        theme: Some("nord".to_owned()),
        theme_dir: None,
        radix: None,
    };
    assert_eq!(options.theme(Depth::True).unwrap(), Theme::new(false));
    // not even a theme that is not there: NO_COLOR says no colour
    options.theme = Some("nope".to_owned());
    assert_eq!(options.theme(Depth::True).unwrap(), Theme::new(false));
    // with colour: a missing theme is an error, `terminal` and none are the terminal's own colours
    options.color = true;
    assert!(options.theme(Depth::True).is_err());
    options.theme = Some("terminal".to_owned());
    assert_eq!(options.theme(Depth::True).unwrap(), Theme::new(true));
    options.theme = None;
    assert_eq!(options.theme(Depth::True).unwrap(), Theme::new(true));
}
