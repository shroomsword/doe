//! Colour themes: what a theme is, the built-in ones, how a theme is found by
//! name or by file, and how its colours are brought down to what a terminal can
//! show.
//!
//! The built-in themes are one table, `themes/themes.toml`, which the web IDE
//! is made from too (`web/src/themes.generated.ts`); the file says what each
//! colour is for.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, anyhow, bail};
use ratatui::style::Color;
use serde::{Deserialize, Deserializer};

/// The table of the built-in themes, as it is in the repository.
pub const BUILTIN_SOURCE: &str = include_str!("../themes/themes.toml");

/// A colour of 8 bits a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Read `#rrggbb`.
    ///
    /// # Errors
    ///
    /// Fails when the text is not a `#` and six hexadecimal digits.
    pub fn parse(text: &str) -> anyhow::Result<Rgb> {
        let digits = text
            .strip_prefix('#')
            .filter(|d| d.len() == 6 && d.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| anyhow!("{text:?} is not a colour: write it as #rrggbb"))?;
        let channel = |from: usize| {
            digits
                .get(from..from + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or_else(|| anyhow!("{text:?} is not a colour: write it as #rrggbb"))
        };
        Ok(Rgb(channel(0)?, channel(2)?, channel(4)?))
    }

    /// The colour as `#rrggbb`.
    #[must_use]
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    /// The relative luminance of WCAG 2.
    #[must_use]
    pub fn luminance(self) -> f64 {
        fn linear(channel: u8) -> f64 {
            let c = f64::from(channel) / 255.0;
            if c <= 0.039_28 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * linear(self.0) + 0.7152 * linear(self.1) + 0.0722 * linear(self.2)
    }

    /// The contrast ratio of WCAG 2 between two colours (1 to 21).
    #[must_use]
    pub fn contrast(self, other: Rgb) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Rgb::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// Dark or light: which of the pair a person who follows the system gets, and
/// how the web IDE tells its controls to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Variant {
    /// Light text on a dark background.
    Dark,
    /// Dark text on a light background.
    Light,
}

impl Variant {
    /// `dark` or `light`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Variant::Dark => "dark",
            Variant::Light => "light",
        }
    }
}

/// The colours of a theme, by what they are for (see `themes/themes.toml`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    missing_docs,
    reason = "each colour is described in themes/themes.toml, which is the table they come from"
)]
pub struct Colors {
    pub background: Rgb,
    pub foreground: Rgb,
    pub muted: Rgb,
    pub line: Rgb,
    pub panel: Rgb,
    pub accent: Rgb,
    pub selection: Rgb,
    pub selection_part: Rgb,
    pub error: Rgb,
    pub warning: Rgb,
    pub number: Rgb,
    pub string: Rgb,
    pub bytes: Rgb,
    #[serde(rename = "enum")]
    pub enum_: Rgb,
    pub boolean: Rgb,
    pub offset: Rgb,
    #[serde(rename = "type")]
    pub type_: Rgb,
    pub instance: Rgb,
}

impl Colors {
    /// Every colour with the name it has in the table, in the order of the table.
    #[must_use]
    pub fn named(&self) -> [(&'static str, Rgb); 18] {
        [
            ("background", self.background),
            ("foreground", self.foreground),
            ("muted", self.muted),
            ("line", self.line),
            ("panel", self.panel),
            ("accent", self.accent),
            ("selection", self.selection),
            ("selection_part", self.selection_part),
            ("error", self.error),
            ("warning", self.warning),
            ("number", self.number),
            ("string", self.string),
            ("bytes", self.bytes),
            ("enum", self.enum_),
            ("boolean", self.boolean),
            ("offset", self.offset),
            ("type", self.type_),
            ("instance", self.instance),
        ]
    }
}

/// The text of a theme, as it is in a file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    #[serde(default)]
    id: Option<String>,
    name: String,
    #[serde(default = "dark")]
    variant: Variant,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    note: Option<String>,
    colors: Colors,
}

fn dark() -> Variant {
    Variant::Dark
}

/// The table of built-in themes.
#[derive(Debug, Deserialize)]
struct Table {
    theme: Vec<Definition>,
}

/// A theme: a name and the colours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorTheme {
    /// What `--theme` takes (empty for a theme from a file that has none).
    pub id: String,
    /// What it is called.
    pub name: String,
    /// Dark or light.
    pub variant: Variant,
    /// Who made it.
    pub author: Option<String>,
    /// The licence it is under.
    pub license: Option<String>,
    /// Where it comes from.
    pub source: Option<String>,
    /// What was changed from the original, if anything.
    pub note: Option<String>,
    /// The colours.
    pub colors: Colors,
}

impl From<Definition> for ColorTheme {
    fn from(d: Definition) -> ColorTheme {
        ColorTheme {
            id: d.id.unwrap_or_default(),
            name: d.name,
            variant: d.variant,
            author: d.author,
            license: d.license,
            source: d.source,
            note: d.note,
            colors: d.colors,
        }
    }
}

impl ColorTheme {
    /// Read a theme from the text of a file: `name`, optionally `variant`
    /// (`dark`, the default, or `light`), `author`, `license`, `source` and
    /// `note`, and a `[colors]` table with every colour of `themes/themes.toml`.
    ///
    /// # Errors
    ///
    /// Fails when the text is not TOML, or a key is missing, unknown or not a
    /// colour (`#rrggbb`).
    pub fn parse(text: &str) -> anyhow::Result<ColorTheme> {
        let definition: Definition = toml::from_str(text).map_err(|e| anyhow!("{e}"))?;
        Ok(definition.into())
    }
}

/// The built-in themes, in the order of the table.
///
/// # Panics
///
/// Never in a build that passed its tests: the table is checked by them.
#[must_use]
pub fn builtin() -> &'static [ColorTheme] {
    static THEMES: OnceLock<Vec<ColorTheme>> = OnceLock::new();
    THEMES.get_or_init(|| match toml::from_str::<Table>(BUILTIN_SOURCE) {
        Ok(table) => table.theme.into_iter().map(ColorTheme::from).collect(),
        Err(error) => {
            log::error!("the table of built-in themes is broken: {error}");
            Vec::new()
        }
    })
}

/// The ids of the built-in themes, for a message.
#[must_use]
pub fn builtin_ids() -> Vec<&'static str> {
    builtin().iter().map(|t| t.id.as_str()).collect()
}

/// What a name is compared as: case, spaces and underscores do not matter.
fn simple(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| match c {
            ' ' | '_' => '-',
            other => other.to_ascii_lowercase(),
        })
        .collect()
}

/// Where the themes of a person are: the directory `themes` next to the
/// configuration file.
#[must_use]
pub fn user_dir(config_file: Option<&Path>) -> Option<PathBuf> {
    config_file
        .and_then(Path::parent)
        .map(|dir| dir.join("themes"))
}

/// The theme that `--theme` or the `theme` key of the configuration names: a
/// built-in theme by its id, a file `NAME.toml` in the directory of the themes
/// of the person, or the path of a file.
///
/// # Errors
///
/// Fails when the name is none of these, or the file is not a theme, with a
/// message that lists the built-in themes.
pub fn resolve(name: &str, user_dir: Option<&Path>) -> anyhow::Result<ColorTheme> {
    let wanted = simple(name);
    if let Some(theme) = builtin().iter().find(|t| t.id == wanted) {
        return Ok(theme.clone());
    }
    if let Some(dir) = user_dir {
        let file = dir.join(format!("{name}.toml"));
        if file.is_file() {
            return load_file(&file);
        }
    }
    let path = Path::new(name);
    if path.is_file() {
        return load_file(path);
    }
    bail!(
        "there is no theme {name:?}: the built-in themes are {}; a theme can also be a file ({name} is not one){}",
        builtin_ids().join(", "),
        match user_dir {
            Some(dir) => format!(" or {}/{name}.toml", dir.display()),
            None => String::new(),
        }
    )
}

/// Read a theme from a file.
///
/// # Errors
///
/// Fails when the file cannot be read or is not a theme.
pub fn load_file(path: &Path) -> anyhow::Result<ColorTheme> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("could not read the theme {}", path.display()))?;
    ColorTheme::parse(&text).with_context(|| format!("the theme {} is not usable", path.display()))
}

/// How many colours the terminal shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// 24 bits.
    True,
    /// The 256 colours of xterm.
    Indexed,
    /// The 16 colours of every terminal.
    Basic,
}

impl Depth {
    /// What the environment says: `COLORTERM` (`truecolor` or `24bit`), else `TERM`
    /// with `256color` in it, else the 16 colours.
    #[must_use]
    pub fn detect() -> Depth {
        Depth::from_env(
            std::env::var("COLORTERM").ok().as_deref(),
            std::env::var("TERM").ok().as_deref(),
        )
    }

    /// [`Depth::detect`] for given values of the two variables.
    #[must_use]
    pub fn from_env(colorterm: Option<&str>, term: Option<&str>) -> Depth {
        let truecolor = colorterm
            .is_some_and(|c| matches!(c.to_ascii_lowercase().as_str(), "truecolor" | "24bit"))
            || term.is_some_and(|t| t.ends_with("-direct"));
        if truecolor {
            Depth::True
        } else if term.is_some_and(|t| t.contains("256color")) {
            Depth::Indexed
        } else {
            Depth::Basic
        }
    }

    /// The colour to ask the terminal for.
    #[must_use]
    pub fn color(self, rgb: Rgb) -> Color {
        match self {
            Depth::True => Color::Rgb(rgb.0, rgb.1, rgb.2),
            Depth::Indexed => Color::Indexed(nearest_indexed(rgb)),
            Depth::Basic => nearest_basic(rgb),
        }
    }
}

impl fmt::Display for Depth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Depth::True => "24-bit colour",
            Depth::Indexed => "256 colours",
            Depth::Basic => "16 colours",
        })
    }
}

/// The squared distance of two colours, weighted as the eye sees them.
fn distance(a: Rgb, b: Rgb) -> u32 {
    let d = |x: u8, y: u8| u32::from(x.abs_diff(y)).pow(2);
    2 * d(a.0, b.0) + 4 * d(a.1, b.1) + 3 * d(a.2, b.2)
}

/// The levels of the 6×6×6 cube of xterm.
const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// The colour of index `index` (16 to 255) of xterm.
fn indexed_rgb(index: u8) -> Rgb {
    if index >= 232 {
        let level = 8 + 10 * (index - 232);
        Rgb(level, level, level)
    } else {
        let n = usize::from(index.saturating_sub(16));
        let level = |i: usize| CUBE.get(i).copied().unwrap_or(0);
        Rgb(level(n / 36), level(n / 6 % 6), level(n % 6))
    }
}

/// The colour of the cube or the greys of xterm that is nearest.
fn nearest_indexed(rgb: Rgb) -> u8 {
    (16..=255_u8)
        .min_by_key(|&index| distance(rgb, indexed_rgb(index)))
        .unwrap_or(16)
}

/// The 16 colours as most terminals draw them.
const BASIC: [(Color, Rgb); 16] = [
    (Color::Black, Rgb(0, 0, 0)),
    (Color::Red, Rgb(205, 49, 49)),
    (Color::Green, Rgb(13, 188, 121)),
    (Color::Yellow, Rgb(229, 229, 16)),
    (Color::Blue, Rgb(36, 114, 200)),
    (Color::Magenta, Rgb(188, 63, 188)),
    (Color::Cyan, Rgb(17, 168, 205)),
    (Color::Gray, Rgb(229, 229, 229)),
    (Color::DarkGray, Rgb(102, 102, 102)),
    (Color::LightRed, Rgb(241, 76, 76)),
    (Color::LightGreen, Rgb(35, 209, 139)),
    (Color::LightYellow, Rgb(245, 245, 67)),
    (Color::LightBlue, Rgb(59, 142, 234)),
    (Color::LightMagenta, Rgb(214, 112, 214)),
    (Color::LightCyan, Rgb(41, 184, 219)),
    (Color::White, Rgb(255, 255, 255)),
];

/// The nearest of the 16 colours.
fn nearest_basic(rgb: Rgb) -> Color {
    BASIC
        .iter()
        .min_by_key(|(_, shown)| distance(rgb, *shown))
        .map_or(Color::Reset, |(color, _)| *color)
}

/// The colours of a theme as a terminal is asked for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    missing_docs,
    reason = "the same colours as `Colors`, which are described in themes/themes.toml"
)]
pub struct Palette {
    pub background: Color,
    pub foreground: Color,
    pub muted: Color,
    pub line: Color,
    pub panel: Color,
    pub accent: Color,
    pub selection: Color,
    pub selection_part: Color,
    pub error: Color,
    pub warning: Color,
    pub number: Color,
    pub string: Color,
    pub bytes: Color,
    pub enum_: Color,
    pub boolean: Color,
    pub offset: Color,
    pub type_: Color,
    pub instance: Color,
}

impl Palette {
    /// The colours of `colors` for a terminal of this depth.
    #[must_use]
    pub fn new(colors: &Colors, depth: Depth) -> Palette {
        let c = |rgb: Rgb| depth.color(rgb);
        Palette {
            background: c(colors.background),
            foreground: c(colors.foreground),
            muted: c(colors.muted),
            line: c(colors.line),
            panel: c(colors.panel),
            accent: c(colors.accent),
            selection: c(colors.selection),
            selection_part: c(colors.selection_part),
            error: c(colors.error),
            warning: c(colors.warning),
            number: c(colors.number),
            string: c(colors.string),
            bytes: c(colors.bytes),
            enum_: c(colors.enum_),
            boolean: c(colors.boolean),
            offset: c(colors.offset),
            type_: c(colors.type_),
            instance: c(colors.instance),
        }
    }
}
