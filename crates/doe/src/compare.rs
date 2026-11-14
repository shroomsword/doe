//! Two parses of the same schema, compared: which fields differ, were added or were removed, and which bytes differ.
//!
//! The two trees are walked together **by path**: a field is matched with the field of the same name in the matched
//! parent, so a field that moved because an earlier one grew is the same field, and a value that did not change is
//! not a difference, wherever it is. The elements of a list are matched by index (`[i]` with `[i]`): the elements that
//! only the longer list has are differences, and so is the length of the list.
//!
//! The result is for both sides, so that a viewer can draw the tree of one file with marks, or the two trees side by
//! side: each side has the marks of its own nodes; what only the other file has is a [`Ghost`] under the parent that
//! would hold it. The bytes that differ are marked **by matched field** (a field with spans of the same length: the
//! bytes that differ; of different lengths: the whole span), never by offset.

use std::collections::HashMap;

use crate::parser::trace::TraceNode;
use crate::value::Value;

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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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

/// Compare two parses of the same schema.
#[must_use]
pub fn compare(a: &Input<'_>, b: &Input<'_>) -> Comparison {
    let mut walker = Walker {
        a,
        b,
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
        let self_changed = a.type_name != b.type_name
            || a_list != b_list
            || (a_list && a.children.len() != b.children.len())
            || a.failed != b.failed;
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
            let ib = match &by_name {
                Some(names) => names.get(ca.name.as_str()).copied(),
                None => (ia < b.children.len()).then_some(ia),
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
            below += self.walk(
                ca,
                child_value(va, &ca.name, ia),
                cb,
                child_value(vb, &cb.name, ib),
            );
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

    fn mark_changed(&mut self) {
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
