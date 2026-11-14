//! Two parses of one schema, compared (#423): the fields that differ, were added or were removed, and the bytes.

use std::collections::BTreeMap;
use std::path::Path;

use doe::compare::{Comparison, Input, Mark, Side, compare};
use doe::copy::path_of;
use doe::parser::cursor::Cursor;
use doe::parser::engine::Engine;
use doe::parser::trace::TraceNode;
use doe::schema::compiler::compile_schema;
use doe::schema::raw::RawSchema;
use doe::value::Value;

const SCHEMA: &str = "meta:\n  id: s\n  endian: le\nseq:\n  - id: version\n    type: u1\n  - id: count\n    type: u1\n  - id: items\n    type: item\n    repeat: expr\n    repeat-expr: count\n  - id: tail\n    type: u2\ntypes:\n  item:\n    seq:\n      - id: a\n        type: u1\n      - id: b\n        type: u1\n";

/// A parse of `data` with `yaml`: what was read, and the value.
fn parse(yaml: &str, data: &[u8]) -> (TraceNode, Option<Value>) {
    let schema = RawSchema::from_yaml_str(yaml).unwrap();
    let registry = compile_schema(&schema).unwrap();
    let outcome = Engine::new(&registry)
        .parse_type_partial(&schema.id, &mut Cursor::new(data))
        .unwrap();
    (outcome.trace, outcome.value)
}

fn run(yaml: &str, a: &[u8], b: &[u8]) -> (Comparison, TraceNode, TraceNode) {
    let (ta, va) = parse(yaml, a);
    let (tb, vb) = parse(yaml, b);
    let read_a = |offset: u64, len: usize| {
        let start = usize::try_from(offset).ok()?;
        a.get(start..start + len).map(<[u8]>::to_vec)
    };
    let read_b = |offset: u64, len: usize| {
        let start = usize::try_from(offset).ok()?;
        b.get(start..start + len).map(<[u8]>::to_vec)
    };
    let comparison = compare(
        &Input {
            trace: &ta,
            value: va.as_ref(),
            read: &read_a,
        },
        &Input {
            trace: &tb,
            value: vb.as_ref(),
            read: &read_b,
        },
    );
    (comparison, ta, tb)
}

/// The marks of a side by the path of the node, as the viewers write it.
fn marks(side: &Side, root: &TraceNode) -> BTreeMap<String, Mark> {
    side.marks
        .iter()
        .map(|(path, mark)| (path_of(root, path), *mark))
        .collect()
}

fn names(map: &BTreeMap<String, Mark>) -> Vec<&str> {
    map.keys().map(String::as_str).collect()
}

#[test]
fn files_that_are_the_same_are_identical_and_nothing_is_marked() {
    let data = [1, 2, 10, 11, 12, 13, 7, 0];
    let (c, _, _) = run(SCHEMA, &data, &data);
    assert!(c.summary.identical());
    assert!(c.a.marks.is_empty() && c.b.marks.is_empty());
    assert!(c.a.bytes.is_empty() && c.b.bytes.is_empty());
    assert!(c.ghosts.is_empty());
    // version, count, items, two items of two fields, and the item and the tail
    assert_eq!(c.summary.fields_a, c.summary.fields_b);
    assert!(c.summary.fields_a >= 10, "{:?}", c.summary);
}

#[test]
fn a_changed_integer_is_marked_with_the_containers_around_it_and_its_byte() {
    let a = [1, 2, 10, 11, 12, 13, 7, 0];
    let mut b = a;
    b[3] = 99; // items[0].b
    let (c, ta, tb) = run(SCHEMA, &a, &b);
    let marks_a = marks(&c.a, &ta);
    assert_eq!(
        marks_a,
        BTreeMap::from([
            ("$".to_owned(), Mark::Inside),
            ("items".to_owned(), Mark::Inside),
            ("items[0]".to_owned(), Mark::Inside),
            ("items[0].b".to_owned(), Mark::Changed),
        ])
    );
    // the other file has the same marks on its own nodes
    assert_eq!(marks(&c.b, &tb), marks_a);
    assert_eq!(c.summary.changed, 1);
    assert_eq!((c.summary.only_a, c.summary.only_b), (0, 0));
    assert_eq!(c.a.bytes, vec![(3, 4)]);
    assert_eq!(c.b.bytes, vec![(3, 4)]);
    // how many differences are below the containers
    assert_eq!(c.a.below.get(&Vec::new()), Some(&1));
}

#[test]
fn a_field_that_moved_but_did_not_change_is_not_a_difference() {
    let yaml = "meta:\n  id: m\n  endian: le\nseq:\n  - id: len\n    type: u1\n  - id: name\n    type: str\n    size: len\n    encoding: ASCII\n  - id: after\n    type: u2\n";
    // `after` is the same 2-byte value, at offset 3 in one file and 4 in the other
    let a = [2, b'a', b'b', 7, 0];
    let b = [3, b'a', b'b', b'c', 7, 0];
    let (c, ta, _) = run(yaml, &a, &b);
    let marks_a = marks(&c.a, &ta);
    assert_eq!(
        names(&marks_a),
        ["$", "len", "name"],
        "`after` moved and is not marked: {marks_a:?}"
    );
    assert_eq!(marks_a.get("after"), None);
    assert_eq!(c.summary.changed, 2);
    // and its bytes are not marked: only `len` and `name` are
    assert!(
        !c.a.bytes.iter().any(|(first, _)| *first >= 3),
        "{:?}",
        c.a.bytes
    );
    assert!(
        !c.b.bytes.iter().any(|(first, _)| *first >= 4),
        "{:?}",
        c.b.bytes
    );
}

#[test]
fn a_changed_length_of_a_list_is_a_difference_and_what_only_one_file_has_is_said() {
    let a = [1, 1, 10, 11, 7, 0];
    let b = [1, 2, 10, 11, 12, 13, 7, 0];
    let (c, ta, tb) = run(SCHEMA, &a, &b);
    let marks_a = marks(&c.a, &ta);
    // the list itself changed (its length), and so did `count`
    assert_eq!(marks_a.get("items"), Some(&Mark::Changed), "{marks_a:?}");
    assert_eq!(marks_a.get("count"), Some(&Mark::Changed));
    // the element only B has is a ghost under the list of A, and is `only` in B's tree
    let list = ta.children.iter().position(|n| n.name == "items").unwrap();
    let ghosts = c.ghosts.get(&vec![list]).expect("a ghost under items");
    assert_eq!(ghosts.len(), 1);
    assert_eq!(ghosts[0].name, "[1]");
    assert_eq!(path_of(&tb, &ghosts[0].b_path), "items[1]");
    let marks_b = marks(&c.b, &tb);
    assert_eq!(marks_b.get("items[1]"), Some(&Mark::Only));
    assert_eq!(c.summary.only_b, 1);
    assert_eq!(c.summary.only_a, 0);
    // and the other way round: A has the element and B has not
    let (c, ta, _) = run(SCHEMA, &b, &a);
    assert_eq!(marks(&c.a, &ta).get("items[1]"), Some(&Mark::Only));
    assert_eq!(c.summary.only_a, 1);
    assert!(c.ghosts.is_empty());
}

#[test]
fn a_field_that_only_one_file_has_is_only_in_that_file() {
    let yaml = "meta:\n  id: c\n  endian: le\nseq:\n  - id: kind\n    type: u1\n  - id: extra\n    type: u2\n    if: kind == 2\n  - id: tail\n    type: u1\n";
    let (c, ta, tb) = run(yaml, &[1, 9], &[2, 5, 0, 9]);
    let marks_a = marks(&c.a, &ta);
    // `tail` moved (and is the same); `kind` changed; `extra` is only in B
    assert_eq!(marks_a.get("kind"), Some(&Mark::Changed));
    assert_eq!(marks_a.get("tail"), None);
    assert_eq!(marks(&c.b, &tb).get("extra"), Some(&Mark::Only));
    assert_eq!(c.summary.only_b, 1);
    assert_eq!(c.ghosts.get(&Vec::new()).map(Vec::len), Some(1));
}

#[test]
fn a_case_of_a_switch_that_is_another_type_is_a_change_of_the_field() {
    let yaml = "meta:\n  id: u\n  endian: le\nseq:\n  - id: kind\n    type: u1\n  - id: body\n    type:\n      switch-on: kind\n      cases:\n        1: one\n        2: two\ntypes:\n  one:\n    seq:\n      - id: x\n        type: u1\n  two:\n    seq:\n      - id: x\n        type: u1\n";
    let (c, ta, _) = run(yaml, &[1, 5], &[2, 5]);
    let marks_a = marks(&c.a, &ta);
    assert_eq!(marks_a.get("body"), Some(&Mark::Changed), "{marks_a:?}");
    // the field inside has the same name and value: it is not a difference
    assert_eq!(marks_a.get("body.x"), None);
}

#[test]
fn a_parse_that_failed_in_one_file_is_compared_as_far_as_it_was_read() {
    let a = [1, 2, 10, 11, 12, 13, 7, 0];
    // the second file stops inside the second item
    let b = [1, 2, 10, 11, 12];
    let (c, ta, _) = run(SCHEMA, &a, &b);
    let marks_a = marks(&c.a, &ta);
    // the first item is the same; the second differs where the other file stopped
    assert_eq!(marks_a.get("items[0].a"), None);
    assert!(
        marks_a.contains_key("items[1]") || marks_a.contains_key("items[1].b"),
        "{marks_a:?}"
    );
    assert!(c.summary.differences() >= 1);
}

#[test]
fn bytes_that_differ_are_marked_by_run_and_runs_are_joined() {
    let yaml = "meta:\n  id: b\nseq:\n  - id: pad\n    type: u1\n  - id: blob\n    size: 10\n";
    let a: Vec<u8> = (0..11).collect();
    let mut b = a.clone();
    // bytes 3 and 4 and 8 of the file differ
    b[3] = 0xaa;
    b[4] = 0xbb;
    b[8] = 0xcc;
    let (c, _, _) = run(yaml, &a, &b);
    assert_eq!(c.a.bytes, vec![(3, 5), (8, 9)]);
    assert_eq!(c.b.bytes, vec![(3, 5), (8, 9)]);
    // fields of different lengths: the whole of each span
    let yaml = "meta:\n  id: d\nseq:\n  - id: len\n    type: u1\n  - id: body\n    size: len\n";
    let (c, _, _) = run(yaml, &[2, 9, 9], &[3, 9, 9, 9]);
    // `len` differs and `body` has another length: the whole of each span, joined
    assert_eq!(c.a.bytes, vec![(0, 3)]);
    assert_eq!(c.b.bytes, vec![(0, 4)]);
}

#[test]
fn a_long_value_is_compared_by_its_bytes_and_one_byte_makes_a_difference() {
    // longer than the bytes that a value keeps, so the value is only where the bytes are
    let yaml = "meta:\n  id: l\nseq:\n  - id: head\n    type: u1\n  - id: blob\n    size: 100000\n";
    let a = vec![7_u8; 100_001];
    let mut b = a.clone();
    let (c, _, _) = run(yaml, &a, &b);
    assert!(c.summary.identical(), "{:?}", c.summary);
    b[70_000] = 8;
    let (c, ta, _) = run(yaml, &a, &b);
    assert_eq!(marks(&c.a, &ta).get("blob"), Some(&Mark::Changed));
    assert_eq!(c.a.bytes, vec![(70_000, 70_001)]);
    // a file that is too short for the value: the parse stops there, and that is a difference
    let (c, _, _) = run(yaml, &a, &a[..90_000]);
    assert!(!c.summary.identical());
}

#[test]
fn two_versions_of_a_library_sample_show_the_entry_that_changed() {
    let types = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../types");
    let yaml = std::fs::read_to_string(types.join("tar.yaml")).unwrap();
    let a = std::fs::read(types.join("samples/sample.tar")).unwrap();
    // a byte of the text of an entry
    let at = a
        .windows(5)
        .position(|w| w == b"hello")
        .expect("the sample has an entry that says hello");
    let mut b = a.clone();
    b[at] = b'J';
    let (c, ta, _) = run(&yaml, &a, &b);
    assert_eq!(c.summary.changed, 1, "{:?}", marks(&c.a, &ta));
    let changed: Vec<String> = marks(&c.a, &ta)
        .into_iter()
        .filter(|(_, mark)| *mark == Mark::Changed)
        .map(|(path, _)| path)
        .collect();
    assert_eq!(changed.len(), 1);
    assert!(changed[0].starts_with("entries["), "{changed:?}");
    assert_eq!(c.a.bytes, vec![(at as u64, at as u64 + 1)]);
    // the same file against itself says so
    let (c, _, _) = run(&yaml, &a, &a);
    assert!(c.summary.identical());
}
