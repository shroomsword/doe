//! The text a viewer puts on the clipboard for a field: its path, bytes and place.
//!
//! The path of a field is written the way a person would write it in an
//! expression: `entries[2].len`, with `$` for the root alone. The names are the
//! ids of the schema (lower case, digits and `_`), so nothing needs quoting; a
//! name that is not an id is written `["name"]`. [`path_of`] and [`resolve`] are
//! inverses, so a path copied from a viewer finds the same field again.

use std::fmt::Write;

use crate::parser::trace::TraceNode;

/// Whether `name` can be written after a dot: an id of the schema.
fn is_id(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Whether `name` is the name of an element of a repeated field, `[3]`.
fn element_index(name: &str) -> Option<usize> {
    name.strip_prefix('[')?.strip_suffix(']')?.parse().ok()
}

/// The path of the node at `path` (indexes of children from `root`): `$` for the
/// root, `entries[2].len` for a field.
#[must_use]
pub fn path_of(root: &TraceNode, path: &[usize]) -> String {
    let mut out = String::new();
    let mut node = root;
    for &index in path {
        let Some(child) = node.children.get(index) else {
            break;
        };
        if element_index(&child.name).is_some() {
            out.push_str(&child.name);
        } else if is_id(&child.name) {
            if !out.is_empty() {
                out.push('.');
            }
            out.push_str(&child.name);
        } else {
            out.push_str("[\"");
            for c in child.name.chars() {
                if c == '"' || c == '\\' {
                    out.push('\\');
                }
                out.push(c);
            }
            out.push_str("\"]");
        }
        node = child;
    }
    if out.is_empty() { "$".to_owned() } else { out }
}

/// One step of a path.
enum Step {
    Name(String),
    Index(usize),
}

/// The steps of `text`, or `None` when it is not a path.
fn steps(text: &str) -> Option<Vec<Step>> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if text == "$" {
        return Some(Vec::new());
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut first = true;
    while i < chars.len() {
        match chars.get(i)? {
            '.' if !first => {
                i += 1;
                let start = i;
                while chars
                    .get(i)
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
                {
                    i += 1;
                }
                if start == i {
                    return None;
                }
                out.push(Step::Name(chars.get(start..i)?.iter().collect()));
            }
            '[' => {
                i += 1;
                if chars.get(i) == Some(&'"') {
                    i += 1;
                    let mut name = String::new();
                    loop {
                        match chars.get(i)? {
                            '\\' => {
                                i += 1;
                                name.push(*chars.get(i)?);
                            }
                            '"' => break,
                            c => name.push(*c),
                        }
                        i += 1;
                    }
                    i += 1;
                    out.push(Step::Name(name));
                } else {
                    let start = i;
                    while chars.get(i).is_some_and(char::is_ascii_digit) {
                        i += 1;
                    }
                    if start == i {
                        return None;
                    }
                    let digits: String = chars.get(start..i)?.iter().collect();
                    out.push(Step::Index(digits.parse().ok()?));
                }
                if chars.get(i) != Some(&']') {
                    return None;
                }
                i += 1;
            }
            c if first && (c.is_ascii_alphabetic() || *c == '_') => {
                let start = i;
                while chars
                    .get(i)
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
                {
                    i += 1;
                }
                out.push(Step::Name(chars.get(start..i)?.iter().collect()));
            }
            _ => return None,
        }
        first = false;
    }
    Some(out)
}

/// The indexes of the node that `text` names (the inverse of [`path_of`]); `None`
/// when `text` is not a path or no node has it.
#[must_use]
pub fn resolve(root: &TraceNode, text: &str) -> Option<Vec<usize>> {
    let mut node = root;
    let mut out = Vec::new();
    for step in steps(text)? {
        let (index, child) = match step {
            Step::Name(name) => node
                .children
                .iter()
                .enumerate()
                .find(|(_, c)| c.name == name)?,
            Step::Index(n) => {
                let name = format!("[{n}]");
                node.children
                    .iter()
                    .enumerate()
                    .find(|(_, c)| c.name == name)?
            }
        };
        out.push(index);
        node = child;
    }
    Some(out)
}

/// Bytes as hex with a space between them: `01 02 ff`.
#[must_use]
pub fn hex_spaced(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Bytes as hex with nothing between them: `0102ff`.
#[must_use]
pub fn hex_continuous(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

/// The name of a C variable made from a path: `entries[2].len` → `entries_2_len`,
/// `$` → `data`; a name that would start with a digit gets a `_` before it.
#[must_use]
pub fn c_name(path: &str) -> String {
    let mut name = String::new();
    for c in path.chars() {
        if c.is_ascii_alphanumeric() {
            name.push(c.to_ascii_lowercase());
        } else if !name.ends_with('_') && !name.is_empty() {
            name.push('_');
        }
    }
    let name = name.trim_end_matches('_').to_owned();
    if name.is_empty() || path == "$" {
        "data".to_owned()
    } else if name.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{name}")
    } else {
        name
    }
}

/// The bytes as a C array named after the path:
/// `unsigned char entries_2_len[] = { 0x01, 0x02 };`.
#[must_use]
pub fn c_array(path: &str, bytes: &[u8]) -> String {
    let body = bytes
        .iter()
        .map(|b| format!("0x{b:02x}"))
        .collect::<Vec<_>>()
        .join(", ");
    if body.is_empty() {
        format!("unsigned char {}[] = {{ }};", c_name(path))
    } else {
        format!("unsigned char {}[] = {{ {body} }};", c_name(path))
    }
}

/// Where the node is, as text: `0x1f4+2` (offset and length in bytes), `0x1f4 bits 3..7`
/// for a bit field, and a note when the offsets are in the bytes a `process:`
/// produced. `None` for a node that is not in the input.
#[must_use]
pub fn place(node: &TraceNode) -> Option<String> {
    let span = node.span?;
    let mut text = if span.start.is_multiple_of(8) && span.end.is_multiple_of(8) {
        format!("{:#x}+{}", span.start / 8, (span.end - span.start) / 8)
    } else {
        let first = span.start % 8;
        let last = first + (span.end - span.start);
        format!("{:#x} bits {first}..{last}", span.start / 8)
    };
    if node.processed {
        text.push_str(" in the decoded bytes");
    }
    Some(text)
}

/// The field as the schema says it, from the type of its value: `len: u2`. `parent`
/// is the type it is in, `entry` in `len: u2 in entry`.
#[must_use]
pub fn schema_field(name: &str, kind: &str, parent: Option<&str>) -> String {
    match parent {
        Some(parent) => format!("{name}: {kind} in {parent}"),
        None => format!("{name}: {kind}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str, children: Vec<TraceNode>) -> TraceNode {
        let mut n = TraceNode::named(name);
        n.children = children;
        n
    }

    fn sample() -> TraceNode {
        node(
            "file",
            vec![
                node("magic", vec![]),
                node(
                    "entries",
                    vec![
                        node("[0]", vec![node("len", vec![])]),
                        node("[1]", vec![node("len", vec![]), node("odd name", vec![])]),
                        node("[2]", vec![node("len", vec![])]),
                    ],
                ),
            ],
        )
    }

    #[test]
    fn paths_are_written_as_an_expression() {
        let root = sample();
        assert_eq!(path_of(&root, &[]), "$");
        assert_eq!(path_of(&root, &[0]), "magic");
        assert_eq!(path_of(&root, &[1]), "entries");
        assert_eq!(path_of(&root, &[1, 2, 0]), "entries[2].len");
        assert_eq!(path_of(&root, &[1, 1, 1]), "entries[1][\"odd name\"]");
    }

    #[test]
    fn every_path_finds_its_node_again() {
        let root = sample();
        let mut stack = vec![Vec::new()];
        while let Some(path) = stack.pop() {
            let text = path_of(&root, &path);
            assert_eq!(resolve(&root, &text), Some(path.clone()), "{text}");
            let mut node = &root;
            for &i in &path {
                node = &node.children[i];
            }
            for i in 0..node.children.len() {
                let mut next = path.clone();
                next.push(i);
                stack.push(next);
            }
        }
    }

    #[test]
    fn text_that_is_not_a_path_names_nothing() {
        let root = sample();
        for bad in [
            "",
            ".magic",
            "magic.",
            "entries[",
            "entries[x]",
            "entries[9]",
            "nothing",
            "entries.len",
            "a b",
            "entries[1]odd",
        ] {
            assert_eq!(resolve(&root, bad), None, "{bad:?}");
        }
        assert_eq!(resolve(&root, " entries[0].len "), Some(vec![1, 0, 0]));
    }

    #[test]
    fn bytes_are_written_as_hex_and_as_a_c_array() {
        assert_eq!(hex_spaced(&[1, 2, 255]), "01 02 ff");
        assert_eq!(hex_continuous(&[1, 2, 255]), "0102ff");
        assert_eq!(hex_spaced(&[]), "");
        assert_eq!(
            c_array("entries[2].len", &[1, 2]),
            "unsigned char entries_2_len[] = { 0x01, 0x02 };"
        );
        assert_eq!(c_array("$", &[]), "unsigned char data[] = { };");
        assert_eq!(c_name("[0].x"), "_0_x");
        assert_eq!(c_name("entries[1][\"odd name\"]"), "entries_1_odd_name");
    }

    #[test]
    fn a_place_is_an_offset_and_a_length() {
        let mut n = TraceNode::named("len");
        assert_eq!(place(&n), None);
        n.span = Some(crate::parser::trace::Span {
            start: 0x1f4 * 8,
            end: 0x1f6 * 8,
        });
        assert_eq!(place(&n).as_deref(), Some("0x1f4+2"));
        n.span = Some(crate::parser::trace::Span {
            start: 0x1f4 * 8 + 3,
            end: 0x1f4 * 8 + 7,
        });
        assert_eq!(place(&n).as_deref(), Some("0x1f4 bits 3..7"));
        n.processed = true;
        assert_eq!(
            place(&n).as_deref(),
            Some("0x1f4 bits 3..7 in the decoded bytes")
        );
        assert_eq!(schema_field("len", "u2", Some("entry")), "len: u2 in entry");
    }
}
