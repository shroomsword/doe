//! Matching the elements of a list by a key field (#444), not by index: an inserted, removed or reordered element is one
//! difference (or none), and a key that cannot be used falls back to the index and says why.

use std::collections::BTreeMap;

use doe::compare::{Comparison, Input, Keys, Mark, compare_with, key_candidates, lists};
use doe::copy::path_of;
use doe::parser::cursor::Cursor;
use doe::parser::engine::Engine;
use doe::parser::trace::TraceNode;
use doe::schema::compiler::compile_schema;
use doe::schema::raw::RawSchema;
use doe::value::Value;

/// A table of records, up to the end of the file: each has an id, a name of one byte and a value.
fn schema(extra: &str) -> String {
    format!(
        "meta:\n  id: t\n  endian: le\nseq:\n  - id: records\n    type: record\n    repeat: eos\n{extra}types:\n  record:\n    seq:\n      - id: id\n        type: u1\n      - id: name\n        type: str\n        size: 1\n        encoding: ASCII\n      - id: value\n        type: u1\n"
    )
}

const KEYED: &str = "    compare-by: id\n";

/// Records of `(id, name, value)`.
fn file(records: &[(u8, char, u8)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (id, name, value) in records {
        out.extend([*id, u8::try_from(*name).unwrap(), *value]);
    }
    out
}

fn parse(yaml: &str, data: &[u8]) -> (TraceNode, Option<Value>) {
    let schema = RawSchema::from_yaml_str(yaml).unwrap();
    let registry = compile_schema(&schema).unwrap();
    let outcome = Engine::new(&registry)
        .parse_type_partial(&schema.id, &mut Cursor::new(data))
        .unwrap();
    (outcome.trace, outcome.value)
}

fn run(yaml: &str, a: &[u8], b: &[u8], keys: &Keys) -> (Comparison, TraceNode, TraceNode) {
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
    let comparison = compare_with(
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
        keys,
    );
    (comparison, ta, tb)
}

fn marks(side: &doe::compare::Side, root: &TraceNode) -> BTreeMap<String, Mark> {
    side.marks
        .iter()
        .map(|(path, mark)| (path_of(root, path), *mark))
        .collect()
}

const ABC: [(u8, char, u8); 3] = [(1, 'a', 10), (2, 'b', 20), (3, 'c', 30)];

#[test]
fn an_element_inserted_at_the_front_is_one_difference_not_one_for_every_later_one() {
    let a = file(&ABC);
    let b = file(&[(9, 'z', 90), (1, 'a', 10), (2, 'b', 20), (3, 'c', 30)]);
    // by index, every record from the first differs
    let (by_index, _, _) = run(&schema(""), &a, &b, &Keys::new());
    assert!(by_index.summary.differences() > 3, "{:?}", by_index.summary);
    // by the key of the schema, the one that is only in B
    let (c, ta, tb) = run(&schema(KEYED), &a, &b, &Keys::new());
    assert_eq!(c.summary.only_b, 1);
    assert_eq!((c.summary.only_a, c.summary.changed), (0, 0));
    assert_eq!(c.summary.differences(), 1, "{:?}", c.summary);
    // the list is not changed for its length: something below it differs
    assert_eq!(marks(&c.a, &ta).get("records"), Some(&Mark::Inside));
    assert_eq!(marks(&c.b, &tb).get("records[0]"), Some(&Mark::Only));
    let ghosts: Vec<_> = c
        .ghosts
        .values()
        .flatten()
        .map(|g| g.name.as_str())
        .collect();
    assert_eq!(ghosts, ["[0]"]);
    // the lists say what they were matched by
    assert_eq!(c.a.keyed.values().collect::<Vec<_>>(), ["id"]);
    assert_eq!(c.b.keyed.values().collect::<Vec<_>>(), ["id"]);
    assert_eq!(c.notes, Vec::<String>::new());
}

#[test]
fn a_removed_element_is_only_in_a_and_the_rest_are_the_same() {
    let a = file(&ABC);
    let b = file(&[(1, 'a', 10), (3, 'c', 30)]);
    let (c, ta, _) = run(&schema(KEYED), &a, &b, &Keys::new());
    assert_eq!(
        (c.summary.only_a, c.summary.only_b, c.summary.changed),
        (1, 0, 0)
    );
    assert_eq!(marks(&c.a, &ta).get("records[1]"), Some(&Mark::Only));
}

#[test]
fn a_reordered_list_of_equal_elements_is_identical() {
    let a = file(&ABC);
    let b = file(&[(3, 'c', 30), (1, 'a', 10), (2, 'b', 20)]);
    let (c, _, _) = run(&schema(KEYED), &a, &b, &Keys::new());
    assert!(c.summary.identical(), "{:?}", c.summary);
    assert!(c.a.marks.is_empty() && c.b.marks.is_empty());
}

#[test]
fn an_element_that_moved_and_changed_is_one_change() {
    let a = file(&ABC);
    let b = file(&[(3, 'c', 30), (2, 'b', 99), (1, 'a', 10)]);
    let (c, ta, tb) = run(&schema(KEYED), &a, &b, &Keys::new());
    assert_eq!(c.summary.changed, 1);
    assert_eq!((c.summary.only_a, c.summary.only_b), (0, 0));
    // in A it is records[1].value, in B records[1].value too here; the partner is by key, not by place
    assert_eq!(
        marks(&c.a, &ta).get("records[1].value"),
        Some(&Mark::Changed)
    );
    assert_eq!(
        marks(&c.b, &tb).get("records[1].value"),
        Some(&Mark::Changed)
    );
    // and a moved one whose partner is at another index
    let b = file(&[(2, 'b', 20), (3, 'c', 77), (1, 'a', 10)]);
    let (c, ta, tb) = run(&schema(KEYED), &a, &b, &Keys::new());
    assert_eq!(
        marks(&c.a, &ta).get("records[2].value"),
        Some(&Mark::Changed)
    );
    assert_eq!(
        marks(&c.b, &tb).get("records[1].value"),
        Some(&Mark::Changed)
    );
    assert_eq!(c.summary.differences(), 1);
}

#[test]
fn a_key_that_a_viewer_chose_beats_the_schemas_and_names_the_list() {
    let a = file(&[(1, 'a', 10), (2, 'b', 20)]);
    let b = file(&[(1, 'b', 20), (2, 'a', 10)]);
    // by the id of the schema, every record changed its name and value
    let (c, _, _) = run(&schema(KEYED), &a, &b, &Keys::new());
    assert!(c.summary.changed > 0);
    // by the name, they are the same elements with other ids
    let mut keys = Keys::new();
    keys.set("records", "name");
    let (c, _, _) = run(&schema(KEYED), &a, &b, &keys);
    assert_eq!(c.summary.changed, 2, "only the ids differ: {:?}", c.summary);
    assert_eq!(c.a.keyed.values().collect::<Vec<_>>(), ["name"]);
    // clearing it goes back to the schema's
    keys.clear("records");
    assert_eq!(keys.get("records"), None);
}

#[test]
fn a_key_that_repeats_falls_back_to_the_index_and_says_so() {
    let a = file(&[(1, 'a', 10), (1, 'b', 20)]);
    let b = file(&[(1, 'a', 10), (1, 'b', 21)]);
    let (c, ta, _) = run(&schema(KEYED), &a, &b, &Keys::new());
    assert!(c.a.keyed.is_empty());
    assert_eq!(c.notes.len(), 1, "{:?}", c.notes);
    assert!(
        c.notes[0].contains("records")
            && c.notes[0].contains("by index")
            && c.notes[0].contains("more than one element"),
        "{:?}",
        c.notes
    );
    // matched by index: the value of the second differs
    assert_eq!(
        marks(&c.a, &ta).get("records[1].value"),
        Some(&Mark::Changed)
    );
}

#[test]
fn a_field_that_the_elements_do_not_have_falls_back_with_a_note() {
    let a = file(&ABC);
    let mut keys = Keys::new();
    keys.set("records", "nothing");
    let (c, _, _) = run(&schema(""), &a, &a, &keys);
    assert!(c.summary.identical());
    assert!(c.notes[0].contains("no field nothing"), "{:?}", c.notes);
}

#[test]
fn a_key_that_is_not_a_number_a_string_or_bytes_is_not_used() {
    // the key would be a struct
    let yaml = "meta:\n  id: t\n  endian: le\nseq:\n  - id: items\n    type: item\n    repeat: expr\n    repeat-expr: 2\ntypes:\n  item:\n    seq:\n      - id: inner\n        type: pair\n  pair:\n    seq:\n      - id: x\n        type: u1\n";
    let data = [1, 2];
    let mut keys = Keys::new();
    keys.set("items", "inner");
    let (c, _, _) = run(yaml, &data, &data, &keys);
    assert!(
        c.notes[0].contains("not a number, a string or bytes"),
        "{:?}",
        c.notes
    );
}

#[test]
fn a_number_and_a_string_with_the_same_digits_are_different_keys() {
    // ids 1 and 2 as numbers in A; as strings in B: nothing pairs, so everything is only in one file
    let a_yaml = "meta:\n  id: t\nseq:\n  - id: items\n    type: item\n    repeat: expr\n    repeat-expr: 2\n    compare-by: k\ntypes:\n  item:\n    seq:\n      - id: k\n        type: u1\n";
    let (c, _, _) = run(a_yaml, &[1, 2], &[2, 1], &Keys::new());
    assert!(
        c.summary.identical(),
        "numbers pair by value: {:?}",
        c.summary
    );
}

#[test]
fn nested_lists_have_their_own_keys_by_name() {
    let yaml = "meta:\n  id: t\n  endian: le\nseq:\n  - id: groups\n    type: group\n    repeat: expr\n    repeat-expr: 2\ntypes:\n  group:\n    seq:\n      - id: gid\n        type: u1\n      - id: members\n        type: member\n        repeat: expr\n        repeat-expr: 2\n  member:\n    seq:\n      - id: mid\n        type: u1\n      - id: v\n        type: u1\n";
    // groups matched by index; the members of each by mid
    let a = [1, 10, 1, 11, 2, 2, 20, 1, 21, 2];
    let b = [1, 11, 2, 10, 1, 2, 20, 1, 21, 2];
    let mut keys = Keys::new();
    keys.set("groups.members", "mid");
    let (c, _, _) = run(yaml, &a, &b, &keys);
    // in the first group the two members swapped places and kept their values: not different
    assert!(c.summary.identical(), "{:?} {:?}", c.summary, c.notes);
    assert_eq!(
        c.a.keyed.len(),
        2,
        "both lists of members were matched by mid"
    );
}

#[test]
fn the_fields_that_could_match_are_the_ones_that_are_all_different() {
    let (_, value) = parse(&schema(""), &file(&ABC));
    let list = match value.as_ref().unwrap() {
        Value::Struct { fields, .. } => fields.iter().find(|(n, _)| n == "records").map(|(_, v)| v),
        _ => None,
    };
    // id and name are all different; the value is too (10, 20, 30)
    assert_eq!(key_candidates(list), ["id", "name", "value"]);
    let (_, value) = parse(&schema(""), &file(&[(1, 'a', 10), (2, 'a', 10)]));
    let list = match value.as_ref().unwrap() {
        Value::Struct { fields, .. } => fields.iter().find(|(n, _)| n == "records").map(|(_, v)| v),
        _ => None,
    };
    assert_eq!(key_candidates(list), ["id"]);
    assert_eq!(key_candidates(None), Vec::<String>::new());
}

#[test]
fn the_lists_of_a_parse_are_named_with_their_fields_that_could_match_and_their_schema_key() {
    let data = file(&ABC);
    let (trace, value) = parse(&schema(KEYED), &data);
    let read = |_: u64, _: usize| None;
    let found = lists(&Input {
        trace: &trace,
        value: value.as_ref(),
        read: &read,
    });
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "records");
    assert_eq!(found[0].len, 3);
    assert_eq!(found[0].schema_key.as_deref(), Some("id"));
    assert_eq!(found[0].candidates, ["id", "name", "value"]);
}

#[test]
fn compare_by_is_checked_when_the_schema_is_compiled() {
    let compile = |text: &str| {
        let schema = RawSchema::from_yaml_str(text).unwrap();
        compile_schema(&schema)
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    // a field that is not repeated
    let once = "meta:\n  id: t\nseq:\n  - id: one\n    type: item\n    compare-by: k\ntypes:\n  item:\n    seq:\n      - id: k\n        type: u1\n";
    assert!(compile(once).unwrap_err().contains("repeated field"));
    // a field that the elements do not have
    let missing = schema("    compare-by: nothing\n");
    assert!(
        compile(&missing)
            .unwrap_err()
            .contains("has no field nothing")
    );
    // a field that is not a number, a string or bytes
    let weird = "meta:\n  id: t\nseq:\n  - id: items\n    type: item\n    repeat: expr\n    repeat-expr: 2\n    compare-by: p\ntypes:\n  item:\n    seq:\n      - id: p\n        type: pair\n  pair:\n    seq:\n      - id: x\n        type: u1\n";
    assert!(
        compile(weird)
            .unwrap_err()
            .contains("is not an integer, a string or bytes")
    );
    // a field that has no type of the schema
    let plain = "meta:\n  id: t\nseq:\n  - id: items\n    type: u1\n    repeat: expr\n    repeat-expr: 2\n    compare-by: k\n";
    assert!(
        compile(plain)
            .unwrap_err()
            .contains("no type of the schema")
    );
    // not the name of a field
    let odd = schema("    compare-by: \"a b\"\n");
    assert!(compile(&odd).unwrap_err().contains("not the name of one"));
    // and a good one compiles
    assert!(compile(&schema(KEYED)).is_ok());
}

#[test]
fn the_trace_of_the_list_carries_the_key_of_the_schema() {
    let (trace, _) = parse(&schema(KEYED), &file(&ABC));
    let list = trace.children.iter().find(|c| c.name == "records").unwrap();
    assert_eq!(list.compare_by.as_deref(), Some("id"));
    assert_eq!(trace.compare_by, None);
}
