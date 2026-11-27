//! Two parses of the same schema, compared: which fields differ, were added or were removed, and which bytes differ.
//!
//! The two trees are walked together **by path**: a field is matched with the field of the same name in the matched
//! parent, so a field that moved because an earlier one grew is the same field, and a value that did not change is
//! not a difference, wherever it is. The elements of a list are matched by index (`[i]` with `[i]`): the elements that
//! only the longer list has are differences, and so is the length of the list.
//!
//! A list can instead be matched **by a key**: a field of its elements (`compare-by:` in the schema, or a [`Keys`]
//! that a viewer chose). The elements that have the same key are the same element, wherever they are; an element
//! that only one file has is a difference, an element that moved is not, and the length of such a list is not a
//! difference of its own. A key that cannot be used (not unique, missing, not a number, a string or bytes) falls back
//! to the index, and [`Comparison::notes`] says why.
//!
//! The result is for both sides, so that a viewer can draw the tree of one file with marks, or the two trees side by
//! side: each side has the marks of its own nodes; what only the other file has is a [`Ghost`] under the parent that
//! would hold it. The bytes that differ are marked **by matched field** (a field with spans of the same length: the
//! bytes that differ; of different lengths: the whole span), never by offset.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;

use crate::parser::trace::TraceNode;
use crate::value::Value;

/// How a viewer draws a comparison of two files.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum View {
    /// The tree of the first file, marked with what differs, and what only the second has under its parents.
    #[default]
    Annotated,
    /// The two trees side by side.
    SideBySide,
}

/// The text is not the name of a way to draw a comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownView(pub String);

impl std::fmt::Display for UnknownView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unknown way to show a comparison `{}`; use annotated or side-by-side",
            self.0
        )
    }
}

impl std::error::Error for UnknownView {}

impl std::str::FromStr for View {
    type Err = UnknownView;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.to_ascii_lowercase().as_str() {
            "annotated" | "marks" => Ok(View::Annotated),
            "side-by-side" | "side_by_side" | "split" => Ok(View::SideBySide),
            _ => Err(UnknownView(text.to_owned())),
        }
    }
}

/// The most elements of a list that are matched by a key; a longer list is matched by index (a map of the keys of
/// millions of elements is what a page cannot always hold).
pub const MAX_KEYED: usize = 5_000_000;

/// The field that a viewer chose to match the elements of a list by, for each list: the name of the list as its names
/// from the root (`header.entries`; the indexes of the elements in between are left out) and the name of the field.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Keys {
    by_list: BTreeMap<String, String>,
}

impl Keys {
    /// No list has a key chosen.
    #[must_use]
    pub fn new() -> Keys {
        Keys::default()
    }

    /// Match the elements of `list` by `field`.
    pub fn set(&mut self, list: &str, field: &str) {
        self.by_list.insert(list.to_owned(), field.to_owned());
    }

    /// Forget the choice for `list`: the key of the schema is used again, if it has one.
    pub fn clear(&mut self, list: &str) {
        self.by_list.remove(list);
    }

    /// Match the elements of `list` by index, whatever key the schema gives it.
    pub fn use_index(&mut self, list: &str) {
        self.by_list.insert(list.to_owned(), String::new());
    }

    /// The field chosen for `list`: empty when the index was chosen (`use_index`).
    #[must_use]
    pub fn get(&self, list: &str) -> Option<&str> {
        self.by_list.get(list).map(String::as_str)
    }

    /// Whether no list has a key chosen.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_list.is_empty()
    }
}

/// What tells an element from the others: a number, a string or bytes (a number of one kind is not a string).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum KeyValue {
    Unsigned(u64),
    Signed(i64),
    Text(String),
    Bytes(Vec<u8>),
}

impl KeyValue {
    fn of(value: &Value) -> Option<KeyValue> {
        match strip(value) {
            Value::UInt(n) => Some(KeyValue::Unsigned(*n)),
            Value::SInt(n) => Some(KeyValue::Signed(*n)),
            Value::Str(text) => Some(KeyValue::Text(text.clone())),
            Value::Bytes(bytes) => Some(KeyValue::Bytes(bytes.clone())),
            _ => None,
        }
    }

    /// How a key is written in a note.
    fn describe(&self) -> String {
        match self {
            KeyValue::Unsigned(n) => n.to_string(),
            KeyValue::Signed(n) => n.to_string(),
            KeyValue::Text(text) => format!("{text:?}"),
            KeyValue::Bytes(bytes) => bytes
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" "),
        }
    }
}

/// The key of every element of a list, in order, or why the list cannot be matched by `field`.
fn keys_of(items: &[Value], field: &str) -> Result<Vec<KeyValue>, String> {
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let Value::Struct { fields, .. } = strip(item) else {
            return Err(format!("element {index} is not a struct with fields"));
        };
        let Some((_, value)) = fields.iter().find(|(name, _)| name == field) else {
            return Err(format!("element {index} has no field {field}"));
        };
        let Some(key) = KeyValue::of(value) else {
            return Err(format!(
                "{field} of element {index} is not a number, a string or bytes"
            ));
        };
        out.push(key);
    }
    Ok(out)
}

/// The key that repeats in `keys`, if one does.
fn repeated(keys: &[KeyValue]) -> Option<&KeyValue> {
    let mut seen: HashSet<&KeyValue> = HashSet::with_capacity(keys.len());
    keys.iter().find(|key| !seen.insert(key))
}

/// The fields of the elements of a list that can match them: numbers, strings or bytes that every element has and
/// that are all different, in the order of the fields. Empty for something that is not a list of structs.
#[must_use]
pub fn key_candidates(value: Option<&Value>) -> Vec<String> {
    let Some(Value::Array(items)) = value.map(strip) else {
        return Vec::new();
    };
    if items.len() > MAX_KEYED {
        return Vec::new();
    }
    let Some(Value::Struct { fields, .. }) = items.first().map(strip) else {
        return Vec::new();
    };
    fields
        .iter()
        .map(|(name, _)| name.as_str())
        .filter(|name| keys_of(items, name).is_ok_and(|keys| repeated(&keys).is_none()))
        .map(str::to_owned)
        .collect()
}

/// How many bytes of a long value are read at a time.
const CHUNK: usize = 64 * 1024;

/// What is wrong with a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// The value differs (a leaf), or the type or the length of a list does (a container).
    Changed,
    /// The value of the container is equal, but something below it differs ([`Side::below`] says how many).
    Inside,
    /// The field or element is in this file and not in the other.
    Only,
}

/// The marks of one of the two files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Side {
    /// The nodes that are marked, by path (indexes of children from the root). A node that is not here is the same.
    pub marks: HashMap<Vec<usize>, Mark>,
    /// For a container that has differences below it (and for one that is changed itself), how many.
    pub below: HashMap<Vec<usize>, usize>,
    /// The bytes that differ, as ranges `[first, end)` of byte offsets in this file, sorted and not overlapping.
    pub bytes: Vec<(u64, u64)>,
    /// The lists whose elements were matched by a key, by path, and the field that was the key.
    pub keyed: HashMap<Vec<usize>, String>,
}

/// A field or element that only the other file (B) has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ghost {
    /// Its name in B's tree (`[3]` for an element).
    pub name: String,
    /// Its path in B's tree.
    pub b_path: Vec<usize>,
}

/// How many fields were compared and how they differ.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    /// The fields of A.
    pub fields_a: usize,
    /// The fields of B.
    pub fields_b: usize,
    /// The fields (leaves, and containers whose type or length changed) whose value differs.
    pub changed: usize,
    /// The fields and elements that only A has (a whole subtree counts once).
    pub only_a: usize,
    /// The fields and elements that only B has (a whole subtree counts once).
    pub only_b: usize,
}

impl Summary {
    /// How many differences there are.
    #[must_use]
    pub fn differences(&self) -> usize {
        self.changed + self.only_a + self.only_b
    }

    /// Whether nothing differs.
    #[must_use]
    pub fn identical(&self) -> bool {
        self.differences() == 0
    }
}

/// The comparison of two parses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Comparison {
    /// The marks of A's nodes, and the bytes of A that differ.
    pub a: Side,
    /// The marks of B's nodes, and the bytes of B that differ.
    pub b: Side,
    /// What only B has, by the path in A's tree of the parent that would hold it.
    pub ghosts: HashMap<Vec<usize>, Vec<Ghost>>,
    /// The counts.
    pub summary: Summary,
    /// What a viewer should say: a list that a key was asked for and could not match, and why (it was matched by index).
    pub notes: Vec<String>,
    /// For each node of A that is marked [`Mark::Changed`], the path of the node of B that it was matched with (they
    /// differ when a list is matched by a key).
    pub partners: HashMap<Vec<usize>, Vec<usize>>,
}

/// A parse, and a way to read bytes of the file it was made from.
pub struct Input<'a> {
    /// What was read.
    pub trace: &'a TraceNode,
    /// The value, if it was kept.
    pub value: Option<&'a Value>,
    /// `len` bytes of the file from byte `offset`, if it has them (for the bytes of a long value, and for the bytes
    /// that differ).
    pub read: &'a dyn Fn(u64, usize) -> Option<Vec<u8>>,
}

impl std::fmt::Debug for Input<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Input")
            .field("trace", &self.trace.name)
            .finish_non_exhaustive()
    }
}

/// Compare two parses of the same schema. The lists that the schema gives a key (`compare-by:`) are matched by it, the
/// others by index.
#[must_use]
pub fn compare(a: &Input<'_>, b: &Input<'_>) -> Comparison {
    compare_with(a, b, &Keys::default())
}

/// Compare two parses of the same schema, matching the lists that `keys` has a field for by it (before the key of the
/// schema), and the others by index.
#[must_use]
pub fn compare_with(a: &Input<'_>, b: &Input<'_>, keys: &Keys) -> Comparison {
    let mut walker = Walker {
        a,
        b,
        keys,
        names: Vec::new(),
        out: Comparison::default(),
        pa: Vec::new(),
        pb: Vec::new(),
    };
    walker.walk(a.trace, a.value, b.trace, b.value);
    let mut out = walker.out;
    out.a.bytes = merge(std::mem::take(&mut out.a.bytes));
    out.b.bytes = merge(std::mem::take(&mut out.b.bytes));
    out
}

/// Sort ranges and join the ones that touch or overlap.
fn merge(mut ranges: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    ranges.sort_unstable();
    let mut out: Vec<(u64, u64)> = Vec::with_capacity(ranges.len());
    for (first, end) in ranges {
        match out.last_mut() {
            Some(last) if first <= last.1 => last.1 = last.1.max(end),
            _ => out.push((first, end)),
        }
    }
    out
}

fn strip(value: &Value) -> &Value {
    match value {
        Value::Instance { value, .. } => strip(value),
        other => other,
    }
}

/// The value of the child `name` (the element at `index` for `[i]`) of `value`.
fn child_value<'v>(value: Option<&'v Value>, name: &str, index: usize) -> Option<&'v Value> {
    match strip(value?) {
        Value::Struct { fields, .. } => fields.iter().find(|(n, _)| n == name).map(|(_, v)| v),
        Value::Array(items) => items.get(index),
        _ => None,
    }
}

fn is_list(node: &TraceNode, value: Option<&Value>) -> bool {
    matches!(value.map(strip), Some(Value::Array(_)))
        || node
            .children
            .first()
            .is_some_and(|c| c.name.starts_with('['))
}

/// How many nodes there are below `node`.
fn count_below(node: &TraceNode) -> usize {
    node.children.iter().map(|c| 1 + count_below(c)).sum()
}

struct Walker<'a, 'b> {
    a: &'b Input<'a>,
    b: &'b Input<'a>,
    keys: &'b Keys,
    /// The names from the root to the node that is being compared, without the indexes of elements.
    names: Vec<String>,
    out: Comparison,
    pa: Vec<usize>,
    pb: Vec<usize>,
}

impl Walker<'_, '_> {
    /// Compare the matched nodes `a` and `b`, at `self.pa` and `self.pb`; how many differences there are at and
    /// below them.
    fn walk(
        &mut self,
        a: &TraceNode,
        va: Option<&Value>,
        b: &TraceNode,
        vb: Option<&Value>,
    ) -> usize {
        self.out.summary.fields_a += 1;
        self.out.summary.fields_b += 1;
        match (a.children.is_empty(), b.children.is_empty()) {
            (true, true) => {
                if self.leaf_equal(a, va, b, vb) {
                    0
                } else {
                    self.mark_changed();
                    self.mark_bytes(a, b);
                    1
                }
            }
            // a field with parts in one file and none in the other (the case of a `switch-on` that chose another type)
            (false, false) => self.walk_container(a, va, b, vb),
            _ => {
                self.out.summary.fields_a += count_below(a);
                self.out.summary.fields_b += count_below(b);
                self.mark_changed();
                1
            }
        }
    }

    fn walk_container(
        &mut self,
        a: &TraceNode,
        va: Option<&Value>,
        b: &TraceNode,
        vb: Option<&Value>,
    ) -> usize {
        let (a_list, b_list) = (is_list(a, va), is_list(b, vb));
        // the elements that a key pairs: for each element of A, the index of its partner in B
        let paired = if a_list && b_list {
            self.pair_by_key(a, va, b, vb)
        } else {
            None
        };
        let self_changed = a.type_name != b.type_name
            || a_list != b_list
            || (a_list && paired.is_none() && a.children.len() != b.children.len())
            || a.failed != b.failed;
        if let Some((_, field)) = &paired {
            self.out.a.keyed.insert(self.pa.clone(), field.clone());
            self.out.b.keyed.insert(self.pb.clone(), field.clone());
        }
        // lists are matched by index, the rest by name
        let by_name: Option<HashMap<&str, usize>> = if a_list && b_list {
            None
        } else {
            Some(
                b.children
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (c.name.as_str(), i))
                    .collect(),
            )
        };
        let mut matched = vec![false; b.children.len()];
        let mut below = 0;
        for (ia, ca) in a.children.iter().enumerate() {
            let ib = match (&paired, &by_name) {
                (Some((partners, _)), _) => partners.get(ia).copied().flatten(),
                (None, Some(names)) => names.get(ca.name.as_str()).copied(),
                (None, None) => (ia < b.children.len()).then_some(ia),
            };
            let Some((ib, cb)) = ib.and_then(|i| b.children.get(i).map(|c| (i, c))) else {
                self.pa.push(ia);
                self.out.a.marks.insert(self.pa.clone(), Mark::Only);
                self.pa.pop();
                self.out.summary.only_a += 1;
                self.out.summary.fields_a += 1 + count_below(ca);
                below += 1;
                continue;
            };
            if let Some(slot) = matched.get_mut(ib) {
                *slot = true;
            }
            self.pa.push(ia);
            self.pb.push(ib);
            let named = !ca.name.starts_with('[');
            if named {
                self.names.push(ca.name.clone());
            }
            below += self.walk(
                ca,
                child_value(va, &ca.name, ia),
                cb,
                child_value(vb, &cb.name, ib),
            );
            if named {
                self.names.pop();
            }
            self.pa.pop();
            self.pb.pop();
        }
        for (ib, cb) in b.children.iter().enumerate() {
            if matched.get(ib).copied().unwrap_or(true) {
                continue;
            }
            self.pb.push(ib);
            self.out.b.marks.insert(self.pb.clone(), Mark::Only);
            self.out
                .ghosts
                .entry(self.pa.clone())
                .or_default()
                .push(Ghost {
                    name: cb.name.clone(),
                    b_path: self.pb.clone(),
                });
            self.pb.pop();
            self.out.summary.only_b += 1;
            self.out.summary.fields_b += 1 + count_below(cb);
            below += 1;
        }
        if self_changed {
            self.mark_changed();
        } else if below > 0 {
            self.out.a.marks.insert(self.pa.clone(), Mark::Inside);
            self.out.b.marks.insert(self.pb.clone(), Mark::Inside);
        }
        if below > 0 {
            self.out.a.below.insert(self.pa.clone(), below);
            self.out.b.below.insert(self.pb.clone(), below);
        }
        below + usize::from(self_changed)
    }

    /// The name of the list that is being compared, from the root: `header.entries`.
    fn list_name(&self) -> String {
        self.names.join(".")
    }

    /// The field to match the elements of this list by: the one a viewer chose, or the schema's.
    fn key_field(&self, a: &TraceNode, b: &TraceNode) -> Option<String> {
        match self.keys.get(&self.list_name()) {
            // the index was asked for
            Some("") => None,
            Some(field) => Some(field.to_owned()),
            None => a.compare_by.clone().or_else(|| b.compare_by.clone()),
        }
    }

    /// Pair the elements of the two lists by the key, if there is one and it can be used: for each element of A, the
    /// index of the element of B that has the same key, and the field. Why a key could not be used is a note, and the
    /// elements are matched by index.
    fn pair_by_key(
        &mut self,
        a: &TraceNode,
        va: Option<&Value>,
        b: &TraceNode,
        vb: Option<&Value>,
    ) -> Option<(Vec<Option<usize>>, String)> {
        let field = self.key_field(a, b)?;
        let list = self.list_name();
        let mut fall_back = |why: String| {
            self.out
                .notes
                .push(format!("{list}: matched by index, not by {field}: {why}"));
            None
        };
        if a.children.len() > MAX_KEYED || b.children.len() > MAX_KEYED {
            return fall_back(format!("the list has more than {MAX_KEYED} elements"));
        }
        let (Some(Value::Array(items_a)), Some(Value::Array(items_b))) =
            (va.map(strip), vb.map(strip))
        else {
            return fall_back("the values of the elements were not kept".to_owned());
        };
        if items_a.len() != a.children.len() || items_b.len() != b.children.len() {
            return fall_back("the elements are not all in the parse".to_owned());
        }
        let keys_a = match keys_of(items_a, &field) {
            Ok(keys) => keys,
            Err(why) => return fall_back(format!("in A, {why}")),
        };
        let keys_b = match keys_of(items_b, &field) {
            Ok(keys) => keys,
            Err(why) => return fall_back(format!("in B, {why}")),
        };
        if let Some(key) = repeated(&keys_a) {
            return fall_back(format!(
                "in A, {} is the {field} of more than one element",
                key.describe()
            ));
        }
        if let Some(key) = repeated(&keys_b) {
            return fall_back(format!(
                "in B, {} is the {field} of more than one element",
                key.describe()
            ));
        }
        let positions: HashMap<&KeyValue, usize> =
            keys_b.iter().enumerate().map(|(i, k)| (k, i)).collect();
        let partners = keys_a
            .iter()
            .map(|key| positions.get(key).copied())
            .collect();
        Some((partners, field))
    }

    fn mark_changed(&mut self) {
        self.out.partners.insert(self.pa.clone(), self.pb.clone());
        self.out.a.marks.insert(self.pa.clone(), Mark::Changed);
        self.out.b.marks.insert(self.pb.clone(), Mark::Changed);
        self.out.summary.changed += 1;
    }

    /// Whether two leaves are equal: their values, or, if a value was not kept, the bytes they were read from.
    fn leaf_equal(
        &self,
        a: &TraceNode,
        va: Option<&Value>,
        b: &TraceNode,
        vb: Option<&Value>,
    ) -> bool {
        if a.failed != b.failed {
            return false;
        }
        match (va, vb) {
            (Some(x), Some(y)) => self.value_eq(strip(x), strip(y)),
            _ => match (byte_span(a), byte_span(b)) {
                (Some(sa), Some(sb)) => {
                    sa.1 - sa.0 == sb.1 - sb.0 && self.same_range(sa.0, sb.0, sa.1 - sa.0)
                }
                // nothing says what to compare
                _ => true,
            },
        }
    }

    fn value_eq(&self, x: &Value, y: &Value) -> bool {
        match (x, y) {
            (Value::UInt(p), Value::UInt(q)) => p == q,
            (Value::SInt(p), Value::SInt(q)) => p == q,
            (Value::Float(p), Value::Float(q)) => p.to_bits() == q.to_bits(),
            (Value::Bool(p), Value::Bool(q)) => p == q,
            (Value::Bytes(p), Value::Bytes(q)) => p == q,
            (Value::Str(p), Value::Str(q)) => p == q,
            (
                Value::Blob {
                    offset: oa,
                    len: la,
                    ..
                },
                Value::Blob {
                    offset: ob,
                    len: lb,
                    ..
                },
            ) => la == lb && self.same_range(*oa, *ob, *la),
            (Value::Enum { value: p, name: pn }, Value::Enum { value: q, name: qn }) => {
                p == q && pn == qn
            }
            (
                Value::Struct {
                    type_name: pt,
                    fields: pf,
                },
                Value::Struct {
                    type_name: qt,
                    fields: qf,
                },
            ) => {
                pt == qt
                    && pf.len() == qf.len()
                    && pf
                        .iter()
                        .zip(qf)
                        .all(|((pn, pv), (qn, qv))| pn == qn && self.value_eq(strip(pv), strip(qv)))
            }
            (Value::Array(p), Value::Array(q)) => {
                p.len() == q.len()
                    && p.iter()
                        .zip(q)
                        .all(|(pv, qv)| self.value_eq(strip(pv), strip(qv)))
            }
            (Value::Absent, Value::Absent) => true,
            _ => false,
        }
    }

    /// Whether `len` bytes of A from `from_a` are the bytes of B from `from_b`, read in pieces. Bytes that cannot be
    /// read are not equal.
    fn same_range(&self, from_a: u64, from_b: u64, len: u64) -> bool {
        let mut done: u64 = 0;
        while done < len {
            let piece = usize::try_from((len - done).min(CHUNK as u64)).unwrap_or(CHUNK);
            let (Some(x), Some(y)) = (
                (self.a.read)(from_a + done, piece),
                (self.b.read)(from_b + done, piece),
            ) else {
                return false;
            };
            if x != y {
                return false;
            }
            done += piece as u64;
        }
        true
    }

    /// Mark the bytes of two leaves that differ: where the spans have the same length, the bytes that differ; where
    /// they do not, the whole of each span.
    fn mark_bytes(&mut self, a: &TraceNode, b: &TraceNode) {
        let (Some(sa), Some(sb)) = (byte_span(a), byte_span(b)) else {
            // a leaf with bytes in one file only: those bytes
            if let Some((first, end)) = byte_span(a) {
                self.out.a.bytes.push((first, end));
            }
            if let Some((first, end)) = byte_span(b) {
                self.out.b.bytes.push((first, end));
            }
            return;
        };
        if sa.1 - sa.0 != sb.1 - sb.0 {
            self.out.a.bytes.push(sa);
            self.out.b.bytes.push(sb);
            return;
        }
        let len = sa.1 - sa.0;
        let mut done: u64 = 0;
        while done < len {
            let piece = usize::try_from((len - done).min(CHUNK as u64)).unwrap_or(CHUNK);
            let (Some(x), Some(y)) = (
                (self.a.read)(sa.0 + done, piece),
                (self.b.read)(sb.0 + done, piece),
            ) else {
                // what cannot be read cannot be told apart: the rest of the span
                self.out.a.bytes.push((sa.0 + done, sa.1));
                self.out.b.bytes.push((sb.0 + done, sb.1));
                return;
            };
            let mut run: Option<u64> = None;
            for (i, (p, q)) in x.iter().zip(&y).enumerate() {
                let at = done + i as u64;
                match (p != q, run) {
                    (true, None) => run = Some(at),
                    (false, Some(first)) => {
                        self.out.a.bytes.push((sa.0 + first, sa.0 + at));
                        self.out.b.bytes.push((sb.0 + first, sb.0 + at));
                        run = None;
                    }
                    _ => {}
                }
            }
            if let Some(first) = run {
                let end = done + piece as u64;
                self.out.a.bytes.push((sa.0 + first, sa.0 + end));
                self.out.b.bytes.push((sb.0 + first, sb.0 + end));
            }
            done += piece as u64;
        }
    }
}

/// The bytes of a node in the file, as `[first, end)`; none for a node that has no bytes of the file (computed, or
/// decoded by a `process:`).
fn byte_span(node: &TraceNode) -> Option<(u64, u64)> {
    if node.processed {
        return None;
    }
    let span = node.span?;
    (span.end > span.start).then(|| (span.start_byte(), span.end_byte()))
}

/// A list of the first file, for a viewer that offers a field to match its elements by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListInfo {
    /// Its path in the tree of the first file.
    pub path: Vec<usize>,
    /// Its name from the root, as [`Keys`] names it (`header.entries`).
    pub name: String,
    /// How many elements it has.
    pub len: usize,
    /// The fields that could match its elements ([`key_candidates`]); none for a list longer than
    /// [`MAX_CANDIDATES`] elements, which a viewer is asked a field for by name.
    pub candidates: Vec<String>,
    /// The field the schema gives (`compare-by:`).
    pub schema_key: Option<String>,
}

/// The longest list that is looked through for the fields that could match its elements.
pub const MAX_CANDIDATES: usize = 200_000;

/// The lists of a parse that have elements which are structs, with the fields that could match them. A list inside the
/// elements of another is named once, from the first element.
#[must_use]
pub fn lists(input: &Input<'_>) -> Vec<ListInfo> {
    let mut out = Vec::new();
    collect_lists(
        input.trace,
        input.value,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut out,
    );
    out
}

fn collect_lists(
    node: &TraceNode,
    value: Option<&Value>,
    path: &mut Vec<usize>,
    names: &mut Vec<String>,
    out: &mut Vec<ListInfo>,
) {
    let list = is_list(node, value);
    if list && !node.children.is_empty() {
        let candidates = if node.children.len() <= MAX_CANDIDATES {
            key_candidates(value)
        } else {
            Vec::new()
        };
        out.push(ListInfo {
            path: path.clone(),
            name: names.join("."),
            len: node.children.len(),
            candidates,
            schema_key: node.compare_by.clone(),
        });
    }
    // a list inside the elements of a list is found in the first element only
    let take = if list { 1 } else { node.children.len() };
    for (index, child) in node.children.iter().enumerate().take(take) {
        let is_field = !child.name.starts_with('[');
        path.push(index);
        if is_field {
            names.push(child.name.clone());
        }
        collect_lists(
            child,
            child_value(value, &child.name, index),
            path,
            names,
            out,
        );
        if is_field {
            names.pop();
        }
        path.pop();
    }
}

// ── the differences as a list ──────────────────────────────────────────────────

/// How many characters of a value a difference shows.
pub const VALUE_WIDTH: usize = 60;

/// How two files differ at a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    /// Both files have it, and it differs.
    Changed,
    /// Only the first file has it.
    OnlyA,
    /// Only the second file has it.
    OnlyB,
}

/// What differs in a field that both files have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    /// The value (for a field that is in one file only, the value it has).
    Value,
    /// The type of a field that is a type.
    Type,
    /// The length of a list that is matched by index.
    Length,
}

/// One difference between two files, by path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Difference {
    /// The path, as the viewers copy it (`entries[2].len`): in the first file's tree, and for what only the second file
    /// has, in the second's.
    pub path: String,
    /// How it differs.
    pub status: DiffStatus,
    /// What differs.
    pub kind: DiffKind,
    /// The value, the type or the length in the first file, as text cut at [`VALUE_WIDTH`] characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    /// The same in the second file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<String>,
    /// For a field or element that only one file has and that has parts: how many fields are below it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<usize>,
    /// The bytes of the first file that differ in this field, as `[first, end)` ranges (all of them for a field that only
    /// the first file has).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub a_bytes: Vec<(u64, u64)>,
    /// The same in the second file.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub b_bytes: Vec<(u64, u64)>,
}

/// A value as one line of text, cut: a number or a string as `doe` prints it, bytes as hex.
fn value_text(value: Option<&Value>) -> String {
    let text = match value.map(strip) {
        None => "?".to_owned(),
        Some(Value::Bytes(bytes)) => bytes
            .iter()
            .take(VALUE_WIDTH)
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" "),
        Some(Value::Struct { fields, .. }) => format!("{} fields", fields.len()),
        Some(other) => other.to_string(),
    };
    if text.chars().count() > VALUE_WIDTH {
        let cut: String = text.chars().take(VALUE_WIDTH).collect();
        format!("{cut}…")
    } else {
        text
    }
}

/// The node and the value at `path` (indexes of children from the root).
fn at<'t>(
    trace: &'t TraceNode,
    value: Option<&'t Value>,
    path: &[usize],
) -> Option<(&'t TraceNode, Option<&'t Value>)> {
    let mut node = trace;
    let mut value = value;
    for &index in path {
        let child = node.children.get(index)?;
        value = child_value(value, &child.name, index);
        node = child;
    }
    Some((node, value))
}

/// The parts of the sorted `ranges` that are inside `span`.
fn within(ranges: &[(u64, u64)], span: Option<(u64, u64)>) -> Vec<(u64, u64)> {
    let Some((first, end)) = span else {
        return Vec::new();
    };
    let from = ranges.partition_point(|r| r.1 <= first);
    ranges
        .iter()
        .skip(from)
        .take_while(|r| r.0 < end)
        .map(|r| (r.0.max(first), r.1.min(end)))
        .collect()
}

/// The differences of a comparison, one per field that changed, per field or element that only one file has (a whole
/// subtree is one), in the order of the first file's tree, with what only the second file has after the children of the
/// parent that would hold it. A container that only has differences below it is not listed.
#[must_use]
pub fn differences(a: &Input<'_>, b: &Input<'_>, comparison: &Comparison) -> Vec<Difference> {
    let mut out = Vec::new();
    let mut walk = Listing {
        a,
        b,
        comparison,
        path: Vec::new(),
        out: &mut out,
    };
    walk.node(a.trace, a.value);
    out
}

struct Listing<'x, 'a, 'o> {
    a: &'x Input<'a>,
    b: &'x Input<'a>,
    comparison: &'x Comparison,
    path: Vec<usize>,
    out: &'o mut Vec<Difference>,
}

impl Listing<'_, '_, '_> {
    fn node(&mut self, node: &TraceNode, value: Option<&Value>) {
        match self.comparison.a.marks.get(&self.path) {
            None => return,
            Some(Mark::Only) => {
                let leaf = node.children.is_empty();
                self.out.push(Difference {
                    path: crate::copy::path_of(self.a.trace, &self.path),
                    status: DiffStatus::OnlyA,
                    kind: DiffKind::Value,
                    a: leaf.then(|| value_text(value)),
                    b: None,
                    fields: (!leaf).then(|| count_below(node)),
                    a_bytes: byte_span(node).into_iter().collect(),
                    b_bytes: Vec::new(),
                });
                return;
            }
            Some(Mark::Changed) => self.changed(node, value),
            Some(Mark::Inside) => {}
        }
        for (index, child) in node.children.iter().enumerate() {
            self.path.push(index);
            self.node(child, child_value(value, &child.name, index));
            self.path.pop();
        }
        // what only B has, under this node
        for ghost in self.comparison.ghosts.get(&self.path).into_iter().flatten() {
            let Some((node_b, value_b)) = at(self.b.trace, self.b.value, &ghost.b_path) else {
                continue;
            };
            let leaf = node_b.children.is_empty();
            self.out.push(Difference {
                path: crate::copy::path_of(self.b.trace, &ghost.b_path),
                status: DiffStatus::OnlyB,
                kind: DiffKind::Value,
                a: None,
                b: leaf.then(|| value_text(value_b)),
                fields: (!leaf).then(|| count_below(node_b)),
                a_bytes: Vec::new(),
                b_bytes: byte_span(node_b).into_iter().collect(),
            });
        }
    }

    fn changed(&mut self, node: &TraceNode, value: Option<&Value>) {
        let Some(partner) = self.comparison.partners.get(&self.path) else {
            return;
        };
        let Some((node_b, value_b)) = at(self.b.trace, self.b.value, partner) else {
            return;
        };
        let (kind, a, b) = if node.children.is_empty() && node_b.children.is_empty() {
            (DiffKind::Value, value_text(value), value_text(value_b))
        } else if node.type_name != node_b.type_name {
            (
                DiffKind::Type,
                node.type_name.clone().unwrap_or_else(|| "none".to_owned()),
                node_b
                    .type_name
                    .clone()
                    .unwrap_or_else(|| "none".to_owned()),
            )
        } else if is_list(node, value) && is_list(node_b, value_b) {
            (
                DiffKind::Length,
                node.children.len().to_string(),
                node_b.children.len().to_string(),
            )
        } else {
            // a value in one file and parts in the other, or a parse that failed in one
            let describe = |n: &TraceNode, v: Option<&Value>| {
                if n.children.is_empty() {
                    value_text(v)
                } else {
                    format!("{} fields", count_below(n))
                }
            };
            (
                DiffKind::Value,
                describe(node, value),
                describe(node_b, value_b),
            )
        };
        self.out.push(Difference {
            path: crate::copy::path_of(self.a.trace, &self.path),
            status: DiffStatus::Changed,
            kind,
            a: Some(a),
            b: Some(b),
            fields: None,
            a_bytes: within(&self.comparison.a.bytes, byte_span(node)),
            b_bytes: within(&self.comparison.b.bytes, byte_span(node_b)),
        });
    }
}

/// Two files compared by their paths, and how they differ.
#[cfg(feature = "fs")]
#[derive(Debug)]
pub struct FileComparison {
    /// The type that both were parsed with.
    pub type_name: String,
    /// The comparison, and the notes in it.
    pub comparison: Comparison,
    /// The differences, in order.
    pub differences: Vec<Difference>,
}

/// Parse two files with one schema and compare them. The files are read as the schema asks for them (by range), so their
/// size does not decide how much memory is needed beyond the parses themselves.
///
/// # Errors
///
/// The schema cannot be found or compiled, a file cannot be read, or **a file does not parse with the schema** (it is not
/// compared as far as it was read: two files that fail in the same place would be called the same).
#[cfg(feature = "fs")]
pub fn compare_files(
    file_a: &std::path::Path,
    file_b: &std::path::Path,
    schema: &str,
    include_paths: &[std::path::PathBuf],
    keys: &Keys,
) -> Result<FileComparison, crate::error::DoeError> {
    use crate::error::DoeError;
    use crate::parser::cursor::Cursor;
    use crate::parser::engine::Engine;
    use crate::schema::compiler::Compiler;

    let mut compiler = Compiler::new(include_paths.to_vec());
    let (schema_arg, wanted) = crate::split_type_arg(schema);
    let id = if crate::is_schema_path(schema_arg) {
        let raw = compiler.read_schema(std::path::Path::new(schema_arg))?;
        let id = raw.id.clone();
        compiler.process_schema(&raw)?;
        id
    } else {
        compiler.load(schema_arg, None)?;
        schema_arg.to_owned()
    };
    let type_name = crate::type_in_schema(&compiler.registry, &id, wanted)?;
    let engine = Engine::new(&compiler.registry);
    let parse = |path: &std::path::Path| -> Result<_, DoeError> {
        let io = |source| DoeError::Io {
            path: path.to_path_buf(),
            source,
        };
        let file = std::fs::File::open(path).map_err(io)?;
        let mut cursor = Cursor::from_file(file, path)?;
        let outcome = engine.parse_type_partial(&type_name, &mut cursor)?;
        if let Some(error) = outcome.error {
            return Err(DoeError::Schema(format!(
                "{} does not parse with {type_name}: {error}",
                path.display()
            )));
        }
        Ok(outcome)
    };
    let outcome_a = parse(file_a)?;
    let outcome_b = parse(file_b)?;
    let read = |path: std::path::PathBuf| {
        move |offset: u64, len: usize| {
            use std::io::{Read, Seek, SeekFrom};
            let mut f = std::fs::File::open(&path).ok()?;
            f.seek(SeekFrom::Start(offset)).ok()?;
            let mut out = vec![0; len];
            f.read_exact(&mut out).ok()?;
            Some(out)
        }
    };
    let (read_a, read_b) = (read(file_a.to_path_buf()), read(file_b.to_path_buf()));
    let input_a = Input {
        trace: &outcome_a.trace,
        value: outcome_a.value.as_ref(),
        read: &read_a,
    };
    let input_b = Input {
        trace: &outcome_b.trace,
        value: outcome_b.value.as_ref(),
        read: &read_b,
    };
    let comparison = compare_with(&input_a, &input_b, keys);
    let differences = differences(&input_a, &input_b, &comparison);
    Ok(FileComparison {
        type_name,
        comparison,
        differences,
    })
}
