//! Keeping the choice of a theme: the one thing the viewer writes to the
//! configuration file, the `theme` key, when `Enter` is pressed in the list of
//! themes.
//!
//! The rest of the file is left as it is (other keys, comments, order and
//! formatting): the text is edited, not written again from a struct. A file
//! that is not valid TOML is not touched. The new text is written to a file
//! beside the configuration and renamed over it, so that a crash does not leave
//! half a file; the permissions of the file are kept and a symbolic link is
//! followed (a configuration kept by a tool for dotfiles is a link), not
//! replaced.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use toml_edit::{DocumentMut, Item, Value};

/// What [`save_theme`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Saved {
    /// The file was written.
    Written,
    /// The file already had what was asked for, so it was not touched.
    Unchanged,
}

/// Set the `theme` key of the configuration file `path` to `theme`, or take the
/// key out when `theme` is `None` (the default is the terminal's own colours).
/// The file and its directory are made when they are not there.
///
/// # Errors
///
/// Fails, and leaves the file as it was, when it cannot be read, is not valid
/// TOML, or cannot be written.
pub fn save_theme(path: &Path, theme: Option<&str>) -> anyhow::Result<Saved> {
    // a link is followed: the file that it points to is the one that is edited
    let target = match fs::canonicalize(path) {
        Ok(real) => real,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => path.to_owned(),
        Err(error) => return Err(error).with_context(|| format!("cannot read {}", path.display())),
    };
    let text = match fs::read_to_string(&target) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot read {}", target.display()));
        }
    };
    let mut document: DocumentMut = text.parse().map_err(|error| {
        anyhow!(
            "{} is not valid TOML, so it was not changed: {error}",
            target.display()
        )
    })?;
    let has = document
        .get("theme")
        .and_then(Item::as_str)
        .map(str::to_owned);
    if let Some(id) = theme {
        if has.as_deref() == Some(id) {
            return Ok(Saved::Unchanged);
        }
        let mut value = Value::from(id);
        // the comment after the old value stays with the new one
        if let Some(old) = document.get("theme").and_then(Item::as_value) {
            *value.decor_mut() = old.decor().clone();
        }
        document.insert("theme", Item::Value(value));
    } else {
        if document.get("theme").is_none() {
            return Ok(Saved::Unchanged);
        }
        document.remove("theme");
    }
    write_beside(&target, &document.to_string())?;
    Ok(Saved::Written)
}

/// Write `text` to a file next to `target` and rename it over `target`.
fn write_beside(target: &Path, text: &str) -> anyhow::Result<()> {
    let dir = target
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_owned);
    fs::create_dir_all(&dir).with_context(|| format!("cannot make {}", dir.display()))?;
    let name = target.file_name().map_or_else(
        || "config.toml".to_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    let temp = dir.join(format!(".{name}.doe-{}.tmp", std::process::id()));
    let result = (|| -> anyhow::Result<()> {
        let mut file = fs::File::create(&temp)
            .with_context(|| format!("cannot write in {}", dir.display()))?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        // the permissions of the file that is replaced
        if let Ok(meta) = fs::metadata(target) {
            fs::set_permissions(&temp, meta.permissions())?;
        }
        fs::rename(&temp, target)
            .with_context(|| format!("cannot replace {}", target.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn a_file_and_its_directory_are_made() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deeper").join("doe").join("config.toml");
        assert_eq!(save_theme(&path, Some("nord")).unwrap(), Saved::Written);
        assert_eq!(read(&path), "theme = \"nord\"\n");
        // no temporary file is left
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn other_keys_comments_and_tables_are_kept_and_the_key_goes_before_the_tables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let before = "# my doe\ninclude_paths = [\"~/types\"]   # where they are\nradix = \"hex\"\n\n[extra]\nkept = 1 # yes\n";
        fs::write(&path, before).unwrap();
        assert_eq!(save_theme(&path, Some("dracula")).unwrap(), Saved::Written);
        let after = read(&path);
        assert!(
            after.starts_with(before.split("\n[extra]").next().unwrap()),
            "{after}"
        );
        assert!(after.contains("[extra]\nkept = 1 # yes\n"), "{after}");
        let key = after.find("theme = \"dracula\"").expect("the key is there");
        assert!(
            key < after.find("[extra]").unwrap(),
            "the key is before the table: {after}"
        );
        // and it reads back
        let parsed: toml_edit::DocumentMut = after.parse().unwrap();
        assert_eq!(parsed["theme"].as_str(), Some("dracula"));
        assert_eq!(parsed["radix"].as_str(), Some("hex"));
    }

    #[test]
    fn a_theme_that_is_there_is_replaced_with_its_comment_and_a_same_value_does_not_touch_the_file()
    {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "a = 1\ntheme = \"nord\"  # the colours\nb = 2\n").unwrap();
        assert_eq!(save_theme(&path, Some("nord")).unwrap(), Saved::Unchanged);
        assert_eq!(
            read(&path),
            "a = 1\ntheme = \"nord\"  # the colours\nb = 2\n"
        );
        assert_eq!(
            save_theme(&path, Some("gruvbox-dark")).unwrap(),
            Saved::Written
        );
        assert_eq!(
            read(&path),
            "a = 1\ntheme = \"gruvbox-dark\"  # the colours\nb = 2\n"
        );
    }

    #[test]
    fn the_terminals_own_colours_take_the_key_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "a = 1\ntheme = \"nord\"\nb = 2\n").unwrap();
        assert_eq!(save_theme(&path, None).unwrap(), Saved::Written);
        assert_eq!(read(&path), "a = 1\nb = 2\n");
        // nothing to take out
        assert_eq!(save_theme(&path, None).unwrap(), Saved::Unchanged);
        let missing = dir.path().join("none.toml");
        assert_eq!(save_theme(&missing, None).unwrap(), Saved::Unchanged);
        assert!(!missing.exists(), "no file is made to remove a key from it");
    }

    #[test]
    fn a_file_that_is_not_toml_is_left_alone_and_the_error_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "this = is = not toml\n").unwrap();
        let error = save_theme(&path, Some("nord")).unwrap_err().to_string();
        assert!(error.contains("not valid TOML"), "{error}");
        assert_eq!(read(&path), "this = is = not toml\n");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_followed_and_the_permissions_are_kept() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("dotfiles").join("doe.toml");
        fs::create_dir_all(real.parent().unwrap()).unwrap();
        fs::write(&real, "a = 1\n").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
        let link = dir.path().join("config.toml");
        symlink(&real, &link).unwrap();
        assert_eq!(save_theme(&link, Some("monokai")).unwrap(), Saved::Written);
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the link is still a link"
        );
        assert_eq!(read(&real), "a = 1\ntheme = \"monokai\"\n");
        assert_eq!(
            fs::metadata(&real).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_that_cannot_be_written_is_an_error_and_the_file_stays() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "a = 1\n").unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
        // a person who is root can write anywhere: then there is nothing to test
        let can = fs::File::create(dir.path().join("probe")).is_ok();
        let result = save_theme(&path, Some("nord"));
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        if can {
            return;
        }
        let error = result.unwrap_err().to_string();
        assert!(error.contains("cannot write"), "{error}");
        assert_eq!(read(&path), "a = 1\n");
    }
}
