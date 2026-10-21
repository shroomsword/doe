//! Where in the text of a schema an error is.

use std::path::{Path, PathBuf};

use doe::DoeError;
use doe::schema::compiler::Compiler;
use doe::schema::locate::{Position, SchemaMap, locate_error};
use doe::schema::source::MemorySource;

/// Compile `main` (with `others` beside it) and return the error and where
/// the text of `main` says it is.
fn locate(main: &str, others: &[(&str, &str)]) -> (DoeError, Option<Position>) {
    let mut files = MemorySource::new().with_file("main.ksy", main);
    for (name, text) in others {
        files.insert(name, *text);
    }
    let mut compiler = Compiler::with_source(Vec::<PathBuf>::new(), Box::new(files));
    let error = match compiler.read_schema(Path::new("main.ksy")) {
        Ok(schema) => compiler.process_schema(&schema).unwrap_err(),
        Err(error) => error,
    };
    let position = locate_error(main, &error);
    (error, position)
}

/// The line and column of the position, and the text it covers.
fn at(main: &str, position: Option<Position>) -> (u32, u32, &str) {
    let position = position.expect("the error has a position");
    let text = match (position.offset, position.len) {
        (Some(offset), Some(len)) => main.get(offset..offset + len).unwrap_or(""),
        _ => "",
    };
    (position.line, position.column, text)
}

#[test]
fn a_syntax_error_has_the_position_the_reader_gives() {
    let main = "id: t\nseq:\n  - id: a\n    type: [u1\n";
    let (error, position) = locate(main, &[]);
    assert!(matches!(error, DoeError::Yaml { .. }), "{error:?}");
    let position = position.unwrap();
    assert!((3..=5).contains(&position.line), "{position:?}");
}

#[test]
fn an_unknown_type_points_at_the_type_of_the_field() {
    let main = "id: t\nseq:\n  - id: a\n    type: u1\n  - id: b\n    type: nope\n";
    let (error, position) = locate(main, &[]);
    assert!(matches!(error, DoeError::UnknownType { .. }), "{error:?}");
    assert_eq!(at(main, position), (6, 11, "nope"));
}

#[test]
fn an_unknown_type_in_a_nested_type_points_inside_it() {
    let main = "meta:\n  id: t\nseq:\n  - id: a\n    type: u1\ntypes:\n  header:\n    seq:\n      - id: x\n        type: nope\n";
    let (_, position) = locate(main, &[]);
    assert_eq!(at(main, position), (10, 15, "nope"));
}

#[test]
fn an_unknown_enum_points_at_the_enum_of_the_field() {
    let main = "id: t\nseq:\n  - id: kind\n    type: u1\n    enum: missing\n";
    let (error, position) = locate(main, &[]);
    assert!(matches!(error, DoeError::UnknownEnum { .. }), "{error:?}");
    assert_eq!(at(main, position), (5, 11, "missing"));
}

#[test]
fn an_expression_that_does_not_parse_points_at_it() {
    let main = "id: t\nseq:\n  - id: n\n    type: u1\n  - id: body\n    size: n +\n";
    let (error, position) = locate(main, &[]);
    assert!(matches!(error, DoeError::ExprParse { .. }), "{error:?}");
    assert_eq!(at(main, position), (6, 11, "n +"));
}

#[test]
fn a_forward_reference_points_at_the_size_that_makes_it() {
    let main = "id: t\nseq:\n  - id: body\n    size: later\n  - id: later\n    type: u1\n";
    let (error, position) = locate(main, &[]);
    assert!(matches!(error, DoeError::FieldError { .. }), "{error:?}");
    assert_eq!(at(main, position), (4, 11, "later"));
}

#[test]
fn a_field_error_without_an_expression_points_at_the_field() {
    let main = "id: t\nseq:\n  - id: a\n    type: u1\n  - id: blob\n    type: bytes\n";
    let (error, position) = locate(main, &[]);
    assert!(matches!(error, DoeError::FieldError { .. }), "{error:?}");
    assert_eq!(at(main, position), (6, 11, "bytes"));
}

#[test]
fn an_error_in_an_instance_points_at_it() {
    let main = "id: t\nseq:\n  - id: a\n    type: u1\ninstances:\n  twice:\n    value: a *\n";
    let (error, position) = locate(main, &[]);
    assert!(matches!(error, DoeError::FieldError { .. }), "{error:?}");
    assert_eq!(at(main, position), (7, 12, "a *"));
}

#[test]
fn a_missing_import_points_at_its_entry() {
    let main = "meta:\n  id: t\n  imports:\n    - there\n    - gone\nseq: []\n";
    let (error, position) = locate(main, &[("there.ksy", "meta:\n  id: there\nseq: []\n")]);
    assert!(
        matches!(error, DoeError::SchemaNotFound { .. }),
        "{error:?}"
    );
    assert_eq!(at(main, position), (5, 7, "gone"));
}

#[test]
fn a_duplicate_type_points_at_its_declaration_or_the_schema_id() {
    let main = "meta:\n  id: t\nseq: []\ntypes:\n  header:\n    seq: []\n";
    let map = SchemaMap::parse(main).unwrap();
    let duplicate = |name: &str| DoeError::DuplicateType {
        name: name.to_owned(),
        first: "a".to_owned(),
        second: "b".to_owned(),
    };
    let header = map.locate(&duplicate("header")).unwrap();
    assert_eq!((header.line, header.column), (5, 3));
    let id = map.locate(&duplicate("t")).unwrap();
    assert_eq!((id.line, id.column), (2, 7));
    assert_eq!(map.locate(&duplicate("elsewhere")), None);
}

#[test]
fn an_error_from_an_imported_schema_has_no_position_in_this_one() {
    let main = "meta:\n  id: t\n  imports:\n    - broken\nseq: []\n";
    let broken = "meta:\n  id: broken\nseq:\n  - id: x\n    type: nope\n";
    let (error, position) = locate(main, &[("broken.ksy", broken)]);
    assert!(matches!(error, DoeError::UnknownType { .. }), "{error:?}");
    assert_eq!(position, None);
    // In the text of the schema it is from, it has one.
    let (line, _, text) = at(broken, locate_error(broken, &error));
    assert_eq!((line, text), (5, "nope"));
}

#[test]
fn errors_without_a_place_in_the_text_have_none() {
    let map = SchemaMap::parse("id: t\nseq: []\n").unwrap();
    assert_eq!(map.locate(&DoeError::Schema("x".to_owned())), None);
    // A text that is not a schema has no map.
    assert!(SchemaMap::parse("[1, 2").is_none());
    assert!(SchemaMap::parse("seq: []\n").is_none());
}

#[test]
fn the_id_may_be_written_either_way() {
    let plain = SchemaMap::parse("id: t\nseq:\n  - id: a\n    size: 1 +\n").unwrap();
    let kaitai = SchemaMap::parse("meta:\n  id: t\nseq:\n  - id: a\n    size: 1 +\n").unwrap();
    let error = DoeError::ExprParse {
        expr: "1 +".to_owned(),
        message: "x".to_owned(),
    };
    assert_eq!(plain.locate(&error).map(|p| p.line), Some(4));
    assert_eq!(kaitai.locate(&error).map(|p| p.line), Some(5));
}
