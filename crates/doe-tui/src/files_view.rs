//! The Files view: an archive or file system as directories and files.
//!
//! A schema that says its files are file systems (`filesystem:`, see `doe::filesystem`) has a list of entries with
//! paths; this nests them by path, orders them, keeps what is open and what is selected, and draws the rows in the
//! place of the tree of fields (the key `F` switches). Moving the selection selects the entry's node in the tree of
//! fields, so the bytes pane follows. Like the tree, it is made of rows that are cut to the width.

use std::collections::{HashMap, HashSet};

use doe::filesystem::{Entry, Kind, Listing};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::format;
use crate::theme::compare_names;
use crate::tree_view::TreeView;
use crate::ui::Theme;

/// How many children of a directory have rows at first, and how many more each time.
pub const CHUNK: usize = 500;

/// A directory or a file of the tree. A directory that only other paths imply has no entry.
#[derive(Debug, Clone)]
struct Node {
    name: String,
    path: Vec<String>,
    entry: Option<Entry>,
    children: Vec<usize>,
    /// The entries inside, at any depth.
    count: usize,
}

impl Node {
    fn is_directory(&self) -> bool {
        self.entry
            .as_ref()
            .is_none_or(|entry| entry.kind == Kind::Directory)
    }
}

/// What a row of the view is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowKind {
    /// A directory or a file.
    Node(usize),
    /// "N more…" for the directory (a node number): the children after the ones that have rows.
    More {
        /// The directory.
        of: usize,
        /// How many are not shown.
        hidden: usize,
    },
}

/// A row of the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// What it is.
    pub kind: RowKind,
    /// How deep it is.
    pub depth: usize,
}

/// The tree of the entries, what is open, and the selected row.
#[derive(Debug)]
pub struct FilesState {
    nodes: Vec<Node>,
    open: HashSet<Vec<String>>,
    shown: HashMap<usize, usize>,
    rows: Vec<Row>,
    selected: usize,
    /// Whether the directories come before the files (the default).
    pub directories_first: bool,
}

impl FilesState {
    /// The view of a listing; the top level is open.
    #[must_use]
    pub fn new(listing: &Listing) -> Self {
        let mut nodes = vec![Node {
            name: String::new(),
            path: Vec::new(),
            entry: None,
            children: Vec::new(),
            count: 0,
        }];
        let mut directories: HashMap<Vec<String>, usize> = HashMap::from([(Vec::new(), 0)]);
        for entry in &listing.entries {
            let path = if entry.path.is_empty() {
                vec![String::new()]
            } else {
                entry.path.clone()
            };
            if entry.kind == Kind::Directory {
                let at = directory(&mut nodes, &mut directories, &path);
                if let Some(node) = nodes.get_mut(at)
                    && node.entry.is_none()
                {
                    node.entry = Some(entry.clone());
                }
                continue;
            }
            let parent_path = path.get(..path.len() - 1).unwrap_or_default().to_vec();
            let parent = directory(&mut nodes, &mut directories, &parent_path);
            let id = nodes.len();
            nodes.push(Node {
                name: path.last().cloned().unwrap_or_default(),
                path,
                entry: Some(entry.clone()),
                children: Vec::new(),
                count: 0,
            });
            if let Some(parent) = nodes.get_mut(parent) {
                parent.children.push(id);
            }
        }
        count(&mut nodes, 0);
        let mut state = FilesState {
            nodes,
            open: HashSet::from([Vec::new()]),
            shown: HashMap::new(),
            rows: Vec::new(),
            selected: 0,
            directories_first: true,
        };
        state.rebuild();
        state
    }

    fn rebuild(&mut self) {
        let keep = self.selected_node();
        let mut rows = Vec::new();
        self.collect(0, 0, &mut rows);
        self.rows = rows;
        self.selected = keep
            .and_then(|id| {
                self.rows
                    .iter()
                    .position(|r| matches!(r.kind, RowKind::Node(n) if n == id))
            })
            .unwrap_or_else(|| self.selected.min(self.rows.len().saturating_sub(1)));
    }

    fn ordered(&self, id: usize) -> Vec<usize> {
        let Some(node) = self.nodes.get(id) else {
            return Vec::new();
        };
        let mut children = node.children.clone();
        let first = self.directories_first;
        children.sort_by(|&a, &b| {
            let (Some(a), Some(b)) = (self.nodes.get(a), self.nodes.get(b)) else {
                return std::cmp::Ordering::Equal;
            };
            if first && a.is_directory() != b.is_directory() {
                return b.is_directory().cmp(&a.is_directory());
            }
            compare_names(&a.name, &b.name)
        });
        children
    }

    fn collect(&self, id: usize, depth: usize, rows: &mut Vec<Row>) {
        let children = self.ordered(id);
        let limit = self.shown.get(&id).copied().unwrap_or(CHUNK);
        for &child in children.iter().take(limit) {
            rows.push(Row {
                kind: RowKind::Node(child),
                depth,
            });
            if let Some(node) = self.nodes.get(child)
                && node.is_directory()
                && self.open.contains(&node.path)
            {
                self.collect(child, depth + 1, rows);
            }
        }
        if children.len() > limit {
            rows.push(Row {
                kind: RowKind::More {
                    of: id,
                    hidden: children.len() - limit,
                },
                depth,
            });
        }
    }

    /// The rows that have rows now.
    #[must_use]
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The index of the selected row.
    #[must_use]
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    fn selected_node(&self) -> Option<usize> {
        match self.rows.get(self.selected)?.kind {
            RowKind::Node(id) => Some(id),
            RowKind::More { .. } => None,
        }
    }

    /// The entry of the selected row, if it is an entry (a directory that only other paths imply has none).
    #[must_use]
    pub fn selected_entry(&self) -> Option<&Entry> {
        self.nodes.get(self.selected_node()?)?.entry.as_ref()
    }

    /// The path of the selected row, as text.
    #[must_use]
    pub fn selected_path(&self) -> String {
        self.selected_node()
            .and_then(|id| self.nodes.get(id))
            .map(|n| n.path.join("/"))
            .unwrap_or_default()
    }

    /// One row up.
    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// One row down.
    pub fn down(&mut self) {
        self.selected = (self.selected + 1).min(self.rows.len().saturating_sub(1));
    }

    /// Up a page of `rows`.
    pub fn page_up(&mut self, rows: usize) {
        self.selected = self.selected.saturating_sub(rows.saturating_sub(1).max(1));
    }

    /// Down a page of `rows`.
    pub fn page_down(&mut self, rows: usize) {
        self.selected =
            (self.selected + rows.saturating_sub(1).max(1)).min(self.rows.len().saturating_sub(1));
    }

    /// The first row.
    pub fn home(&mut self) {
        self.selected = 0;
    }

    /// The last row.
    pub fn end(&mut self) {
        self.selected = self.rows.len().saturating_sub(1);
    }

    /// Open the directory, or go into it; on "N more" show more.
    pub fn right(&mut self) {
        match self.rows.get(self.selected).map(|r| r.kind.clone()) {
            Some(RowKind::More { of, .. }) => {
                *self.shown.entry(of).or_insert(CHUNK) += CHUNK;
                self.rebuild();
            }
            Some(RowKind::Node(id)) => {
                let Some(node) = self.nodes.get(id) else {
                    return;
                };
                if !node.is_directory() {
                    return;
                }
                if self.open.insert(node.path.clone()) {
                    self.rebuild();
                } else {
                    self.down();
                }
            }
            None => {}
        }
    }

    /// Close the directory, or go to the parent.
    pub fn left(&mut self) {
        let Some(id) = self.selected_node() else {
            return;
        };
        let Some(node) = self.nodes.get(id) else {
            return;
        };
        if node.is_directory() && self.open.remove(&node.path) {
            self.rebuild();
            return;
        }
        let parent: Vec<String> = node
            .path
            .get(..node.path.len().saturating_sub(1))
            .unwrap_or_default()
            .to_vec();
        if let Some(at) = self.rows.iter().position(|r| {
            matches!(r.kind, RowKind::Node(n) if self.nodes.get(n).is_some_and(|n| n.path == parent))
        }) {
            self.selected = at;
        }
    }

    /// Open or close the directory.
    pub fn toggle(&mut self) {
        match self.rows.get(self.selected).map(|r| r.kind.clone()) {
            Some(RowKind::More { .. }) => self.right(),
            Some(RowKind::Node(id)) => {
                let Some(node) = self.nodes.get(id) else {
                    return;
                };
                if !node.is_directory() {
                    return;
                }
                if !self.open.remove(&node.path) {
                    self.open.insert(node.path.clone());
                }
                self.rebuild();
            }
            None => {}
        }
    }

    /// Select the entry that holds the node at `path` of the trace, opening its directories; nothing is selected
    /// when the node is not inside an entry. Returns whether an entry was found.
    pub fn select_trace(&mut self, path: &[usize]) -> bool {
        for length in (1..=path.len()).rev() {
            let wanted = path.get(..length).unwrap_or_default();
            let Some(id) = self.nodes.iter().position(|n| {
                n.entry
                    .as_ref()
                    .is_some_and(|e| e.trace_path.as_slice() == wanted)
            }) else {
                continue;
            };
            let parents: Vec<Vec<String>> = self
                .nodes
                .get(id)
                .map(|n| {
                    (0..n.path.len())
                        .map(|i| n.path.get(..i).unwrap_or_default().to_vec())
                        .collect()
                })
                .unwrap_or_default();
            for parent in parents {
                self.open.insert(parent);
            }
            self.rebuild();
            if let Some(at) = self
                .rows
                .iter()
                .position(|r| matches!(r.kind, RowKind::Node(n) if n == id))
            {
                self.selected = at;
                return true;
            }
        }
        false
    }
}

/// The node of a directory, made with its parents when it is not there.
fn directory(
    nodes: &mut Vec<Node>,
    directories: &mut HashMap<Vec<String>, usize>,
    path: &[String],
) -> usize {
    if let Some(&id) = directories.get(path) {
        return id;
    }
    let parent = directory(
        nodes,
        directories,
        path.get(..path.len().saturating_sub(1)).unwrap_or_default(),
    );
    let id = nodes.len();
    nodes.push(Node {
        name: path.last().cloned().unwrap_or_default(),
        path: path.to_vec(),
        entry: None,
        children: Vec::new(),
        count: 0,
    });
    if let Some(parent) = nodes.get_mut(parent) {
        parent.children.push(id);
    }
    directories.insert(path.to_vec(), id);
    id
}

fn count(nodes: &mut [Node], id: usize) -> usize {
    let children = nodes
        .get(id)
        .map(|n| n.children.clone())
        .unwrap_or_default();
    let total = children.iter().map(|&c| 1 + count(nodes, c)).sum();
    if let Some(node) = nodes.get_mut(id) {
        node.count = total;
    }
    total
}

/// The letter of `ls -l` for a kind.
fn letter(kind: Kind) -> char {
    match kind {
        Kind::File => '-',
        Kind::Directory => 'd',
        Kind::Symlink => 'l',
        Kind::Hardlink => 'h',
        Kind::Device => 'c',
        Kind::Fifo => 'p',
        Kind::Socket => 's',
    }
}

/// `drwxr-xr-x` for a mode and kind; just the letter when the format has no mode.
#[must_use]
pub fn mode_text(mode: Option<u32>, kind: Kind) -> String {
    let mut out = String::from(letter(kind));
    if let Some(mode) = mode {
        for (i, c) in "rwxrwxrwx".chars().enumerate() {
            out.push(if mode & (1 << (8 - i)) == 0 { '-' } else { c });
        }
    }
    out
}

/// A size in bytes as `6 B`, `1.5 KiB`...
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "a size shown with one decimal does not need the last bits"
)]
pub fn size_text(size: u64) -> String {
    if size < 1024 {
        return format!("{size} B");
    }
    let mut value = size as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < 3 {
        value /= 1024.0;
        unit += 1;
    }
    let units = ["KiB", "MiB", "GiB", "TiB"];
    let name = units.get(unit).copied().unwrap_or("TiB");
    if value < 10.0 {
        format!("{value:.1} {name}")
    } else {
        format!("{value:.0} {name}")
    }
}

/// A time in seconds since 1970 as `2026-11-04 12:00` (UTC).
#[must_use]
pub fn time_text(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    // civil from days (H. Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60
    )
}

/// The cells of a row's columns, before the name: the size, mode and time, right of the name.
fn columns(entry: Option<&Entry>, directory: bool) -> String {
    let Some(entry) = entry else {
        return String::new();
    };
    let size = if directory {
        String::new()
    } else {
        entry.size.map(size_text).unwrap_or_default()
    };
    let time = entry.mtime.map(time_text).unwrap_or_default();
    format!(
        "{size:>9}  {:<10}  {time}",
        mode_text(entry.mode, entry.kind)
    )
}

/// Draw the rows that are in view in `area`, the selected one marked.
pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    files: &FilesState,
    view: &mut TreeView,
    theme: &Theme,
) {
    let rows = files.rows();
    view.follow(files.selected_index(), rows.len(), usize::from(area.height));
    if area.width == 0 || area.height == 0 {
        return;
    }
    let width = usize::from(area.width);
    let lines: Vec<Line<'static>> = rows
        .iter()
        .enumerate()
        .skip(view.top())
        .take(usize::from(area.height))
        .map(|(index, row)| row_line(files, row, index == files.selected_index(), width, theme))
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

fn row_line(
    files: &FilesState,
    row: &Row,
    selected: bool,
    width: usize,
    theme: &Theme,
) -> Line<'static> {
    let indent = "  ".repeat(row.depth);
    let (left, style, right) = match &row.kind {
        RowKind::More { hidden, .. } => (
            format!("{indent}  {hidden} more…"),
            theme.dim(),
            String::new(),
        ),
        RowKind::Node(id) => {
            let Some(node) = files.nodes.get(*id) else {
                return Line::raw("");
            };
            let directory = node.is_directory();
            let marker = if directory {
                if files.open.contains(&node.path) {
                    "▾ "
                } else {
                    "▸ "
                }
            } else {
                "  "
            };
            let kind = node.entry.as_ref().map_or('d', |e| letter(e.kind));
            let mut name = if node.name.is_empty() {
                "(no name)".to_owned()
            } else {
                format::safe(&node.name)
            };
            if let Some(link) = node.entry.as_ref().and_then(|e| e.link.as_ref()) {
                name = format!("{name} -> {}", format::safe(link));
            }
            if directory && node.count > 0 {
                name = format!("{name}  {}", node.count);
            }
            let flag = if node.entry.as_ref().is_some_and(|e| !e.flags.is_empty()) {
                " ⚠"
            } else {
                ""
            };
            (
                format!("{indent}{marker}{kind} {name}{flag}"),
                if directory {
                    theme.name()
                } else {
                    Style::default()
                },
                columns(node.entry.as_ref(), directory),
            )
        }
    };
    let right_cells = format::width(&right);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let room = width.saturating_sub(right_cells + 2);
    let left_text = format::truncate(&left, room.max(1));
    let used = format::width(&left_text);
    spans.push(Span::styled(left_text, style));
    let gap = width.saturating_sub(used + right_cells);
    spans.push(Span::raw(" ".repeat(gap)));
    if gap > 0 || right_cells > 0 {
        spans.push(Span::styled(right, theme.dim()));
    }
    let mut line = Line::from(spans);
    let cells = line.width();
    if cells < width {
        line.spans.push(Span::raw(" ".repeat(width - cells)));
    }
    if selected {
        line = line.patch_style(theme.selected());
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, kind: Kind, trace: &[usize]) -> Entry {
        serde_json::from_value(serde_json::json!({
            "path": path.split('/').filter(|c| !c.is_empty()).collect::<Vec<_>>(),
            "kind": match kind { Kind::Directory => "directory", _ => "file" },
            "size": 6, "mode": 0o644, "mtime": 1_700_000_000,
            "link": null, "trace_path": trace, "data": null, "flags": []
        }))
        .unwrap()
    }

    fn names(files: &FilesState) -> Vec<String> {
        files
            .rows()
            .iter()
            .map(|r| match r.kind {
                RowKind::Node(id) => files.nodes[id].name.clone(),
                RowKind::More { hidden, .. } => format!("{hidden} more"),
            })
            .collect()
    }

    fn sample() -> FilesState {
        FilesState::new(&Listing {
            entries: vec![
                entry("b.txt", Kind::File, &[0, 0]),
                entry("docs/a.txt", Kind::File, &[0, 1]),
                entry("docs", Kind::Directory, &[0, 2]),
                entry("docs/sub/c.txt", Kind::File, &[0, 3]),
            ],
        })
    }

    #[test]
    fn the_entries_are_nested_by_path_with_the_directories_first_and_closed_until_opened() {
        let mut files = sample();
        assert_eq!(names(&files), ["docs", "b.txt"]);
        files.right();
        assert_eq!(names(&files), ["docs", "sub", "a.txt", "b.txt"]);
        // the directory that only the paths imply has no entry, the one that is named has
        files.down();
        assert!(files.selected_entry().is_none());
        files.up();
        assert!(files.selected_entry().is_some());
        files.left();
        assert_eq!(names(&files), ["docs", "b.txt"]);
    }

    #[test]
    fn left_goes_to_the_parent_and_a_trace_path_selects_the_entry_and_opens_its_directories() {
        let mut files = sample();
        assert!(files.select_trace(&[0, 3, 1]));
        assert_eq!(files.selected_path(), "docs/sub/c.txt");
        files.left();
        assert_eq!(files.selected_path(), "docs/sub");
        assert!(!files.select_trace(&[7]));
    }

    #[test]
    fn a_long_directory_has_a_row_that_shows_more() {
        let entries = (0..(CHUNK + 5))
            .map(|i| entry(&format!("f{i:04}"), Kind::File, &[0, i]))
            .collect();
        let mut files = FilesState::new(&Listing { entries });
        assert_eq!(files.rows().len(), CHUNK + 1);
        files.end();
        assert_eq!(names(&files).last().unwrap(), "5 more");
        files.right();
        assert_eq!(files.rows().len(), CHUNK + 5);
    }

    #[test]
    fn modes_sizes_and_times_are_written_as_ls_would() {
        assert_eq!(mode_text(Some(0o755), Kind::Directory), "drwxr-xr-x");
        assert_eq!(mode_text(None, Kind::Symlink), "l");
        assert_eq!(size_text(6), "6 B");
        assert_eq!(size_text(1536), "1.5 KiB");
        assert_eq!(size_text(5 * 1024 * 1024), "5.0 MiB");
        assert_eq!(time_text(1_700_000_000), "2023-11-14 22:13");
        assert_eq!(time_text(0), "1970-01-01 00:00");
        assert_eq!(time_text(951_782_400), "2000-02-29 00:00");
    }
}
