//! The state of a viewer that was started with `--build OUT`: the schema that is
//! being built (a [`Draft`]), where it will be saved, and the temporary file it is
//! written to after every change so that the ordinary viewer can parse the file
//! with it.

use std::path::{Path, PathBuf};

use crate::draft::{Draft, Edit, check_name};

/// A schema being built.
#[derive(Debug)]
pub struct Builder {
    /// The schema as it is now.
    pub draft: Draft,
    /// Where it is saved.
    pub out: PathBuf,
    /// The draft as it was when it was last saved (or begun), without the comments that
    /// have the offsets (those depend on the file).
    saved: String,
    /// The changes that can be taken back, the last one at the end.
    pub(crate) history: Vec<Edit>,
    /// The changes that were taken back and can be done again, the next one at the end.
    pub(crate) redo: Vec<Edit>,
    /// Whether `out` is a file of this builder's: it was there at the start (a schema to go on
    /// with), or the builder has written it.
    owned: bool,
    /// Where the schema is written for the viewer to parse with.
    dir: tempfile::TempDir,
}

/// The first line of a saved schema.
pub const HEADER: &str =
    "# Made with doe --tui --build, which can go on with it; other edits may not be read back.\n";

impl Builder {
    /// Begin a schema that will be saved to `out`, or go on with the one that is there
    /// if the builder made it.
    ///
    /// # Errors
    ///
    /// `out` is a schema that the builder did not make, or a file that is not a schema, or
    /// its name is not one a schema can have, or the temporary directory cannot be made.
    pub fn open(out: &Path) -> Result<Builder, String> {
        if !out.exists() {
            return Builder::new(out);
        }
        let text = std::fs::read_to_string(out)
            .map_err(|e| format!("could not read {} to go on with it: {e}", out.display()))?;
        let draft = Draft::from_yaml(&text)
            .map_err(|e| format!("{} cannot be gone on with: {e}", out.display()))?;
        Builder::with_draft(out, draft, true)
    }

    /// Begin a new schema that will be saved to `out`. The schema is named for the file.
    ///
    /// # Errors
    ///
    /// `out` exists already, or its name is not one a schema can have, or the
    /// temporary directory cannot be made.
    pub fn new(out: &Path) -> Result<Builder, String> {
        if out.exists() {
            return Err(format!(
                "{} exists already; --build goes on with a schema it made, and makes a new one only for a file that is not there",
                out.display()
            ));
        }
        let stem = out
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("{} has no name to give the schema", out.display()))?;
        check_name(stem).map_err(|e| {
            format!("the schema is named for its file, and {e}; name the file with lower case letters, digits and _")
        })?;
        let draft = Draft::new(stem).map_err(|e| e.to_string())?;
        Builder::with_draft(out, draft, false)
    }

    fn with_draft(out: &Path, draft: Draft, owned: bool) -> Result<Builder, String> {
        let dir = tempfile::Builder::new()
            .prefix("doe-build-")
            .tempdir()
            .map_err(|e| format!("could not make a temporary directory: {e}"))?;
        let builder = Builder {
            saved: draft.to_text(),
            draft,
            out: out.to_path_buf(),
            history: Vec::new(),
            redo: Vec::new(),
            owned,
            dir,
        };
        builder
            .write()
            .map_err(|e| format!("could not write the temporary schema: {e}"))?;
        Ok(builder)
    }

    /// The file the viewer parses with: the draft as it is.
    #[must_use]
    pub fn schema_path(&self) -> PathBuf {
        self.dir.path().join(format!("{}.yaml", self.draft.id()))
    }

    /// Write the draft to [`schema_path`](Self::schema_path).
    ///
    /// # Errors
    ///
    /// The file cannot be written.
    pub fn write(&self) -> std::io::Result<()> {
        std::fs::write(self.schema_path(), self.draft.to_yaml())
    }

    /// Write the schema to `out`, without a half-written file being seen: it is written to a
    /// file beside it, which then takes its place. A file that was not there when the
    /// builder began, and is now, is not replaced; nor is a symbolic link written through.
    ///
    /// # Errors
    ///
    /// Why it was not written.
    pub fn save(&mut self) -> Result<(), String> {
        let out = &self.out;
        match std::fs::symlink_metadata(out) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "{} is a symbolic link: the builder does not write through one",
                    out.display()
                ));
            }
            Ok(_) if !self.owned => {
                return Err(format!(
                    "{} is there now, and it was not when the builder began: it will not replace it",
                    out.display()
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("could not look at {}: {e}", out.display())),
        }
        let name = out
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("{} has no file name", out.display()))?;
        let folder = out
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let temporary = folder.join(format!(".{name}.doe-build-{}", std::process::id()));
        let text = format!("{HEADER}{}", self.draft.to_yaml());
        let written = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .and_then(|mut file| std::io::Write::write_all(&mut file, text.as_bytes()))
            .and_then(|()| std::fs::rename(&temporary, out));
        if let Err(e) = written {
            let _ = std::fs::remove_file(&temporary);
            return Err(format!("could not write {}: {e}", out.display()));
        }
        self.owned = true;
        self.mark_saved();
        Ok(())
    }

    /// Whether the draft has changed since it was saved.
    #[must_use]
    pub fn dirty(&self) -> bool {
        self.draft.to_text() != self.saved
    }

    /// Note that the draft was saved as it is now.
    pub fn mark_saved(&mut self) {
        self.saved = self.draft.to_text();
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;
    use crate::draft::{Field, Kind};

    #[test]
    fn the_schema_is_named_for_the_file_and_written_where_the_viewer_can_read_it() {
        let dir = tempfile::tempdir().unwrap();
        let builder = Builder::new(&dir.path().join("my_format.yaml")).unwrap();
        assert_eq!(builder.draft.id(), "my_format");
        let text = std::fs::read_to_string(builder.schema_path()).unwrap();
        assert!(text.contains("id: my_format"), "{text}");
        assert!(!builder.out.exists(), "nothing is saved yet");
    }

    #[test]
    fn a_file_that_is_there_or_a_name_that_is_not_a_schema_name_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let there = dir.path().join("there.yaml");
        std::fs::write(&there, "x").unwrap();
        let error = Builder::new(&there).unwrap_err();
        assert!(error.contains("exists already"), "{error}");
        let error = Builder::new(&dir.path().join("My-Format.yaml")).unwrap_err();
        assert!(error.contains("lower case"), "{error}");
    }

    #[test]
    fn the_draft_is_dirty_from_a_change_until_it_is_saved() {
        let dir = tempfile::tempdir().unwrap();
        let mut builder = Builder::new(&dir.path().join("t.yaml")).unwrap();
        assert!(!builder.dirty());
        builder
            .draft
            .push(Field {
                id: "a".to_owned(),
                kind: Kind::Unsigned(1),
            })
            .unwrap();
        assert!(builder.dirty());
        builder.mark_saved();
        assert!(!builder.dirty());
    }
}
