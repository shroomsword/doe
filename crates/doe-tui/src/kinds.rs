//! Which kind of value owns each byte of the file, for the colours of the bytes pane.
//!
//! The tree colours a value by its kind (a number, a string, a run of bytes, an enum, a boolean); the bytes pane draws each
//! byte in the colour of the leaf field it belongs to, by the same function ([`crate::ui::Theme::value`]), so the two panes
//! cannot drift apart. The index is a sorted list of disjoint segments, made once per parse and searched for the bytes in
//! view: memory is proportional to the number of leaf fields, and a line of the pane costs a binary search.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// The kind of a value, as far as its colour goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An integer or a float.
    Number,
    /// A string.
    Str,
    /// A run of bytes.
    Bytes,
    /// An integer with an enum.
    Enum,
    /// A bit or a boolean.
    Bool,
}

/// A byte range (first, and the byte after the last) of a leaf field, with its kind (none: a field with no kind of its own)
/// and its priority (lower holds a byte first).
#[derive(Debug, Clone, Copy)]
pub struct Leaf {
    /// The first byte.
    pub first: u64,
    /// The byte after the last.
    pub end: u64,
    /// The kind of its value.
    pub kind: Option<Kind>,
    /// The order in the sequence; instances come after the fields.
    pub priority: u64,
}

/// The kinds of the bytes of a file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ByteKinds {
    starts: Vec<u64>,
    ends: Vec<u64>,
    kinds: Vec<Option<Kind>>,
}

impl ByteKinds {
    /// How many segments.
    #[must_use]
    pub fn len(&self) -> usize {
        self.starts.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    /// The kind of the leaf field that owns `byte`, or None for a field with no kind, a gap, or past the last field.
    #[must_use]
    pub fn kind_at(&self, byte: u64) -> Option<Kind> {
        let after = self.starts.partition_point(|start| *start <= byte);
        let at = after.checked_sub(1)?;
        if byte >= *self.ends.get(at)? {
            return None;
        }
        *self.kinds.get(at)?
    }

    /// Cut the leaves into disjoint segments, each with the kind of the leaf of the best priority that covers it.
    #[must_use]
    pub fn from_leaves(mut leaves: Vec<Leaf>) -> ByteKinds {
        leaves.retain(|leaf| leaf.end > leaf.first);
        // a sequence is mostly in the order of its bytes already (the sort keeps the order of equal starts)
        if !leaves.is_sorted_by_key(|leaf| leaf.first) {
            leaves.sort_by_key(|leaf| leaf.first);
        }
        let mut out = ByteKinds::default();
        // the leaves that cover the position, the best priority at the top
        let mut heap: BinaryHeap<Reverse<(u64, usize)>> = BinaryHeap::new();
        let mut position = 0_u64;
        let mut next = 0;
        while next < leaves.len() || !heap.is_empty() {
            if heap.is_empty() {
                position = leaves.get(next).map_or(position, |leaf| leaf.first);
            }
            while let Some(leaf) = leaves.get(next).filter(|leaf| leaf.first <= position) {
                heap.push(Reverse((leaf.priority, next)));
                next += 1;
            }
            while let Some(Reverse((_, index))) = heap.peek().copied()
                && leaves.get(index).is_none_or(|leaf| leaf.end <= position)
            {
                heap.pop();
            }
            let Some(Reverse((_, top))) = heap.peek().copied() else {
                continue;
            };
            let Some(leaf) = leaves.get(top) else {
                heap.pop();
                continue;
            };
            let limit = leaves.get(next).map_or(u64::MAX, |l| l.first);
            let end = leaf.end.min(limit);
            let joined = out.ends.last() == Some(&position) && out.kinds.last() == Some(&leaf.kind);
            if joined {
                if let Some(last) = out.ends.last_mut() {
                    *last = end;
                }
            } else {
                out.starts.push(position);
                out.ends.push(end);
                out.kinds.push(leaf.kind);
            }
            position = end;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;

    fn leaf(first: u64, end: u64, kind: Option<Kind>, priority: u64) -> Leaf {
        Leaf {
            first,
            end,
            kind,
            priority,
        }
    }

    #[test]
    fn each_byte_has_the_kind_of_the_leaf_that_holds_it_and_a_gap_has_none() {
        let kinds = ByteKinds::from_leaves(vec![
            leaf(0, 2, Some(Kind::Number), 0),
            leaf(2, 3, Some(Kind::Enum), 1),
            leaf(4, 7, Some(Kind::Str), 2),
            leaf(8, 10, Some(Kind::Bytes), 3),
            leaf(10, 11, Some(Kind::Bool), 4),
            leaf(11, 12, None, 5),
        ]);
        let at = |i: u64| kinds.kind_at(i);
        assert_eq!([0, 1].map(at), [Some(Kind::Number); 2]);
        assert_eq!(at(2), Some(Kind::Enum));
        assert_eq!(at(3), None);
        assert_eq!([4, 5, 6].map(at), [Some(Kind::Str); 3]);
        assert_eq!(at(7), None);
        assert_eq!([8, 9].map(at), [Some(Kind::Bytes); 2]);
        assert_eq!(at(10), Some(Kind::Bool));
        assert_eq!(at(11), None);
        assert_eq!(at(12), None);
        assert_eq!(at(u64::MAX), None);
        assert!(!kinds.is_empty());
    }

    #[test]
    fn where_leaves_overlap_the_better_priority_holds_the_byte() {
        // an instance (priority 1000) over bytes 0..6, fields a (2..4) and b (3..5) in the sequence
        let kinds = ByteKinds::from_leaves(vec![
            leaf(0, 6, Some(Kind::Str), 1000),
            leaf(2, 4, Some(Kind::Number), 0),
            leaf(3, 5, Some(Kind::Str), 1),
        ]);
        let all: Vec<_> = (0..7).map(|i| kinds.kind_at(i)).collect();
        assert_eq!(
            all,
            [
                Some(Kind::Str),
                Some(Kind::Str),
                Some(Kind::Number),
                Some(Kind::Number),
                Some(Kind::Str),
                Some(Kind::Str),
                None
            ]
        );
    }

    #[test]
    fn a_million_leaves_are_indexed_and_looked_up() {
        let leaves: Vec<Leaf> = (0..1_000_000_u64)
            .map(|i| {
                let kind =
                    [Kind::Number, Kind::Str, Kind::Bytes][usize::try_from(i % 3).unwrap_or(0)];
                leaf(i * 2, i * 2 + 2, Some(kind), i)
            })
            .collect();
        let kinds = ByteKinds::from_leaves(leaves);
        assert_eq!(kinds.kind_at(0), Some(Kind::Number));
        assert_eq!(kinds.kind_at(2), Some(Kind::Str));
        assert_eq!(kinds.kind_at(4), Some(Kind::Bytes));
        assert_eq!(kinds.kind_at(1_999_999), Some(Kind::Number));
        assert!(kinds.len() <= 1_000_000);
    }
}
