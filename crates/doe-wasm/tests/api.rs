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
    assert_eq!(count["integer"]["value"], "2");
    assert_eq!(count["integer"]["name"], Value::Null);
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
    assert_eq!(kind["integer"]["value"], "1");
    assert_eq!(kind["integer"]["name"], "text");
    assert_eq!(items["integer"], Value::Null);
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

#[test]
fn signature_gives_what_the_files_start_with_and_the_names_of_the_format() {
    let schema = "meta:\n  id: Pic\n  file-extension: [PIC, pc]\nseq:\n  - id: magic\n    contents: [0x89, PIC]\n  - id: n\n    type: u1\n";
    let answer: Value = serde_json::from_str(&doe_wasm::signature(
        &json!({"schema": schema, "path": "formats/pic.ksy"}).to_string(),
    ))
    .unwrap();
    assert_eq!(answer["ok"], true);
    assert_eq!(
        answer["magic"],
        json!([{"offset": 0, "bytes": [0x89, 80, 73, 67]}])
    );
    assert_eq!(answer["names"], json!(["pic", "pic", "pic", "pc"]));

    let bad: Value = serde_json::from_str(&doe_wasm::signature(
        &json!({"schema": "seq: [", "path": "x.ksy"}).to_string(),
    ))
    .unwrap();
    assert_eq!(bad["ok"], false);
    assert_ne!(bad["diagnostics"].as_array().unwrap().len(), 0);
}

#[test]
fn detect_order_ranks_by_signature_then_name_and_leaves_out_what_cannot_fit() {
    let request = json!({
        "sources": [
            {"name": "plain", "magic": [], "names": ["dat"]},
            {"name": "png", "magic": [{"offset": 0, "bytes": [137, 80]}], "names": ["png"]},
            {"name": "gif", "magic": [{"offset": 0, "bytes": [71, 73]}], "names": ["gif"]},
        ],
        "name": "x.dat",
    });
    let answer: Value =
        serde_json::from_str(&doe_wasm::detect_order(&request.to_string(), &[137, 80, 1])).unwrap();
    assert_eq!(answer["ok"], true);
    assert_eq!(
        answer["order"],
        json!([{"index": 1, "why": "signature"}, {"index": 0, "why": "name"}])
    );
    assert_eq!(answer["excluded"], 1);

    let bad: Value = serde_json::from_str(&doe_wasm::detect_order("{", &[])).unwrap();
    assert_eq!(bad["ok"], false);
}

fn compare(request: &Value, a: &[u8], b: &[u8]) -> Value {
    serde_json::from_str(&doe_wasm::compare(&request.to_string(), a, b)).unwrap()
}

/// Two parses of `CHUNKS`: a text item, then the same with another body.
const A: [u8; 6] = [1, 1, 2, b'h', b'i', 0];
const B: [u8; 6] = [1, 1, 2, b'h', b'o', 0];

#[test]
fn two_files_that_are_the_same_are_identical() {
    let answer = compare(&json!({ "schema": CHUNKS }), &A, &A);
    assert_eq!(answer["ok"], true);
    assert_eq!(answer["summary"]["identical"], true);
    assert_eq!(answer["summary"]["changed"], 0);
    assert_eq!(answer["ghosts"], json!([]));
    assert_eq!(answer["a"]["bytes"], json!([]));
    // no node has a mark
    assert_eq!(answer["a"]["trace"]["diff"], Value::Null);
}

#[test]
fn a_field_that_differs_is_marked_in_both_files_and_its_bytes_are_given() {
    let answer = compare(&json!({ "schema": CHUNKS }), &A, &B);
    assert_eq!(answer["ok"], true);
    assert_eq!(answer["summary"]["identical"], false);
    assert_eq!(answer["summary"]["changed"], 1);
    for side in ["a", "b"] {
        let root = &answer[side]["trace"];
        assert_eq!(root["diff"], "inside");
        assert_eq!(root["diff_below"], 1);
        let items = &root["children"][1];
        assert_eq!(items["name"], "items");
        assert_eq!(items["diff"], "inside");
        let body = &items["children"][0]["children"][2];
        assert_eq!(body["name"], "body");
        assert_eq!(body["diff"], "changed");
    }
    // the byte of the body that differs, in the file: `i` and `o`
    assert_eq!(answer["a"]["bytes"], json!([[4, 5]]));
    assert_eq!(answer["b"]["bytes"], json!([[4, 5]]));
}

#[test]
fn an_element_that_only_the_second_file_has_is_a_ghost_under_the_list() {
    let one = [1, 1, 1, b'x', 0];
    let two = [2, 1, 1, b'x', 1, 1, b'y', 0];
    let answer = compare(&json!({ "schema": CHUNKS }), &one, &two);
    assert_eq!(answer["summary"]["only_b"], 1);
    assert_eq!(answer["summary"]["only_a"], 0);
    let ghosts = answer["ghosts"].as_array().unwrap();
    assert_eq!(ghosts.len(), 1);
    // under `items` (the second child of the root), as element [1], and the value is the one of the second file
    assert_eq!(ghosts[0]["parent"], json!([1]));
    assert_eq!(ghosts[0]["name"], "[1]");
    assert_eq!(
        answer["b"]["trace"]["children"][1]["children"][1]["diff"],
        "only"
    );
}

#[test]
fn a_schema_that_does_not_compile_or_a_request_that_is_not_json_is_said() {
    let answer = compare(&json!({ "schema": "seq: [" }), &A, &B);
    assert_eq!(answer["ok"], false);
    assert!(
        !answer["diagnostics"].as_array().unwrap().is_empty(),
        "the schema has a mistake: {answer}"
    );
    assert_eq!(answer["a"], Value::Null);
    // a request that is not JSON
    let answer: Value = serde_json::from_str(&doe_wasm::compare("not json", &A, &B)).unwrap();
    assert_eq!(answer["ok"], false);
}

#[test]
fn a_file_that_is_too_big_is_said_with_its_name_and_nothing_is_compared() {
    // zeroed memory that is not touched: the size is checked before anything is read
    let big = vec![0_u8; 256 * 1024 * 1024 + 1];
    let answer = compare(&json!({ "schema": CHUNKS }), &A, &big);
    assert_eq!(answer["ok"], false);
    assert_eq!(answer["error"]["kind"], "input");
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("the second file: the input is"),
        "{answer}"
    );
    assert_eq!(answer["a"], Value::Null);
    let answer = compare(&json!({ "schema": CHUNKS }), &big, &A);
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("the first file:")
    );
}

#[test]
fn the_language_names_the_keys_of_each_place_the_types_and_the_values() {
    let answer: Value = serde_json::from_str(&doe_wasm::language()).unwrap();
    let keys = |place: &str| -> Vec<String> {
        answer["places"][place]
            .as_array()
            .unwrap()
            .iter()
            .filter(|k| k["hidden"] == false)
            .map(|k| k["key"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(keys("switch"), ["switch-on", "cases"]);
    assert!(keys("schema").contains(&"seq".to_owned()));
    assert!(keys("field").contains(&"repeat-expr".to_owned()));
    assert!(
        !keys("field").contains(&"value".to_owned()),
        "a field has no value"
    );
    assert!(keys("instance").contains(&"value".to_owned()));
    // what the parser accepts and is not for people is marked, and has no line
    let field = answer["places"]["field"].as_array().unwrap();
    let hidden = field.iter().find(|k| k["key"] == "-orig-id").unwrap();
    assert_eq!(hidden["hidden"], true);
    assert_eq!(hidden["doc"], "");
    // every offered key has a line
    for place in answer["places"].as_object().unwrap().values() {
        for key in place
            .as_array()
            .unwrap()
            .iter()
            .filter(|k| k["hidden"] == false)
        {
            assert_ne!(key["doc"], "", "{key}");
        }
    }
    let types: Vec<&str> = answer["types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(types.contains(&"u2le") && types.contains(&"strz") && types.contains(&"b13"));
    assert_eq!(answer["values"]["endian"][0]["name"], "le");
    assert_eq!(answer["values"]["repeat"].as_array().unwrap().len(), 3);
    assert!(answer["values"]["encoding"].as_array().unwrap().len() > 30);
    assert!(
        answer["values"]["process"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "zlib")
    );
}

#[test]
fn the_builder_is_one_call_over_the_text_of_the_schema() {
    let request = json!({"text": "", "id": "t", "op": {"op": "add", "kind": "u1", "name": "a"}});
    let answer: Value = serde_json::from_str(&doe_wasm::draft(&request.to_string(), &[7])).unwrap();
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["buildable"], true);
    assert_eq!(answer["layout"]["fields"][0]["id"], "a");
    // the text it wrote is a schema that parses the file
    let parsed: Value = serde_json::from_str(&doe_wasm::parse(
        &json!({"schema": answer["text"]}).to_string(),
        &[7],
    ))
    .unwrap();
    assert_eq!(parsed["ok"], true, "{parsed}");
    assert_eq!(parsed["value"]["a"], 7);
}

/// A table of records, up to the end of the file: an id, a letter and a value in each.
const TABLE: &str = "meta:\n  id: table\n  endian: le\nseq:\n  - id: records\n    type: record\n    repeat: eos\ntypes:\n  record:\n    seq:\n      - id: id\n        type: u1\n      - id: letter\n        type: u1\n";

#[test]
fn a_list_is_matched_by_the_key_of_the_request_and_the_lists_are_named() {
    // an element inserted at the front
    let a = [1, b'a', 2, b'b', 3, b'c'];
    let b = [9, b'z', 1, b'a', 2, b'b', 3, b'c'];
    let by_index = compare(&json!({ "schema": TABLE }), &a, &b);
    assert!(by_index["summary"]["changed"].as_u64().unwrap() > 1);
    assert_eq!(by_index["notes"], json!([]));
    // the lists, with the fields that could match them, are said whatever was asked
    assert_eq!(by_index["lists"][0]["name"], "records");
    assert_eq!(by_index["lists"][0]["candidates"], json!(["id", "letter"]));
    assert_eq!(by_index["lists"][0]["schema_key"], Value::Null);
    let keyed = compare(
        &json!({ "schema": TABLE, "compare_keys": { "records": "id" } }),
        &a,
        &b,
    );
    assert_eq!(keyed["summary"]["only_b"], 1);
    assert_eq!(keyed["summary"]["changed"], 0);
    assert_eq!(keyed["a"]["trace"]["children"][0]["matched_by"], "id");
    assert_eq!(keyed["b"]["trace"]["children"][0]["matched_by"], "id");
    // and the key of the schema does the same without being asked
    let schema = TABLE.replace("repeat: eos\n", "repeat: eos\n    compare-by: id\n");
    let by_schema = compare(&json!({ "schema": schema }), &a, &b);
    assert_eq!(by_schema["summary"], keyed["summary"]);
    assert_eq!(by_schema["lists"][0]["schema_key"], "id");
    // a key that cannot be used is a note, and the list is matched by index
    let repeated = compare(
        &json!({ "schema": TABLE, "compare_keys": { "records": "letter" } }),
        &[1, b'a', 2, b'a'],
        &[1, b'a', 2, b'a'],
    );
    assert_eq!(repeated["ok"], true);
    assert!(
        repeated["notes"][0].as_str().unwrap().contains("by index"),
        "{repeated}"
    );
}
