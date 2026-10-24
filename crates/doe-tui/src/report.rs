//! What went wrong, in words for the screen: a parse that stopped part of the
//! way (with the field and the type it stopped in), or a schema, a type or a
//! file that stopped everything (with the place in the schema when there is one).

use std::error::Error as _;
use std::fmt::Write as _;
use std::path::Path;

use doe::DoeError;
use doe::schema::locate::locate_error;

use crate::format;
use crate::load::{LoadFailure, Stage};

/// The most of a schema that is read to find the place of an error.
const MAX_SCHEMA_BYTES: u64 = 4 * 1024 * 1024;

/// A place in the text of a schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// The file, as it was named.
    pub file: String,
    /// The line, from 1.
    pub line: u32,
    /// The column, from 1, in characters.
    pub column: u32,
    /// The text of the line, safe to print.
    pub source_line: String,
    /// How many cells in the line the error starts.
    pub caret_cells: usize,
}

/// What stopped the viewer from showing everything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// One line: what happened.
    pub headline: String,
    /// The message of the error and of what caused it, a line each.
    pub causes: Vec<String>,
    /// Where in the schema, when that is known.
    pub place: Option<Place>,
    /// Whether nothing at all could be parsed (the tree is empty), against a parse that stopped part of the way.
    pub fatal: bool,
}

/// An error and what caused it, a message each.
fn chain(error: &DoeError) -> Vec<String> {
    let mut messages = vec![format::safe(&error.to_string())];
    let mut cause = error.source();
    while let Some(next) = cause {
        messages.push(format::safe(&next.to_string()));
        cause = next.source();
    }
    messages
}

/// Where in the schema file `error` is, if the file can be read and the error is about it.
fn place_in(path: &Path, error: &DoeError) -> Option<Place> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_SCHEMA_BYTES {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let position = locate_error(&text, error)?;
    let line = text
        .lines()
        .nth(usize::try_from(position.line).ok()?.checked_sub(1)?)?;
    let before: String = line
        .chars()
        .take(usize::try_from(position.column).ok()?.saturating_sub(1))
        .collect();
    Some(Place {
        file: format::safe(&path.display().to_string()),
        line: position.line,
        column: position.column,
        source_line: format::safe(line),
        caret_cells: format::width(&format::safe(&before)),
    })
}

impl Report {
    /// Nothing could be parsed.
    #[must_use]
    pub fn from_failure(failure: &LoadFailure, schema_file: Option<&Path>) -> Report {
        let headline = match failure.stage {
            Stage::Schema => format!(
                "the schema {} cannot be used",
                format::safe(&failure.type_name)
            ),
            Stage::File => "the file cannot be read".to_owned(),
        };
        let place = match failure.stage {
            Stage::Schema => schema_file.and_then(|path| place_in(path, &failure.error)),
            Stage::File => None,
        };
        Report {
            headline,
            causes: chain(&failure.error),
            place,
            fatal: true,
        }
    }

    /// The parse stopped at `field` (the names from the root), in the type `in_type`.
    #[must_use]
    pub fn from_parse(error: &DoeError, field: &[String], in_type: Option<&str>) -> Report {
        let at = if field.len() > 1 {
            let path = crate::status::field_path(field);
            match in_type {
                Some(t) => format!(" at {path} (in {})", format::safe(t)),
                None => format!(" at {path}"),
            }
        } else {
            String::new()
        };
        Report {
            headline: format!("the parse failed{at}"),
            causes: chain(error),
            place: None,
            fatal: false,
        }
    }

    /// The report in a single line, for the message after the viewer has been left.
    #[must_use]
    pub fn one_line(&self) -> String {
        let mut line = self.headline.clone();
        if let Some(cause) = self.causes.first() {
            line.push_str(": ");
            line.push_str(cause);
        }
        if let Some(place) = &self.place {
            let _ = write!(line, " ({}:{}:{})", place.file, place.line, place.column); // writing to a String cannot fail
        }
        line
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;
    use crate::Options;
    use crate::load::try_load;

    fn options(dir: &Path, schema: &str, data: Option<&[u8]>) -> Options {
        std::fs::write(dir.join("t.yaml"), schema).unwrap();
        if let Some(data) = data {
            std::fs::write(dir.join("t.bin"), data).unwrap();
        }
        Options {
            file: dir.join("t.bin"),
            type_or_schema: dir.join("t.yaml").to_string_lossy().into_owned(),
            include_paths: vec![dir.to_path_buf()],
            color: false,
        }
    }

    #[test]
    fn a_schema_that_does_not_compile_has_its_place() {
        let dir = tempfile::tempdir().unwrap();
        let schema = "id: bad\nseq:\n  - id: a\n    type: nope\n";
        let options = options(dir.path(), schema, Some(&[1]));
        let failure = try_load(&options).unwrap_err();
        assert_eq!(failure.stage, Stage::Schema);
        let report = Report::from_failure(&failure, crate::load::schema_file(&options).as_deref());
        assert!(report.fatal);
        assert_eq!(report.headline, "the schema bad cannot be used");
        assert!(report.causes[0].contains("nope"), "{:?}", report.causes);
        let place = report.place.unwrap();
        assert_eq!((place.line, place.column), (4, 11));
        assert_eq!(place.source_line, "    type: nope");
        assert_eq!(place.caret_cells, 10);
        assert!(place.file.ends_with("t.yaml"));
    }

    #[test]
    fn an_unreadable_file_and_an_unknown_type_have_no_place() {
        let dir = tempfile::tempdir().unwrap();
        let options = options(dir.path(), "id: ok\nseq:\n  - id: a\n    type: u1\n", None);
        let failure = try_load(&options).unwrap_err();
        assert_eq!(failure.stage, Stage::File);
        let report = Report::from_failure(&failure, None);
        assert_eq!(report.headline, "the file cannot be read");
        assert!(
            report.causes[0].contains("could not read file"),
            "{:?}",
            report.causes
        );
        assert!(
            report.causes.len() >= 2,
            "the cause (no such file) follows: {:?}",
            report.causes
        );
        assert!(report.place.is_none());

        let unknown = Options {
            type_or_schema: "no_such_type".to_owned(),
            ..options
        };
        let failure = try_load(&unknown).unwrap_err();
        assert_eq!(failure.stage, Stage::Schema);
        let report = Report::from_failure(&failure, crate::load::schema_file(&unknown).as_deref());
        assert!(
            report.causes[0].contains("schema not found"),
            "{:?}",
            report.causes
        );
        assert!(report.place.is_none());
    }

    #[test]
    fn a_yaml_error_has_its_place_too() {
        let dir = tempfile::tempdir().unwrap();
        let options = options(dir.path(), "id: x\nseq: [\n", Some(&[1]));
        let failure = try_load(&options).unwrap_err();
        let report = Report::from_failure(&failure, crate::load::schema_file(&options).as_deref());
        assert!(report.place.is_some(), "{report:?}");
    }

    #[test]
    fn the_report_of_a_parse_names_the_field_and_the_type_and_is_one_line_when_asked() {
        let error = DoeError::Schema("boom".to_owned());
        let field: Vec<String> = ["rec", "items", "[2]", "kind"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let report = Report::from_parse(&error, &field, Some("rec::item"));
        assert_eq!(
            report.headline,
            "the parse failed at rec/items/[2]/kind (in rec::item)"
        );
        assert!(!report.fatal);
        assert_eq!(
            report.one_line(),
            format!(
                "the parse failed at rec/items/[2]/kind (in rec::item): {}",
                report.causes[0]
            )
        );
        // a failure at the root has no field to name
        let root = Report::from_parse(&error, &["rec".to_owned()], None);
        assert_eq!(root.headline, "the parse failed");
    }

    #[test]
    fn nothing_in_a_report_is_a_control_character() {
        let error = DoeError::Schema("a\x1b[2Jb\x07".to_owned());
        let report = Report::from_parse(
            &error,
            &["x\x1by".to_owned(), "z".to_owned()],
            Some("t\x1b"),
        );
        let all = format!("{} {:?}", report.headline, report.causes);
        assert!(!all.chars().any(char::is_control), "{all:?}");
    }
}
