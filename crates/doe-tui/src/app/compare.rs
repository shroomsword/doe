//! Comparing the file with another one parsed with the same schema (`--compare`): the second file, the marks of what
//! differs, the keys that go from one difference to the next, and the choice between the tree of the first file with
//! marks and the two trees side by side.
//!
//! The first file is the one the viewer was opened on. `V` makes the other one the current file (the trees, the bytes
//! and the file are swapped, so everything that works on the current file works on it), and `V` again goes back.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use doe::compare::{Comparison, Input, Keys};

use super::{App, FileBytes, Focus, LoadFailure, Loaded, read_range};
use crate::format;
use crate::tree::{GhostRow, TreeModel};
use crate::tree_view::TreeView;

/// What only the second file has, to be shown as rows under their parents in the tree of the first one.
type Ghosts = HashMap<Vec<usize>, Vec<GhostRow>>;

/// The second file of a comparison, and what is known of how the two differ.
#[derive(Debug)]
pub struct Compared {
    /// The tree of the other file, with its marks (the tree of the first file after `V`).
    pub tree: TreeModel,
    /// The bytes of the other file.
    pub bytes: Option<FileBytes>,
    /// Which kind of value owns each byte of the other file.
    pub byte_kinds: Option<crate::kinds::ByteKinds>,
    /// The other file.
    pub file: PathBuf,
    /// Its size.
    pub size: u64,
    /// Where its tree is scrolled to, when the two trees are side by side.
    pub view: TreeView,
    /// How the two differ, by the nodes and bytes of the first file (`a`) and the second (`b`).
    pub comparison: Comparison,
    /// Whether the two trees are side by side.
    pub side_by_side: bool,
    /// Whether the other file is the current one (`V`).
    pub swapped: bool,
    /// What only the second file has, to be shown under its parents in the tree of the first one.
    pub ghosts: Ghosts,
    /// What the two trees looked like at the last synchronisation (the selection and what is open), so that nothing is
    /// done again when it has not changed.
    synced: Option<(Vec<usize>, Vec<Vec<usize>>)>,
}

/// The path in `to` of the node that `path` of `from` names (`entries[2].len` is the same field in both trees).
fn counterpart(from: &TreeModel, to: &TreeModel, path: &[usize]) -> Option<Vec<usize>> {
    let text = doe::copy::path_of(from.node(&[])?, path);
    doe::copy::resolve(to.node(&[])?, &text)
}

/// The path of the node in `to` that `path` names, or of its closest ancestor that is there.
fn closest(from: &TreeModel, to: &TreeModel, path: &[usize]) -> Vec<usize> {
    let mut path = path.to_vec();
    loop {
        if let Some(found) = counterpart(from, to, &path) {
            return found;
        }
        if path.pop().is_none() {
            return Vec::new();
        }
    }
}

impl App {
    /// Whether a second file is being compared with the first.
    #[must_use]
    pub fn comparing(&self) -> bool {
        self.compare.is_some()
    }

    /// What the title says about the comparison: how many fields differ, and how.
    #[must_use]
    pub fn compare_summary(&self) -> Option<String> {
        let summary = self.compare.as_ref()?.comparison.summary;
        if summary.identical() {
            return Some("identical".to_owned());
        }
        let mut parts = vec![format!(
            "{} of {} fields differ",
            summary.changed,
            summary.fields_a.max(summary.fields_b)
        )];
        if summary.only_a > 0 {
            parts.push(format!("{} only in A", summary.only_a));
        }
        if summary.only_b > 0 {
            parts.push(format!("{} only in B", summary.only_b));
        }
        let matched = self.matched_lists();
        if !matched.is_empty() {
            parts.push(format!("matched by key: {}", matched.join(", ")));
        }
        Some(parts.join(" · "))
    }

    /// The lists whose elements were matched by a key, as `entries by name`, in the order of the first file's tree.
    fn matched_lists(&self) -> Vec<String> {
        let Some(c) = &self.compare else {
            return Vec::new();
        };
        // the first file's tree is the current one unless the files are swapped
        let tree_a = if c.swapped { &c.tree } else { &self.tree };
        let Some(root) = tree_a.node(&[]) else {
            return Vec::new();
        };
        let mut found: Vec<(&Vec<usize>, String)> = c
            .comparison
            .a
            .keyed
            .iter()
            .map(|(path, field)| {
                (
                    path,
                    format!("{} by {field}", doe::copy::path_of(root, path)),
                )
            })
            .collect();
        found.sort();
        found.into_iter().map(|(_, text)| text).collect()
    }

    /// Which of the two files is the current one: `A` (the one the viewer was opened on) or `B`.
    #[must_use]
    pub fn compare_letter(&self) -> Option<char> {
        self.compare
            .as_ref()
            .map(|c| if c.swapped { 'B' } else { 'A' })
    }

    /// The bytes of the current file that differ from the other one, as sorted ranges.
    #[must_use]
    pub fn changed_bytes(&self) -> &[(u64, u64)] {
        match &self.compare {
            Some(c) if c.swapped => &c.comparison.b.bytes,
            Some(c) => &c.comparison.a.bytes,
            None => &[],
        }
    }

    /// The value of the selected field in the other file, for the inspector: the text, or why there is none.
    #[must_use]
    pub fn other_value(&self) -> Option<String> {
        let c = self.compare.as_ref()?;
        let letter = if c.swapped { 'A' } else { 'B' };
        let Some(path) = counterpart(&self.tree, &c.tree, self.tree.selected_path()) else {
            return Some(format!("{letter}: nothing like it"));
        };
        let value = c
            .tree
            .summary_at(&path)
            .unwrap_or_else(|| "(not a single value)".to_owned());
        Some(format!("{letter}: {value}"))
    }

    /// The second file is parsed (or could not be): compare it with the first.
    pub fn finish_compare(&mut self, result: Result<Loaded, Box<LoadFailure>>) {
        let Some(comparing) = self.options.as_ref().and_then(|o| o.compare.clone()) else {
            return;
        };
        match result {
            Ok(loaded) => self.build_compare(&comparing.file, comparing.side_by_side, loaded),
            Err(failure) => {
                self.notice = Some(format!(
                    "could not read {} to compare: {}",
                    crate::format::safe(&comparing.file.display().to_string()),
                    crate::format::safe(&failure.error.to_string())
                ));
            }
        }
    }

    fn build_compare(&mut self, file_b: &Path, side_by_side: bool, loaded: Loaded) {
        let size = loaded.size;
        let byte_kinds = loaded.byte_kinds;
        let mut tree_b = TreeModel::new(loaded.trace, loaded.value).with_docs(loaded.docs);
        if self.compare_keys.is_empty() {
            self.compare_keys = self.starting_keys();
        }
        let file_a = self.file.clone();
        let Some((comparison, ghosts)) =
            Self::run_comparison(&self.tree, &tree_b, &file_a, file_b, &self.compare_keys)
        else {
            return;
        };
        self.tree.set_comparison(
            &comparison.a,
            if side_by_side {
                HashMap::new()
            } else {
                ghosts.clone()
            },
        );
        tree_b.set_comparison(&comparison.b, HashMap::new());
        self.compare = Some(Compared {
            tree: tree_b,
            bytes: FileBytes::open(file_b).ok(),
            byte_kinds: Some(byte_kinds),
            file: file_b.to_path_buf(),
            size,
            view: TreeView::new(),
            comparison,
            side_by_side,
            swapped: false,
            ghosts,
            synced: None,
        });
        self.sync_other();
        self.notice = self.compare_summary().map(|s| {
            format!(
                "compared with {}: {s}",
                format::safe(&file_b.display().to_string())
            )
        });
        self.say_notes();
    }

    /// What a comparison could not do as asked (a key that cannot match), in the line under the panes.
    fn say_notes(&mut self) {
        let Some(c) = &self.compare else {
            return;
        };
        if let Some(first) = c.comparison.notes.first() {
            let more = c.comparison.notes.len() - 1;
            let text = format::safe(first);
            self.notice = Some(if more == 0 {
                text
            } else {
                format!("{text} (and {more} more)")
            });
        }
    }

    /// Compare two trees with the keys: the comparison, and the rows that only the second one has.
    fn run_comparison(
        tree_a: &TreeModel,
        tree_b: &TreeModel,
        file_a: &Path,
        file_b: &Path,
        keys: &Keys,
    ) -> Option<(Comparison, Ghosts)> {
        let read_a = |offset: u64, len: usize| read_range(file_a, offset, len);
        let read_b = |offset: u64, len: usize| read_range(file_b, offset, len);
        let (root_a, root_b) = (tree_a.node(&[])?, tree_b.node(&[])?);
        let comparison = doe::compare::compare_with(
            &Input {
                trace: root_a,
                value: tree_a.value_at(&[]),
                read: &read_a,
            },
            &Input {
                trace: root_b,
                value: tree_b.value_at(&[]),
                read: &read_b,
            },
            keys,
        );
        let mut ghosts: HashMap<Vec<usize>, Vec<GhostRow>> = HashMap::new();
        for (parent, list) in &comparison.ghosts {
            ghosts.insert(
                parent.clone(),
                list.iter()
                    .map(|ghost| GhostRow {
                        name: ghost.name.clone(),
                        b_path: ghost.b_path.clone(),
                        summary: tree_b.summary_at(&ghost.b_path),
                    })
                    .collect(),
            );
        }
        Some((comparison, ghosts))
    }

    /// The choices of fields to match lists by when a comparison starts: what was kept for this schema, then `--compare-by`.
    fn starting_keys(&self) -> Keys {
        let mut keys = Keys::new();
        if let Some((store, key)) = self.keys_store() {
            for (list, field) in store.read_keys(&key) {
                keys.set(&list, &field);
            }
        }
        for (list, field) in self
            .options
            .as_ref()
            .and_then(|o| o.compare.as_ref())
            .map_or_default(|c| c.by.as_slice())
        {
            keys.set(list, field);
        }
        keys
    }

    /// Where the choices for this schema are kept, when the viewer keeps things.
    fn keys_store(&self) -> Option<(crate::restore::Store, String)> {
        let options = self.options.as_ref()?;
        let dir = options.restore.as_ref()?;
        if self.builder.is_some() {
            return None;
        }
        let key = crate::restore::keys_key_of(&options.type_or_schema, &self.type_name);
        Some((crate::restore::Store::new(dir), key))
    }

    /// `#`: ask which field matches the elements of the list that is selected.
    pub(super) fn ask_match_key(&mut self) {
        let Some(c) = self.compare.as_ref() else {
            self.notice = Some("not comparing: start with --compare FILE".to_owned());
            return;
        };
        if c.swapped {
            self.notice = Some(
                "go back to the first file (V) to choose how its lists are matched".to_owned(),
            );
            return;
        }
        let Some(root) = self.tree.node(&[]) else {
            return;
        };
        let nothing = |_: u64, _: usize| None;
        let lists = doe::compare::lists(&Input {
            trace: root,
            value: self.tree.value_at(&[]),
            read: &nothing,
        });
        let selected = self.tree.selected_path().to_vec();
        let Some(list) = lists
            .into_iter()
            .filter(|l| selected.starts_with(&l.path))
            .max_by_key(|l| l.path.len())
        else {
            self.notice = Some(
                "select a list of records (or something inside one) first: its elements are matched by a field of theirs".to_owned(),
            );
            return;
        };
        let current = self
            .compare_keys
            .get(&list.name)
            .map(str::to_owned)
            .or_else(|| list.schema_key.clone())
            .or_else(|| list.candidates.first().cloned())
            .unwrap_or_default();
        self.notice = Some(if list.candidates.is_empty() {
            format!(
                "{}: no field is different in every element; type the name of one, or nothing for the index",
                list.name
            )
        } else {
            format!(
                "{} can be matched by: {}",
                list.name,
                list.candidates.join(", ")
            )
        });
        self.match_list = Some(list);
        self.open_prompt_with(crate::prompt::Kind::MatchKey, &current);
    }

    /// The field was typed: match the list by it (nothing: by index), compare again and keep the choice.
    pub(super) fn match_key_answered(&mut self, text: &str) {
        let Some(list) = self.match_list.take() else {
            return;
        };
        let field = text.trim();
        if field.is_empty() {
            self.compare_keys.use_index(&list.name);
        } else {
            self.compare_keys.set(&list.name, field);
        }
        self.recompare();
        if let Some((store, key)) = self.keys_store() {
            store.write_keys(&key, &self.chosen_keys());
        }
        let says = self
            .matched_lists()
            .into_iter()
            .find(|m| m.starts_with(&format!("{} by", list.name)));
        let first_note = self
            .compare
            .as_ref()
            .and_then(|c| c.comparison.notes.first().cloned());
        self.notice = Some(match (first_note, says) {
            (Some(note), _) => format::safe(&note),
            (None, Some(says)) => format!("matched: {says}"),
            (None, None) => format!("{} is matched by index", list.name),
        });
    }

    /// The choices that are kept: the lists that were given a field or the index.
    fn chosen_keys(&self) -> Vec<(String, String)> {
        let Some(root) = self.tree.node(&[]) else {
            return Vec::new();
        };
        let nothing = |_: u64, _: usize| None;
        doe::compare::lists(&Input {
            trace: root,
            value: self.tree.value_at(&[]),
            read: &nothing,
        })
        .into_iter()
        .filter_map(|l| {
            self.compare_keys
                .get(&l.name)
                .map(|f| (l.name, f.to_owned()))
        })
        .collect()
    }

    /// Compare the two files again with the keys as they are now (the first file is the current one).
    fn recompare(&mut self) {
        let Some(c) = self.compare.as_ref() else {
            return;
        };
        if c.swapped {
            return;
        }
        let Some((comparison, ghosts)) =
            Self::run_comparison(&self.tree, &c.tree, &self.file, &c.file, &self.compare_keys)
        else {
            return;
        };
        let Some(c) = self.compare.as_mut() else {
            return;
        };
        self.tree.set_comparison(
            &comparison.a,
            if c.side_by_side {
                HashMap::new()
            } else {
                ghosts.clone()
            },
        );
        c.tree.set_comparison(&comparison.b, HashMap::new());
        c.comparison = comparison;
        c.ghosts = ghosts;
        c.synced = None;
        self.sync_other();
    }

    /// Read the second file again and compare (after the first one was read again).
    pub(super) fn reload_compare(&mut self) {
        let Some(options) = self.options.clone() else {
            return;
        };
        let Some(comparing) = options.compare.clone() else {
            return;
        };
        // the first file is the current one again
        if self.compare.as_ref().is_some_and(|c| c.swapped) {
            return;
        }
        self.compare = None;
        let mut other = options;
        other.file = comparing.file;
        let result = crate::load::try_load(&other);
        self.finish_compare(result);
    }

    /// In side by side, the other tree follows this one: the same fields open, the same field selected.
    pub fn sync_other(&mut self) {
        let Some(c) = self.compare.as_mut() else {
            return;
        };
        if !c.side_by_side {
            return;
        }
        let path = self.tree.selected_path().to_vec();
        let open = self.tree.open_paths();
        if c.synced
            .as_ref()
            .is_some_and(|(p, o)| *p == path && *o == open)
        {
            return;
        }
        let there: Vec<Vec<usize>> = open
            .iter()
            .filter_map(|p| counterpart(&self.tree, &c.tree, p))
            .collect();
        c.tree.mirror_open(there);
        let target = closest(&self.tree, &c.tree, &path);
        c.tree.select_path(&target);
        c.synced = Some((path, open));
    }

    /// `>` and `<`: go to the next or the previous difference: in the tree the next field that differs or the
    /// place where the other file has something more, in the bytes the next run of bytes that differ.
    pub(super) fn go_to_difference(&mut self, forward: bool) {
        if self.compare.is_none() {
            self.notice = Some("not comparing: start with --compare FILE".to_owned());
            return;
        }
        if self.focus == Focus::Hex {
            self.go_to_changed_bytes(forward);
        } else {
            self.go_to_changed_field(forward);
        }
        self.sync_other();
    }

    fn go_to_changed_field(&mut self, forward: bool) {
        let paths = self.tree.difference_paths();
        if paths.is_empty() {
            self.notice = Some("nothing differs".to_owned());
            return;
        }
        // where the selection is, in the order of the paths (a ghost is after the children of its parent)
        let mut here = self.tree.selected_path().to_vec();
        if matches!(
            self.tree.selected().kind,
            crate::tree::RowKind::Ghost { .. }
        ) {
            here.push(crate::tree::AFTER_CHILDREN);
        }
        let found = if forward {
            paths.iter().find(|p| **p > here)
        } else {
            paths.iter().rev().find(|p| **p < here)
        };
        let (target, wrapped) = match found {
            Some(target) => (target.clone(), false),
            None => (
                if forward { paths.first() } else { paths.last() }
                    .cloned()
                    .unwrap_or_default(),
                true,
            ),
        };
        self.tree.select_difference(&target);
        self.show_selected_field();
        if wrapped {
            self.notice = Some(
                if forward {
                    "back at the first difference"
                } else {
                    "back at the last difference"
                }
                .to_owned(),
            );
        }
    }

    fn go_to_changed_bytes(&mut self, forward: bool) {
        let ranges = self.changed_bytes().to_vec();
        if ranges.is_empty() {
            self.notice = Some("no bytes differ".to_owned());
            return;
        }
        let cursor = self.hex.cursor();
        let found = if forward {
            ranges.iter().find(|r| r.0 > cursor)
        } else {
            ranges.iter().rev().find(|r| r.0 < cursor)
        };
        let (start, wrapped) = match found {
            Some(r) => (r.0, false),
            None => (
                if forward {
                    ranges.first()
                } else {
                    ranges.last()
                }
                .map_or(0, |r| r.0),
                true,
            ),
        };
        let size = self.bytes.as_ref().map_or(0, FileBytes::size);
        self.hex.set_cursor(start, size);
        self.follow_cursor();
        if wrapped {
            self.notice = Some(
                if forward {
                    "back at the first difference"
                } else {
                    "back at the last difference"
                }
                .to_owned(),
            );
        }
    }

    /// `=`: leave out the fields that are the same, or show them again.
    pub(super) fn toggle_only_differences(&mut self) {
        let Some(c) = self.compare.as_mut() else {
            self.notice = Some("not comparing: start with --compare FILE".to_owned());
            return;
        };
        let on = !self.tree.only_diffs();
        self.tree.set_only_diffs(on);
        c.tree.set_only_diffs(on);
        c.synced = None;
        self.notice = Some(
            if on {
                "only the differences (= shows everything again)"
            } else {
                "everything is shown"
            }
            .to_owned(),
        );
        self.sync_other();
    }

    /// `V`: make the other file the current one, with the same field selected (or the closest one that it has), or go
    /// back to the first.
    pub(super) fn toggle_other_file(&mut self) {
        let Some(c) = self.compare.as_mut() else {
            self.notice = Some("not comparing: start with --compare FILE".to_owned());
            return;
        };
        let wanted = self.tree.selected_path().to_vec();
        let target = closest(&self.tree, &c.tree, &wanted);
        let exact = counterpart(&self.tree, &c.tree, &wanted).is_some();
        c.tree.select_path(&target);
        std::mem::swap(&mut self.tree, &mut c.tree);
        std::mem::swap(&mut self.bytes, &mut c.bytes);
        std::mem::swap(&mut self.byte_kinds, &mut c.byte_kinds);
        std::mem::swap(&mut self.file, &mut c.file);
        std::mem::swap(&mut self.size, &mut c.size);
        std::mem::swap(&mut self.view, &mut c.view);
        c.swapped = !c.swapped;
        c.synced = None;
        let letter = if c.swapped { 'B' } else { 'A' };
        self.show_selected_field();
        self.notice = Some(if exact {
            format!("the bytes and the tree are of file {letter}")
        } else {
            format!(
                "file {letter}: the field has nothing like it there; the closest one that exists is selected"
            )
        });
        self.sync_other();
    }

    /// `|`: the two trees side by side, or the tree of the first file with marks.
    pub(super) fn toggle_compare_view(&mut self) {
        let Some(c) = self.compare.as_mut() else {
            self.notice = Some("not comparing: start with --compare FILE".to_owned());
            return;
        };
        c.side_by_side = !c.side_by_side;
        c.synced = None;
        // the ghosts are in the tree of the first file, whichever is the current one
        let ghosts = if c.side_by_side {
            HashMap::new()
        } else {
            c.ghosts.clone()
        };
        if c.swapped {
            c.tree.set_ghosts(ghosts);
        } else {
            self.tree.set_ghosts(ghosts);
        }
        self.notice = Some(
            if c.side_by_side {
                "the two files side by side (| goes back to the marks)"
            } else {
                "the first file with marks (| puts the two side by side)"
            }
            .to_owned(),
        );
        self.sync_other();
    }
}
