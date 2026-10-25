//! `SCHEMA::TYPE`: naming a type inside a schema, and listing the schemas of a
//! schema from disk.

use std::path::Path;

use doe::inspect::inspect;
use doe::schema::compiler::Compiler;
use doe::{OutputFormat, parse_file, split_type_arg, type_in_schema};

const SCHEMA: &str = "meta:
  id: top
  endian: le
  imports:
    - lib
seq:
  - id: head
    type: u1
types:
  chunk:
    seq:
      - id: n
        type: u2
    types:
      inner:
        seq:
          - id: m
            type: u1
";

const LIB: &str = "meta:
  id: lib
seq: []
types:
  piece:
    seq:
      - id: p
        type: u1
";

fn write(dir: &Path) -> std::path::PathBuf {
    std::fs::write(dir.join("top.yaml"), SCHEMA).unwrap();
    std::fs::write(dir.join("lib.yaml"), LIB).unwrap();
    std::fs::write(dir.join("data.bin"), [7, 1, 0, 9]).unwrap();
    dir.join("top.yaml")
}

#[test]
fn the_type_is_split_from_the_schema_at_the_first_double_colon() {
    assert_eq!(split_type_arg("a.yaml::chunk"), ("a.yaml", Some("chunk")));
    assert_eq!(
        split_type_arg("png::chunk::data"),
        ("png", Some("chunk::data"))
    );
    assert_eq!(split_type_arg("./a b/c.ksy::t"), ("./a b/c.ksy", Some("t")));
    assert_eq!(split_type_arg("png"), ("png", None));
    // nothing on one side: it is a name, not a schema and a type
    assert_eq!(split_type_arg("::x"), ("::x", None));
    assert_eq!(split_type_arg("x::"), ("x::", None));
    // the colon of a Windows drive is not one of two
    assert_eq!(
        split_type_arg("C:\\types\\a.yaml"),
        ("C:\\types\\a.yaml", None)
    );
    assert_eq!(
        split_type_arg("C:\\types\\a.yaml::t"),
        ("C:\\types\\a.yaml", Some("t"))
    );
}

#[test]
fn a_type_inside_a_schema_is_parsed_with_and_the_others_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let schema = write(dir.path());
    let file = dir.path().join("data.bin");
    let run = |arg: &str| {
        parse_file(
            &file,
            arg,
            &[],
            Some(Path::new("/nonexistent/c.toml")),
            OutputFormat::Json,
        )
    };
    let top = schema.to_str().unwrap();
    assert_eq!(run(top).unwrap(), r#"{"$type":"top","head":7}"#);
    // a type of the schema, and one nested in it
    assert_eq!(
        run(&format!("{top}::chunk")).unwrap(),
        r#"{"$type":"top::chunk","n":263}"#
    );
    assert_eq!(
        run(&format!("{top}::chunk::inner")).unwrap(),
        r#"{"$type":"top::chunk::inner","m":7}"#
    );
    // a type of an imported schema, written in full
    assert_eq!(
        run(&format!("{top}::lib::piece")).unwrap(),
        r#"{"$type":"lib::piece","p":7}"#
    );
    // a type that is not there says which ones are
    let error = run(&format!("{top}::nope")).unwrap_err().to_string();
    assert!(error.contains("nope"), "{error}");
    assert!(
        error.contains("chunk") && error.contains("chunk::inner"),
        "{error}"
    );
}

#[test]
fn a_type_of_the_library_can_be_named_in_the_same_way() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path());
    let file = dir.path().join("data.bin");
    let include = [dir.path().to_path_buf()];
    let out = parse_file(
        &file,
        "top::chunk",
        &include,
        Some(Path::new("/nonexistent/c.toml")),
        OutputFormat::Json,
    )
    .unwrap();
    assert_eq!(out, r#"{"$type":"top::chunk","n":263}"#);
}

#[test]
fn the_type_names_of_a_schema_are_its_id_and_its_nested_types() {
    let dir = tempfile::tempdir().unwrap();
    let schema = write(dir.path());
    let mut compiler = Compiler::new(vec![]);
    let raw = compiler.read_schema(&schema).unwrap();
    compiler.process_schema(&raw).unwrap();
    assert_eq!(
        type_in_schema(&compiler.registry, "top", None).unwrap(),
        "top"
    );
    assert_eq!(
        type_in_schema(&compiler.registry, "top", Some("chunk")).unwrap(),
        "top::chunk"
    );
    assert_eq!(
        type_in_schema(&compiler.registry, "top", Some("lib::piece")).unwrap(),
        "lib::piece"
    );
    assert!(type_in_schema(&compiler.registry, "top", Some("zzz")).is_err());
}

#[test]
fn the_schemas_on_disk_are_listed_with_what_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let schema = write(dir.path());
    std::fs::write(
        dir.path().join("top.yaml"),
        SCHEMA.replace("    - lib\n", "    - lib\n    - gone\n"),
    )
    .unwrap();
    let compiler = Compiler::new(vec![]);
    let found = inspect(&compiler, &schema);
    let names: Vec<_> = found
        .schemas
        .iter()
        .map(|s| s.path.file_name().unwrap().to_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["top.yaml", "lib.yaml"]);
    assert_eq!(
        found.schemas[0].types,
        ["top", "top::chunk", "top::chunk::inner"]
    );
    assert_eq!(found.missing.len(), 1);
    assert_eq!(found.missing[0].name, "gone");
    assert_eq!(found.missing[0].from, schema);
}
