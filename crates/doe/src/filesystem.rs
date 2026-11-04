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
            flags,
        });
    }
    Some(Listing { entries })
}
