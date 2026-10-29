//! The types that are available: those of the type paths (`-I` and
//! `include_paths`, with their subdirectories), grouped by the path they are
//! in, with a filter, the text of one to read, and a choice of the one to parse
//! with. `s` lists what is open; this is what could be opened.
//!
//! The model has no terminal: [`TypeBrowser`] is made from the files on disk,
//! moved by the methods below and drawn by `ui`.

use std::path::{Path, PathBuf};

use std::io::Read as _;
use std::time::{Duration, Instant};

use doe::config::{AvailableType, discover_types};
use doe::detect::{HEAD_BYTES, Source, Why, rank};

use crate::format;
use crate::schemas::Pager;

/// One type that can be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeEntry {
    /// The id to parse with.
    pub id: String,
    /// The file it is in.
    pub path: PathBuf,
    /// The type path it is found in (what the group is named for).
    pub group: usize,
    /// The first line of its `doc`, if it has one.
    pub doc: Option<String>,
    /// Its signature and names, once its schema has been read (see [`TypeBrowser::step`]).
    pub source: Option<Source>,
    /// Why its schema could not be compiled, if it could not.
    pub error: Option<String>,
}

impl TypeEntry {
    /// Whether it starts with something that can be checked, if that is known yet.
    #[must_use]
    pub fn has_signature(&self) -> Option<bool> {
        self.source.as_ref().map(|s| !s.magic.is_empty())
    }
}

/// A line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// The name of a type path.
    Group(usize),
    /// A type (an index into the entries).
    Type(usize),
}

/// What a key did to the browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing that the viewer must act on.
    Nothing,
    /// This type was chosen to parse with.
    Choose(String),
    /// The browser is closed.
    Close,
}

/// The available types.
#[derive(Debug)]
pub struct TypeBrowser {
    /// The type paths, as they were given.
    pub groups: Vec<PathBuf>,
    /// Every type found.
    pub entries: Vec<TypeEntry>,
    /// The text that narrows the list.
    pub filter: String,
    /// Whether keys are typed into the filter.
    pub editing: bool,
    /// The lines of the list that the filter keeps.
    pub rows: Vec<Row>,
    /// The line that is selected.
    pub selected: usize,
    /// The text that is being read, if one is.
    pub pager: Option<Pager>,
    /// Why the list is empty or short, if it is.
    pub problem: Option<String>,
    /// The next entry whose schema is to be read for its signature.
    pub next: usize,
    /// Whether the list is ordered by what fits the open file.
    pub ranked: bool,
    /// The start of the open file, and its name, for ranking.
    pub head: Vec<u8>,
    /// The name of the open file.
    pub file_name: Option<String>,
    /// The order of ranking: the entries that can be the type of the file, and why.
    pub order: Vec<(usize, Why)>,
}

impl TypeBrowser {
    /// The types found on `include_paths`.
    #[must_use]
    pub fn read(include_paths: &[PathBuf]) -> TypeBrowser {
        let (found, problem) = match discover_types(include_paths) {
            Ok(found) => (found, None),
            Err(duplicates) => (
                Vec::new(),
                Some(format!(
                    "{} id{} declared by more than one file in a type path: {}",
                    duplicates.len(),
                    if duplicates.len() == 1 {
                        " is"
                    } else {
                        "s are"
                    },
                    duplicates
                        .iter()
                        .map(|d| d.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            ),
        };
        let entries = found
            .into_iter()
            .map(|available| TypeEntry {
                group: group_of(include_paths, &available.source),
                doc: available
                    .doc
                    .as_deref()
                    .and_then(|doc| doc.lines().map(str::trim).find(|l| !l.is_empty()))
                    .map(str::to_owned),
                path: available.source,
                id: available.id,
                source: None,
                error: None,
            })
            .collect();
        TypeBrowser::new(include_paths.to_vec(), entries, problem)
    }

    /// A browser of these types.
    #[must_use]
    pub fn new(groups: Vec<PathBuf>, entries: Vec<TypeEntry>, problem: Option<String>) -> Self {
        let mut browser = TypeBrowser {
            groups,
            entries,
            filter: String::new(),
            editing: false,
            rows: Vec::new(),
            selected: 0,
            pager: None,
            problem,
            next: 0,
            ranked: false,
            head: Vec::new(),
            file_name: None,
            order: Vec::new(),
        };
        browser.refilter();
        browser
    }

    /// Whether an entry is kept by the filter: every word of it is in the id,
    /// the path or the doc, in any case.
    fn keeps(&self, entry: &TypeEntry) -> bool {
        let text = format!(
            "{} {} {}",
            entry.id,
            entry.path.display(),
            entry.doc.as_deref().unwrap_or("")
        )
        .to_lowercase();
        self.filter
            .to_lowercase()
            .split_whitespace()
            .all(|word| text.contains(word))
    }

    /// Make the lines again from the filter, keeping the selected type if it is
    /// still there.
    pub fn refilter(&mut self) {
        let keep = self.selected_id().map(str::to_owned);
        let mut rows = Vec::new();
        if self.ranked && self.pending() == 0 {
            // by what fits the file: no groups, the best first
            for (index, _) in &self.order {
                if self.entries.get(*index).is_some_and(|e| self.keeps(e)) {
                    rows.push(Row::Type(*index));
                }
            }
            self.rows = rows;
            self.selected = keep
                .and_then(|id| self.position_of(&id))
                .or_else(|| (!self.rows.is_empty()).then_some(0))
                .unwrap_or(0);
            return;
        }
        for group in 0..self.groups.len() {
            let kept: Vec<usize> = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.group == group && self.keeps(e))
                .map(|(index, _)| index)
                .collect();
            if kept.is_empty() {
                continue;
            }
            rows.push(Row::Group(group));
            rows.extend(kept.into_iter().map(Row::Type));
        }
        self.rows = rows;
        self.selected = keep
            .and_then(|id| self.position_of(&id))
            .or_else(|| self.rows.iter().position(|r| matches!(r, Row::Type(_))))
            .unwrap_or(0);
    }

    fn position_of(&self, id: &str) -> Option<usize> {
        self.rows.iter().position(
            |row| matches!(row, Row::Type(i) if self.entries.get(*i).is_some_and(|e| e.id == id)),
        )
    }

    /// The type on the selected line.
    #[must_use]
    pub fn selected_entry(&self) -> Option<&TypeEntry> {
        match self.rows.get(self.selected)? {
            Row::Type(index) => self.entries.get(*index),
            Row::Group(_) => None,
        }
    }

    /// The id of the type on the selected line.
    #[must_use]
    pub fn selected_id(&self) -> Option<&str> {
        self.selected_entry().map(|e| e.id.as_str())
    }

    /// Put the selection on the type with this id, if the list has it.
    pub fn select_type(&mut self, id: &str) {
        if let Some(index) = self.position_of(id) {
            self.selected = index;
        }
    }

    /// Move the selection by `delta` lines, stopping at the ends.
    pub fn move_by(&mut self, delta: isize) {
        let last = self.rows.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    /// Move the selection to the first or the last line.
    pub fn jump(&mut self, to_end: bool) {
        self.selected = if to_end {
            self.rows.len().saturating_sub(1)
        } else {
            0
        };
    }

    /// Scroll the text that is being read, by `delta` lines.
    pub fn scroll(&mut self, delta: isize) {
        if let Some(pager) = &mut self.pager {
            let last = pager.lines.len().saturating_sub(1);
            pager.top = pager.top.saturating_add_signed(delta).min(last);
        }
    }

    /// Scroll the text that is being read to its start or its end.
    pub fn scroll_to(&mut self, end: bool) {
        if let Some(pager) = &mut self.pager {
            pager.top = if end {
                pager.lines.len().saturating_sub(1)
            } else {
                0
            };
        }
    }

    /// Read the text of the selected type.
    pub fn view(&mut self) {
        if let Some(entry) = self.selected_entry() {
            self.pager = Some(Pager::of_file(&entry.path));
        }
    }

    /// Choose the selected type.
    #[must_use]
    pub fn choose(&self) -> Outcome {
        match self.selected_id() {
            Some(id) => Outcome::Choose(id.to_owned()),
            None => Outcome::Nothing,
        }
    }

    /// How many entries have not had their schema read yet.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.entries.len().saturating_sub(self.next)
    }

    /// Read the schemas of the entries that have not been, one after the other
    /// for about `budget`, to see their signatures. Returns whether more are
    /// left. When the last is read and the list is ranked, it is ranked.
    pub fn step(&mut self, include_paths: &[PathBuf], budget: Duration) -> bool {
        let started = Instant::now();
        while self.next < self.entries.len() && started.elapsed() < budget {
            if let Some(entry) = self.entries.get_mut(self.next) {
                let available = AvailableType {
                    id: entry.id.clone(),
                    doc: entry.doc.clone(),
                    source: entry.path.clone(),
                };
                let (source, error) = doe::detect::source_of(include_paths, &available);
                entry.source = Some(source);
                entry.error = error;
            }
            self.next += 1;
        }
        if self.pending() == 0 && self.ranked {
            self.rerank();
        }
        self.pending() > 0
    }

    /// Put the entries in the order in which `--detect` would try them on the file.
    fn rerank(&mut self) {
        let sources: Vec<Source> = self
            .entries
            .iter()
            .map(|e| {
                e.source.clone().unwrap_or_else(|| Source {
                    name: e.id.clone(),
                    magic: Vec::new(),
                    names: Vec::new(),
                })
            })
            .collect();
        self.order = rank(&sources, &self.head, self.file_name.as_deref(), false)
            .into_iter()
            .map(|r| (r.index, r.why))
            .collect();
        self.refilter();
    }

    /// Turn the ranking by the open file on or off.
    ///
    /// # Errors
    ///
    /// Why the file cannot be read.
    pub fn toggle_rank(&mut self, file: &Path) -> Result<(), String> {
        if self.ranked {
            self.ranked = false;
            self.refilter();
            return Ok(());
        }
        let mut head = Vec::new();
        std::fs::File::open(file)
            .and_then(|f| f.take(HEAD_BYTES as u64).read_to_end(&mut head))
            .map_err(|e| {
                format!(
                    "could not read {}: {e}",
                    format::safe(&file.display().to_string())
                )
            })?;
        self.head = head;
        self.file_name = file.file_name().and_then(|n| n.to_str()).map(str::to_owned);
        self.ranked = true;
        if self.pending() == 0 {
            self.rerank();
        }
        Ok(())
    }

    /// Why an entry is ranked where it is, when the list is ranked.
    #[must_use]
    pub fn why(&self, index: usize) -> Option<Why> {
        if !self.ranked {
            return None;
        }
        self.order
            .iter()
            .find(|(i, _)| *i == index)
            .map(|(_, why)| *why)
    }

    /// Close the text that is being read, else stop editing the filter, else
    /// clear it, else close the browser.
    pub fn back(&mut self) -> Outcome {
        if self.pager.take().is_some() {
            return Outcome::Nothing;
        }
        if self.editing {
            self.editing = false;
            return Outcome::Nothing;
        }
        if !self.filter.is_empty() {
            self.filter.clear();
            self.refilter();
            return Outcome::Nothing;
        }
        Outcome::Close
    }

    /// Type a character into the filter.
    pub fn type_char(&mut self, c: char) {
        self.filter.push(c);
        self.refilter();
    }

    /// Delete the last character of the filter.
    pub fn backspace(&mut self) {
        self.filter.pop();
        self.refilter();
    }

    /// How many types the list has, and how many the filter keeps.
    #[must_use]
    pub fn counts(&self) -> (usize, usize) {
        (
            self.entries.len(),
            self.rows
                .iter()
                .filter(|r| matches!(r, Row::Type(_)))
                .count(),
        )
    }

    /// The name of a group as it is shown.
    #[must_use]
    pub fn group_name(&self, group: usize) -> String {
        self.groups
            .get(group)
            .map_or_else(String::new, |p| format::safe(&p.display().to_string()))
    }
}

/// The type path that holds `file`: the first one it is below.
fn group_of(include_paths: &[PathBuf], file: &Path) -> usize {
    include_paths
        .iter()
        .position(|dir| file.starts_with(dir))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;

    fn library() -> (TypeBrowser, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("images")).unwrap();
        std::fs::write(
            dir.path().join("images/png.yaml"),
            "id: png\ndoc: |\n  PNG image\n  more\nseq: []\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("elf.yaml"), "id: elf\nseq: []\n").unwrap();
        std::fs::write(
            dir.path().join("images/nested.ksy"),
            "meta:\n  id: gif\nseq: []\n",
        )
        .unwrap();
        let browser = TypeBrowser::read(&[dir.path().to_path_buf()]);
        (browser, dir)
    }

    fn ids(browser: &TypeBrowser) -> Vec<String> {
        browser
            .rows
            .iter()
            .filter_map(|row| match row {
                Row::Type(i) => Some(browser.entries[*i].id.clone()),
                Row::Group(_) => None,
            })
            .collect()
    }

    #[test]
    fn the_types_of_a_path_and_its_subdirectories_are_listed_under_the_path() {
        let (browser, _dir) = library();
        assert_eq!(ids(&browser), ["elf", "gif", "png"]);
        assert!(matches!(browser.rows[0], Row::Group(0)));
        assert_eq!(browser.counts(), (3, 3));
        let png = browser.entries.iter().find(|e| e.id == "png").unwrap();
        assert_eq!(png.doc.as_deref(), Some("PNG image"));
        // the first line is selected, not the name of the path
        assert_eq!(browser.selected_id(), Some("elf"));
    }

    #[test]
    fn the_filter_narrows_by_id_path_and_doc_and_keeps_the_selection_if_it_can() {
        let (mut browser, _dir) = library();
        browser.select_type("png");
        for c in "image".chars() {
            browser.type_char(c);
        }
        // the doc of png and the directory `images` both match
        assert_eq!(ids(&browser), ["gif", "png"]);
        assert_eq!(browser.selected_id(), Some("png"));
        for c in " gif".chars() {
            browser.type_char(c);
        }
        assert_eq!(ids(&browser), ["gif"]);
        browser.backspace();
        browser.backspace();
        browser.backspace();
        browser.backspace();
        assert_eq!(browser.filter, "image");
        // nothing matches: no rows, and nothing to choose
        browser.filter = "zzz".to_owned();
        browser.refilter();
        assert_eq!(browser.rows, []);
        assert_eq!(browser.choose(), Outcome::Nothing);
    }

    #[test]
    fn enter_chooses_the_type_and_a_group_line_chooses_nothing() {
        let (mut browser, _dir) = library();
        browser.select_type("gif");
        assert_eq!(browser.choose(), Outcome::Choose("gif".to_owned()));
        browser.selected = 0;
        assert_eq!(browser.choose(), Outcome::Nothing);
    }

    #[test]
    fn the_text_of_a_type_is_read_and_esc_goes_back_one_step_at_a_time() {
        let (mut browser, _dir) = library();
        browser.select_type("png");
        browser.view();
        assert!(browser.pager.as_ref().unwrap().lines[0].contains("id: png"));
        browser.editing = true;
        browser.filter = "p".to_owned();
        assert_eq!(browser.back(), Outcome::Nothing); // the text
        assert!(browser.pager.is_none() && browser.editing);
        assert_eq!(browser.back(), Outcome::Nothing); // editing
        assert!(!browser.editing && browser.filter == "p");
        assert_eq!(browser.back(), Outcome::Nothing); // the filter
        assert_eq!(browser.filter, "");
        assert_eq!(browser.back(), Outcome::Close);
    }

    #[test]
    fn the_second_type_path_is_its_own_group() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        std::fs::write(first.path().join("a.yaml"), "id: a\nseq: []\n").unwrap();
        std::fs::write(second.path().join("b.yaml"), "id: b\nseq: []\n").unwrap();
        let browser = TypeBrowser::read(&[first.path().to_path_buf(), second.path().to_path_buf()]);
        assert_eq!(
            browser.rows,
            [Row::Group(0), Row::Type(0), Row::Group(1), Row::Type(1)]
        );
    }

    #[test]
    fn two_files_with_one_id_in_a_path_are_said_and_nothing_is_listed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.yaml"), "id: same\nseq: []\n").unwrap();
        std::fs::write(dir.path().join("b.yaml"), "id: same\nseq: []\n").unwrap();
        let browser = TypeBrowser::read(&[dir.path().to_path_buf()]);
        assert!(browser.problem.as_deref().unwrap().contains("same"));
        assert_eq!(browser.rows, []);
    }
}
