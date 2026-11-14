//! Where a parse broke and what usually causes it.
//!
//! A failed parse leaves an error and a trace whose failed node is the value that was being read. [`explain`] turns
//! them into a message that names the byte and the field, and into suggestions. A suggestion is only offered when it
//! has been tried: the edited schema is parsed again, and it counts only if that parse succeeds or goes further than
//! the first one did. A failure that is not one of the known kinds gets the plain message and no suggestion.

use std::fmt::Write as _;

use serde::Serialize;

use crate::copy::path_of;
use crate::error::DoeError;
use crate::parser::trace::TraceNode;

/// Where a parse broke, and what might fix it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct Failure {
    /// What kind of failure it is: `past_end` (a read needs more bytes than there are), `partial_element` (the
    /// input ends in the middle of an element of a list) or `contents` (a signature that is not there).
    pub kind: &'static str,
    /// The byte of the input where it broke.
    pub offset: u64,
    /// The failed value, as indexes of children from the root of the trace.
    pub path: Vec<usize>,
    /// The failed value, written as the schema does (`entries[2].len`).
    pub field: String,
    /// What happened, with the byte and the field: `byte 0x1f4, in entries[2].len: needs 4 bytes, 2 are left`.
    pub message: String,
    /// What to try, each of them checked.
    pub suggestions: Vec<Suggestion>,
}

/// A change to the schema that was tried, and helped.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct Suggestion {
    /// What it is: `other_byte_order`.
    pub id: &'static str,
    /// What it says, for the person.
    pub text: String,
    /// The text of the schema with the change.
    pub schema: String,
    /// What the parse with the change did: `succeeds` or `goes further`.
    pub result: &'static str,
}

/// What a parse with another schema did, as [`explain`] needs to know it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reparse {
    /// It did not fail.
    Succeeds,
    /// It failed at this byte.
    FailsAt(u64),
    /// It could not be tried (the schema does not compile, say).
    Unknown,
}

/// The byte order of an integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Order {
    Little,
    Big,
}

impl Order {
    fn other(self) -> Order {
        match self {
            Order::Little => Order::Big,
            Order::Big => Order::Little,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Order::Little => "little-endian",
            Order::Big => "big-endian",
        }
    }

    fn value(self, bytes: &[u8]) -> u64 {
        let fold = |acc: u64, b: &u8| (acc << 8) | u64::from(*b);
        match self {
            Order::Big => bytes.iter().fold(0, fold),
            Order::Little => bytes.iter().rev().fold(0, fold),
        }
    }
}

/// Say where the parse broke and what is likely to fix it.
///
/// - `error` and `trace` are what the failed parse gave.
/// - `schema` is the text of the schema, which a suggestion edits.
/// - `read` gives `len` bytes of the input from byte `offset`, if they can be had.
/// - `reparse` parses again with another text of the schema.
///
/// `None` when the error is not one that has a place (it is not in the data).
pub fn explain(
    error: &DoeError,
    trace: &TraceNode,
    schema: &str,
    read: &dyn Fn(u64, usize) -> Option<Vec<u8>>,
    reparse: &dyn Fn(&str) -> Reparse,
) -> Option<Failure> {
    let path = trace.failed_node()?;
    let field = path_of(trace, &path);
    let (kind, offset, detail) = match error {
        DoeError::Overrun {
            offset,
            requested,
            remaining,
        } => (
            "past_end",
            *offset as u64,
            format!(
                "needs {requested} byte{}, {remaining} {} left",
                plural(*requested),
                if *remaining == 1 { "is" } else { "are" }
            ),
        ),
        DoeError::ContentsMismatch {
            offset,
            expected,
            actual,
        } => (
            "contents",
            *offset as u64,
            format!("expected {}, found {}", hex(expected), hex(actual)),
        ),
        _ => return None,
    };
    let mut failure = Failure {
        kind,
        offset,
        message: format!("byte {offset:#x}, in {field}: {detail}"),
        path,
        field,
        suggestions: Vec::new(),
    };
    if kind == "past_end" {
        if let Some(found) = partial_element(trace, &failure.path) {
            failure.kind = "partial_element";
            failure.message = format!(
                "byte {offset:#x}, in {}: the input ends in the middle of an element: {found}",
                failure.field
            );
        } else if let Some(suggestion) =
            other_byte_order(trace, &failure.path, schema, offset, read, reparse)
        {
            failure.suggestions.push(suggestion);
        }
    }
    Some(failure)
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (i, b) in bytes.iter().take(16).enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let _ = write!(out, "{b:02x}");
    }
    if bytes.len() > 16 {
        out.push_str(" …");
    }
    out
}

/// If the failed value is an element of a list whose earlier elements are all as long as the read that failed, say
/// how many bytes are left after them: the input is cut, or the list does not end where the schema thinks.
fn partial_element(trace: &TraceNode, path: &[usize]) -> Option<String> {
    let (&index, parent_path) = path.split_last()?;
    let parent = trace.node_at(parent_path)?;
    let failed = parent.children.get(index)?;
    if !failed.name.starts_with('[') || index == 0 {
        return None;
    }
    let size = |node: &TraceNode| node.span.map(|s| s.end.saturating_sub(s.start));
    let first = size(parent.children.first()?)?;
    let before = parent.children.get(..index)?;
    if first == 0 || !before.iter().all(|e| size(e) == Some(first)) {
        return None;
    }
    let start = failed.span?.start_byte();
    let whole = before.last()?.span?.end_byte();
    let left = start.max(whole);
    Some(format!(
        "{index} whole elements of {} byte{} were read; the bytes after them are not enough for another (from byte {left:#x})",
        first / 8,
        plural(usize::try_from(first / 8).unwrap_or(0))
    ))
}

/// The integer fields that decide the size of the failed value, read the other way round, if that makes the parse go
/// further.
fn other_byte_order(
    trace: &TraceNode,
    path: &[usize],
    schema: &str,
    offset: u64,
    read: &dyn Fn(u64, usize) -> Option<Vec<u8>>,
    reparse: &dyn Fn(&str) -> Reparse,
) -> Option<Suggestion> {
    let failed = trace.node_at(path)?;
    for names in &failed.decided_by {
        let Some(decider) = trace.resolve_names(path, names) else {
            continue;
        };
        let node = trace.node_at(&decider)?;
        let span = node.span?;
        let len = usize::try_from(span.end_byte().checked_sub(span.start_byte())?).ok()?;
        if !(2..=8).contains(&len) || span.start % 8 != 0 {
            continue;
        }
        let bytes = read(span.start_byte(), len)?;
        if bytes.len() != len {
            continue;
        }
        let id = node.name.as_str();
        let Some((text, was)) = flip_order(schema, id) else {
            continue;
        };
        let (now, then) = (was.value(&bytes), was.other().value(&bytes));
        if now == then {
            continue;
        }
        let result = match reparse(&text) {
            Reparse::Succeeds => "succeeds",
            Reparse::FailsAt(at) if at > offset => "goes further",
            _ => continue,
        };
        return Some(Suggestion {
            id: "other_byte_order",
            text: format!(
                "{id} is {now} read {}; read {} it is {then}, and the parse {}",
                was.name(),
                was.other().name(),
                if result == "succeeds" {
                    "then succeeds"
                } else {
                    "then goes further"
                }
            ),
            schema: text,
            result,
        });
    }
    None
}

/// The schema with the byte order of the field `id` turned round, and the order it had. The field's own type
/// (`u4le`) is changed when it has one; otherwise the `endian:` of the schema.
fn flip_order(schema: &str, id: &str) -> Option<(String, Order)> {
    let lines: Vec<&str> = schema.split_inclusive('\n').collect();
    // the field's own type: `- id: len` and, before the next item, `type: u4le`
    let marker = format!("id: {id}");
    if let Some(at) = lines.iter().position(|l| {
        let t = l.trim_start();
        t.strip_prefix("- ")
            .is_some_and(|rest| rest.trim_end() == marker)
    }) {
        let indent = indent_of(lines.get(at)?).len();
        for (i, line) in lines.iter().enumerate().skip(at + 1) {
            if !line.trim().is_empty() && indent_of(line).len() <= indent {
                break;
            }
            if let Some((new, was)) = flip_type(line) {
                let mut out = String::new();
                for (j, l) in lines.iter().enumerate() {
                    out.push_str(if j == i { &new } else { l });
                }
                return Some((out, was));
            }
        }
    }
    // the schema's own
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("endian:") {
            let (value, was) = match rest.trim() {
                "le" => ("be", Order::Little),
                "be" => ("le", Order::Big),
                _ => continue,
            };
            let indent = indent_of(line);
            let newline = if line.ends_with('\n') { "\n" } else { "" };
            let mut out = String::new();
            for (j, l) in lines.iter().enumerate() {
                if j == i {
                    let _ = write!(out, "{indent}endian: {value}{newline}");
                } else {
                    out.push_str(l);
                }
            }
            return Some((out, was));
        }
    }
    None
}

/// The white space a line starts with.
fn indent_of(line: &str) -> &str {
    line.split_at_checked(line.len() - line.trim_start().len())
        .map_or("", |(indent, _)| indent)
}

/// `    type: u4le` as `    type: u4be`, and the order it had.
fn flip_type(line: &str) -> Option<(String, Order)> {
    let t = line.trim_start();
    let value = t.strip_prefix("type:")?.trim();
    let (base, was, other) = if let Some(base) = value.strip_suffix("le") {
        (base, Order::Little, "be")
    } else {
        (value.strip_suffix("be")?, Order::Big, "le")
    };
    let mut chars = base.chars();
    let kind = chars.next()?;
    if !matches!(kind, 'u' | 's' | 'f') || !chars.as_str().chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let indent = indent_of(line);
    let newline = if line.ends_with('\n') { "\n" } else { "" };
    Some((format!("{indent}type: {base}{other}{newline}"), was))
}
