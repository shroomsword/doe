//! The schemas that are open: the one on the command line and the files it
//! imports, with their types, the imports that are not there, and a pager for
//! the text of one. The type to parse with is chosen from the list.
//!
//! This is the model, without a terminal: [`SchemaList`] is made from the
//! files on disk, moved with the methods below (the keys of the viewer call
//! them), and drawn by `ui`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use doe::inspect::inspect;
use doe::schema::compiler::Compiler;
use doe::schema::locate::locate_error;

use crate::Options;
use crate::format;
use crate::load::schema_file;

/// Why a schema file could not be read, and where in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorInfo {
    /// What is wrong.
    pub message: String,
    /// The line, from 1, when the place is known.
    pub line: Option<usize>,
    /// The column, from 1.
    pub column: Option<usize>,
}

/// One schema file.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Where it is.
    pub path: PathBuf,
    /// Whether it is the schema named on the command line.
    pub main: bool,
    /// Its id, when it could be read.
    pub id: Option<String>,
    /// The types it defines, as `SCHEMA::TYPE` takes them.
    pub types: Vec<String>,
    /// Why it could not be read.
    pub error: Option<ErrorInfo>,
    /// When the file was last changed.
    pub modified: Option<SystemTime>,
}

/// An import that was not found.
#[derive(Debug, Clone)]
pub struct MissingEntry {
    /// The name as the importing file writes it.
    pub name: String,
    /// The file that asked for it.
    pub from: PathBuf,
    /// What was searched.
    pub message: String,
}

/// A line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A schema file (an index into the entries).
    Schema(usize),
    /// A type of one of them.
    Type {
        /// The entry that defines it.
        entry: usize,
        /// The name to parse with.
        name: String,
    },
    /// An import that is not there (an index into the missing ones).
    Missing(usize),
}

/// The text of one schema file, to read with the arrow keys.
#[derive(Debug)]
pub struct Pager {
    /// The file.
    pub path: PathBuf,
    /// Its lines, made safe to show.
    pub lines: Vec<String>,
    /// The first line shown.
    pub top: usize,
    /// The line of the error, from 0, when there is one.
    pub error_line: Option<usize>,
    /// What the error is.
    pub message: Option<String>,
}

/// What a key did to the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing that the viewer must act on.
    Nothing,
    /// The type with this name was chosen to parse with.
    Choose(String),
    /// The list is closed.
    Close,
}

/// The schemas that are open.
#[derive(Debug)]
pub struct SchemaList {
    /// The schema files: the main one first.
    pub entries: Vec<Entry>,
    /// The imports that are not there.
    pub missing: Vec<MissingEntry>,
    /// The lines of the list.
    pub rows: Vec<Row>,
    /// The line that is selected.
    pub selected: usize,
    /// The text that is being read, if one is.
    pub pager: Option<Pager>,
}

fn error_info(error: &doe::DoeError, path: &Path) -> ErrorInfo {
    let place = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| locate_error(&text, error));
    ErrorInfo {
        message: error.to_string(),
        line: place.as_ref().map(|p| p.line as usize),
        column: place.as_ref().map(|p| p.column as usize),
    }
}

impl SchemaList {
    /// Read the schema named in `options` and everything it imports from disk.
    ///
    /// # Errors
    ///
    /// Fails when it is not known which file the schema is in (a type name that
    /// the include paths do not have).
    pub fn read(options: &Options) -> Result<SchemaList, String> {
        let main = schema_file(options).ok_or_else(|| {
            format!(
                "the schema of {} is not known: no file on the include paths has that type",
                options.type_or_schema
            )
        })?;
        let compiler = Compiler::new(options.include_paths.clone());
        let found = inspect(&compiler, &main);
        let entries = found
            .schemas
            .iter()
            .enumerate()
            .map(|(index, schema)| Entry {
                path: schema.path.clone(),
                main: index == 0,
                id: schema.id.clone(),
                types: schema.types.clone(),
                error: schema
                    .error
                    .as_ref()
                    .map(|error| error_info(error, &schema.path)),
                modified: std::fs::metadata(&schema.path)
                    .and_then(|m| m.modified())
                    .ok(),
            })
            .collect();
        let missing = found
            .missing
            .iter()
            .map(|m| MissingEntry {
                name: m.name.clone(),
                from: m.from.clone(),
                message: m.error.to_string(),
            })
            .collect();
        Ok(SchemaList::new(entries, missing))
    }

    /// A list of these files.
    #[must_use]
    pub fn new(entries: Vec<Entry>, missing: Vec<MissingEntry>) -> SchemaList {
        let mut rows = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            rows.push(Row::Schema(index));
            for name in &entry.types {
                rows.push(Row::Type {
                    entry: index,
                    name: name.clone(),
                });
            }
        }
        for index in 0..missing.len() {
            rows.push(Row::Missing(index));
        }
        SchemaList {
            entries,
            missing,
            rows,
            selected: 0,
            pager: None,
        }
    }

    /// Put the selection on the type with this name, if the list has it.
    pub fn select_type(&mut self, name: &str) {
        if let Some(index) = self
            .rows
            .iter()
            .position(|row| matches!(row, Row::Type { name: n, .. } if n == name))
        {
            self.selected = index;
        }
    }

    /// The id of the schema on the command line.
    #[must_use]
    pub fn main_id(&self) -> Option<&str> {
        self.entries.first().and_then(|e| e.id.as_deref())
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

    /// Do what Enter does on the selected line: read a schema's text, or choose a type.
    pub fn enter(&mut self) -> Outcome {
        match self.rows.get(self.selected).cloned() {
            Some(Row::Schema(index)) => {
                if let Some(entry) = self.entries.get(index) {
                    self.pager = Some(Pager::open(entry));
                }
                Outcome::Nothing
            }
            Some(Row::Type { name, .. }) => Outcome::Choose(name),
            Some(Row::Missing(_)) | None => Outcome::Nothing,
        }
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

    /// Close the text that is being read, or else the list.
    pub fn back(&mut self) -> Outcome {
        if self.pager.take().is_some() {
            Outcome::Nothing
        } else {
            Outcome::Close
        }
    }
}

impl Pager {
    /// Read the text of an entry.
    #[must_use]
    pub fn open(entry: &Entry) -> Pager {
        let (lines, message) = match std::fs::read_to_string(&entry.path) {
            Ok(text) => (text.lines().map(format::safe).collect(), None),
            Err(error) => (Vec::new(), Some(format!("could not read it: {error}"))),
        };
        let error_line = entry
            .error
            .as_ref()
            .and_then(|e| e.line)
            .map(|line| line.saturating_sub(1));
        let message = message.or_else(|| entry.error.as_ref().map(|e| e.message.clone()));
        Pager {
            path: entry.path.clone(),
            top: error_line.map_or(0, |line| line.saturating_sub(3)),
            lines,
            error_line,
            message,
        }
    }
}

/// A time as `2026-10-25 12:34:56 UTC`.
#[must_use]
pub fn format_utc(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    // days since 1970-01-01 to a civil date (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use std::time::Duration;

    use super::*;

    const TOP: &str = "meta:\n  id: top\n  imports:\n    - lib\n    - gone\nseq:\n  - id: a\n    type: u1\ntypes:\n  chunk:\n    seq: []\n";
    const LIB: &str = "meta:\n  id: lib\nseq: []\ntypes:\n  piece:\n    seq: []\n";

    fn options(dir: &Path) -> Options {
        std::fs::write(dir.join("top.yaml"), TOP).unwrap();
        std::fs::write(dir.join("lib.yaml"), LIB).unwrap();
        Options {
            file: dir.join("f.bin"),
            type_or_schema: dir.join("top.yaml").to_string_lossy().into_owned(),
            include_paths: vec![],
            color: false,
            watch: false,
            theme: None,
            theme_dir: None,
        }
    }

    #[test]
    fn a_time_is_written_in_utc() {
        let at = |s: u64| format_utc(UNIX_EPOCH + Duration::from_secs(s));
        assert_eq!(at(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(at(951_782_400), "2000-02-29 00:00:00 UTC");
        assert_eq!(at(1_782_390_896), "2026-06-25 12:34:56 UTC");
        assert_eq!(at(4_102_444_799), "2099-12-31 23:59:59 UTC");
    }

    #[test]
    fn the_list_has_the_schemas_their_types_and_what_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let list = SchemaList::read(&options(dir.path())).unwrap();
        assert_eq!(list.entries.len(), 2);
        assert!(list.entries[0].main && !list.entries[1].main);
        assert_eq!(list.main_id(), Some("top"));
        assert_eq!(list.entries[0].types, ["top", "top::chunk"]);
        assert_eq!(list.entries[1].types, ["lib", "lib::piece"]);
        assert_eq!(list.missing.len(), 1);
        assert_eq!(list.missing[0].name, "gone");
        assert!(list.missing[0].from.ends_with("top.yaml"));
        assert!(list.entries[0].modified.is_some());
        let kinds: Vec<String> = list
            .rows
            .iter()
            .map(|row| match row {
                Row::Schema(i) => format!("schema {}", list.entries[*i].id.as_deref().unwrap()),
                Row::Type { name, .. } => format!("type {name}"),
                Row::Missing(i) => format!("missing {}", list.missing[*i].name),
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "schema top",
                "type top",
                "type top::chunk",
                "schema lib",
                "type lib",
                "type lib::piece",
                "missing gone"
            ]
        );
    }

    #[test]
    fn the_selection_moves_stops_at_the_ends_and_finds_a_type() {
        let dir = tempfile::tempdir().unwrap();
        let mut list = SchemaList::read(&options(dir.path())).unwrap();
        list.move_by(-1);
        assert_eq!(list.selected, 0);
        list.move_by(2);
        assert_eq!(list.selected, 2);
        list.jump(true);
        assert_eq!(list.selected, 6);
        list.move_by(5);
        assert_eq!(list.selected, 6);
        list.jump(false);
        assert_eq!(list.selected, 0);
        list.select_type("lib::piece");
        assert_eq!(list.selected, 5);
        list.select_type("nope");
        assert_eq!(list.selected, 5);
    }

    #[test]
    fn enter_reads_a_schema_chooses_a_type_and_does_nothing_on_a_missing_import() {
        let dir = tempfile::tempdir().unwrap();
        let mut list = SchemaList::read(&options(dir.path())).unwrap();
        // a schema: its text, read-only, with the lines
        assert_eq!(list.enter(), Outcome::Nothing);
        let pager = list.pager.as_ref().unwrap();
        assert!(pager.path.ends_with("top.yaml"));
        assert_eq!(pager.lines[0], "meta:");
        assert_eq!(pager.lines.len(), 11);
        list.scroll(4);
        assert_eq!(list.pager.as_ref().unwrap().top, 4);
        list.scroll(100);
        assert_eq!(list.pager.as_ref().unwrap().top, 10);
        list.scroll_to(false);
        assert_eq!(list.pager.as_ref().unwrap().top, 0);
        // Esc closes the text, then the list
        assert_eq!(list.back(), Outcome::Nothing);
        assert!(list.pager.is_none());
        assert_eq!(list.back(), Outcome::Close);
        // a type
        list.select_type("top::chunk");
        assert_eq!(list.enter(), Outcome::Choose("top::chunk".to_owned()));
        // a missing import
        list.jump(true);
        assert_eq!(list.enter(), Outcome::Nothing);
        assert!(list.pager.is_none());
    }

    #[test]
    fn a_file_that_cannot_be_read_has_its_error_and_the_place_in_the_text() {
        let dir = tempfile::tempdir().unwrap();
        let options = options(dir.path());
        std::fs::write(dir.path().join("lib.yaml"), "id: lib\nseq:\n  - id: [\n").unwrap();
        let mut list = SchemaList::read(&options).unwrap();
        let broken = &list.entries[1];
        assert_eq!(broken.id, None);
        let error = broken.error.as_ref().unwrap();
        assert!(error.line.is_some_and(|l| l >= 1), "{error:?}");
        // its text is shown with the error
        list.selected = list.rows.iter().position(|r| *r == Row::Schema(1)).unwrap();
        list.enter();
        let pager = list.pager.as_ref().unwrap();
        assert!(pager.error_line.is_some());
        assert!(pager.message.is_some());
    }

    #[test]
    fn a_type_name_the_library_does_not_have_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut options = options(dir.path());
        options.type_or_schema = "no_such_type".to_owned();
        let error = SchemaList::read(&options).unwrap_err();
        assert!(error.contains("no_such_type"), "{error}");
    }
}
