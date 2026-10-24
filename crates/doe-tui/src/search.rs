//! Searching the fields: names, types and values (the text the tree shows).
//!
//! A tree can have millions of fields, so a search is not a loop that runs to
//! the end: it is a [`Search`] that is advanced by [`Search::step`], a number
//! of fields at a time, between the redraws of the screen and the reading of
//! keys. The screen stays alive and any key stops it.

use crate::tree::TreeModel;

/// What to look for. A query with no capital letter ignores case (like `less`
/// and `vim`'s smartcase); one with a capital letter does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Needle {
    query: String,
    folded: String,
    case_sensitive: bool,
}

impl Needle {
    /// What to look for; nothing for an empty query.
    #[must_use]
    pub fn new(query: &str) -> Option<Needle> {
        if query.is_empty() {
            return None;
        }
        let case_sensitive = query.chars().any(char::is_uppercase);
        Some(Needle {
            query: query.to_owned(),
            folded: query.to_lowercase(),
            case_sensitive,
        })
    }

    /// The query as it was typed.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Whether `text` has the query in it.
    #[must_use]
    pub fn matches(&self, text: &str) -> bool {
        if self.case_sensitive {
            text.contains(&self.query)
        } else {
            text.to_lowercase().contains(&self.folded)
        }
    }
}

/// How a step ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// It has not found anything yet, and has not looked everywhere.
    Running,
    /// A field matches: its path.
    Found(Vec<usize>),
    /// Nothing matches anywhere.
    NotFound,
}

/// A search that is going on.
#[derive(Debug, Clone)]
pub struct Search {
    needle: Needle,
    forward: bool,
    /// Where it started: the search ends when it has come round to it.
    start: Vec<usize>,
    /// The last field that was looked at.
    cursor: Vec<usize>,
    visited: u64,
    wrapped: bool,
}

impl Search {
    /// A search from after (or before) the field at `start`.
    #[must_use]
    pub fn new(needle: Needle, forward: bool, start: Vec<usize>) -> Search {
        Search {
            needle,
            forward,
            cursor: start.clone(),
            start,
            visited: 0,
            wrapped: false,
        }
    }

    /// What is looked for.
    #[must_use]
    pub fn needle(&self) -> &Needle {
        &self.needle
    }

    /// How many fields have been looked at.
    #[must_use]
    pub fn visited(&self) -> u64 {
        self.visited
    }

    /// Whether it went past the end of the tree and came back to the other.
    #[must_use]
    pub fn wrapped(&self) -> bool {
        self.wrapped
    }

    /// Look at up to `budget` fields.
    pub fn step(&mut self, tree: &TreeModel, budget: usize) -> Outcome {
        for _ in 0..budget {
            let next = if self.forward {
                tree.next_in_order(&self.cursor)
            } else {
                tree.previous_in_order(&self.cursor)
            };
            self.cursor = next.unwrap_or_else(|| {
                self.wrapped = true;
                if self.forward {
                    Vec::new()
                } else {
                    tree.last_path()
                }
            });
            self.visited += 1;
            let needle = &self.needle;
            if tree.node_matches(&self.cursor, |text| needle.matches(text)) {
                return Outcome::Found(self.cursor.clone());
            }
            // it has been all the way round, the field it started at included
            if self.cursor == self.start {
                return Outcome::NotFound;
            }
        }
        Outcome::Running
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;
    use crate::tree::tests::model;

    const SCHEMA: &str = "id: rec
seq:
  - id: name
    type: str
    size: 5
    encoding: UTF-8
  - id: Count
    type: u1
  - id: items
    type: item
    repeat: expr
    repeat-expr: 3
types:
  item:
    seq:
      - id: kind
        type: u1
        enum: kinds
      - id: label
        type: str
        size: 2
        encoding: UTF-8
enums:
  kinds:
    1: alpha
    2: beta
";

    fn data() -> Vec<u8> {
        let mut data = b"Hello".to_vec();
        data.push(7);
        for (kind, label) in [(1_u8, "aa"), (2, "bB"), (1, "cc")] {
            data.push(kind);
            data.extend(label.as_bytes());
        }
        data
    }

    fn tree() -> TreeModel {
        model(SCHEMA, &data()).0
    }

    /// Run a search to its end and say what it found, by names.
    fn run(
        tree: &TreeModel,
        query: &str,
        forward: bool,
        from: &[usize],
    ) -> Result<Vec<String>, ()> {
        let mut search = Search::new(Needle::new(query).unwrap(), forward, from.to_vec());
        loop {
            match search.step(tree, 3) {
                Outcome::Running => {}
                Outcome::Found(path) => return Ok(tree.names_to(&path)),
                Outcome::NotFound => return Err(()),
            }
        }
    }

    fn names(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn it_matches_names_types_and_values() {
        let tree = tree();
        // a name
        assert_eq!(
            run(&tree, "label", true, &[]),
            Ok(names(&["rec", "items", "[0]", "label"]))
        );
        // a type name (the items are `rec::item`)
        assert_eq!(
            run(&tree, "rec::item", true, &[]),
            Ok(names(&["rec", "items", "[0]"]))
        );
        // a value, as shown: `"Hello"`, the number 7 and its hex, an enum member
        assert_eq!(run(&tree, "ello", true, &[]), Ok(names(&["rec", "name"])));
        assert_eq!(run(&tree, "7", true, &[]), Ok(names(&["rec", "Count"])));
        assert_eq!(
            run(&tree, "beta", true, &[]),
            Ok(names(&["rec", "items", "[1]", "kind"]))
        );
        assert_eq!(
            run(&tree, "(2)", true, &[]),
            Ok(names(&["rec", "items", "[1]", "kind"]))
        );
    }

    #[test]
    fn case_is_ignored_unless_the_query_has_a_capital() {
        let tree = tree();
        assert_eq!(run(&tree, "hello", true, &[]), Ok(names(&["rec", "name"])));
        assert_eq!(run(&tree, "HELLO", true, &[]), Err(()));
        assert_eq!(run(&tree, "Hello", true, &[]), Ok(names(&["rec", "name"])));
        // "count" matches the field `Count` with a small query, "COUNT" does not
        assert_eq!(run(&tree, "count", true, &[]), Ok(names(&["rec", "Count"])));
        assert_eq!(run(&tree, "COUNT", true, &[]), Err(()));
        // the value "bB" is found by "bb" (any case) and by "bB" (exactly), not by "BB"
        assert_eq!(
            run(&tree, "bb", true, &[]),
            Ok(names(&["rec", "items", "[1]", "label"]))
        );
        assert_eq!(
            run(&tree, "bB", true, &[]),
            Ok(names(&["rec", "items", "[1]", "label"]))
        );
        assert_eq!(run(&tree, "BB", true, &[]), Err(()));
    }

    #[test]
    fn it_finds_what_comes_after_and_goes_round() {
        let tree = tree();
        // forward from the first `kind`: the next match of "kind" is in the next item
        let first = tree.names_to(&[2, 0, 0]);
        assert_eq!(first, names(&["rec", "items", "[0]", "kind"]));
        assert_eq!(
            run(&tree, "kind", true, &[2, 0, 0]),
            Ok(names(&["rec", "items", "[1]", "kind"]))
        );
        // after the last match it goes round to the first
        let mut search = Search::new(Needle::new("kind").unwrap(), true, vec![2, 2, 0]);
        let found = loop {
            match search.step(&tree, 2) {
                Outcome::Found(path) => break path,
                Outcome::Running => {}
                Outcome::NotFound => panic!("there are matches"),
            }
        };
        assert_eq!(found, [2, 0, 0]);
        assert!(search.wrapped());
        // backward goes the other way, and round
        assert_eq!(
            run(&tree, "kind", false, &[2, 1, 0]),
            Ok(names(&["rec", "items", "[0]", "kind"]))
        );
        assert_eq!(
            run(&tree, "kind", false, &[2, 0, 0]),
            Ok(names(&["rec", "items", "[2]", "kind"]))
        );
        // the only match is the field it starts at: found after going all round
        assert_eq!(run(&tree, "ello", true, &[1]), Ok(names(&["rec", "name"])));
        assert_eq!(run(&tree, "ello", false, &[1]), Ok(names(&["rec", "name"])));
    }

    #[test]
    fn nothing_matching_ends_and_does_not_loop() {
        let tree = tree();
        let mut search = Search::new(Needle::new("zzz").unwrap(), true, vec![1]);
        let mut steps = 0;
        loop {
            steps += 1;
            assert!(steps < 100, "it never ends");
            match search.step(&tree, 4) {
                Outcome::NotFound => break,
                Outcome::Running => {}
                Outcome::Found(path) => panic!("found {path:?}"),
            }
        }
        // every node was looked at once (the tree has 1 + 3 + 3 * 3 nodes)
        assert_eq!(search.visited(), 1 + 3 + 3 * 3);
        assert!(Needle::new("").is_none());
        // the root is a node too: a search for its name finds it
        assert_eq!(run(&tree, "rec", true, &[2, 2, 1]), Ok(names(&["rec"])));
    }

    #[test]
    fn a_step_looks_at_no_more_than_its_budget() {
        let tree = tree();
        let mut search = Search::new(Needle::new("zzz").unwrap(), true, vec![]);
        assert_eq!(search.step(&tree, 5), Outcome::Running);
        assert_eq!(search.visited(), 5);
        assert_eq!(search.step(&tree, 0), Outcome::Running);
        assert_eq!(search.visited(), 5);
    }

    #[test]
    fn the_order_of_the_tree_both_ways_visits_every_node_once() {
        let tree = tree();
        let mut forward = vec![Vec::new()];
        while let Some(next) = tree.next_in_order(forward.last().unwrap()) {
            forward.push(next);
        }
        assert_eq!(forward.len(), 13);
        let mut backward = vec![tree.last_path()];
        while let Some(previous) = tree.previous_in_order(backward.last().unwrap()) {
            backward.push(previous);
        }
        backward.reverse();
        assert_eq!(forward, backward);
        assert_eq!(tree.previous_in_order(&[]), None);
    }

    /// A million fields: a step is bounded by its budget however large the tree is.
    #[test]
    fn a_tree_of_a_million_rows_is_searched_in_steps() {
        let count = 1_000_000;
        // the only match is the last element, at the far end
        let mut tail = vec![1_u8; count];
        tail[count - 1] = 200;
        let (tree, _) = model(
            &format!(
                "id: big\nseq:\n  - id: items\n    type: u1\n    repeat: expr\n    repeat-expr: {count}\n"
            ),
            &tail,
        );
        let mut search = Search::new(Needle::new("0xc8").unwrap(), true, vec![]);
        let budget = 10_000;
        let mut steps = 0_u64;
        let found = loop {
            let before = search.visited();
            let outcome = search.step(&tree, budget);
            // no step looks at more than its budget: the screen can be redrawn between steps
            assert!(search.visited() - before <= budget as u64);
            steps += 1;
            match outcome {
                Outcome::Running => {}
                Outcome::Found(path) => break path,
                Outcome::NotFound => panic!("the value is there"),
            }
        };
        assert_eq!(found, [0, count - 1]);
        assert!(steps > 90, "it took {steps} steps of {budget}");
    }
}
