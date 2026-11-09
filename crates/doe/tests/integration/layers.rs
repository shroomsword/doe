//! Files inside files: raw deflate, gzip and Brotli as `process:`, and the layers of a `.tar.gz` and a `.tar.br`.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use doe::detect::Options;
use doe::layers::{MAX_DEPTH, unwrap};
use doe::parser::process::{brotli_decode, gunzip, inflate_raw};
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

/// A real Brotli stream that cramjam (the Rust encoder) wrote, from the conformance samples.
fn brotli_sample(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/samples_without_reference/archive/brotli")
        .join(format!("{name}.bin"));
    std::fs::read(path).unwrap()
}

fn sha256(data: &[u8]) -> String {
    doe::filesystem::digest_of(
        doe::filesystem::Algorithm::Sha256,
        data,
        (0, data.len() as u64),
    )
    .unwrap()
}

#[test]
fn brotli_decodes_the_streams_of_a_real_encoder_and_the_hand_made_one() {
    // the text at the lowest and the highest quality, noise (in an uncompressed meta-block or a compressed one), nothing
    let text = "73ca8906cff2c1c6a934d2c9197df3766bcc95599961ddf5e093580fdcabf657";
    let noise = "caf5bf0a6be52b5759719cc653e906d8992f4c93a2ccd85af24a8332cda350b3";
    for (file, length, hash) in [
        ("cramjam_text_q1", 2520, text),
        ("cramjam_text_q11", 2520, text),
        ("cramjam_noise_q1", 1280, noise),
        ("cramjam_noise_q11", 1280, noise),
    ] {
        let out = brotli_decode(&brotli_sample(file), 1 << 20).unwrap();
        assert_eq!(out.len(), length, "{file}");
        assert_eq!(sha256(&out), hash, "{file}");
    }
    assert_eq!(
        brotli_decode(&brotli_sample("cramjam_empty"), 100).unwrap(),
        Vec::<u8>::new()
    );
    // the stream of types/samples/sample.br holds an uncompressed meta-block (and a metadata one that decodes to nothing)
    let sample = std::fs::read(types().join("samples/sample.br")).unwrap();
    let out = brotli_decode(&sample, 100).unwrap();
    assert!(!out.is_empty(), "{out:?}");
}

#[test]
fn brotli_stops_at_the_limit_and_says_so_for_a_cut_a_corrupt_and_a_foreign_stream() {
    let text = brotli_sample("cramjam_text_q11");
    let err = brotli_decode(&text, 100).unwrap_err();
    assert!(err.to_string().contains("limit"), "{err}");
    // cut: the end of the input in the middle of a meta-block
    let noise = brotli_sample("cramjam_noise_q1");
    assert!(brotli_decode(&noise[..noise.len() - 10], 1 << 20).is_err());
    assert!(brotli_decode(&text[..text.len() - 3], 1 << 20).is_err());
    // corrupt: flipped bits in the compressed part
    let mut bad = text.clone();
    let middle = bad.len() / 2;
    bad[middle] ^= 0xff;
    bad[middle + 1] ^= 0xaa;
    let outcome = brotli_decode(&bad, 1 << 20);
    assert!(outcome.is_err() || outcome.unwrap() != brotli_decode(&text, 1 << 20).unwrap());
    // bytes after the end of the stream are not looked at (the decoder reads ahead, so it cannot tell where the stream
    // ended in the input); the result is the stream's
    let trailing = [text.clone(), b"extra bytes".to_vec()].concat();
    assert_eq!(
        brotli_decode(&trailing, 1 << 20).unwrap(),
        brotli_decode(&text, 1 << 20).unwrap()
    );
    // not Brotli at all
    assert!(brotli_decode(b"this is not a brotli stream, it is text", 1 << 20).is_err());
    assert!(brotli_decode(&[], 100).is_err());
    // the message says what it is
    let err = brotli_decode(&noise[..20], 1 << 20)
        .unwrap_err()
        .to_string();
    assert!(err.contains("brotli"), "{err}");
}

#[test]
fn a_schema_can_read_a_field_through_process_brotli() {
    let schema = "meta:
  id: t
seq:
  - id: len
    type: u1
  - id: body
    size: len
    process: brotli
    type: str
    encoding: ASCII
  - id: after
    type: u1
";
    let coded = brotli_sample("cramjam_text_q11");
    let mut data = vec![u8::try_from(coded.len()).unwrap()];
    data.extend(&coded);
    data.push(7);
    let request = serde_json::json!({"schema": schema});
    let response = doe::api::parse(&request.to_string(), &data);
    assert!(response.ok, "{:?}", response.error);
    let value = response.value.unwrap();
    let body = value["body"].as_str().unwrap();
    assert_eq!(body.len(), 2520);
    assert!(body.starts_with("decoder of everything reads binary files"));
    assert_eq!(value["after"], 7);
    // the trace marks what the process made as not in the input
    let trace = response.trace.unwrap();
    assert!(trace.children.iter().any(|c| c.name == "body"));
    // a stream that is not Brotli names the field and the process
    let bad = doe::api::parse(&request.to_string(), &[4, 1, 2, 3, 4, 7]);
    let message = bad.error.unwrap().message;
    assert!(
        message.contains("body") && message.contains("brotli"),
        "{message}"
    );
    // and a field whose stream is cut
    let cut = [&[10_u8][..], &coded[..10], &[7][..]].concat();
    let cut = doe::api::parse(&request.to_string(), &cut);
    assert!(!cut.ok);
    assert!(cut.error.unwrap().message.contains("body"));
    // the process list in the error for a name that is not one
    let unknown = serde_json::json!({"schema": schema.replace("brotli", "lz77")});
    let message = doe::api::parse(&unknown.to_string(), &data).diagnostics[0]
        .message
        .clone();
    assert!(message.contains("brotli"), "{message}");
}

#[test]
fn a_tar_br_is_a_brotli_file_and_then_a_tar_and_no_unwrap_shows_the_stream() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("sample.tar.br");
    std::fs::write(&file, brotli_sample("cramjam_tar")).unwrap();
    let found = unwrap(&file, &[types()], &Options::default(), MAX_DEPTH, 1 << 20).unwrap();
    assert_eq!(found.describe(), "brotli > tar");
    assert_eq!(found.data.len(), 1);
    assert_eq!(
        found.data[0],
        std::fs::read(types().join("samples/sample.tar")).unwrap()
    );
    assert!(found.notes.is_empty(), "{:?}", found.notes);
    // only the outermost layer: the stream itself
    let outer = unwrap(&file, &[types()], &Options::default(), 1, 1 << 20).unwrap();
    assert_eq!(outer.describe(), "brotli");
    // a file that fits no schema once decoded stops with a note, it does not fail
    let plain = dir.path().join("words.txt.br");
    std::fs::write(&plain, brotli_sample("cramjam_text_q11")).unwrap();
    let found = unwrap(&plain, &[types()], &Options::default(), MAX_DEPTH, 1 << 20).unwrap();
    assert_eq!(found.describe(), "brotli");
    assert_eq!(found.notes.len(), 1, "{:?}", found.notes);
}
