//! Finding the schema of a file among the types of the include paths.

use std::path::{Path, PathBuf};

use doe::detect::{Magic, Options, Outcome, Why, detect_file, magic_of};
use doe::schema::compiler::Compiler;

fn schema(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(format!("{name}.yaml")), text).unwrap();
}

/// A schema `id` that starts with `contents` (as the text of a YAML list) and
/// then reads `rest`.
fn with_signature(id: &str, contents: &str, rest: &str) -> String {
    format!("meta:\n  id: {id}\nseq:\n  - id: magic\n    contents: {contents}\n{rest}")
}

fn file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn library() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn detect(dir: &Path, file: &Path, options: &Options) -> doe::detect::Detection {
    detect_file(file, &[dir.to_owned()], options).unwrap()
}

#[test]
fn the_signature_of_a_schema_is_what_its_first_fields_say() {
    let mut compiler = Compiler::new(vec![]);
    let text = "meta:
  id: t
seq:
  - id: head
    contents: [0x89, PNG]
  - id: version
    type: u2le
  - id: second
    contents: XY
  - id: size
    type: u4le
  - id: tail
    type: str
    size: size
    encoding: ascii
  - id: late
    contents: NO
";
    compiler
        .process_schema(&doe::schema::raw::RawSchema::from_yaml_str(text).unwrap())
        .unwrap();
    let magic = magic_of(&compiler.registry, "t");
    // the u2 is skipped over, the string of a size that is not fixed ends it
    assert_eq!(
        magic,
        [
            Magic {
                offset: 0,
                bytes: vec![0x89, b'P', b'N', b'G']
            },
            Magic {
                offset: 6,
                bytes: b"XY".to_vec()
            },
        ]
    );
}

#[test]
fn of_two_schemas_for_one_signature_the_one_that_fits_is_chosen_and_the_other_is_reported() {
    let dir = library();
    // a_first is tried first (the same signature, the order of the names) and does not fit
    schema(
        dir.path(),
        "a_first",
        &with_signature(
            "a_first",
            "AB",
            "  - id: big\n    type: u4le\n  - id: more\n    type: u4le\n",
        ),
    );
    schema(
        dir.path(),
        "b_second",
        &with_signature("b_second", "AB", "  - id: n\n    type: u1\n"),
    );
    // another signature: left out without being tried
    schema(
        dir.path(),
        "c_other",
        &with_signature("c_other", "CD", "  - id: n\n    type: u1\n"),
    );
    let data = file(dir.path(), "data.bin", b"ABx");
    let found = detect(dir.path(), &data, &Options::default());

    assert_eq!(found.known, 3);
    assert_eq!(found.excluded, 1);
    assert_eq!(found.attempts.len(), 2);
    assert_eq!(found.attempts[0].name, "a_first");
    assert_eq!(found.attempts[0].why, Why::Signature);
    assert!(
        matches!(&found.attempts[0].outcome, Outcome::Failed(_)),
        "{:?}",
        found.attempts[0].outcome
    );
    assert_eq!(found.attempts[1].name, "b_second");
    assert_eq!(found.attempts[1].outcome, Outcome::Fit { consumed: 3 });
    assert_eq!(found.chosen().unwrap().name, "b_second");
    let report = found.report();
    assert!(
        report.contains("a_first") && report.contains("did not fit"),
        "{report}"
    );
    assert!(
        report.contains("b_second") && report.contains("parsed"),
        "{report}"
    );
    assert!(report.contains("1 left out"), "{report}");
}

#[test]
fn a_schema_without_a_signature_is_found_by_the_name_of_the_file_and_the_rest_only_when_asked() {
    let dir = library();
    schema(
        dir.path(),
        "plain",
        "meta:\n  id: plain\n  file-extension: xyz\nseq:\n  - id: n\n    type: u1\n",
    );
    schema(
        dir.path(),
        "other",
        "meta:\n  id: other\nseq:\n  - id: n\n    type: u1\n",
    );
    let named = file(dir.path(), "dump.XYZ", &[1, 2]);
    let found = detect(dir.path(), &named, &Options::default());
    assert_eq!(found.chosen().unwrap().name, "plain");
    assert_eq!(found.chosen().unwrap().why, Why::Name);
    assert_eq!(found.attempts.len(), 1);

    // no name that fits: nothing is tried, and all of them are when asked
    let unnamed = file(dir.path(), "dump", &[1, 2]);
    let found = detect(dir.path(), &unnamed, &Options::default());
    assert_eq!((found.chosen, found.attempts.len()), (None, 0));
    let found = detect(
        dir.path(),
        &unnamed,
        &Options {
            all: true,
            ..Options::default()
        },
    );
    // the first that parses without an error wins, even if it leaves bytes
    assert_eq!(found.attempts.len(), 1);
    assert_eq!(found.attempts[0].why, Why::Remaining);
    assert_eq!(found.chosen().unwrap().name, "other");
}

#[test]
fn a_schema_that_reads_too_much_is_stopped_and_the_next_one_is_tried() {
    let dir = library();
    schema(
        dir.path(),
        "a_long",
        &with_signature(
            "a_long",
            "AB",
            "  - id: items\n    type: u1\n    repeat: eos\n",
        ),
    );
    schema(
        dir.path(),
        "b_short",
        &with_signature("b_short", "AB", "  - id: n\n    type: u1\n"),
    );
    let mut bytes = b"AB".to_vec();
    bytes.extend(std::iter::repeat_n(7_u8, 1000));
    let data = file(dir.path(), "data.bin", &bytes);
    let options = Options {
        max_fields: 100,
        ..Options::default()
    };
    let found = detect(dir.path(), &data, &options);
    assert!(
        matches!(&found.attempts[0].outcome, Outcome::OutOfLimits(_)),
        "{:?}",
        found.attempts[0].outcome
    );
    assert_eq!(found.chosen().unwrap().name, "b_short");
}

#[test]
fn a_schema_that_cannot_be_compiled_is_reported_and_does_not_stop_the_others() {
    let dir = library();
    schema(
        dir.path(),
        "a_broken",
        "meta:\n  id: a_broken\n  file-extension: dat\nseq:\n  - id: n\n    type: nonesuch\n",
    );
    schema(
        dir.path(),
        "b_fine",
        "meta:\n  id: b_fine\n  file-extension: dat\nseq:\n  - id: n\n    type: u1\n",
    );
    let data = file(dir.path(), "x.dat", &[1]);
    let found = detect(dir.path(), &data, &Options::default());
    assert!(matches!(&found.attempts[0].outcome, Outcome::Unusable(_)));
    assert_eq!(found.chosen().unwrap().name, "b_fine");
}

#[test]
fn a_file_that_nothing_fits_has_every_reason() {
    let dir = library();
    schema(
        dir.path(),
        "one",
        &with_signature("one", "AB", "  - id: n\n    type: u4le\n"),
    );
    let data = file(dir.path(), "x.bin", b"ABC");
    let found = detect(dir.path(), &data, &Options::default());
    assert_eq!(found.chosen, None);
    assert_eq!(found.attempts.len(), 1);
    assert!(found.report().contains("did not fit"));
}

#[test]
fn the_source_of_one_type_has_its_signature_and_names() {
    let dir = library();
    schema(
        dir.path(),
        "png_file",
        &with_signature("png", "[0x89, PNG]", "").replace("seq:", "  file-extension: png\nseq:"),
    );
    schema(dir.path(), "plain", "meta:\n  id: plain\nseq: []\n");
    let include = vec![dir.path().to_owned()];
    let types = doe::config::discover_types(&include).unwrap();
    let png = types.iter().find(|t| t.id == "png").unwrap();
    let (source, error) = doe::detect::source_of(&include, png);
    assert_eq!(error, None);
    assert_eq!(source.name, "png");
    assert_eq!(source.magic.len(), 1);
    // the id, the name of the file and the extension
    for name in ["png", "png_file"] {
        assert!(
            source.names.iter().any(|n| n == name),
            "{name}: {:?}",
            source.names
        );
    }
    let plain = types.iter().find(|t| t.id == "plain").unwrap();
    let (source, error) = doe::detect::source_of(&include, plain);
    assert_eq!((source.magic.len(), error), (0, None));
}

#[test]
fn a_type_that_does_not_compile_has_the_error_and_no_signature() {
    let dir = library();
    schema(
        dir.path(),
        "bad",
        "meta:\n  id: bad\nseq:\n  - id: x\n    type: nowhere\n",
    );
    let include = vec![dir.path().to_owned()];
    let types = doe::config::discover_types(&include).unwrap();
    let (source, error) = doe::detect::source_of(&include, &types[0]);
    assert_eq!(source.magic, []);
    assert!(error.is_some_and(|e| e.contains("nowhere")), "{source:?}");
}

#[test]
fn the_signature_of_a_type_of_a_schema_is_asked_by_the_type() {
    let schema = "meta:\n  id: top\nseq:\n  - id: a\n    type: u1\ntypes:\n  inner:\n    seq:\n      - id: m\n        contents: XY\n";
    let ask = |type_name: Option<&str>| {
        let mut request = serde_json::json!({"schema": schema});
        if let Some(name) = type_name {
            request["type"] = serde_json::json!(name);
        }
        doe::api::signature(&request.to_string())
    };
    assert_eq!(ask(None).magic, []);
    let inner = ask(Some("top::inner"));
    assert_eq!(inner.magic.len(), 1);
    assert_eq!(inner.magic[0].bytes, b"XY");
}

/// A capture file as `pcap` reads it: a magic number that is an enum, then the header.
const CAPTURE: &str = "meta:
  id: capture
  file-extension: cap
seq:
  - id: magic_number
    type: u4be
    enum: magic
  - id: rest
    size-eos: true
enums:
  magic:
    0xd4c3b2a1: le_microseconds
    0xa1b2c3d4: be_microseconds
    0x4d3cb2a1: le_nanoseconds
    0xa1b23c4d: be_nanoseconds
";

/// The format the name says: it starts with a block type, which is no signature, and checks it.
const BLOCKS: &str = "meta:
  id: blocks
  file-extension: cap
  endian: le
seq:
  - id: block_type
    type: u4
    valid: 0x0a0d0d0a
";

#[test]
fn a_magic_number_that_is_an_enum_gives_a_signature_for_each_value() {
    let mut compiler = Compiler::new(vec![]);
    compiler
        .process_schema(&doe::schema::raw::RawSchema::from_yaml_str(CAPTURE).unwrap())
        .unwrap();
    let signatures = doe::detect::signatures_of(&compiler.registry, "capture");
    let bytes: Vec<&[u8]> = signatures
        .iter()
        .map(|s| {
            assert_eq!(s.len(), 1);
            assert_eq!(s[0].offset, 0);
            s[0].bytes.as_slice()
        })
        .collect();
    // in the order of the numbers, as the file has them: a u4be
    assert_eq!(
        bytes,
        [
            &[0x4d, 0x3c, 0xb2, 0xa1][..],
            &[0xa1, 0xb2, 0x3c, 0x4d],
            &[0xa1, 0xb2, 0xc3, 0xd4],
            &[0xd4, 0xc3, 0xb2, 0xa1],
        ]
    );
    // magic_of says nothing of it: one signature is one start
    assert_eq!(magic_of(&compiler.registry, "capture"), []);
}

#[test]
fn an_enum_that_is_not_a_magic_number_says_nothing_of_the_start_of_the_file() {
    // a kind may have values the enum does not list
    let text = CAPTURE.replace("magic_number", "kind");
    let mut compiler = Compiler::new(vec![]);
    compiler
        .process_schema(&doe::schema::raw::RawSchema::from_yaml_str(&text).unwrap())
        .unwrap();
    assert_eq!(
        doe::detect::signatures_of(&compiler.registry, "capture"),
        Vec::<Vec<Magic>>::new()
    );
}

#[test]
fn a_capture_with_the_name_of_another_format_is_found_by_its_signature() {
    let dir = library();
    schema(dir.path(), "capture", CAPTURE);
    schema(dir.path(), "blocks", BLOCKS);
    // every byte order and precision, with the name of the other format
    for magic in [
        [0xd4, 0xc3, 0xb2, 0xa1],
        [0xa1, 0xb2, 0xc3, 0xd4],
        [0x4d, 0x3c, 0xb2, 0xa1],
        [0xa1, 0xb2, 0x3c, 0x4d],
    ] {
        let mut bytes = magic.to_vec();
        bytes.extend_from_slice(&[2, 0, 4, 0, 0, 0, 0, 0]);
        let data = file(dir.path(), "x.cap", &bytes);
        let found = detect(dir.path(), &data, &Options::default());
        let chosen = found.chosen().unwrap();
        assert_eq!(chosen.name, "capture", "{magic:?}: {}", found.report());
        // the signature comes before the name: the format the name fits is not even tried
        assert_eq!(chosen.why, Why::SignatureAndName);
        assert_eq!(found.attempts.len(), 1, "{}", found.report());
    }
    // a file that is what its name says still is
    let data = file(dir.path(), "y.cap", &[0x0a, 0x0d, 0x0d, 0x0a]);
    let found = detect(dir.path(), &data, &Options::default());
    assert_eq!(found.chosen().unwrap().name, "blocks", "{}", found.report());
    // a start that is no value of the enum leaves the capture out
    let data = file(dir.path(), "z.cap", &[1, 2, 3, 4, 5, 6, 7, 8]);
    let found = detect(dir.path(), &data, &Options::default());
    assert!(found.chosen().is_none());
    assert_eq!(found.excluded, 2);
}

#[test]
fn when_all_are_tried_a_schema_that_checks_nothing_comes_after_one_that_does() {
    let dir = library();
    // a_loose is first by name and fits any file
    schema(
        dir.path(),
        "a_loose",
        "meta:\n  id: a_loose\nseq:\n  - id: all\n    size-eos: true\n",
    );
    schema(
        dir.path(),
        "b_strict",
        "meta:\n  id: b_strict\nseq:\n  - id: n\n    type: u1\n    valid: 7\n  - id: all\n    size-eos: true\n",
    );
    let data = file(dir.path(), "data", &[7, 1, 2]);
    let all = Options {
        all: true,
        ..Options::default()
    };
    let found = detect(dir.path(), &data, &all);
    assert_eq!(
        found.chosen().unwrap().name,
        "b_strict",
        "{}",
        found.report()
    );
}
