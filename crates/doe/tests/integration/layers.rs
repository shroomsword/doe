//! Files inside files: raw deflate and gzip as `process:`, and the layers of a `.tar.gz`.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use doe::detect::Options;
use doe::layers::{MAX_DEPTH, unwrap};
use doe::parser::process::{gunzip, inflate_raw};
use flate2::Compression;
use flate2::write::{DeflateEncoder, GzEncoder};

fn types() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../types")
}

fn gz(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn raw(data: &[u8]) -> Vec<u8> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn raw_deflate_and_gzip_decode_and_stop_at_the_limit() {
    let text = b"hello hello hello hello hello".repeat(10);
    assert_eq!(inflate_raw(&raw(&text), 1000).unwrap(), text);
    assert_eq!(gunzip(&gz(&text), 1000).unwrap(), text);
    // a limit that the data goes past is an error that says so, and nothing more is produced
    let err = gunzip(&gz(&text), 100).unwrap_err();
    assert!(err.to_string().contains("limit"), "{err}");
    let err = inflate_raw(&raw(&text), 100).unwrap_err();
    assert!(err.to_string().contains("limit"), "{err}");
}

#[test]
fn gunzip_reads_every_member_and_checks_each_one() {
    let both = [gz(b"first "), gz(b"second")].concat();
    assert_eq!(gunzip(&both, 100).unwrap(), b"first second");
    // a flipped bit in the data, a wrong CRC, a cut stream and bytes that are not gzip are errors
    let mut bad_crc = gz(b"some data to check");
    let at = bad_crc.len() - 8;
    bad_crc[at] ^= 0xff;
    assert!(gunzip(&bad_crc, 100).is_err());
    let good = gz(&b"abcdefgh".repeat(50));
    assert!(gunzip(&good[..good.len() - 12], 1000).is_err());
    assert!(gunzip(b"not gzip at all", 100).is_err());
    assert!(inflate_raw(&[0xff, 0xff, 0xff], 100).is_err());
}

#[test]
fn a_schema_can_read_a_field_through_process_deflate_and_gzip() {
    let schema = "meta:\n  id: t\nseq:\n  - id: len\n    type: u1\n  - id: body\n    size: len\n    process: PROCESS\n  - id: after\n    type: u1\n";
    for (process, coded) in [("deflate", raw(b"hello")), ("gzip", gz(b"hello"))] {
        let mut data = vec![u8::try_from(coded.len()).unwrap()];
        data.extend(&coded);
        data.push(7);
        let request = serde_json::json!({"schema": schema.replace("PROCESS", process)});
        let response = doe::api::parse(&request.to_string(), &data);
        assert!(response.ok, "{process}: {:?}", response.error);
        let value = response.value.unwrap();
        assert_eq!(value["body"]["len"], 5, "{process}");
        assert_eq!(value["after"], 7, "{process}");
    }
    // data that is not what the process says names the field
    let bad = serde_json::json!({"schema": schema.replace("PROCESS", "gzip")});
    let response = doe::api::parse(&bad.to_string(), &[4, 1, 2, 3, 4, 7]);
    let message = response.error.unwrap().message;
    assert!(
        message.contains("body") && message.contains("gzip"),
        "{message}"
    );
}

#[test]
fn a_tar_gz_is_a_gzip_file_and_then_a_tar() {
    let file = types().join("samples/sample.tar.gz");
    let found = unwrap(&file, &[types()], &Options::default(), MAX_DEPTH, 1 << 20).unwrap();
    assert_eq!(found.describe(), "gzip_file > tar");
    // the second layer is the tar itself, byte for byte
    assert_eq!(found.data.len(), 1);
    assert_eq!(
        found.data[0],
        std::fs::read(types().join("samples/sample.tar")).unwrap()
    );
    assert!(found.notes.is_empty(), "{:?}", found.notes);
    // one detection for each layer
    assert_eq!(found.detections.len(), 2);
    // asking for the outer layer only is what detection did before
    let outer = unwrap(&file, &[types()], &Options::default(), 1, 1 << 20).unwrap();
    assert_eq!(outer.describe(), "gzip_file");
    assert_eq!(outer.data.len(), 0);
}

#[test]
fn a_payload_that_fits_nothing_or_cannot_be_decoded_leaves_the_outer_layer_with_a_note() {
    let found = unwrap(
        &types().join("samples/sample.txt.gz"),
        &[types()],
        &Options::default(),
        MAX_DEPTH,
        1 << 20,
    )
    .unwrap();
    assert_eq!(found.describe(), "gzip_file");
    assert!(
        found.notes.iter().any(|n| n.contains("fits no schema")),
        "{:?}",
        found.notes
    );
    // a corrupt stream
    let dir = tempfile::tempdir().unwrap();
    let mut data = std::fs::read(types().join("samples/sample.tar.gz")).unwrap();
    let at = data.len() - 8;
    data[at] ^= 0xff;
    let path = dir.path().join("bad.tar.gz");
    std::fs::write(&path, data).unwrap();
    let found = unwrap(&path, &[types()], &Options::default(), MAX_DEPTH, 1 << 20).unwrap();
    assert_eq!(found.describe(), "gzip_file");
    assert!(
        found.notes.iter().any(|n| n.contains("could not unwrap")),
        "{:?}",
        found.notes
    );
    // a limit that the file or what it decodes to goes past
    let found = unwrap(
        &types().join("samples/sample.tar.gz"),
        &[types()],
        &Options::default(),
        MAX_DEPTH,
        100,
    )
    .unwrap();
    assert_eq!(found.describe(), "gzip_file");
    assert!(
        found.notes.iter().any(|n| n.contains("limit")),
        "{:?}",
        found.notes
    );
}

#[test]
fn the_name_of_a_payload_drops_the_extension_that_the_outer_layer_added() {
    // the second layer is found by its name when it has no signature: a tar is `x.tar` inside `x.tar.gz` and `x.tgz`
    let dir = tempfile::tempdir().unwrap();
    let tar = std::fs::read(types().join("samples/sample.tar")).unwrap();
    for name in ["a.tar.gz", "a.tgz"] {
        let path = dir.path().join(name);
        std::fs::write(&path, gz(&tar)).unwrap();
        let found = unwrap(&path, &[types()], &Options::default(), MAX_DEPTH, 1 << 20).unwrap();
        assert_eq!(found.describe(), "gzip_file > tar", "{name}");
    }
}

#[test]
fn the_gzip_header_is_read_and_a_file_with_an_extra_field_is_too() {
    let named = doe::parse_file_with(
        &types().join("samples/sample.tar.gz"),
        "gzip_file",
        &[types()],
        Some(Path::new("/nonexistent/config.toml")),
        doe::OutputFormat::Json,
        &doe::ParseOptions::default(),
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&named).unwrap();
    assert_eq!(v["name"], "sample.tar");
    assert_eq!(v["method"]["name"], "deflate");
    assert_eq!(v["os"]["name"], "unix");
    assert_eq!(v["uncompressed_size"], 10240);
    let extra = doe::parse_file_with(
        &types().join("samples/sample.txt.gz"),
        "gzip_file",
        &[types()],
        Some(Path::new("/nonexistent/config.toml")),
        doe::OutputFormat::Json,
        &doe::ParseOptions::default(),
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&extra).unwrap();
    assert_eq!(v["extra"]["length"], 6);
    assert!(v["name"].is_null());
}

#[test]
fn the_payload_key_is_checked_like_the_others() {
    let unknown = "meta:\n  id: t\npayload:\n  process: gzip\n  to: 3\nseq: []\n";
    assert!(doe::schema::raw::RawSchema::from_yaml_str(unknown).is_err());
    let missing = "meta:\n  id: t\npayload:\n  from: 3\nseq: []\n";
    assert!(doe::schema::raw::RawSchema::from_yaml_str(missing).is_err());
}
