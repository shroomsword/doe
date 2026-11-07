//! The entries of an archive or file system as a list of files and directories.
//!
//! A schema says that its files are file systems with a `filesystem:` key ([`RawFilesystem`]): the list of entries and
//! the names of the fields of an entry that are its path, size, mode, time, kind, link target and contents. [`list`]
//! reads those from a parsed value and gives every entry a normalised path, a kind, and the **path in the trace** of
//! its node (so that a viewer can select the entry in the tree of fields and in the bytes) and the bytes of its
//! contents. A viewer nests the entries by path, to show directories; the library does not build the tree, so that
//! a list of a million entries costs a million small records, and no more.
//!
//! An entry that cannot be placed safely (an empty name, an absolute path, a `..`, a path that an earlier entry has)
//! is in the list with a flag, never left out and never resolved: nothing here touches the file system.

use serde::{Deserialize, Serialize};

use crate::parser::trace::TraceNode;
use crate::schema::raw::{OneOrMany, RawFilesystem};
use crate::value::Value;

/// What an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
    /// A symbolic link.
    Symlink,
    /// A hard link to another entry.
    Hardlink,
    /// A block or character device.
    Device,
    /// A named pipe.
    Fifo,
    /// A socket.
    Socket,
}

/// Why an entry is flagged.
pub mod flag {
    /// The path has no name.
    pub const EMPTY: &str = "empty";
    /// The path starts with `/`.
    pub const ABSOLUTE: &str = "absolute";
    /// The path has a `..` component.
    pub const PARENT: &str = "parent";
    /// An earlier entry has the same path.
    pub const REPEATED: &str = "repeated";
}

fn yes() -> bool {
    true
}

/// One entry of the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Entry {
    /// The components of the path, without empty ones and `.`; `..` is kept as it is.
    pub path: Vec<String>,
    /// What the entry is.
    pub kind: Kind,
    /// The size in bytes, if the schema says where it is.
    pub size: Option<u64>,
    /// The mode: permission bits, and the type bits when the format has them.
    pub mode: Option<u32>,
    /// The modification time, in seconds since 1970.
    pub mtime: Option<i64>,
    /// The target of a link.
    pub link: Option<String>,
    /// The indexes of the children that lead to the entry's node in the trace.
    pub trace_path: Vec<usize>,
    /// The bytes of the contents in the input: first, and the byte after the last.
    pub data: Option<(u64, u64)>,
    /// Whether `data` is the file's own bytes. False when the schema's `data_is_file` says the range holds them
    /// changed (a zip entry that is deflated): a preview, a saved file or a checksum of those bytes would mislead.
    #[serde(default = "yes")]
    pub data_is_file: bool,
    /// Why the entry cannot be placed as it is, from [`flag`].
    pub flags: Vec<String>,
}

/// The entries of a value that a schema says is a file system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listing {
    /// The entries, in the order of the archive.
    pub entries: Vec<Entry>,
}

/// The value inside an instance, or the value.
fn plain(value: &Value) -> &Value {
    match value {
        Value::Instance { value, .. } => value,
        other => other,
    }
}

/// The value at a dotted path of field names, from a struct.
fn field<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut at = plain(value);
    for name in path.split('.') {
        let Value::Struct { fields, .. } = at else {
            return None;
        };
        at = plain(&fields.iter().find(|(n, _)| n == name)?.1);
    }
    Some(at)
}

/// The child of a trace node that is called `name`.
fn trace_child<'a>(node: &'a TraceNode, name: &str) -> Option<(usize, &'a TraceNode)> {
    node.children
        .iter()
        .enumerate()
        .find(|(_, c)| c.name == name)
}

/// The trace node at a dotted path from `node`, with the indexes that lead to it.
fn trace_at<'a>(node: &'a TraceNode, path: &str) -> Option<(Vec<usize>, &'a TraceNode)> {
    let mut at = node;
    let mut indexes = Vec::new();
    for name in path.split('.') {
        let (index, child) = trace_child(at, name)?;
        indexes.push(index);
        at = child;
    }
    Some((indexes, at))
}

fn text_of(value: &Value) -> Option<String> {
    match plain(value) {
        Value::Str(s) => Some(s.clone()),
        Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        Value::Enum { name: Some(n), .. } => Some(n.clone()),
        Value::UInt(n) => Some(n.to_string()),
        Value::SInt(n) => Some(n.to_string()),
        Value::Enum { value, name: None } => Some(value.to_string()),
        _ => None,
    }
}

fn number_of(value: &Value) -> Option<i128> {
    match plain(value) {
        Value::UInt(n) => Some(i128::from(*n)),
        Value::SInt(n) => Some(i128::from(*n)),
        Value::Enum { value, .. } => Some(*value),
        Value::Str(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Whether a value of the `kinds` table of the schema is the value of the field.
fn same(wanted: &serde_json::Value, found: &str) -> bool {
    match wanted {
        serde_json::Value::String(s) => s == found,
        other => {
            // a number in the table (`5`) is the text of the field ("5")
            let text = other.to_string();
            text == found
        }
    }
}

fn kind_of(spec: &RawFilesystem, entry: &Value, mode: Option<u32>, trailing_slash: bool) -> Kind {
    if let Some(found) = spec
        .kind
        .as_deref()
        .and_then(|path| field(entry, path))
        .and_then(text_of)
    {
        for (name, values) in &spec.kinds {
            if values.iter().any(|v| same(v, &found)) {
                match name.as_str() {
                    "directory" => return Kind::Directory,
                    "symlink" => return Kind::Symlink,
                    "hardlink" => return Kind::Hardlink,
                    "device" => return Kind::Device,
                    "fifo" => return Kind::Fifo,
                    "socket" => return Kind::Socket,
                    "file" => return Kind::File,
                    _ => {}
                }
            }
        }
    }
    match mode.map(|m| m & 0o170_000) {
        Some(0o040_000) => Kind::Directory,
        Some(0o120_000) => Kind::Symlink,
        Some(0o020_000 | 0o060_000) => Kind::Device,
        Some(0o010_000) => Kind::Fifo,
        Some(0o140_000) => Kind::Socket,
        Some(0o100_000) => Kind::File,
        _ if trailing_slash => Kind::Directory,
        _ => Kind::File,
    }
}

/// A checksum algorithm for the files of a listing: MD5, SHA-1 and SHA-256. A viewer that asks for one by name, and lists
/// them with [`Algorithm::ALL`], does not change when one is added.
///
/// MD5 and SHA-1 are checksums for comparing files, not security measures: both have known collisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Algorithm {
    /// MD5, 32 hex digits, as `md5sum` writes.
    Md5,
    /// SHA-1, 40 hex digits, as `sha1sum` writes.
    Sha1,
    /// SHA-256, 64 hex digits, as `sha256sum` writes.
    Sha256,
}

impl Algorithm {
    /// Every algorithm, in the order a viewer offers and cycles them.
    pub const ALL: &'static [Algorithm] = &[Algorithm::Md5, Algorithm::Sha1, Algorithm::Sha256];

    /// The name as a person and `--` options write it: `md5`, `sha1`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Algorithm::Md5 => "md5",
            Algorithm::Sha1 => "sha1",
            Algorithm::Sha256 => "sha256",
        }
    }

    /// The name as a header shows it: `MD5`, `SHA-1`.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Algorithm::Md5 => "MD5",
            Algorithm::Sha1 => "SHA-1",
            Algorithm::Sha256 => "SHA-256",
        }
    }

    /// How many hex digits a checksum has.
    #[must_use]
    pub fn hex_len(self) -> usize {
        match self {
            Algorithm::Md5 => 32,
            Algorithm::Sha1 => 40,
            Algorithm::Sha256 => 64,
        }
    }

    /// Whether the algorithm is only for comparing files and not for security (it has known collisions): MD5 and
    /// SHA-1. A viewer says so in the tooltip of the header; SHA-256 does not need the warning.
    #[must_use]
    pub fn is_weak(self) -> bool {
        matches!(self, Algorithm::Md5 | Algorithm::Sha1)
    }

    /// The name of the file that a list of checksums is saved to: `checksums.md5`, `checksums.sha1`.
    #[must_use]
    pub fn file_name(self) -> String {
        format!("checksums.{}", self.name())
    }

    /// The algorithm of a name (`md5`, `sha1`, `sha-1`, in any case).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Algorithm> {
        Algorithm::ALL
            .iter()
            .copied()
            .find(|a| a.name().eq_ignore_ascii_case(name) || a.label().eq_ignore_ascii_case(name))
    }
}

/// A checksum that is computed as bytes come, so that a file of any size needs a chunk of memory and no more.
#[derive(Debug, Clone)]
pub struct Hasher {
    state: HasherState,
}

#[derive(Debug, Clone)]
enum HasherState {
    Md5(md5::Md5),
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
}

impl Hasher {
    /// A new checksum of `algorithm`.
    #[must_use]
    pub fn new(algorithm: Algorithm) -> Hasher {
        use md5::Digest;
        let state = match algorithm {
            Algorithm::Md5 => HasherState::Md5(md5::Md5::new()),
            Algorithm::Sha1 => HasherState::Sha1(sha1::Sha1::new()),
            Algorithm::Sha256 => HasherState::Sha256(sha2::Sha256::new()),
        };
        Hasher { state }
    }

    /// The next bytes.
    pub fn update(&mut self, bytes: &[u8]) {
        use md5::Digest;
        match &mut self.state {
            HasherState::Md5(state) => state.update(bytes),
            HasherState::Sha1(state) => state.update(bytes),
            HasherState::Sha256(state) => state.update(bytes),
        }
    }

    /// The checksum as lower case hex digits.
    #[must_use]
    pub fn finish(self) -> String {
        use md5::Digest;
        match self.state {
            HasherState::Md5(state) => hex(&state.finalize()),
            HasherState::Sha1(state) => hex(&state.finalize()),
            HasherState::Sha256(state) => hex(&state.finalize()),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // writing to a String cannot fail
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// The checksum of the bytes of `input` in `range` (first, and the byte after the last), or None when the range is not
/// inside `input`.
#[must_use]
pub fn digest_of(algorithm: Algorithm, input: &[u8], range: (u64, u64)) -> Option<String> {
    let bytes = input.get(usize::try_from(range.0).ok()?..usize::try_from(range.1).ok()?)?;
    let mut hasher = Hasher::new(algorithm);
    hasher.update(bytes);
    Some(hasher.finish())
}

/// The MD5 of the bytes of `input` in `range`: [`digest_of`] with `Algorithm::Md5`.
#[must_use]
pub fn md5_of(input: &[u8], range: (u64, u64)) -> Option<String> {
    digest_of(Algorithm::Md5, input, range)
}

/// The entries of `value`, as the `filesystem:` key `spec` says, with the trace of the parse that made it. None when
/// the value has no list at the path (a parse that stopped before it).
#[must_use]
pub fn list(value: &Value, trace: &TraceNode, spec: &RawFilesystem) -> Option<Listing> {
    let Value::Array(items) = field(value, &spec.entries)? else {
        return None;
    };
    let (list_path, list_node) = trace_at(trace, &spec.entries)?;
    let mut seen = std::collections::HashSet::new();
    let mut entries = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let Some(node) = list_node.children.get(index) else {
            break;
        };
        let mut texts = Vec::new();
        for name in spec.path.names() {
            if let Some(text) = field(item, name).and_then(text_of) {
                texts.push(text);
            }
        }
        let joined = texts
            .iter()
            .filter(|t| !t.is_empty())
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("/");
        if spec.ignore.contains(&joined) {
            continue;
        }
        let absolute = joined.starts_with('/');
        let trailing_slash = joined.ends_with('/');
        let path: Vec<String> = joined
            .split('/')
            .filter(|c| !c.is_empty() && *c != ".")
            .map(str::to_owned)
            .collect();
        let mut flags = Vec::new();
        if path.is_empty() {
            flags.push(flag::EMPTY.to_owned());
        }
        if absolute {
            flags.push(flag::ABSOLUTE.to_owned());
        }
        if path.iter().any(|c| c == "..") {
            flags.push(flag::PARENT.to_owned());
        }
        if !path.is_empty() && !seen.insert(path.clone()) {
            flags.push(flag::REPEATED.to_owned());
        }
        // the first of the names that the entry has
        let first = |names: &Option<OneOrMany>| {
            names
                .as_ref()
                .and_then(|names| names.names().into_iter().find_map(|p| field(item, p)))
        };
        let mode = first(&spec.mode)
            .and_then(number_of)
            .and_then(|n| u32::try_from(n).ok());
        let number = |names: &Option<OneOrMany>| first(names).and_then(number_of);
        let mut trace_path = list_path.clone();
        trace_path.push(index);
        let data = spec
            .data
            .as_ref()
            .and_then(|names| names.names().into_iter().find_map(|p| trace_at(node, p)))
            .and_then(|(_, n)| n.span)
            .filter(|s| !s.is_empty())
            .map(|s| (s.start_byte(), s.end_byte()));
        // a value that is false, 0 or absent-with-the-key-present means the range is not the file; no key: it is
        let data_is_file = spec.data_is_file.is_none()
            || first(&spec.data_is_file).is_some_and(|v| match plain(v) {
                Value::Bool(b) => *b,
                other => number_of(other).is_some_and(|n| n != 0),
            });
        let size = number(&spec.size).and_then(|n| u64::try_from(n).ok());
        // the block that ends a tar archive is an entry with no name, no size and no contents
        if path.is_empty() && size.unwrap_or(0) == 0 && data.is_none() {
            continue;
        }
        entries.push(Entry {
            kind: kind_of(
                spec,
                item,
                mode,
                trailing_slash && spec.directory_slash != Some(false),
            ),
            size,
            mode,
            mtime: number(&spec.mtime).and_then(|n| i64::try_from(n).ok()),
            link: first(&spec.link)
                .and_then(text_of)
                .filter(|t| !t.is_empty()),
            path,
            trace_path,
            data,
            data_is_file,
            flags,
        });
    }
    Some(Listing { entries })
}
