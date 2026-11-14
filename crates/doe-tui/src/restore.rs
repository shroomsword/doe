//! Where you were: the record that the viewer keeps for each file and schema, and reads when they are opened again.
//!
//! A record holds paths and positions and nothing of the file: the open nodes and the selected one as paths
//! (`entries[2].len`), the cursor and the scroll, the radix and the settings of the view. It is a small TOML file in
//! a directory of the user's own, named by a hash of the file and the schema. It is never required and never an error:
//! a directory or a record that cannot be read, is too big, is of another version or is not the one asked for is
//! ignored, and a record that cannot be written is not written.

use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The version of the format of a record; another one is ignored.
pub const VERSION: u32 = 1;
/// The most records that the directory keeps: when one is written past this, the oldest go.
pub const MAX_RECORDS: usize = 200;
/// The most bytes of a record that is written: the lists are cut to fit.
pub const MAX_RECORD_BYTES: usize = 64 * 1024;
/// A record bigger than this is not read.
const MAX_READ_BYTES: u64 = 256 * 1024;

/// A window of the children of a node that has many, as the tree shows them: the first and the end.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowRecord {
    /// The node, as a path.
    pub path: String,
    /// The first child shown.
    pub start: usize,
    /// The end of the children shown.
    pub end: usize,
}

/// The tree as it was left.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeRecord {
    /// The nodes that were open, as paths.
    pub open: Vec<String>,
    /// The windows of the long lists.
    pub windows: Vec<WindowRecord>,
    /// The selected node, as a path.
    pub selected: String,
    /// The index of the first row shown.
    pub first_row: usize,
}

/// The bytes as they were left.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BytesRecord {
    /// The byte the cursor was on.
    pub cursor: u64,
    /// The offset of the first line shown.
    pub top: u64,
    /// Where the selection of bytes began, if there was one.
    pub range_from: Option<u64>,
}

/// What the view showed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "a record of the switches of the view, one field each, as it is written in the file"
)]
pub struct ViewRecord {
    /// The pane that had the keys: `tree` or `bytes`.
    pub focus: String,
    /// Whether a pane filled the screen.
    pub fullscreen: bool,
    /// Whether the inspector was shown.
    pub inspector: bool,
    /// Whether the files of an archive were shown in the place of the fields.
    pub files: bool,
    /// Whether the bytes were coloured by the field they belong to.
    pub byte_colours: bool,
    /// Whether the descriptions of the fields were shown.
    pub descriptions: bool,
    /// Whether Markdown entries were shown rendered.
    pub markdown_rendered: bool,
    /// The algorithm of the checksum column, by its label, if it was shown.
    pub checksums: Option<String>,
    /// The radix of the integers that have none of their own, by its name.
    pub radix: String,
}

/// The radix that a field chose for itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RadixRecord {
    /// The key of the field.
    pub field: String,
    /// The name of the radix.
    pub radix: String,
}

/// Where you were with a file and a schema.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// The version of the format.
    pub version: u32,
    /// The file and the schema that this is for, as [`key_of`] writes them. The name of the record is a hash of it, and
    /// the hash is not trusted.
    pub key: String,
    /// The size of the file when it was left.
    pub file_size: u64,
    /// When the file was last changed, in seconds, when it was left (0 if it could not be told).
    pub file_mtime: i64,
    /// The tree.
    pub tree: TreeRecord,
    /// The bytes.
    pub bytes: BytesRecord,
    /// The view.
    pub view: ViewRecord,
    /// The radix of the fields that have one of their own.
    pub radixes: Vec<RadixRecord>,
}

/// What names a record: the file, the schema as it was asked for and the type that was parsed.
#[must_use]
pub fn key_of(file: &Path, schema: &str, type_name: &str) -> String {
    let canonical = |path: &Path| fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let (named, wanted) = doe::split_type_arg(schema);
    let schema = if doe::is_schema_path(named) {
        let path = canonical(Path::new(named));
        match wanted {
            Some(wanted) => format!("{}::{wanted}", path.display()),
            None => path.display().to_string(),
        }
    } else {
        schema.to_owned()
    };
    format!(
        "file={}\nschema={schema}\ntype={type_name}",
        canonical(file).display()
    )
}

/// The name of the file of the record with this key: 16 hex digits of FNV-1a, which does not change with the Rust
/// version as the standard hasher may.
#[must_use]
pub fn name_of(key: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}.toml")
}

/// The directory of the records.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// The records in `dir` (made when the first is written).
    #[must_use]
    pub fn new(dir: &Path) -> Self {
        Store {
            dir: dir.join("viewer"),
        }
    }

    /// The record for `key`, if there is a good one.
    #[must_use]
    pub fn read(&self, key: &str) -> Option<Record> {
        let path = self.dir.join(name_of(key));
        let mut file = fs::File::open(path).ok()?;
        if file.metadata().ok()?.len() > MAX_READ_BYTES {
            return None;
        }
        let mut text = String::new();
        file.read_to_string(&mut text).ok()?;
        let record: Record = toml::from_str(&text).ok()?;
        (record.version == VERSION && record.key == key).then_some(record)
    }

    /// Keep `record`, and drop the oldest records past [`MAX_RECORDS`]. Nothing is reported if it cannot be done.
    pub fn write(&self, record: &Record) {
        let Some(text) = fit(record) else {
            return;
        };
        if self.make_dir().is_err() {
            return;
        }
        let name = name_of(&record.key);
        let temporary = self.dir.join(format!(".{name}.tmp"));
        if write_private(&temporary, text.as_bytes()).is_err()
            || fs::rename(&temporary, self.dir.join(&name)).is_err()
        {
            let _ = fs::remove_file(&temporary);
            return;
        }
        self.prune();
    }

    fn make_dir(&self) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt as _;
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&self.dir)
        }
        #[cfg(not(unix))]
        {
            fs::create_dir_all(&self.dir)
        }
    }

    /// Keep the newest [`MAX_RECORDS`] records.
    fn prune(&self) {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        let mut records: Vec<(std::time::SystemTime, PathBuf)> = entries
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .collect();
        if records.len() <= MAX_RECORDS {
            return;
        }
        records.sort();
        let extra = records.len() - MAX_RECORDS;
        for (_, path) in records.into_iter().take(extra) {
            let _ = fs::remove_file(path);
        }
    }
}

/// A file that only the user can read, with `bytes` in it.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)
}

/// The text of `record`, with its lists cut until it is no more than [`MAX_RECORD_BYTES`]; `None` if it still is
/// (or cannot be written).
fn fit(record: &Record) -> Option<String> {
    let mut record = record.clone();
    loop {
        let text = toml::to_string(&record).ok()?;
        if text.len() <= MAX_RECORD_BYTES {
            return Some(text);
        }
        let (open, windows, radixes) = (
            record.tree.open.len(),
            record.tree.windows.len(),
            record.radixes.len(),
        );
        if open + windows + radixes == 0 {
            return None;
        }
        record.tree.open.truncate(open / 2);
        record.tree.windows.truncate(windows / 2);
        record.radixes.truncate(radixes / 2);
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;

    fn record(key: &str) -> Record {
        Record {
            version: VERSION,
            key: key.to_owned(),
            file_size: 12,
            file_mtime: 1_700_000_000,
            tree: TreeRecord {
                open: vec!["entries".to_owned(), "entries[2]".to_owned()],
                windows: vec![WindowRecord {
                    path: "entries".to_owned(),
                    start: 100,
                    end: 300,
                }],
                selected: "entries[2].len".to_owned(),
                first_row: 4,
            },
            bytes: BytesRecord {
                cursor: 7,
                top: 0,
                range_from: Some(3),
            },
            view: ViewRecord {
                focus: "bytes".to_owned(),
                fullscreen: true,
                inspector: true,
                files: false,
                byte_colours: true,
                descriptions: true,
                markdown_rendered: false,
                checksums: Some("SHA-1".to_owned()),
                radix: "hex".to_owned(),
            },
            radixes: vec![RadixRecord {
                field: "t.len".to_owned(),
                radix: "bin".to_owned(),
            }],
        }
    }

    #[test]
    fn a_record_is_read_as_it_was_written() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let one = record("file=/a\nschema=s\ntype=t");
        store.write(&one);
        assert_eq!(store.read(&one.key), Some(one));
        assert_eq!(store.read("file=/b\nschema=s\ntype=t"), None);
    }

    #[test]
    fn a_record_that_cannot_be_used_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let one = record("file=/a\nschema=s\ntype=t");
        store.write(&one);
        let path = dir.path().join("viewer").join(name_of(&one.key));
        // not TOML
        fs::write(&path, "[[[ not toml").unwrap();
        assert_eq!(store.read(&one.key), None);
        // another version
        let mut other = one.clone();
        other.version = 99;
        fs::write(&path, toml::to_string(&other).unwrap()).unwrap();
        assert_eq!(store.read(&one.key), None);
        // the record of another key under this name: the hash is not trusted
        let mut other = one.clone();
        other.key = "file=/other\nschema=s\ntype=t".to_owned();
        fs::write(&path, toml::to_string(&other).unwrap()).unwrap();
        assert_eq!(store.read(&one.key), None);
        // too big to read
        fs::write(&path, vec![b'#'; 300 * 1024]).unwrap();
        assert_eq!(store.read(&one.key), None);
        // a field of the wrong kind
        fs::write(&path, "version = \"one\"\n").unwrap();
        assert_eq!(store.read(&one.key), None);
    }

    #[test]
    fn a_directory_that_cannot_be_made_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        // a file where the directory should be
        fs::write(dir.path().join("viewer"), "in the way").unwrap();
        let store = Store::new(dir.path());
        store.write(&record("file=/a\nschema=s\ntype=t"));
        assert_eq!(store.read("file=/a\nschema=s\ntype=t"), None);
    }

    #[test]
    fn the_oldest_records_go_past_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        for i in 0..MAX_RECORDS + 5 {
            store.write(&record(&format!("file=/f{i}\nschema=s\ntype=t")));
            // the time of a file has the resolution of the file system
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let count = fs::read_dir(dir.path().join("viewer"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
            .count();
        assert_eq!(count, MAX_RECORDS);
        assert!(
            store.read("file=/f0\nschema=s\ntype=t").is_none(),
            "the oldest went"
        );
        assert!(
            store
                .read(&format!("file=/f{}\nschema=s\ntype=t", MAX_RECORDS + 4))
                .is_some()
        );
    }

    #[test]
    fn a_record_that_is_too_big_is_cut_to_fit() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let mut big = record("file=/a\nschema=s\ntype=t");
        big.tree.open = (0..20_000).map(|i| format!("entries[{i}]")).collect();
        store.write(&big);
        let path = dir.path().join("viewer").join(name_of(&big.key));
        assert!(fs::metadata(path).unwrap().len() <= MAX_RECORD_BYTES as u64);
        let read = store.read(&big.key).unwrap();
        assert!(!read.tree.open.is_empty() && read.tree.open.len() < 20_000);
        assert_eq!(
            read.tree.open.first().map(String::as_str),
            Some("entries[0]")
        );
        assert_eq!(read.bytes, big.bytes, "what is not a list is kept");
    }

    #[cfg(unix)]
    #[test]
    fn the_directory_and_the_records_are_the_user_s_alone() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let one = record("file=/a\nschema=s\ntype=t");
        store.write(&one);
        let viewer = dir.path().join("viewer");
        assert_eq!(
            fs::metadata(&viewer).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let file = viewer.join(name_of(&one.key));
        assert_eq!(
            fs::metadata(file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn the_key_names_the_file_the_schema_and_the_type_and_the_name_is_stable() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f.bin");
        fs::write(&file, [1]).unwrap();
        let a = key_of(&file, "tar", "tar");
        let b = key_of(&file, "zip_archive", "zip_archive");
        assert_ne!(a, b, "another schema is another record");
        assert_ne!(
            key_of(&file, "t.yaml::inner", "inner"),
            key_of(&file, "t.yaml", "t")
        );
        // the same text always has the same name (this one is written on disk)
        assert_eq!(
            name_of("file=/a\nschema=s\ntype=t"),
            name_of("file=/a\nschema=s\ntype=t")
        );
        assert_eq!(name_of(""), "cbf29ce484222325.toml");
        assert_eq!(name_of("a"), "af63dc4c8601ec8c.toml");
    }
}
