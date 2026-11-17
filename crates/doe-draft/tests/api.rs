//! The builder as one call: the worked example of the README is built with it from the bytes of its file, and the text that
//! comes out is the schema that the README shows (the terminal viewer's test builds the same one with keys).

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use doe_draft::api::draft;
use serde_json::{Value, json};

const README: &str = include_str!("../../../README.md");
const BLIP: &[u8] = include_bytes!("../../doe-tui/tests/example/blip.bin");

/// The fenced block that follows the line `<!-- example: NAME -->` in the README.
fn block(name: &str) -> String {
    let marker = format!("<!-- example: {name} -->");
    let after = README.split_once(&marker).unwrap().1;
    let mut lines = after.lines().skip_while(|l| !l.starts_with("```")).skip(1);
    let mut out = Vec::new();
    for line in lines.by_ref() {
        if line.starts_with("```") {
            break;
        }
        out.push(line);
    }
    out.join("\n") + "\n"
}

/// One call: the text, a change, the file.
fn call(text: &str, op: Option<Value>, bytes: &[u8]) -> Value {
    call_with(text, op, &json!({}), bytes)
}

fn call_with(text: &str, op: Option<Value>, files: &Value, bytes: &[u8]) -> Value {
    let mut request = json!({"text": text, "id": "blip", "files": files});
    if let Some(op) = op {
        request["op"] = op;
    }
    serde_json::from_str(&draft(&request.to_string(), bytes)).unwrap()
}

/// Make the change, and go on with the text that comes back.
fn step(text: &mut String, op: Value, bytes: &[u8]) -> Value {
    let answer = call(text, Some(op), bytes);
    assert_eq!(answer["ok"], true, "{answer}");
    text.clone_from(&answer["text"].as_str().unwrap().to_owned());
    answer
}

fn add(kind: &str, name: &str) -> Value {
    json!({"op": "add", "kind": kind, "name": name})
}

#[test]
fn the_example_of_the_readme_is_built_from_the_bytes_with_no_yaml_typed() {
    let mut text = String::new();
    step(
        &mut text,
        json!({"op": "add", "kind": "magic", "name": "signature", "magic": [0x42, 0x4c, 0x49, 0x50]}),
        BLIP,
    );
    step(&mut text, add("u1", "version"), BLIP);
    step(&mut text, add("u1", "count"), BLIP);
    step(
        &mut text,
        json!({"op": "add", "kind": "u2", "name": "values", "repeat": "count"}),
        BLIP,
    );
    step(&mut text, add("strz", "title"), BLIP);
    step(&mut text, add("b3", "kind"), BLIP);
    let last = step(&mut text, add("b5", "level"), BLIP);
    assert_eq!(text, block("blip.yaml"));
    // where each field is
    let fields = last["layout"]["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 7);
    assert_eq!(fields[3]["id"], "values");
    assert_eq!(fields[3]["kind"], "u2[]");
    assert_eq!(
        (
            fields[3]["start_bit"].as_u64(),
            fields[3]["end_bit"].as_u64()
        ),
        (Some(48), Some(96))
    );
    // reading it back changes nothing
    let again = call(&text, None, BLIP);
    assert_eq!(
        (again["ok"].clone(), again["text"].as_str()),
        (json!(true), Some(text.as_str()))
    );
}

#[test]
fn a_dialog_begins_with_the_integer_of_the_selection_and_a_name_no_field_has() {
    let mut text = String::new();
    step(&mut text, add("u2", "u2_1"), BLIP);
    let p = call(&text, Some(json!({"op": "propose", "selection": 2})), BLIP);
    assert_eq!(p["proposal"], json!({"kind": "u2", "name": "u2_2"}));
    let p = call(&text, Some(json!({"op": "propose", "selection": 3})), BLIP);
    assert_eq!(p["proposal"]["kind"], "bytes");
    let p = call(&text, Some(json!({"op": "propose", "kind": "b3"})), BLIP);
    assert_eq!(p["proposal"]["name"], "bits_1");
}

#[test]
fn a_change_that_does_not_fit_the_file_is_refused_and_the_text_is_not_touched() {
    let mut text = String::new();
    step(&mut text, add("u4", "a"), BLIP);
    let answer = call(&text, Some(add("bytes", "b")), BLIP);
    assert_eq!(answer["ok"], false);
    assert_eq!(answer["buildable"], true);
    assert_eq!(answer["text"], text.as_str());
    assert!(
        answer["error"].as_str().unwrap().contains("a length"),
        "{answer}"
    );
    // and a field that would run past the end of the file
    let answer = call(
        &text,
        Some(json!({"op": "add", "kind": "bytes", "name": "b", "length": 100})),
        BLIP,
    );
    assert_eq!(answer["ok"], false);
    assert!(
        answer["error"].as_str().unwrap().contains("does not fit"),
        "{answer}"
    );
    assert_eq!(answer["text"], text.as_str());
}

#[test]
fn the_rules_of_the_terminal_are_the_rules_of_the_page() {
    let mut text = String::new();
    step(&mut text, add("u1", "a"), BLIP);
    // a name that is taken, a name that is not one, a repeat that counts nothing
    for (op, wanted) in [
        (add("u1", "a"), "there is a field called a already"),
        (add("u1", "Bad Name"), "Bad Name"),
        (
            json!({"op": "add", "kind": "u2", "name": "c", "repeat": "nothing"}),
            "nothing",
        ),
    ] {
        let answer = call(&text, Some(op), BLIP);
        assert_eq!(answer["ok"], false, "{answer}");
        assert!(
            answer["error"].as_str().unwrap().contains(wanted),
            "{answer}"
        );
    }
}

#[test]
fn rename_retype_remove_and_the_byte_order() {
    let mut text = String::new();
    step(&mut text, add("u1", "a"), BLIP);
    step(&mut text, add("u1", "b"), BLIP);
    step(
        &mut text,
        json!({"op": "rename", "index": 0, "to": "first"}),
        BLIP,
    );
    step(
        &mut text,
        json!({"op": "retype", "index": 1, "kind": "u2"}),
        BLIP,
    );
    let answer = step(&mut text, json!({"op": "endian", "to": "be"}), BLIP);
    assert_eq!(answer["layout"]["endian"], "be");
    assert_eq!(answer["layout"]["fields"][0]["id"], "first");
    assert_eq!(answer["layout"]["fields"][1]["kind"], "u2");
    let answer = step(&mut text, json!({"op": "remove", "index": 0}), BLIP);
    assert_eq!(answer["layout"]["fields"].as_array().unwrap().len(), 1);
}

#[test]
fn fields_are_grouped_into_a_type_and_taken_apart() {
    let mut text = String::new();
    step(&mut text, add("u1", "a"), BLIP);
    step(&mut text, add("u1", "b"), BLIP);
    step(&mut text, add("u1", "c"), BLIP);
    let answer = step(
        &mut text,
        json!({"op": "group", "start": 0, "count": 2, "type_id": "head", "field_id": "head"}),
        BLIP,
    );
    assert_eq!(answer["layout"]["types"][0]["id"], "head");
    assert_eq!(
        answer["layout"]["types"][0]["fields"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(answer["layout"]["fields"][0]["kind"], "head");
    let answer = step(&mut text, json!({"op": "ungroup", "start": 0}), BLIP);
    assert_eq!(answer["layout"]["fields"].as_array().unwrap().len(), 3);
    assert_eq!(answer["layout"]["types"], json!([]));
}

#[test]
fn a_hand_edit_between_two_changes_is_kept() {
    let mut text = String::new();
    step(&mut text, add("u1", "a"), BLIP);
    // the person renames the field in the editor
    text = text.replace("id: a", "id: renamed");
    let answer = step(&mut text, add("u1", "b"), BLIP);
    assert_eq!(answer["layout"]["fields"][0]["id"], "renamed");
}

#[test]
fn a_text_that_the_builder_did_not_write_is_refused_with_its_line_and_never_rewritten() {
    let text = "meta:\n  id: x\nseq:\n  - id: a\n    type: u1\n    if: 1 == 1\n";
    let answer = call(text, Some(add("u1", "b")), BLIP);
    assert_eq!(answer["ok"], false);
    assert_eq!(answer["buildable"], false);
    assert_eq!(answer["text"], text);
    assert!(
        answer["error"].as_str().unwrap().contains("line"),
        "{answer}"
    );
    assert!(answer["layout"].is_null());
    // and the same for text that is not a schema at all
    let answer = call("hello", None, BLIP);
    assert_eq!(answer["buildable"], false);
}

#[test]
fn a_request_that_is_not_valid_says_so() {
    let answer: Value = serde_json::from_str(&draft("{", BLIP)).unwrap();
    assert_eq!(answer["ok"], false);
    assert!(answer["error"].as_str().unwrap().contains("not valid"));
}

#[test]
fn a_type_of_the_library_is_a_kind_and_the_schema_imports_it() {
    let other =
        "meta:\n  id: pair\n  endian: le\nseq:\n  - id: x\n    type: u1\n  - id: y\n    type: u1\n";
    let files = json!({"pair.ksy": other});
    let answer = call_with(
        "",
        Some(json!({"op": "add", "kind": "type", "type_id": "pair", "name": "first"})),
        &files,
        BLIP,
    );
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["layout"]["imports"], json!(["pair"]));
    assert!(
        answer["text"]
            .as_str()
            .unwrap()
            .contains("imports:\n  - pair")
    );
    // it takes the two bytes that the type has
    assert_eq!(answer["layout"]["fields"][0]["kind"], "pair");
}

#[test]
fn a_file_that_the_schema_does_not_fit_can_be_mended_by_a_change() {
    let mut text = String::new();
    step(&mut text, add("u1", "a"), &[1, 2, 3]);
    // the file got shorter: the text no longer fits it, and taking the field out is allowed
    let answer = call(&text, None, &[]);
    assert_eq!(answer["ok"], false);
    let answer = call(&text, Some(json!({"op": "remove", "index": 0})), &[]);
    assert_eq!(answer["ok"], true, "{answer}");
}
