//! What the tree pane shows, without drawing it: the rows that are visible,
//! which nodes are open and which row is selected.
//!
//! The model is made from the trace of a parse (where each value came from)
//! and its value (what was read). A node is named by its **path**: the indexes
//! of the children that lead to it from the root. Everything the person has
//! done to the tree (what is open, what is selected) is kept by path, so a
//! model made from a changed value can take it over (reload, #106).
//!
//! Only open nodes have rows, and a long list is shown in pieces of
//! [`CHUNK`] with a row that says how many more there are, so opening a list
//! of a million elements makes about [`CHUNK`] rows, not a million.

use std::collections::{HashMap, HashSet};

use doe::Value;
use doe::api::value_summary;
use doe::parser::trace::{Span, TraceNode};

/// How many children of a node are shown at first, and how many more each time.
pub const CHUNK: usize = 200;

/// What a row is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowKind {
    /// A field, an element or an instance.
    Node,
    /// "N more": the children of the parent that are not shown yet.
    More {
        /// How many are hidden.
        hidden: usize,
    },
}

/// The longest text of a string that a row keeps; the rest is cut when the row is made.
const MAX_SHOWN_CHARS: usize = 1024;
/// How many bytes of a byte sequence a row keeps.
const PREVIEW_BYTES: usize = 16;

/// A value by kind, for drawing it the way its kind is read.
#[derive(Debug, Clone, PartialEq)]
pub enum Shown {
    /// An unsigned integer.
    UInt(u64),
    /// A signed integer.
    SInt(i64),
    /// An integer with a name from an enum, when it has one.
    Enum {
        /// The number.
        value: i128,
        /// The member, if the number is one.
        name: Option<String>,
    },
    /// A float.
    Float(f64),
    /// A bit or a boolean.
    Bool(bool),
    /// A string (cut at 1024 characters; `cut` says it was).
    Str {
        /// The text.
        text: String,
        /// Whether it was longer.
        cut: bool,
    },
    /// A byte sequence: how long it is and its first bytes.
    Bytes {
        /// The length in bytes.
        len: u64,
        /// The first bytes.
        preview: Vec<u8>,
    },
}

/// What a value is shown as; `None` for a type, a list and an absent value.
fn shown_of(value: Option<&Value>) -> Option<Shown> {
    Some(match value? {
        Value::Instance { value, .. } => return shown_of(Some(value)),
        Value::UInt(n) => Shown::UInt(*n),
        Value::SInt(n) => Shown::SInt(*n),
        Value::Float(f) => Shown::Float(*f),
        Value::Bool(b) => Shown::Bool(*b),
        Value::Enum { value, name } => Shown::Enum {
            value: *value,
            name: name.clone(),
        },
        Value::Str(text) => {
            let mut chars = text.chars();
            let kept: String = chars.by_ref().take(MAX_SHOWN_CHARS).collect();
            Shown::Str {
                cut: chars.next().is_some(),
                text: kept,
            }
        }
        Value::Bytes(bytes) => Shown::Bytes {
            len: bytes.len() as u64,
            preview: bytes.iter().copied().take(PREVIEW_BYTES).collect(),
        },
        Value::Blob { len, preview, .. } => Shown::Bytes {
            len: *len,
            preview: preview.iter().copied().take(PREVIEW_BYTES).collect(),
        },
        Value::Struct { .. } | Value::Array(_) | Value::Absent => return None,
    })
}

/// A line of the tree.
#[derive(Debug, Clone, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each flag is a separate fact a row can show; they are not a state machine"
)]
pub struct Row {
    /// What it is.
    pub kind: RowKind,
    /// The path to its node (to the parent for a `More` row, with the index
    /// of the first child that is not shown).
    pub path: Vec<usize>,
    /// How deep it is; the root is 0.
    pub depth: usize,
    /// The name of the field, `[i]` for an element, the type for the root.
    pub name: String,
    /// The user type read here.
    pub type_name: Option<String>,
    /// The value as text, for a value that is not a type or a list.
    pub summary: Option<String>,
    /// The value by kind, to draw it.
    pub shown: Option<Shown>,
    /// The bits it was read from; none for an instance computed by an expression.
    pub span: Option<Span>,
    /// Whether it is an instance.
    pub instance: bool,
    /// Whether the parse failed here.
    pub failed: bool,
    /// Whether the offsets are in the bytes a `process:` produced.
    pub processed: bool,
    /// Whether it has children, so can be opened.
    pub openable: bool,
    /// Whether it is open.
    pub open: bool,
}

/// The tree, its open nodes and its selection.
#[derive(Debug)]
pub struct TreeModel {
    root: TraceNode,
    value: Option<Value>,
    open: HashSet<Vec<usize>>,
    /// How many children of a node are shown, when more than [`CHUNK`].
    shown: HashMap<Vec<usize>, usize>,
    rows: Vec<Row>,
    /// The selected row.
    selected: usize,
    /// The root's row, for the case that there are no rows (there always are).
    fallback: Row,
}

/// The value of the child `name` (or element `[i]`) of `value`.
fn child_value<'a>(value: Option<&'a Value>, index: usize, name: &str) -> Option<&'a Value> {
    let value = match value? {
        Value::Instance { value, .. } => value.as_ref(),
        other => other,
    };
    match value {
        Value::Struct { fields, .. } => fields.iter().find(|(n, _)| n == name).map(|(_, v)| v),
        Value::Array(items) => {
            // The trace names elements "[i]" in order, so the index is the child's.
            let from_name = name
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
                .and_then(|n| n.parse::<usize>().ok());
            items.get(from_name.unwrap_or(index))
        }
        _ => None,
    }
}

/// The row of a node.
fn row_of(node: &TraceNode, value: Option<&Value>, path: &[usize], open: bool) -> Row {
    Row {
        kind: RowKind::Node,
        path: path.to_vec(),
        depth: path.len(),
        name: node.name.clone(),
        type_name: node.type_name.clone(),
        summary: value.and_then(value_summary),
        shown: shown_of(value),
        span: node.span,
        instance: node.is_instance,
        failed: node.failed,
        processed: node.processed,
        openable: !node.children.is_empty(),
        open,
    }
}

impl TreeModel {
    /// A tree of what a parse read. The root is open and selected.
    #[must_use]
    pub fn new(root: TraceNode, value: Option<Value>) -> Self {
        let mut model = TreeModel {
            fallback: row_of(&root, value.as_ref(), &[], false),
            root,
            value,
            open: HashSet::from([Vec::new()]),
            shown: HashMap::new(),
            rows: Vec::new(),
            selected: 0,
        };
        model.rebuild();
        model
    }

    /// The rows that are visible, from the top.
    #[must_use]
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The index in [`rows`](Self::rows) of the selected row.
    #[must_use]
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// The selected row. There is always one: the root.
    #[must_use]
    pub fn selected(&self) -> &Row {
        self.rows.get(self.selected).unwrap_or(&self.fallback)
    }

    /// The path of the selected node (of the parent for a `More` row).
    #[must_use]
    pub fn selected_path(&self) -> &[usize] {
        &self.selected().path
    }

    /// The trace node at `path`.
    #[must_use]
    pub fn node(&self, path: &[usize]) -> Option<&TraceNode> {
        let mut node = &self.root;
        for &index in path {
            node = node.children.get(index)?;
        }
        Some(node)
    }

    // ── building the rows ────────────────────────────────────────────────────

    fn rebuild(&mut self) {
        let keep = self
            .rows
            .get(self.selected)
            .map(|row| (row.kind.clone(), row.path.clone()));
        let mut rows = Vec::new();
        let mut path = Vec::new();
        self.push_rows(&self.root, self.value.as_ref(), &mut path, &mut rows);
        self.rows = rows;
        // The same row if it is still there, else the closest one above.
        self.selected = keep
            .and_then(|(kind, path)| {
                self.rows
                    .iter()
                    .position(|r| r.kind == kind && r.path == path)
            })
            .unwrap_or_else(|| self.selected.min(self.rows.len().saturating_sub(1)));
    }

    fn push_rows(
        &self,
        node: &TraceNode,
        value: Option<&Value>,
        path: &mut Vec<usize>,
        rows: &mut Vec<Row>,
    ) {
        let open = self.open.contains(path.as_slice()) && !node.children.is_empty();
        rows.push(row_of(node, value, path, open));
        if !open {
            return;
        }
        let shown = self
            .shown
            .get(path.as_slice())
            .copied()
            .unwrap_or(CHUNK)
            .max(CHUNK);
        for (index, child) in node.children.iter().enumerate().take(shown) {
            path.push(index);
            self.push_rows(child, child_value(value, index, &child.name), path, rows);
            path.pop();
        }
        if node.children.len() > shown {
            rows.push(Row {
                kind: RowKind::More {
                    hidden: node.children.len() - shown,
                },
                path: path.clone(),
                depth: path.len() + 1,
                name: format!("{} more…", node.children.len() - shown),
                type_name: None,
                summary: None,
                shown: None,
                span: None,
                instance: false,
                failed: false,
                processed: false,
                openable: false,
                open: false,
            });
        }
    }

    // ── open and close ───────────────────────────────────────────────────────

    /// Open the selected node; on a "more" row, show the next piece.
    pub fn expand(&mut self) {
        let row = self.selected().clone();
        match row.kind {
            RowKind::More { .. } => self.show_more(&row.path),
            RowKind::Node if row.openable && !row.open => {
                self.open.insert(row.path);
                self.rebuild();
            }
            RowKind::Node => {}
        }
    }

    fn show_more(&mut self, parent: &[usize]) {
        let shown = self.shown.get(parent).copied().unwrap_or(CHUNK).max(CHUNK);
        self.shown.insert(parent.to_vec(), shown + CHUNK);
        self.rebuild();
        // The first row that was hidden is selected.
        let first_new = shown;
        if let Some(position) = self.rows.iter().position(|r| {
            r.kind == RowKind::Node
                && r.path.len() == parent.len() + 1
                && r.path.starts_with(parent)
                && r.path.last() == Some(&first_new)
        }) {
            self.selected = position;
        }
    }

    /// Close the selected node. A closed node or a leaf stays as it is.
    pub fn collapse(&mut self) {
        let row = self.selected().clone();
        if row.kind == RowKind::Node && row.open {
            self.open.remove(&row.path);
            self.rebuild();
        }
    }

    /// Open the selected node if it is closed, else close it.
    pub fn toggle(&mut self) {
        if self.selected().open {
            self.collapse();
        } else {
            self.expand();
        }
    }

    // ── moving the selection ─────────────────────────────────────────────────

    fn select_index(&mut self, index: usize) {
        self.selected = index.min(self.rows.len().saturating_sub(1));
    }

    /// One row up.
    pub fn up(&mut self) {
        self.select_index(self.selected.saturating_sub(1));
    }

    /// One row down.
    pub fn down(&mut self) {
        self.select_index(self.selected + 1);
    }

    /// `rows` rows up.
    pub fn page_up(&mut self, rows: usize) {
        self.select_index(self.selected.saturating_sub(rows.max(1)));
    }

    /// `rows` rows down.
    pub fn page_down(&mut self, rows: usize) {
        self.select_index(self.selected.saturating_add(rows.max(1)));
    }

    /// The first row.
    pub fn home(&mut self) {
        self.select_index(0);
    }

    /// The last row.
    pub fn end(&mut self) {
        self.select_index(self.rows.len().saturating_sub(1));
    }

    /// The parent of the selected node; the root has none.
    pub fn parent(&mut self) {
        let row = self.selected();
        let parent: &[usize] = match row.kind {
            RowKind::More { .. } => &row.path,
            RowKind::Node => match row.path.split_last() {
                Some((_, parent)) => parent,
                None => return,
            },
        };
        if let Some(position) = self
            .rows
            .iter()
            .position(|r| r.kind == RowKind::Node && r.path == parent)
        {
            self.selected = position;
        }
    }

    /// The first child of the selected node, opening it if it is closed.
    pub fn first_child(&mut self) {
        if !self.selected().openable {
            return;
        }
        if !self.selected().open {
            self.expand();
        }
        if self.selected().open {
            self.select_index(self.selected + 1);
        }
    }

    /// The next node with the same parent, past the children of this one.
    pub fn next_sibling(&mut self) {
        let depth = self.selected().depth;
        for (index, row) in self.rows.iter().enumerate().skip(self.selected + 1) {
            if row.depth < depth {
                return;
            }
            if row.depth == depth {
                self.selected = index;
                return;
            }
        }
    }

    /// The previous node with the same parent.
    pub fn prev_sibling(&mut self) {
        let depth = self.selected().depth;
        for (index, row) in self.rows.iter().enumerate().take(self.selected).rev() {
            if row.depth < depth {
                return;
            }
            if row.depth == depth {
                self.selected = index;
                return;
            }
        }
    }

    // ── selecting by path ────────────────────────────────────────────────────

    /// Select the node at `path`, opening what is above it and showing the
    /// piece of a long list it is in. If the tree has no such node (a reload
    /// that changed the value), the closest node that exists is selected: the
    /// longest prefix of the path.
    pub fn select_path(&mut self, path: &[usize]) {
        let mut found = Vec::new();
        let mut node = &self.root;
        for &index in path {
            match node.children.get(index) {
                Some(child) => {
                    found.push(index);
                    node = child;
                }
                None => break,
            }
        }
        for (depth, &index) in found.iter().enumerate() {
            let ancestor = found.get(..depth).unwrap_or_default().to_vec();
            let needed = index + 1;
            if needed > CHUNK {
                let shown = self.shown.entry(ancestor.clone()).or_insert(CHUNK);
                *shown = (*shown).max(needed.div_ceil(CHUNK) * CHUNK);
            }
            self.open.insert(ancestor);
        }
        self.rebuild();
        if let Some(position) = self
            .rows
            .iter()
            .position(|r| r.kind == RowKind::Node && r.path == found)
        {
            self.selected = position;
        }
    }

    /// Take over the open nodes and the selection of another model, for a
    /// tree made from a changed value: what no longer exists is dropped.
    pub fn take_state_of(&mut self, other: &TreeModel) {
        for path in &other.open {
            if self.node(path).is_some() {
                self.open.insert(path.clone());
            }
        }
        for (path, shown) in &other.shown {
            if self.node(path).is_some() {
                self.shown.insert(path.clone(), *shown);
            }
        }
        let path = other.selected_path().to_vec();
        self.select_path(&path);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;
    use crate::Options;
    use crate::load::load;

    /// A model made by parsing `bytes` with `schema` (a file is needed: `load` reads one).
    pub(crate) fn model(schema: &str, bytes: &[u8]) -> (TreeModel, Option<String>) {
        let dir = tempfile::tempdir().unwrap();
        let schema_path = dir.path().join("t.yaml");
        std::fs::write(&schema_path, schema).unwrap();
        let file = dir.path().join("t.bin");
        std::fs::write(&file, bytes).unwrap();
        let loaded = load(&Options {
            file,
            type_or_schema: schema_path.to_string_lossy().into_owned(),
            include_paths: Vec::new(),
            color: false,
        })
        .unwrap();
        let error = loaded.error.as_ref().map(ToString::to_string);
        (TreeModel::new(loaded.trace, loaded.value), error)
    }

    /// The path of the failed node, found by walking the tree.
    fn failed_path(tree: &TreeModel, path: &mut Vec<usize>) -> Option<Vec<usize>> {
        let node = tree.node(path)?;
        if node.failed {
            return Some(path.clone());
        }
        for index in 0..node.children.len() {
            path.push(index);
            let found = failed_path(tree, path);
            path.pop();
            if found.is_some() {
                return found;
            }
        }
        None
    }

    const PAIRS: &str = "id: t
seq:
  - id: count
    type: u1
  - id: items
    type: item
    repeat: expr
    repeat-expr: count
  - id: tail
    type: u1
    if: count > 100
types:
  item:
    seq:
      - id: a
        type: u1
      - id: b
        type: u1
instances:
  total:
    value: count * 2
";

    /// Two items, then nothing for `tail` (absent).
    fn pairs() -> TreeModel {
        model(PAIRS, &[2, 1, 2, 3, 4]).0
    }

    fn names(model: &TreeModel) -> Vec<String> {
        model
            .rows()
            .iter()
            .map(|r| format!("{}{}", "  ".repeat(r.depth), r.name))
            .collect()
    }

    #[test]
    fn rows_are_made_from_the_value_and_the_trace() {
        let tree = pairs();
        // root open; `items` and the items are closed; `tail` is absent: no row
        assert_eq!(names(&tree), ["t", "  count", "  items", "  total"]);
        let rows = tree.rows();
        assert_eq!(rows[0].type_name.as_deref(), Some("t"));
        assert_eq!(rows[0].summary, None);
        assert_eq!(rows[1].summary.as_deref(), Some("2"));
        assert_eq!(rows[1].span.map(|s| (s.start, s.end)), Some((0, 8)));
        assert!(rows[2].openable && !rows[2].open);
        assert_eq!(rows[2].summary, None);
        // an instance has a row, marked, and no bytes of its own when it is computed
        assert!(rows[3].instance && !rows[1].instance);
        assert_eq!(rows[3].summary.as_deref(), Some("4"));
        assert_eq!(rows[3].span, None);
    }

    #[test]
    fn open_and_close() {
        let mut tree = pairs();
        tree.down();
        tree.down(); // items
        tree.expand();
        assert_eq!(
            names(&tree),
            ["t", "  count", "  items", "    [0]", "    [1]", "  total"]
        );
        tree.down();
        tree.expand();
        assert_eq!(
            names(&tree),
            [
                "t", "  count", "  items", "    [0]", "      a", "      b", "    [1]", "  total"
            ]
        );
        assert_eq!(tree.selected().summary, None);
        // a second expand changes nothing; collapse closes; toggle does both
        tree.expand();
        assert_eq!(tree.rows().len(), 8);
        tree.collapse();
        assert_eq!(tree.rows().len(), 6);
        tree.toggle();
        assert_eq!(tree.rows().len(), 8);
        tree.toggle();
        assert_eq!(tree.rows().len(), 6);
        // a leaf neither opens nor closes
        tree.home();
        tree.down();
        tree.expand();
        tree.collapse();
        assert_eq!(tree.rows().len(), 6);
        // collapsing the root leaves it alone, selected
        tree.home();
        tree.collapse();
        assert_eq!(names(&tree), ["t"]);
        assert_eq!(tree.selected_index(), 0);
    }

    #[test]
    fn the_selection_stays_on_its_row_when_rows_above_open() {
        let mut tree = pairs();
        tree.end(); // total
        let path = tree.selected_path().to_vec();
        tree.home();
        tree.down();
        tree.down();
        tree.expand(); // items: two rows are put before `total`
        tree.end();
        assert_eq!(tree.selected_path(), path);
    }

    #[test]
    fn every_movement_and_the_ends() {
        let mut tree = pairs();
        tree.down();
        tree.down();
        tree.expand();
        tree.down();
        tree.expand(); // t, count, items, [0], a, b, [1], total
        tree.home();
        assert_eq!(tree.selected_index(), 0);
        tree.up();
        assert_eq!(tree.selected_index(), 0, "up at the top stays");
        tree.parent();
        assert_eq!(tree.selected_index(), 0, "the root has no parent");
        tree.prev_sibling();
        tree.next_sibling();
        assert_eq!(tree.selected_index(), 0, "the root has no siblings");

        tree.end();
        assert_eq!(tree.selected().name, "total");
        tree.down();
        assert_eq!(tree.selected().name, "total", "down at the bottom stays");
        tree.next_sibling();
        assert_eq!(tree.selected().name, "total");

        tree.page_up(3);
        assert_eq!(tree.selected_index(), 4);
        tree.page_up(100);
        assert_eq!(tree.selected_index(), 0);
        tree.page_down(2);
        assert_eq!(tree.selected_index(), 2);
        tree.page_down(1000);
        assert_eq!(tree.selected().name, "total");
        tree.page_up(0); // at least one row
        assert_eq!(tree.selected_index(), 6);

        // siblings skip the children of a node
        tree.select_path(&[1]); // items
        assert_eq!(tree.selected().name, "items");
        tree.next_sibling();
        assert_eq!(tree.selected().name, "total");
        tree.prev_sibling();
        assert_eq!(tree.selected().name, "items");
        tree.prev_sibling();
        assert_eq!(tree.selected().name, "count");
        tree.prev_sibling();
        assert_eq!(tree.selected().name, "count", "no sibling before the first");

        // parent and first child
        tree.select_path(&[1, 0, 1]);
        assert_eq!(tree.selected().name, "b");
        tree.parent();
        assert_eq!(tree.selected().name, "[0]");
        tree.parent();
        assert_eq!(tree.selected().name, "items");
        tree.first_child();
        assert_eq!(tree.selected().name, "[0]");
        tree.first_child();
        assert_eq!(tree.selected().name, "a");
        tree.first_child();
        assert_eq!(tree.selected().name, "a", "a leaf has no child");
        // first child opens a closed node
        tree.select_path(&[1, 1]);
        tree.collapse();
        tree.first_child();
        assert_eq!(tree.selected().name, "a");
    }

    #[test]
    fn a_type_with_no_fields_is_a_single_row() {
        let (mut tree, error) = model("id: empty\nseq: []\n", b"");
        assert!(error.is_none());
        assert_eq!(names(&tree), ["empty"]);
        for step in [
            TreeModel::up,
            TreeModel::down,
            TreeModel::home,
            TreeModel::end,
            TreeModel::parent,
            TreeModel::first_child,
            TreeModel::next_sibling,
            TreeModel::prev_sibling,
            TreeModel::expand,
            TreeModel::collapse,
            TreeModel::toggle,
        ] {
            step(&mut tree);
            assert_eq!(tree.selected_index(), 0);
            assert_eq!(tree.rows().len(), 1);
        }
        tree.page_down(10);
        tree.page_up(10);
        assert_eq!(tree.selected_index(), 0);
        assert!(!tree.selected().openable);
    }

    #[test]
    fn a_failed_parse_has_a_failed_row_and_what_was_read() {
        // three items are announced; the bytes have one and a half
        let (mut tree, error) = model(PAIRS, &[3, 1, 2, 3]);
        assert!(error.is_some());
        let failed = failed_path(&tree, &mut Vec::new()).expect("a failed node");
        // the second element's field
        assert_eq!(failed.len(), 3, "{failed:?}");
        tree.select_path(&failed);
        assert!(tree.selected().failed);
        assert_eq!(
            names(&tree)
                .iter()
                .filter(|n| n.trim() == "[0]" || n.trim() == "[1]")
                .count(),
            2
        );
        // what was read before the failure is there
        tree.select_path(&[1, 0, 0]);
        assert_eq!(tree.selected().summary.as_deref(), Some("1"));
    }

    #[test]
    fn the_selection_by_path_falls_back_to_the_nearest_row() {
        let mut tree = pairs();
        tree.select_path(&[1, 1, 0]);
        assert_eq!(tree.selected().name, "a");
        assert_eq!(tree.selected_path(), [1, 1, 0]);
        // the ancestors were opened
        assert!(tree.rows().iter().any(|r| r.name == "[1]" && r.open));

        // nothing at [1, 5]: the nearest is `items`
        tree.select_path(&[1, 5, 0]);
        assert_eq!(tree.selected().name, "items");
        tree.select_path(&[9]);
        assert_eq!(tree.selected().name, "t");
        tree.select_path(&[]);
        assert_eq!(tree.selected().name, "t");
    }

    #[test]
    fn a_model_made_from_a_changed_value_takes_over_what_was_open_and_selected() {
        let mut before = pairs();
        before.select_path(&[1, 1, 1]); // items[1].b, with items and [1] open
        let mut after = model(PAIRS, &[3, 1, 2, 3, 4, 5, 6]).0;
        after.take_state_of(&before);
        assert_eq!(after.selected_path(), [1, 1, 1]);
        assert_eq!(after.selected().summary.as_deref(), Some("4"));
        assert!(
            after.rows().iter().any(|r| r.name == "[2]"),
            "the new element is there"
        );

        // the value shrinks: what is gone is dropped, the nearest row is selected
        let mut smaller = model(PAIRS, &[1, 9, 9]).0;
        smaller.take_state_of(&before);
        assert_eq!(smaller.selected_path(), [1]);
        assert_eq!(smaller.selected().name, "items");
    }

    /// A million elements: `items` has this many.
    const MANY: usize = 1_000_000;

    fn many() -> TreeModel {
        model(
            &format!("id: big\nseq:\n  - id: items\n    type: u1\n    repeat: expr\n    repeat-expr: {MANY}\n"),
            &vec![7_u8; MANY],
        )
        .0
    }

    #[test]
    fn a_list_of_a_million_elements_opens_in_pieces_and_closes() {
        let mut tree = many();
        tree.down();
        assert_eq!(tree.selected().name, "items");
        tree.expand();
        // CHUNK elements and the "more" row, not a million rows
        assert_eq!(tree.rows().len(), 3 + CHUNK);
        let more = tree.rows().last().unwrap();
        assert_eq!(
            more.kind,
            RowKind::More {
                hidden: MANY - CHUNK
            }
        );
        assert_eq!(more.name, format!("{} more…", MANY - CHUNK));

        // expanding the "more" row shows the next piece and selects its first element
        tree.end();
        tree.expand();
        assert_eq!(tree.rows().len(), 3 + 2 * CHUNK);
        assert_eq!(tree.selected().name, format!("[{CHUNK}]"));
        assert_eq!(tree.selected().summary.as_deref(), Some("7"));

        // closing drops all of them
        tree.select_path(&[0]);
        tree.collapse();
        assert_eq!(tree.rows().len(), 2);
    }

    #[test]
    fn selecting_deep_in_a_long_list_shows_the_pieces_up_to_it() {
        let count = 5_000;
        let (mut tree, _) = model(
            &format!(
                "id: big\nseq:\n  - id: items\n    type: u1\n    repeat: expr\n    repeat-expr: {count}\n"
            ),
            &vec![7_u8; count],
        );
        tree.select_path(&[0, 4_500]);
        assert_eq!(tree.selected().name, "[4500]");
        // the pieces up to the one it is in, and a "more" row for the rest
        assert_eq!(tree.rows().len(), 2 + 4_600 + 1);
        tree.select_path(&[0, 4_999]);
        assert_eq!(tree.selected().name, "[4999]");
        assert!(
            !tree
                .rows()
                .iter()
                .any(|r| matches!(r.kind, RowKind::More { .. }))
        );
    }

    #[test]
    fn closing_a_list_drops_its_rows() {
        let mut tree = pairs();
        tree.select_path(&[1, 0]);
        let open = tree.rows().len();
        tree.select_path(&[1]);
        tree.collapse();
        assert!(tree.rows().len() < open);
        assert_eq!(tree.rows().len(), 4);
    }
}
