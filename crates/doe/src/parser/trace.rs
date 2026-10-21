//! A record of where every parsed value came from.
//!
//! [`Engine::parse_type_traced`](crate::parser::engine::Engine::parse_type_traced)
//! returns, next to the [`Value`](crate::Value), a tree of [`TraceNode`]s with
//! the bits of the input that each field, element of a repeated field and
//! instance was read from. A tool that shows the input next to the result (a
//! hex view that selects the bytes of a field, or the field of a byte) is
//! built on it.

use std::collections::HashMap;

/// A range of the input, in bits from its start: `start..end`.
///
/// Bits, because a bit field (`b4`) can start inside a byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Span {
    /// The first bit of the range.
    pub start: u64,
    /// The bit after the last one of the range.
    pub end: u64,
}

impl Span {
    /// The offset of the byte the range starts in.
    pub fn start_byte(&self) -> u64 {
        self.start / 8
    }

    /// The offset after the last byte the range touches.
    pub fn end_byte(&self) -> u64 {
        self.end.div_ceil(8)
    }

    /// Whether the range holds no bits.
    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    /// Whether the byte at `offset` is in the range.
    pub fn contains_byte(&self, offset: u64) -> bool {
        !self.is_empty() && self.start_byte() <= offset && offset < self.end_byte()
    }
}

/// One parsed value and where it was read from.
///
/// The tree follows the [`Value`](crate::Value) it was made with: a type has a
/// child for each of its fields, a repeated field one for each element, named
/// `[0]`, `[1]`, ... An instance is a child of the type it belongs to, after
/// its fields.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TraceNode {
    /// The name of the field or instance, `[index]` for an element of a
    /// repeated field, and the name of the type for the root.
    pub name: String,
    /// The type, for a value that is read by a user type.
    pub type_name: Option<String>,
    /// The bits the value was read from; `None` for an instance computed from
    /// an expression (`value:`), which is not in the input. For a field placed
    /// with `pos:` it is where the field is, not where the sequence went on.
    pub span: Option<Span>,
    /// Whether this is an instance and not a field.
    pub is_instance: bool,
    /// Whether the parse failed while reading this value. Only the innermost
    /// value being read when it failed has it; the ones around it end where it
    /// started to fail.
    pub failed: bool,
    /// Whether the offsets are in bytes that a transform produced
    /// (`process:`), not in the input. The field with the `process:` itself has
    /// the offsets of the input; what is read from its result does not.
    pub processed: bool,
    /// The values inside this one.
    pub children: Vec<TraceNode>,
}

impl TraceNode {
    /// A root that records nothing, for a parse that was not traced.
    pub(crate) fn untraced(name: &str) -> Self {
        TraceNode {
            name: name.to_owned(),
            type_name: None,
            span: None,
            is_instance: false,
            failed: false,
            processed: false,
            children: Vec::new(),
        }
    }

    /// The innermost node whose range holds the byte at `offset`, with the path
    /// of indexes of children that leads to it. Nodes inside a transformed
    /// field are not looked in, as their offsets are of other bytes.
    pub fn find_byte(&self, offset: u64) -> Option<Vec<usize>> {
        let mut path = Vec::new();
        let mut node = self;
        if !node.span.is_some_and(|span| span.contains_byte(offset)) {
            return None;
        }
        loop {
            let next = node.children.iter().enumerate().find(|(_, child)| {
                !child.processed
                    && !child.is_instance
                    && child.span.is_some_and(|span| span.contains_byte(offset))
            });
            match next {
                Some((index, child)) => {
                    path.push(index);
                    node = child;
                }
                None => return Some(path),
            }
        }
    }
}

/// One node while the tree is built: the nodes live in a list and refer to each
/// other by position, so that a node can be completed after others have been
/// started inside it.
#[derive(Debug)]
struct Entry {
    node: TraceNode,
    children: Vec<usize>,
    /// The node it is a child of.
    parent: Option<usize>,
    /// Where the value really is, when the cursor ends up elsewhere (`pos:`).
    placed: Option<Span>,
    /// Where it started, until it is finished.
    start: u64,
    /// Whether it was put somewhere with `pos:`.
    moved: bool,
}

/// Builds the tree of [`TraceNode`]s while the engine parses.
#[derive(Debug, Default)]
pub(crate) struct TraceBuilder {
    entries: Vec<Entry>,
    /// The nodes that are being read, innermost last.
    open: Vec<usize>,
    /// The node of each type that is being read, innermost last: where its
    /// instances go, wherever they are computed from.
    owners: Vec<usize>,
    /// The depth of `process:` fields that are being read.
    processed: usize,
    /// The node an instance that waited was given, by the number of its
    /// placeholder.
    waiting: HashMap<u64, usize>,
    /// A node to open again for the next instance, not a new one.
    reuse: Option<usize>,
}

impl TraceBuilder {
    pub(crate) fn new() -> Self {
        TraceBuilder::default()
    }

    fn push(&mut self, name: &str, start: u64, is_instance: bool, parent: Option<usize>) -> usize {
        let index = self.entries.len();
        self.entries.push(Entry {
            node: TraceNode {
                name: name.to_owned(),
                type_name: None,
                span: None,
                is_instance,
                failed: false,
                processed: self.processed > 0,
                children: Vec::new(),
            },
            children: Vec::new(),
            parent,
            placed: None,
            start,
            moved: false,
        });
        if let Some(parent) = parent
            && let Some(entry) = self.entries.get_mut(parent)
        {
            entry.children.push(index);
        }
        index
    }

    /// Start a field, an element or the root, as a child of the node that is
    /// being read.
    pub(crate) fn begin(&mut self, name: &str, start: u64) {
        let parent = self.open.last().copied();
        let index = self.push(name, start, false, parent);
        self.open.push(index);
    }

    /// Start an instance, as a child of the type it belongs to; or, when it
    /// waited before, open the node it was given.
    pub(crate) fn begin_instance(&mut self, name: &str, start: u64) {
        if let Some(index) = self.reuse.take() {
            self.open.push(index);
            return;
        }
        let parent = self
            .owners
            .last()
            .copied()
            .or_else(|| self.open.last().copied());
        let index = self.push(name, start, true, parent);
        self.open.push(index);
    }

    /// How many nodes are open, to go back to after an error.
    pub(crate) fn depth(&self) -> usize {
        self.open.len()
    }

    /// Set the type of the node being read, which is the one of the type that
    /// is entered.
    pub(crate) fn enter_type(&mut self, type_name: &str) {
        let Some(&index) = self.open.last() else {
            return;
        };
        if let Some(entry) = self.entries.get_mut(index) {
            entry.node.type_name = Some(type_name.to_owned());
        }
        self.owners.push(index);
    }

    pub(crate) fn leave_type(&mut self) {
        self.owners.pop();
    }

    /// Say where the node being read really is.
    pub(crate) fn place(&mut self, start: u64, end: u64) {
        if let Some(&index) = self.open.last()
            && let Some(entry) = self.entries.get_mut(index)
        {
            entry.placed = Some(Span { start, end });
            entry.moved = true;
        }
    }

    /// Finish the node being read, which ends at `end` unless it was placed.
    ///
    /// A value that holds others starts where the first of them does: what
    /// it is made of can start in the middle of a byte, or on the next one.
    pub(crate) fn finish(&mut self, end: u64) {
        let Some(index) = self.open.pop() else {
            return;
        };
        let first = self.entries.get(index).and_then(|entry| {
            entry
                .children
                .iter()
                .filter_map(|&child| self.entries.get(child))
                .find(|child| !child.node.is_instance)
        });
        let first_start = first
            .filter(|child| !child.moved)
            .and_then(|child| child.node.span)
            .map(|span| span.start);
        if let Some(entry) = self.entries.get_mut(index) {
            let mut start = entry.start;
            if let Some(first_start) = first_start
                && first_start > start
            {
                start = first_start;
            }
            entry.node.span = Some(entry.placed.take().unwrap_or(Span { start, end }));
        }
    }

    /// Finish an instance that is not in the input (`value:`).
    pub(crate) fn finish_without_span(&mut self) {
        let Some(index) = self.open.pop() else {
            return;
        };
        if let Some(entry) = self.entries.get_mut(index) {
            entry.node.span = None;
        }
    }

    /// Take back the node being read, and what was started inside it: the
    /// value was absent, or its instance waits.
    pub(crate) fn discard(&mut self, depth: usize) {
        let Some(&index) = self.open.get(depth) else {
            return;
        };
        self.open.truncate(depth);
        let parent = self.entries.get(index).and_then(|entry| entry.parent);
        if let Some(parent) = parent.and_then(|parent| self.entries.get_mut(parent)) {
            parent.children.retain(|&child| child != index);
        }
    }

    /// Keep an empty node for an instance that waits to be computed later,
    /// as `id`, in the type that is being read.
    pub(crate) fn keep_for(&mut self, id: u64, name: &str) {
        let parent = self.owners.last().copied();
        let index = self.push(name, 0, true, parent);
        self.waiting.insert(id, index);
    }

    /// Give up the node being read because its value could not be computed:
    /// an instance that waits keeps its node, empty, for the next try.
    pub(crate) fn abandon(&mut self, depth: usize) {
        let Some(&index) = self.open.get(depth) else {
            return;
        };
        if self.waiting.values().any(|&waiting| waiting == index) {
            self.open.truncate(depth);
            if let Some(entry) = self.entries.get_mut(index) {
                entry.children.clear();
                entry.placed = None;
                entry.node.type_name = None;
            }
        } else {
            self.discard(depth);
        }
    }

    /// Say where the node being read is, unless that has been said.
    pub(crate) fn place_unless_placed(&mut self, start: u64, end: u64) {
        if let Some(&index) = self.open.last()
            && let Some(entry) = self.entries.get_mut(index)
            && entry.placed.is_none()
        {
            entry.placed = Some(Span { start, end });
        }
    }

    /// Finish the node being read at the place it was given.
    pub(crate) fn finish_placed(&mut self) {
        let Some(index) = self.open.pop() else {
            return;
        };
        if let Some(entry) = self.entries.get_mut(index) {
            entry.node.span = entry.placed.take().or(Some(Span {
                start: entry.start,
                end: entry.start,
            }));
        }
    }

    /// Make the next instance that starts the one that waited as `id`, empty.
    pub(crate) fn resume(&mut self, id: u64) {
        if let Some(&index) = self.waiting.get(&id) {
            if let Some(entry) = self.entries.get_mut(index) {
                entry.children.clear();
                entry.placed = None;
            }
            self.reuse = Some(index);
        }
    }

    /// The parse failed: close every node that is being read. The innermost is
    /// the one that failed; each ends where the last thing read inside it
    /// ended, as it was never finished.
    pub(crate) fn close_failed(&mut self) {
        // An instance that was waiting and never was computed is not there.
        let never: Vec<usize> = self
            .waiting
            .values()
            .copied()
            .filter(|&index| {
                self.entries
                    .get(index)
                    .is_some_and(|e| e.node.span.is_none())
            })
            .collect();
        for index in never {
            let parent = self.entries.get(index).and_then(|entry| entry.parent);
            if let Some(parent) = parent.and_then(|parent| self.entries.get_mut(parent)) {
                parent.children.retain(|&child| child != index);
            }
        }
        let mut innermost = true;
        while let Some(index) = self.open.pop() {
            let end = self
                .entries
                .get(index)
                .map(|entry| {
                    entry
                        .children
                        .iter()
                        .filter_map(|&child| self.entries.get(child))
                        .filter(|child| !child.moved && !child.node.is_instance)
                        .filter_map(|child| child.node.span.map(|span| span.end))
                        .max()
                        .unwrap_or(entry.start)
                        .max(entry.start)
                })
                .unwrap_or_default();
            if let Some(entry) = self.entries.get_mut(index) {
                entry.node.span = Some(Span {
                    start: entry.start,
                    end,
                });
                entry.node.failed = innermost;
            }
            innermost = false;
        }
    }

    pub(crate) fn enter_processed(&mut self) {
        self.processed += 1;
    }

    pub(crate) fn leave_processed(&mut self) {
        self.processed = self.processed.saturating_sub(1);
    }

    /// The tree, from the first node started.
    pub(crate) fn into_tree(self) -> Option<TraceNode> {
        fn build(entries: &[Entry], index: usize, depth: usize) -> Option<TraceNode> {
            if depth > MAX_TREE_DEPTH {
                return None;
            }
            let entry = entries.get(index)?;
            let mut node = entry.node.clone();
            let mut children: Vec<TraceNode> = entry
                .children
                .iter()
                .filter_map(|&child| build(entries, child, depth + 1))
                .collect();
            // The fields, then the instances, each in the order they came.
            children.sort_by_key(|child| child.is_instance);
            node.children = children;
            Some(node)
        }
        build(&self.entries, 0, 0)
    }
}

/// Deeper than any type the engine reads, so the tree can be built without
/// recursion that its input could make unbounded.
const MAX_TREE_DEPTH: usize = 4096;
