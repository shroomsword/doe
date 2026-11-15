//! The language of a schema, as the compiler defines it (#425): the keys at each place, found by asking the parser, and
//! what they and the built-in types mean, which a test keeps in step with the parser and the compiler.

use std::collections::HashSet;

use doe::schema::compiler::compile_schema;
use doe::schema::language::{
    ENCODINGS, Place, accepted_keys, builtin_types, describe, documented_keys, is_hidden, keys,
    values,
};
use doe::schema::raw::RawSchema;

fn compiles(yaml: &str) -> Result<(), String> {
    let schema = RawSchema::from_yaml_str(yaml).map_err(|e| e.to_string())?;
    compile_schema(&schema)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[test]
fn the_message_of_the_parser_is_the_one_that_the_probe_reads() {
    // a change of the parser, or of the library that words its errors, is a failure here and not a list that is empty
    for place in Place::ALL {
        let found = accepted_keys(place);
        assert!(
            !found.is_empty(),
            "{}: the parser named no keys",
            place.name()
        );
        let unique: HashSet<&String> = found.iter().collect();
        assert_eq!(unique.len(), found.len(), "{}: a key twice", place.name());
    }
    let top = accepted_keys(Place::Schema);
    for key in [
        "id",
        "doc",
        "meta",
        "seq",
        "types",
        "instances",
        "enums",
        "imports",
    ] {
        assert!(top.iter().any(|k| k == key), "{key} at the top: {top:?}");
    }
    assert_eq!(accepted_keys(Place::Switch), ["switch-on", "cases"]);
    assert_eq!(accepted_keys(Place::Payload), ["process", "from"]);
}

#[test]
fn every_key_that_the_parser_accepts_has_a_line_and_every_line_is_for_a_key_that_it_accepts() {
    for place in Place::ALL {
        let accepted = accepted_keys(place);
        for key in &accepted {
            if is_hidden(key) {
                continue;
            }
            assert!(
                describe(place, key).is_some_and(|line| !line.is_empty()),
                "{}: `{key}` is accepted and has no line",
                place.name()
            );
        }
        for key in documented_keys(place) {
            assert!(
                accepted.iter().any(|k| k == key),
                "{}: there is a line for `{key}`, which the parser does not accept",
                place.name()
            );
        }
    }
}

#[test]
fn the_keys_that_are_not_for_people_are_accepted_and_hidden() {
    let field = keys(Place::Field);
    let hidden: Vec<&str> = field
        .iter()
        .filter(|k| k.hidden)
        .map(|k| k.key.as_str())
        .collect();
    assert!(
        hidden.contains(&"-orig-id") && hidden.contains(&"doc-ref"),
        "{hidden:?}"
    );
    assert!(
        field
            .iter()
            .filter(|k| !k.hidden)
            .all(|k| !k.doc.is_empty())
    );
    let meta = keys(Place::Meta);
    assert!(meta.iter().any(|k| k.key == "ks-version" && k.hidden));
    assert!(meta.iter().any(|k| k.key == "endian" && !k.hidden));
    // an instance has what a field has, and its own
    let instance: Vec<String> = keys(Place::Instance)
        .into_iter()
        .filter(|k| !k.hidden)
        .map(|k| k.key)
        .collect();
    assert!(
        instance.iter().any(|k| k == "value")
            && instance.iter().any(|k| k == "io")
            && instance.iter().any(|k| k == "size")
    );
    let fields: Vec<String> = keys(Place::Field)
        .into_iter()
        .filter(|k| !k.hidden)
        .map(|k| k.key)
        .collect();
    assert!(
        !fields.iter().any(|k| k == "value"),
        "a field has no value:"
    );
}

#[test]
fn every_built_in_type_compiles_and_a_name_that_is_not_one_does_not() {
    let types = builtin_types();
    let names: HashSet<&str> = types.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names.len(), types.len(), "a name twice");
    for expected in [
        "u1", "u4le", "s8be", "f4", "f8le", "bytes", "str", "strz", "bits", "b1", "b64",
    ] {
        assert!(names.contains(expected), "{expected}");
    }
    for t in &types {
        assert!(!t.doc.is_empty(), "{} has no meaning", t.name);
        let extra = match t.name.as_str() {
            "bytes" | "str" => "    size: 1\n",
            "bits" => "    bit_size: 3\n",
            _ => "",
        };
        let yaml = format!(
            "meta:\n  id: t\n  endian: le\nseq:\n  - id: a\n    type: {}\n{extra}",
            t.name
        );
        assert_eq!(compiles(&yaml), Ok(()), "{}", t.name);
    }
    // the check means something: names that were in use before, and one that was never there
    for bad in ["u16", "i32", "f64", "int", "b65"] {
        let yaml = format!("meta:\n  id: t\nseq:\n  - id: a\n    type: {bad}\n");
        assert!(compiles(&yaml).is_err(), "{bad} compiles");
    }
}

#[test]
fn the_values_that_keys_take_are_read_by_the_parser() {
    for name in values("endian").iter().chain(&values("bit-endian")) {
        let yaml = format!(
            "meta:\n  id: t\n  endian: {0}\n  bit-endian: {0}\nseq:\n  - id: a\n    type: u2\n",
            name.name
        );
        assert_eq!(compiles(&yaml), Ok(()), "{}", name.name);
    }
    assert_eq!(
        values("repeat")
            .iter()
            .map(|v| v.name.as_str())
            .collect::<Vec<_>>(),
        ["eos", "expr", "until"]
    );
    assert_eq!(
        compiles("meta:\n  id: t\nseq:\n  - id: a\n    type: u1\n    repeat: eos\n"),
        Ok(())
    );
    assert_eq!(
        compiles(
            "meta:\n  id: t\nseq:\n  - id: n\n    type: u1\n  - id: a\n    type: u1\n    repeat: expr\n    repeat-expr: n\n"
        ),
        Ok(())
    );
    assert_eq!(
        compiles(
            "meta:\n  id: t\nseq:\n  - id: a\n    type: u1\n    repeat: until\n    repeat-until: _ == 0\n"
        ),
        Ok(())
    );
    assert!(
        compiles("meta:\n  id: t\nseq:\n  - id: a\n    type: u1\n    repeat: forever\n").is_err()
    );
    for encoding in ENCODINGS {
        let yaml = format!(
            "meta:\n  id: t\nseq:\n  - id: a\n    type: str\n    size: 1\n    encoding: {encoding}\n"
        );
        assert_eq!(compiles(&yaml), Ok(()), "{encoding}");
    }
    assert!(
        compiles(
            "meta:\n  id: t\nseq:\n  - id: a\n    type: str\n    size: 1\n    encoding: klingon\n"
        )
        .is_err()
    );
    assert!(
        values("size").is_empty(),
        "size takes an expression, not a name from a list"
    );
}

#[test]
fn the_transforms_of_process_are_read_by_the_compiler() {
    for process in values("process") {
        // the ones with an argument are shown with its name: write a number
        let name = process.name.replace("(key)", "(1)").replace("(n)", "(1)");
        let yaml = format!("meta:\n  id: t\nseq:\n  - id: a\n    size: 4\n    process: {name}\n");
        assert_eq!(compiles(&yaml), Ok(()), "{name}");
        assert!(!process.doc.is_empty(), "{} has no meaning", process.name);
    }
    assert!(
        compiles("meta:\n  id: t\nseq:\n  - id: a\n    size: 4\n    process: rot13\n").is_err()
    );
}

#[test]
fn the_places_have_names_that_are_their_own() {
    let names: HashSet<&str> = Place::ALL.iter().map(|p| p.name()).collect();
    assert_eq!(names.len(), Place::ALL.len());
}
