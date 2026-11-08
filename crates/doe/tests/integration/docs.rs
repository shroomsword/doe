//! The descriptions (`doc:`) of fields and instances in the answer of a parse, for the viewers to show next to the fields.

use doe::doc::{doc_of, first_line};

const SCHEMA: &str = "meta:
  id: d
  endian: le
doc: The description of the whole type, which a field does not repeat.
seq:
  - id: count
    type: u1
    doc: |
      How many entries follow.

      A second paragraph that the row of the field does not show.
  - id: entries
    type: entry
    repeat: expr
    repeat-expr: count
    doc: The entries.
  - id: plain
    type: u1
instances:
  twice:
    value: count * 2
    doc: Twice the count (an instance).
  later:
    pos: 3
    type: u1
    doc: A byte read at a place.
types:
  entry:
    doc: One entry (the type's own description).
    seq:
      - id: kind
        type: u1
        doc: What the entry is.
      - id: unused_doc
        type: u1
  never_used:
    seq:
      - id: z
        type: u1
        doc: In a type that the parse does not enter.
";

fn parse(input: &[u8]) -> doe::api::ParseResponse {
    let request = serde_json::json!({ "schema": SCHEMA });
    let response = doe::api::parse(&request.to_string(), input);
    assert!(response.ok, "{:?}", response.error);
    response
}

#[test]
fn the_answer_has_the_descriptions_of_fields_and_instances_by_type_and_name() {
    let response = parse(&[2, 10, 11, 12, 13, 14]);
    let docs = &response.docs;
    assert_eq!(docs["d"]["entries"], "The entries.");
    assert_eq!(docs["d"]["twice"], "Twice the count (an instance).");
    assert_eq!(docs["d"]["later"], "A byte read at a place.");
    // a block scalar is kept whole, without its closing line end
    assert!(docs["d"]["count"].starts_with("How many entries follow."));
    assert!(docs["d"]["count"].contains("A second paragraph"));
    assert!(!docs["d"]["count"].ends_with('\n'));
    // the type's name is the fully-qualified one the trace has
    assert_eq!(docs["d::entry"]["kind"], "What the entry is.");
    // a field without a description is not in it, and neither is the description of a type or a type the parse did not enter
    assert!(!docs["d"].contains_key("plain"));
    assert!(!docs["d::entry"].contains_key("unused_doc"));
    assert!(!docs.contains_key("d::never_used"));
    assert!(docs["d"].values().all(|doc| !doc.contains("whole type")));
}

#[test]
fn a_node_has_the_description_of_its_name_in_the_table_of_its_parents_type() {
    let response = parse(&[2, 10, 11, 12, 13, 14]);
    let trace = response.trace.expect("a trace");
    // the root has the type of the schema; its children are fields of it
    assert_eq!(trace.type_name.as_deref(), Some("d"));
    let named = |name: &str| trace.children.iter().find(|c| c.name == name).unwrap();
    assert_eq!(
        doc_of(
            &response.docs,
            trace.type_name.as_deref(),
            &named("entries").name
        ),
        Some("The entries.")
    );
    // an element is named [0]: it has no description of its own (the field has it, once)
    let entries = named("entries");
    let element = &entries.children[0];
    assert_eq!(element.name, "[0]");
    assert_eq!(
        doc_of(&response.docs, entries.type_name.as_deref(), &element.name),
        None
    );
    // the fields of the element are looked up in the type of the element
    assert_eq!(element.type_name.as_deref(), Some("d::entry"));
    assert_eq!(
        doc_of(&response.docs, element.type_name.as_deref(), "kind"),
        Some("What the entry is.")
    );
    assert_eq!(
        doc_of(&response.docs, element.type_name.as_deref(), "unused_doc"),
        None
    );
    assert_eq!(doc_of(&response.docs, None, "kind"), None);
}

#[test]
fn the_row_shows_the_first_line() {
    assert_eq!(
        first_line("How many entries follow.\n\nMore."),
        "How many entries follow."
    );
    assert_eq!(first_line("  one line  "), "one line");
    assert_eq!(first_line(""), "");
}

#[test]
fn a_schema_with_no_descriptions_has_an_empty_table_and_the_table_serialises_as_an_object() {
    let request =
        serde_json::json!({ "schema": "meta:\n  id: n\nseq:\n  - id: a\n    type: u1\n" });
    let response = doe::api::parse(&request.to_string(), &[1]);
    assert!(response.docs.is_empty());
    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json["docs"], serde_json::json!({}));
    let with = serde_json::to_value(parse(&[0, 9, 1, 2])).unwrap();
    assert_eq!(with["docs"]["d"]["entries"], "The entries.");
}

#[test]
fn the_library_types_have_descriptions_to_show() {
    // the types of the library are documented field by field: a PNG is the first look at the result
    let types = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../types");
    let schema = std::fs::read_to_string(types.join("zip_archive.yaml")).unwrap();
    let bytes = std::fs::read(types.join("samples/sample.zip")).unwrap();
    let request = serde_json::json!({ "schema": schema, "path": "zip_archive.yaml" });
    let response = doe::api::parse(&request.to_string(), &bytes);
    assert!(response.ok, "{:?}", response.error);
    let docs = &response.docs;
    assert!(
        docs.values()
            .map(std::collections::BTreeMap::len)
            .sum::<usize>()
            > 5,
        "{docs:#?}"
    );
}

#[test]
fn the_table_of_a_million_nodes_costs_one_walk_and_a_few_strings() {
    use doe::doc::field_docs;
    use doe::parser::trace::TraceNode;
    use doe::schema::ir::TypeRegistry;
    let mut root = TraceNode::named("big");
    root.type_name = Some("big".to_owned());
    for i in 0..1_000_000_u32 {
        let mut item = TraceNode::named(&format!("[{i}]"));
        item.type_name = Some(if i % 2 == 0 { "big::a" } else { "big::b" }.to_owned());
        root.children.push(item);
    }
    let start = std::time::Instant::now();
    let docs = field_docs(&TypeRegistry::new(), &root);
    let took = start.elapsed();
    // the registry has none of the types: nothing to say, and the walk is the whole cost
    assert!(docs.is_empty());
    assert!(took < std::time::Duration::from_secs(2), "{took:?}");
}
