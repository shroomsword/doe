//! The list of the colour themes (`T`): the terminal's own colours, the built-in
//! themes, and the person's own files, with the viewer showing the one that is
//! selected behind the list.
//!
//! The list is made here, without the screen; [`crate::ui`] draws it and
//! [`crate::app::App`] applies the theme of the selected entry to the viewer.

use std::path::{Path, PathBuf};

use crate::theme::{self, ColorTheme, Depth, Palette, Variant};
use crate::ui::Theme;

/// Where an entry comes from, which is also the heading it is listed under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// The colours of the terminal itself.
    Terminal,
    /// A built-in theme with light text on a dark background.
    Dark,
    /// A built-in theme with dark text on a light background.
    Light,
    /// A file of the person's `themes` directory.
    Yours,
}

impl Group {
    /// The heading of the group.
    #[must_use]
    pub fn heading(self) -> &'static str {
        match self {
            Group::Terminal => "Terminal",
            Group::Dark => "Built-in dark",
            Group::Light => "Built-in light",
            Group::Yours => "Yours",
        }
    }
}

/// One theme in the list.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The group it is listed in.
    pub group: Group,
    /// What `--theme` takes for it: `terminal`, the id of a built-in theme, or
    /// the name of the file (without `.toml`).
    pub id: String,
    /// What it is called.
    pub name: String,
    /// Who made it, if it says.
    pub author: Option<String>,
    /// The colours; none for the terminal's own, and for a file that does not read.
    pub colors: Option<ColorTheme>,
    /// Why a file of the person's cannot be used.
    pub error: Option<String>,
}

impl Entry {
    /// The styles of the viewer with this theme at the terminal's `depth`; none
    /// for a file that cannot be used.
    #[must_use]
    pub fn styles(&self, depth: Depth) -> Option<Theme> {
        match (&self.colors, self.group) {
            (Some(found), _) => Some(Theme::themed(Palette::new(&found.colors, depth))),
            (None, Group::Terminal) => Some(Theme::new(true)),
            (None, _) => None,
        }
    }

    /// The line to put in the config file, or the option, to keep this theme.
    #[must_use]
    pub fn how_to_keep(&self) -> String {
        match self.group {
            Group::Terminal => {
                "to keep it, leave out `theme` in the config file (or --theme terminal)".to_owned()
            }
            _ => format!(
                "to keep it: theme = \"{}\" in the config file, or --theme {}",
                self.id, self.id
            ),
        }
    }
}

/// The list while it is open.
#[derive(Debug)]
pub struct ThemeBrowser {
    /// Every theme, in the order of the groups.
    pub entries: Vec<Entry>,
    /// The filter typed with `/`.
    pub filter: String,
    /// Whether keys are being typed into the filter.
    pub editing: bool,
    /// The selected row, as an index into [`ThemeBrowser::visible`].
    pub selected: usize,
    /// The theme that was in use when the list was opened, put back by `Esc`.
    pub original: Option<Theme>,
    /// The id of the entry that was in use, to mark it.
    pub in_use: Option<String>,
    /// Whether there is any colour in this run (`NO_COLOR`, `--color never`).
    pub color: bool,
}

impl ThemeBrowser {
    /// Read the themes: the built-in ones and the files of `user_dir` now.
    #[must_use]
    pub fn read(
        user_dir: Option<&Path>,
        original: Option<Theme>,
        in_use: Option<String>,
        color: bool,
    ) -> ThemeBrowser {
        let mut entries = vec![Entry {
            group: Group::Terminal,
            id: "terminal".to_owned(),
            name: "The terminal's own colours".to_owned(),
            author: None,
            colors: None,
            error: None,
        }];
        for variant in [Variant::Dark, Variant::Light] {
            for found in theme::builtin().iter().filter(|t| t.variant == variant) {
                entries.push(Entry {
                    group: if variant == Variant::Dark {
                        Group::Dark
                    } else {
                        Group::Light
                    },
                    id: found.id.clone(),
                    name: found.name.clone(),
                    author: found.author.clone(),
                    colors: Some(found.clone()),
                    error: None,
                });
            }
        }
        entries.extend(user_entries(user_dir));
        let mut browser = ThemeBrowser {
            entries,
            filter: String::new(),
            editing: false,
            selected: 0,
            original,
            in_use,
            color,
        };
        browser.select_in_use();
        browser
    }

    /// Put the selection on the theme that is in use.
    fn select_in_use(&mut self) {
        let wanted = self.in_use.clone().unwrap_or_else(|| "terminal".to_owned());
        if let Some(at) = self
            .visible()
            .iter()
            .position(|&i| self.entries.get(i).is_some_and(|e| e.id == wanted))
        {
            self.selected = at;
        }
    }

    /// The indexes of the entries that the filter keeps: every word of it is in
    /// the name, the id or the author, whatever the case.
    #[must_use]
    pub fn visible(&self) -> Vec<usize> {
        let words: Vec<String> = self
            .filter
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                let text = format!(
                    "{} {} {} {}",
                    entry.name,
                    entry.id,
                    entry.author.as_deref().unwrap_or(""),
                    entry.group.heading()
                )
                .to_lowercase();
                words.iter().all(|w| text.contains(w))
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// The entry that is selected, if the filter leaves any.
    #[must_use]
    pub fn current(&self) -> Option<&Entry> {
        let visible = self.visible();
        visible
            .get(self.selected)
            .and_then(|&i| self.entries.get(i))
    }

    /// Move the selection by `delta` rows, stopping at the ends.
    pub fn move_by(&mut self, delta: isize) {
        let last = self.visible().len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    /// Go to the first or the last row.
    pub fn jump(&mut self, end: bool) {
        self.selected = if end {
            self.visible().len().saturating_sub(1)
        } else {
            0
        };
    }

    /// Type a character into the filter.
    pub fn type_char(&mut self, c: char) {
        self.filter.push(c);
        self.selected = 0;
    }

    /// Take the last character out of the filter.
    pub fn backspace(&mut self) {
        self.filter.pop();
        self.selected = 0;
    }
}

/// The files `NAME.toml` of the person's directory, in the order of their names.
/// A file that does not read is listed with its error.
fn user_entries(dir: Option<&Path>) -> Vec<Entry> {
    let Some(dir) = dir else {
        return Vec::new();
    };
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "toml"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let id = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            match theme::load_file(&path) {
                Ok(found) => Entry {
                    group: Group::Yours,
                    name: if found.name.is_empty() {
                        id.clone()
                    } else {
                        found.name.clone()
                    },
                    author: found.author.clone(),
                    colors: Some(found),
                    error: None,
                    id,
                },
                Err(error) => Entry {
                    group: Group::Yours,
                    name: id.clone(),
                    author: None,
                    colors: None,
                    error: Some(format!("{error:#}")),
                    id,
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;

    fn browser(dir: Option<&Path>) -> ThemeBrowser {
        ThemeBrowser::read(dir, None, None, true)
    }

    const GOOD: &str = "name = \"Mine\"\nauthor = \"A. Person\"\nvariant = \"light\"\n[colors]\nbackground = \"#ffffff\"\nforeground = \"#000000\"\nmuted = \"#555555\"\nline = \"#cccccc\"\npanel = \"#eeeeee\"\naccent = \"#0000aa\"\nselection = \"#dddddd\"\nselection_part = \"#eeeeee\"\nerror = \"#aa0000\"\nwarning = \"#885500\"\nnumber = \"#aa5500\"\nstring = \"#007700\"\nbytes = \"#aa0000\"\nenum = \"#770077\"\nboolean = \"#007777\"\noffset = \"#555555\"\ntype = \"#0000aa\"\ninstance = \"#555555\"\n";

    #[test]
    fn the_terminal_comes_first_then_the_built_in_themes_dark_then_light() {
        let b = browser(None);
        assert_eq!(b.entries[0].group, Group::Terminal);
        let groups: Vec<Group> = b.entries.iter().map(|e| e.group).collect();
        let dark = groups.iter().filter(|g| **g == Group::Dark).count();
        let light = groups.iter().filter(|g| **g == Group::Light).count();
        assert!(dark > 5 && light > 3, "{dark} dark, {light} light");
        let first_light = groups.iter().position(|g| *g == Group::Light).unwrap();
        assert!(groups[..first_light].iter().all(|g| *g != Group::Light));
        assert!(!groups.contains(&Group::Yours));
        assert!(b.entries[0].styles(Depth::True).is_some());
    }

    #[test]
    fn the_files_of_the_person_are_listed_and_one_that_does_not_read_has_its_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mine.toml"), GOOD).unwrap();
        std::fs::write(dir.path().join("broken.toml"), "not a theme [").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "ignored").unwrap();
        let b = browser(Some(dir.path()));
        let yours: Vec<&Entry> = b
            .entries
            .iter()
            .filter(|e| e.group == Group::Yours)
            .collect();
        assert_eq!(
            yours.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["broken", "mine"]
        );
        assert!(yours[0].error.is_some() && yours[0].styles(Depth::True).is_none());
        assert_eq!(yours[1].name, "Mine");
        assert_eq!(yours[1].author.as_deref(), Some("A. Person"));
        assert!(yours[1].styles(Depth::True).is_some());
    }

    #[test]
    fn the_filter_keeps_the_entries_with_every_word_in_any_case() {
        let mut b = browser(None);
        let all = b.visible().len();
        b.filter = "NORD".to_owned();
        let kept = b.visible();
        assert!(!kept.is_empty() && kept.len() < all);
        assert!(kept.iter().all(|&i| b.entries[i].id.contains("nord")));
        b.filter = "zzz no such theme".to_owned();
        assert!(b.visible().is_empty() && b.current().is_none());
        b.filter = "dark".to_owned();
        assert!(
            b.visible()
                .iter()
                .any(|&i| b.entries[i].group == Group::Dark)
        );
    }

    #[test]
    fn the_selection_starts_on_the_theme_in_use_and_moves_within_the_list() {
        let b = ThemeBrowser::read(None, None, Some("nord".to_owned()), true);
        assert_eq!(b.current().unwrap().id, "nord");
        let mut b = browser(None);
        assert_eq!(b.current().unwrap().id, "terminal");
        b.move_by(-5);
        assert_eq!(b.selected, 0);
        b.move_by(1);
        assert_ne!(b.current().unwrap().id, "terminal");
        b.jump(true);
        assert_eq!(b.selected, b.visible().len() - 1);
        b.move_by(10);
        assert_eq!(b.selected, b.visible().len() - 1);
    }

    #[test]
    fn what_to_put_in_the_config_to_keep_a_theme() {
        let b = browser(None);
        assert!(b.entries[0].how_to_keep().contains("leave out `theme`"));
        let nord = b.entries.iter().find(|e| e.id == "nord").unwrap();
        assert!(nord.how_to_keep().contains("theme = \"nord\""));
        assert!(nord.how_to_keep().contains("--theme nord"));
    }
}
