//! The differences of a comparison as a list (#445): one per changed field, one per field or element that only a file has,
//! in the order of the first file's tree, with the bytes that differ in each.

use doe::compare::{DiffKind, DiffStatus, Difference, Input, Keys, compare_with, differences};
use doe::parser::cursor::Cursor;
use doe::parser::engine::Engine;
use doe::schema::compiler::compile_schema;
use doe::schema::raw::RawSchema;

fn list(yaml: &str, a: &[u8], b: &[u8], keys: &Keys) -> Vec<Difference> {
    let schema = RawSchema::from_yaml_str(yaml).unwrap();
    let registry = compile_schema(&schema).unwrap();
    let parse = |data: &[u8]| {
        Engine::new(&registry)
            .parse_type_partial(&schema.id, &mut Cursor::new(data))
            .unwrap()
    };
    let (oa, ob) = (parse(a), parse(b));
    let read_a = |offset: u64, len: usize| {
        let start = usize::try_from(offset).ok()?;
        a.get(start..start + len).map(<[u8]>::to_vec)
    };
    let read_b = |offset: u64, len: usize| {
        let start = usize::try_from(offset).ok()?;
        b.get(start..start + len).map(<[u8]>::to_vec)
    };
    let input_a = Input {
        trace: &oa.trace,
        value: oa.value.as_ref(),
        read: &read_a,
    };
    let input_b = Input {
        trace: &ob.trace,
        value: ob.value.as_ref(),
        read: &read_b,
    };
    let comparison = compare_with(&input_a, &input_b, keys);
    differences(&input_a, &input_b, &comparison)
}

fn show(found: &[Difference]) -> Vec<String> {
    found
        .iter()
        .map(|d| {
            format!(
                "{:?} {:?} {} {:?} {:?} {:?}",
                d.status, d.kind, d.path, d.a, d.b, d.fields
            )
        })
        .collect()
}

const BYTES: &str = "meta:\n  id: t\nseq:\n  - id: head\n    type: u1\n  - id: items\n    type: u1\n    repeat: eos\n";

#[test]
fn a_changed_leaf_has_both_values_and_the_bytes_that_differ() {
    let found = list(BYTES, &[1, 5, 6], &[1, 5, 9], &Keys::new());
    assert_eq!(found.len(), 1, "{found:?}");
    let d = &found[0];
    assert_eq!(
        (d.path.as_str(), d.status, d.kind),
        ("items[1]", DiffStatus::Changed, DiffKind::Value)
    );
    assert_eq!((d.a.as_deref(), d.b.as_deref()), (Some("6"), Some("9")));
    assert_eq!(
        (d.a_bytes.clone(), d.b_bytes.clone()),
        (vec![(2, 3)], vec![(2, 3)])
    );
}

#[test]
fn an_element_that_only_one_file_has_is_one_line_with_its_value_and_a_length_is_one() {
    let found = list(BYTES, &[1, 5, 6, 7], &[1, 5], &Keys::new());
    assert_eq!(
        show(&found),
        [
            "Changed Length items Some(\"3\") Some(\"1\") None",
            "OnlyA Value items[1] Some(\"6\") None None",
            "OnlyA Value items[2] Some(\"7\") None None",
        ]
    );
    // the other way round the elements are B's
    let found = list(BYTES, &[1, 5], &[1, 5, 6], &Keys::new());
    assert_eq!(
        found.last().map(|d| (d.status, d.path.as_str())),
        Some((DiffStatus::OnlyB, "items[1]"))
    );
    assert_eq!(found.last().and_then(|d| d.b.clone()).as_deref(), Some("6"));
    assert_eq!(found.last().map(|d| d.b_bytes.clone()), Some(vec![(2, 3)]));
}

const SWITCH: &str = "meta:\n  id: t\n  endian: le\nseq:\n  - id: kind\n    type: u1\n  - id: body\n    type:\n      switch-on: kind\n      cases:\n        1: small\n        2: big\ntypes:\n  small:\n    seq:\n      - id: x\n        type: u1\n  big:\n    seq:\n      - id: x\n        type: u1\n      - id: y\n        type: u1\n";

#[test]
fn a_field_whose_type_changed_says_which_types() {
    let found = list(SWITCH, &[1, 7], &[2, 7, 8], &Keys::new());
    let body = found.iter().find(|d| d.path == "body").unwrap();
    assert_eq!(body.kind, DiffKind::Type);
    assert_eq!(
        (body.a.as_deref(), body.b.as_deref()),
        (Some("t::small"), Some("t::big"))
    );
    // and what only the second type has is a field of its own
    assert!(
        found
            .iter()
            .any(|d| d.status == DiffStatus::OnlyB && d.path == "body.y"),
        "{found:?}"
    );
}

#[test]
fn a_long_value_is_cut() {
    let yaml =
        "meta:\n  id: t\nseq:\n  - id: text\n    type: str\n    size: 100\n    encoding: ASCII\n";
    let long = vec![b'a'; 100];
    let mut other = long.clone();
    other[0] = b'b';
    let found = list(yaml, &long, &other, &Keys::new());
    let a = found[0].a.clone().unwrap();
    assert!(
        a.ends_with('…') && a.chars().count() == doe::compare::VALUE_WIDTH + 1,
        "{a}"
    );
}

const TABLE: &str = "meta:\n  id: t\nseq:\n  - id: records\n    type: record\n    repeat: eos\ntypes:\n  record:\n    seq:\n      - id: id\n        type: u1\n      - id: value\n        type: u1\n";

#[test]
fn elements_matched_by_a_key_are_listed_at_their_own_paths() {
    let mut keys = Keys::new();
    keys.set("records", "id");
    // the second record moved to the front and its value changed
    let found = list(TABLE, &[1, 10, 2, 20], &[2, 21, 1, 10], &keys);
    assert_eq!(
        show(&found),
        ["Changed Value records[1].value Some(\"20\") Some(\"21\") None"]
    );
    // the bytes are each file's own
    assert_eq!(
        (found[0].a_bytes.clone(), found[0].b_bytes.clone()),
        (vec![(3, 4)], vec![(1, 2)])
    );
}

#[test]
fn identical_files_have_no_differences() {
    assert_eq!(
        list(TABLE, &[1, 10], &[1, 10], &Keys::new()),
        Vec::<Difference>::new()
    );
}
