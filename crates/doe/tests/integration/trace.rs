//! The record of where every value was read from.

use doe::parser::cursor::Cursor;
use doe::parser::engine::Engine;
use doe::parser::trace::TraceNode;
use doe::schema::compiler::compile_schema;
use doe::schema::raw::RawSchema;
use indoc::indoc;

fn trace(yaml: &str, data: &[u8]) -> TraceNode {
    let schema = RawSchema::from_yaml_str(yaml).unwrap();
    let registry = compile_schema(&schema).unwrap();
    let engine = Engine::new(&registry);
    let (_, root) = engine
        .parse_type_traced(&schema.id, &mut Cursor::new(data))
        .unwrap();
    root
}

/// The span of a node in bits, as `(start, end)`.
fn bits(node: &TraceNode) -> Option<(u64, u64)> {
    node.span.map(|span| (span.start, span.end))
}

fn child<'a>(node: &'a TraceNode, name: &str) -> &'a TraceNode {
    node.children
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| {
            panic!(
                "no child {name} in {:?}",
                node.children.iter().map(|c| &c.name).collect::<Vec<_>>()
            )
        })
}

#[test]
fn each_field_has_the_bits_it_was_read_from() {
    let yaml = indoc! {"
        meta:
          id: t
          endian: le
        seq:
          - id: a
            type: u1
          - id: b
            type: u2
          - id: name
            type: str
            size: 3
            encoding: ASCII
    "};
    let root = trace(yaml, &[1, 2, 3, b'a', b'b', b'c']);
    assert_eq!(root.name, "t");
    assert_eq!(bits(&root), Some((0, 48)));
    assert_eq!(bits(child(&root, "a")), Some((0, 8)));
    assert_eq!(bits(child(&root, "b")), Some((8, 24)));
    assert_eq!(bits(child(&root, "name")), Some((24, 48)));
    assert_eq!(child(&root, "b").span.unwrap().start_byte(), 1);
    assert_eq!(child(&root, "b").span.unwrap().end_byte(), 3);
}

#[test]
fn bit_fields_have_bit_ranges() {
    let yaml = indoc! {"
        meta:
          id: t
        seq:
          - id: hi
            type: b4
          - id: lo
            type: b4
          - id: wide
            type: b12
          - id: after
            type: u1
    "};
    let root = trace(yaml, &[0xAB, 0xCD, 0xEF, 0x01]);
    assert_eq!(bits(child(&root, "hi")), Some((0, 4)));
    assert_eq!(bits(child(&root, "lo")), Some((4, 8)));
    assert_eq!(bits(child(&root, "wide")), Some((8, 20)));
    // A byte field starts on the next byte.
    assert_eq!(bits(child(&root, "after")), Some((24, 32)));
    assert!(child(&root, "hi").span.unwrap().contains_byte(0));
}

#[test]
fn types_and_elements_are_nodes_and_absent_fields_are_not() {
    let yaml = indoc! {"
        meta:
          id: t
        seq:
          - id: n
            type: u1
          - id: items
            type: item
            repeat: expr
            repeat-expr: n
          - id: skipped
            type: u1
            if: n == 0
        types:
          item:
            seq:
              - id: x
                type: u2le
    "};
    let root = trace(yaml, &[2, 1, 0, 2, 0]);
    let items = child(&root, "items");
    assert_eq!(bits(items), Some((8, 40)));
    assert_eq!(items.children.len(), 2);
    assert_eq!(items.children[0].name, "[0]");
    assert_eq!(items.children[0].type_name.as_deref(), Some("t::item"));
    assert_eq!(bits(&items.children[1]), Some((24, 40)));
    assert_eq!(bits(child(&items.children[1], "x")), Some((24, 40)));
    assert!(root.children.iter().all(|c| c.name != "skipped"));
}

#[test]
fn a_type_in_a_window_has_offsets_of_the_whole_input() {
    let yaml = indoc! {"
        meta:
          id: t
        seq:
          - id: skip
            size: 4
          - id: body
            type: body
            size: 3
        types:
          body:
            seq:
              - id: a
                type: u1
              - id: b
                type: u1
    "};
    let root = trace(yaml, &[0, 0, 0, 0, 7, 8, 9]);
    let body = child(&root, "body");
    assert_eq!(bits(body), Some((32, 56)));
    assert_eq!(bits(child(body, "a")), Some((32, 40)));
    assert_eq!(bits(child(body, "b")), Some((40, 48)));
}

#[test]
fn a_field_placed_with_pos_has_the_place_it_was_read_from() {
    let yaml = indoc! {"
        meta:
          id: t
        seq:
          - id: offset
            type: u1
          - id: table
            type: u2le
            pos: offset
          - id: trailer
            type: u1
    "};
    let root = trace(yaml, &[4, 9, 9, 9, 0x34, 0x12]);
    assert_eq!(bits(child(&root, "table")), Some((32, 48)));
    // The sequence went on where it was.
    assert_eq!(bits(child(&root, "trailer")), Some((8, 16)));
}

#[test]
fn instances_follow_the_fields_and_say_where_they_were_read() {
    let yaml = indoc! {"
        meta:
          id: t
        seq:
          - id: offset
            type: u1
          - id: last
            type: u1
        instances:
          tail:
            pos: offset
            type: u1
          double:
            value: last * 2
    "};
    let root = trace(yaml, &[3, 5, 0, 77]);
    let names: Vec<&str> = root.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["offset", "last", "tail", "double"]);
    let tail = child(&root, "tail");
    assert!(tail.is_instance);
    assert_eq!(bits(tail), Some((24, 32)));
    // An expression is not in the input.
    assert_eq!(child(&root, "double").span, None);
    assert!(child(&root, "double").is_instance);
}

#[test]
fn an_instance_a_field_uses_is_a_child_of_its_type_not_of_the_field() {
    let yaml = indoc! {"
        meta:
          id: t
        seq:
          - id: a
            type: u1
          - id: body
            size: size
        instances:
          size:
            pos: 0
            type: u1
    "};
    let root = trace(yaml, &[2, 8, 8]);
    assert_eq!(child(&root, "body").children.len(), 0);
    assert_eq!(bits(child(&root, "size")), Some((0, 8)));
}

#[test]
fn an_instance_that_waits_is_filled_in_when_it_is_computed() {
    let yaml = indoc! {"
        meta:
          id: t
        seq:
          - id: header
            type: header
          - id: item
            type: item
        types:
          header:
            seq:
              - id: n
                type: u1
            instances:
              late:
                pos: _root.item.x
                type: u1
          item:
            seq:
              - id: x
                type: u1
    "};
    let root = trace(yaml, &[1, 2, 9]);
    let late = child(child(&root, "header"), "late");
    assert_eq!(bits(late), Some((16, 24)));
}

#[test]
fn what_is_read_from_a_transformed_field_is_marked() {
    let yaml = indoc! {"
        meta:
          id: t
        seq:
          - id: secret
            type: inner
            size: 2
            process: xor(0xff)
        types:
          inner:
            seq:
              - id: a
                type: u1
              - id: b
                type: u1
    "};
    let root = trace(yaml, &[0xfe, 0xfd]);
    let secret = child(&root, "secret");
    // The field is where its bytes are in the input ...
    assert_eq!(bits(secret), Some((0, 16)));
    assert!(!secret.processed);
    // ... what is read from them is in the transformed bytes.
    let a = child(secret, "a");
    assert!(a.processed);
    assert_eq!(bits(a), Some((0, 8)));
    // So a byte of the input is found in the field and not in `a`.
    assert_eq!(root.find_byte(1), Some(vec![0]));
}

#[test]
fn find_byte_gives_the_path_to_the_innermost_field() {
    let yaml = indoc! {"
        meta:
          id: t
        seq:
          - id: head
            type: u1
          - id: body
            type: body
        types:
          body:
            seq:
              - id: a
                type: u1
              - id: b
                type: u2le
    "};
    let root = trace(yaml, &[1, 2, 3, 4]);
    assert_eq!(root.find_byte(0), Some(vec![0]));
    assert_eq!(root.find_byte(1), Some(vec![1, 0]));
    assert_eq!(root.find_byte(3), Some(vec![1, 1]));
    assert_eq!(root.find_byte(4), None);
}

#[test]
fn a_parse_without_a_trace_is_the_same() {
    let yaml = indoc! {"
        meta:
          id: t
        seq:
          - id: a
            type: u1
          - id: list
            type: u1
            repeat: eos
    "};
    let schema = RawSchema::from_yaml_str(yaml).unwrap();
    let registry = compile_schema(&schema).unwrap();
    let engine = Engine::new(&registry);
    let data = [1u8, 2, 3];
    let plain = engine.parse_type("t", &mut Cursor::new(&data)).unwrap();
    let (traced, _) = engine
        .parse_type_traced("t", &mut Cursor::new(&data))
        .unwrap();
    assert_eq!(plain, traced);
}
