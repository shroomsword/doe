//! The JSON of the calls, as the page sees it.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use serde_json::{Value, json};

/// Call `check` the way the page does and read the answer back.
fn check(request: &Value) -> Value {
    serde_json::from_str(&doe_wasm::check(&request.to_string())).unwrap()
}

fn parse(request: &Value, bytes: &[u8]) -> Value {
    serde_json::from_str(&doe_wasm::parse(&request.to_string(), bytes)).unwrap()
}

const CHUNKS: &str = "meta:
  id: chunks
  endian: le
seq:
  - id: count
    type: u1
  - id: items
    type: item
    repeat: expr
    repeat-expr: count
types:
  item:
    seq:
      - id: kind
        type: u1
        enum: kinds
      - id: len
        type: u1
      - id: body
        size: len
    enums:
      kinds:
        1: text
";

#[test]
fn a_schema_that_compiles_is_ok() {
    let answer = check(&json!({ "schema": CHUNKS }));
    assert_eq!(
        answer,
        json!({ "ok": true, "id": "chunks", "diagnostics": [] })
    );
}

#[test]
fn a_schema_with_a_mistake_says_where() {
    let schema = "id: t\nseq:\n  - id: n\n    type: u1\n  - id: body\n    size: n +\n";
    let answer = check(&json!({ "schema": schema }));
    assert_eq!(answer["ok"], false);
    assert_eq!(answer["id"], Value::Null);
    let diagnostic = &answer["diagnostics"][0];
    assert_eq!(diagnostic["severity"], "error");
    assert_eq!(diagnostic["kind"], "schema");
    let position = &diagnostic["position"];
    assert_eq!(
        (position["line"].as_u64(), position["column"].as_u64()),
        (Some(6), Some(11))
    );
    let offset = usize::try_from(position["offset"].as_u64().unwrap()).unwrap();
    let len = usize::try_from(position["len"].as_u64().unwrap()).unwrap();
    assert_eq!(schema.get(offset..offset + len), Some("n +"));
}

#[test]
fn a_schema_that_is_not_yaml_is_a_yaml_diagnostic() {
    let answer = check(&json!({ "schema": "id: t\nseq: [unclosed\n" }));
    assert_eq!(answer["diagnostics"][0]["kind"], "yaml");
    assert!(
        answer["diagnostics"][0]["position"]["line"]
            .as_u64()
            .is_some()
    );
}

#[test]
fn a_request_that_is_not_valid_is_a_diagnostic_not_a_crash() {
    for request in ["", "not json", "{}", "{\"schema\": 3}"] {
        let answer: Value = serde_json::from_str(&doe_wasm::check(request)).unwrap();
        assert_eq!(answer["ok"], false, "{request}");
        assert_eq!(answer["diagnostics"][0]["kind"], "request", "{request}");
    }
    let answer: Value = serde_json::from_str(&doe_wasm::parse("nope", &[])).unwrap();
    assert_eq!(answer["ok"], false);
    assert_eq!(answer["diagnostics"][0]["kind"], "request");
}

#[test]
fn imports_come_from_the_files_of_the_request() {
    let main = "meta:\n  id: main\n  imports:\n    - common\n    - /lib/shared\nseq:\n  - id: c\n    type: common\n";
    let answer = check(&json!({
        "schema": main,
        "files": {
            "common.ksy": "meta:\n  id: common\nseq:\n  - id: x\n    type: u1\n",
            "inc/lib/shared.ksy": "meta:\n  id: shared\nseq: []\n",
        },
        "include_paths": ["inc"],
    }));
    assert_eq!(answer["ok"], true, "{answer}");
    // A file that is missing is found in the text of the import.
    let answer = check(&json!({ "schema": main, "files": {} }));
    assert_eq!(answer["ok"], false);
    let position = &answer["diagnostics"][0]["position"];
    assert_eq!(position["line"], 4);
}

#[test]
fn a_parse_gives_the_value_and_where_each_part_came_from() {
    // Two items: kind 1 with a 2-byte body, kind 9 (no name) with a 1-byte body.
    let bytes = [2, 1, 2, b'h', b'i', 9, 1, 7];
    let answer = parse(&json!({ "schema": CHUNKS }), &bytes);
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["id"], "chunks");
    assert_eq!(answer["error"], Value::Null);
    assert_eq!(answer["value"]["count"], 2);

    let trace = &answer["trace"];
    assert_eq!(trace["name"], "chunks");
    assert_eq!(
        (trace["start"].as_u64(), trace["end"].as_u64()),
        (Some(0), Some(64))
    );
    let count = &trace["children"][0];
    assert_eq!(count["name"], "count");
    assert_eq!(count["summary"], "2");
    assert_eq!(
        (count["start"].as_u64(), count["end"].as_u64()),
        (Some(0), Some(8))
    );

    let items = &trace["children"][1];
    assert_eq!(items["name"], "items");
    assert_eq!(items["summary"], Value::Null);
    let first = &items["children"][0];
    assert_eq!(first["name"], "[0]");
    assert_eq!(first["type"], "chunks::item");
    assert_eq!(
        (first["start"].as_u64(), first["end"].as_u64()),
        (Some(8), Some(40))
    );
    let kind = &first["children"][0];
    assert_eq!(kind["name"], "kind");
    assert_eq!(kind["summary"], "text (1)");
    let second_kind = &items["children"][1]["children"][0];
    assert_eq!(
        (second_kind["start"].as_u64(), second_kind["end"].as_u64()),
        (Some(40), Some(48))
    );
}

#[test]
fn a_parse_that_fails_keeps_what_was_read_and_marks_where() {
    // The second item says its body has 5 bytes and there is 1.
    let bytes = [2, 1, 1, b'x', 9, 5, 7];
    let answer = parse(&json!({ "schema": CHUNKS }), &bytes);
    assert_eq!(answer["ok"], false);
    assert_eq!(answer["diagnostics"], json!([]));
    assert_eq!(answer["error"]["kind"], "parse");
    assert_ne!(answer["error"]["message"].as_str().unwrap(), "");
    // What was read before the failure is there.
    assert_eq!(answer["value"]["count"], 2);
    assert_eq!(answer["value"]["items"].as_array().unwrap().len(), 2);
    // The node where it failed says so, and only that one.
    let mut failed = Vec::new();
    let mut stack = vec![(String::new(), &answer["trace"])];
    while let Some((path, node)) = stack.pop() {
        let path = format!("{path}/{}", node["name"].as_str().unwrap());
        if node["failed"] == true {
            failed.push(path.clone());
        }
        for child in node["children"].as_array().unwrap() {
            stack.push((path.clone(), child));
        }
    }
    assert_eq!(failed, ["/chunks/items/[1]/body"]);
}

#[test]
fn a_schema_that_does_not_compile_has_no_value() {
    let answer = parse(
        &json!({ "schema": "id: t\nseq:\n  - id: a\n    type: nope\n" }),
        &[1],
    );
    assert_eq!(answer["ok"], false);
    assert_eq!(answer["value"], Value::Null);
    assert_eq!(answer["trace"], Value::Null);
    assert_eq!(answer["diagnostics"][0]["kind"], "schema");
    assert_eq!(answer["diagnostics"][0]["position"]["line"], 4);
}

#[test]
fn a_type_can_be_chosen() {
    let schema = "meta:\n  id: t\nseq:\n  - id: a\n    type: u1\ntypes:\n  other:\n    seq:\n      - id: wide\n        type: u2le\n";
    let answer = parse(&json!({ "schema": schema, "type": "t::other" }), &[1, 2]);
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["value"]["wide"], 513);
    let answer = parse(&json!({ "schema": schema, "type": "t::nothing" }), &[1, 2]);
    assert_eq!(answer["ok"], false);
    assert_eq!(answer["error"]["kind"], "input");
}

#[test]
fn long_values_are_cut_in_the_trace() {
    let schema = "id: t\nseq:\n  - id: text\n    type: str\n    size: 400\n    encoding: ASCII\n";
    let answer = parse(&json!({ "schema": schema }), &[b'a'; 400]);
    let summary = answer["trace"]["children"][0]["summary"].as_str().unwrap();
    assert!(
        summary.ends_with('…') && summary.chars().count() == 201,
        "{}",
        summary.len()
    );
}

#[test]
fn the_version_is_the_crates() {
    assert_eq!(doe_wasm::version(), env!("CARGO_PKG_VERSION"));
}

const SCHEMA: &str = "meta:\n  id: pairs\nseq:\n  - id: count\n    type: u1\n  - id: values\n    type: u2le\n    repeat: expr\n    repeat-expr: count\n";

#[test]
fn export_gives_what_the_command_line_prints() {
    use doe::OutputFormat;
    let dir = tempfile::tempdir().unwrap();
    let schema_path = dir.path().join("t.yaml");
    std::fs::write(&schema_path, SCHEMA).unwrap();
    let input_path = dir.path().join("t.bin");
    let bytes = [2_u8, 1, 0, 2, 0];
    std::fs::write(&input_path, bytes).unwrap();
    for (format, wanted) in [
        (
            "text",
            OutputFormat::Text {
                indent: "  ".into(),
            },
        ),
        ("json", OutputFormat::Json),
        ("pretty", OutputFormat::JsonPretty),
    ] {
        let answer =
            doe_wasm::api::export(&json!({ "schema": SCHEMA }).to_string(), &bytes, format);
        assert!(answer.ok, "{format}: {answer:?}");
        let cli = doe::parse_file(
            &input_path,
            schema_path.to_str().unwrap(),
            &[],
            None,
            wanted,
        )
        .unwrap();
        assert_eq!(answer.output.as_deref(), Some(cli.as_str()), "{format}");
    }
}

#[test]
fn export_says_why_there_is_no_output() {
    let request = json!({ "schema": SCHEMA }).to_string();
    // the parse fails: no output
    let failed = doe_wasm::api::export(&request, &[3, 1, 0], "json");
    assert!(!failed.ok && failed.output.is_none());
    assert_eq!(failed.error.unwrap().kind, "parse");
    // the schema does not compile
    let bad = doe_wasm::api::export(
        &json!({ "schema": "id: t\nseq:\n  - id: a\n    type: nope\n" }).to_string(),
        &[],
        "json",
    );
    assert!(!bad.ok && !bad.diagnostics.is_empty());
    // the format is not known
    let format = doe_wasm::api::export(&request, &[2, 1, 0, 2, 0], "xml");
    assert_eq!(format.error.unwrap().kind, "request");
}

fn inspect(request: &Value) -> Value {
    serde_json::from_str(&doe_wasm::inspect(&request.to_string())).unwrap()
}

#[test]
fn inspect_lists_the_main_schema_its_imports_and_their_types() {
    let answer = inspect(&json!({
        "schema": "meta:\n  id: top\n  imports:\n    - common\n    - ./sub/leaf.ksy\nseq:\n  - id: a\n    type: common\ntypes:\n  inner:\n    seq:\n      - id: x\n        type: u1\n    types:\n      deeper:\n        seq: []\n",
        "files": {
            "common.ksy": "meta:\n  id: common\nseq:\n  - id: b\n    type: u1\ntypes:\n  piece:\n    seq: []\n",
            "sub/leaf.ksy": "meta:\n  id: leaf\n  imports:\n    - ../common.ksy\nseq: []\n",
        },
    }));
    assert_eq!(answer["ok"], true);
    assert_eq!(answer["missing"], json!([]));
    let paths: Vec<_> = answer["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["path"].as_str().unwrap())
        .collect();
    // the main schema first; a file imported twice is listed once
    assert_eq!(paths, ["main.ksy", "common.ksy", "sub/leaf.ksy"]);
    let main = &answer["schemas"][0];
    assert_eq!(main["id"], "top");
    assert_eq!(main["imports"], json!(["common", "./sub/leaf.ksy"]));
    assert_eq!(
        main["types"],
        json!(["top", "top::inner", "top::inner::deeper"])
    );
    assert_eq!(
        answer["schemas"][1]["types"],
        json!(["common", "common::piece"])
    );
    assert_eq!(answer["schemas"][2]["id"], "leaf");
    assert!(main["error"].is_null());
}

#[test]
fn inspect_lists_the_imports_that_are_missing_and_the_file_that_asked() {
    let answer = inspect(&json!({
        "schema": "meta:\n  id: top\n  imports:\n    - there\n    - gone\nseq: []\n",
        "files": { "there.ksy": "meta:\n  id: there\n  imports:\n    - also_gone\nseq: []\n" },
    }));
    assert_eq!(answer["ok"], true);
    let missing: Vec<(String, String)> = answer["missing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["name"].as_str().unwrap().to_owned(),
                m["from"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        missing,
        [
            ("gone".to_owned(), "main.ksy".to_owned()),
            ("also_gone".to_owned(), "there.ksy".to_owned())
        ]
    );
    assert!(
        answer["missing"][0]["message"]
            .as_str()
            .unwrap()
            .contains("gone")
    );
    // the file that is there is listed
    assert_eq!(answer["schemas"][1]["path"], "there.ksy");
}

#[test]
fn inspect_answers_for_a_schema_that_does_not_compile_and_for_a_file_that_cannot_be_read() {
    // a type that is not there: it reads, so it is listed
    let answer = inspect(&json!({ "schema": "id: t\nseq:\n  - id: a\n    type: nope\n" }));
    assert_eq!(answer["schemas"][0]["id"], "t");
    assert!(answer["schemas"][0]["error"].is_null());
    // an import that is not YAML: listed with the error and its place in that file
    let answer = inspect(&json!({
        "schema": "id: t\nimports:\n  - broken\nseq: []\n",
        "files": { "broken.ksy": "id: broken\nseq:\n  - id: [\n" },
    }));
    let broken = &answer["schemas"][1];
    assert_eq!(broken["path"], "broken.ksy");
    assert!(broken["id"].is_null());
    assert_eq!(broken["error"]["kind"], "yaml");
    assert_eq!(broken["error"]["position"]["line"], 3);
    // the main schema itself is not YAML
    let answer = inspect(&json!({ "schema": "id: [\n" }));
    assert_eq!(answer["schemas"][0]["error"]["kind"], "yaml");
    assert!(answer["schemas"][0]["error"]["position"]["line"].is_number());
}

#[test]
fn inspect_refuses_a_request_that_is_not_valid_and_follows_a_cycle_once() {
    let answer = inspect(&json!({ "schema": 5 }));
    assert_eq!(answer["ok"], false);
    assert_eq!(answer["diagnostics"][0]["kind"], "request");
    let answer = inspect(&json!({
        "schema": "id: a\nimports:\n  - b\nseq: []\n",
        "files": { "b.ksy": "id: b\nimports:\n  - main\nseq: []\n" },
    }));
    let paths: Vec<_> = answer["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, ["main.ksy", "b.ksy"]);
}

#[test]
fn a_type_of_an_imported_schema_can_be_the_type_to_parse_with() {
    let schema = "meta:\n  id: top\n  imports:\n    - common\nseq:\n  - id: a\n    type: common\n";
    let files = json!({ "common.ksy": "meta:\n  id: common\nseq:\n  - id: b\n    type: u1\ntypes:\n  piece:\n    seq:\n      - id: p\n        type: u2le\n" });
    let own = parse(&json!({ "schema": schema, "files": files }), &[7, 0, 0]);
    assert_eq!(own["ok"], true);
    assert_eq!(own["trace"]["type"], "top");
    let imported = parse(
        &json!({ "schema": schema, "files": files, "type": "common::piece" }),
        &[7, 0, 0],
    );
    assert_eq!(imported["ok"], true);
    assert_eq!(imported["trace"]["children"][0]["name"], "p");
    assert_eq!(imported["value"]["p"], 7);
}
