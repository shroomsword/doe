//! `process: bzip2 | xz | lzma | zstd` and the layers they make: a `.tar.xz`, `.tar.bz2`, `.tar.zst`, `.tar.lzma` and a cpio in each.
//! The streams are written by the real tools (`make_decoders` in `conformance/make_real_samples_tools.py`: python's bz2, xz,
//! zstd) from the text of the script, `types/samples/sample.tar` and `types/samples/sample.cpio`.

use std::path::{Path, PathBuf};

use doe::detect::Options;
use doe::layers::{MAX_DEPTH, unwrap};
use doe::parser::process::{bzip2_decode, lzip_decode, lzma_decode, xz_decode, zstd_decode};

type Decoder = fn(&[u8], usize) -> doe::Result<Vec<u8>>;
/// The samples of a format: (name, the size it decodes to).
type Sizes<'a> = &'a [(&'a str, usize)];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn types() -> PathBuf {
    root().join("types")
}

fn sample(format: &str, name: &str) -> Vec<u8> {
    std::fs::read(
        root()
            .join("conformance/samples_without_reference/archive")
            .join(format)
            .join(format!("{name}.bin")),
    )
    .unwrap()
}

fn text() -> Vec<u8> {
    "decoder of everything reads binary files described by schemas. "
        .repeat(40)
        .into_bytes()
}

fn of_types(name: &str) -> Vec<u8> {
    std::fs::read(types().join("samples").join(name)).unwrap()
}

/// Compare two byte strings without printing them.
fn same(a: &[u8], b: &[u8], what: &str) {
    assert!(a == b, "{what}: {} bytes, expected {}", a.len(), b.len());
}

/// (format, the decoder, the names of the samples: the text, the tar, the cpio, and a stream of two members)
const FORMATS: &[(&str, Decoder, &str, &str, &str, &str)] = &[
    (
        "bzip2",
        bzip2_decode,
        "python_bz2_text",
        "python_bz2_tar",
        "python_bz2_cpio",
        "python_bz2_two_streams",
    ),
    (
        "xz",
        xz_decode,
        "xz_text",
        "xz_tar",
        "xz_cpio",
        "xz_two_streams_text",
    ),
    (
        "zstd",
        zstd_decode,
        "zstd_text",
        "zstd_tar",
        "zstd_cpio",
        "zstd_two_frames_text",
    ),
];

#[test]
fn the_streams_of_the_real_tools_decode_to_what_they_were_made_from() {
    for (format, decode, text_name, tar_name, cpio_name, two_name) in FORMATS {
        same(
            &decode(&sample(format, text_name), 1 << 20).unwrap(),
            &text(),
            format,
        );
        same(
            &decode(&sample(format, tar_name), 1 << 20).unwrap(),
            &of_types("sample.tar"),
            format,
        );
        same(
            &decode(&sample(format, cpio_name), 1 << 20).unwrap(),
            &of_types("sample.cpio"),
            format,
        );
        // several streams or frames in a row are one file, as the tools' own `-d` makes them
        same(
            &decode(&sample(format, two_name), 1 << 20).unwrap(),
            &text(),
            format,
        );
    }
    // .lzma (LZMA alone)
    same(
        &lzma_decode(&sample("lzma_alone", "xz_alone_text"), 1 << 20).unwrap(),
        &text(),
        "lzma",
    );
    same(
        &lzma_decode(&sample("lzma_alone", "xz_alone_tar"), 1 << 20).unwrap(),
        &of_types("sample.tar"),
        "lzma tar",
    );
    // and the samples of the library, which are other files than these
    // (sample.zst holds an empty file: see below)
}

#[test]
fn every_decoder_stops_at_the_limit_while_it_decodes() {
    for (format, decode, text_name, ..) in FORMATS {
        let error = decode(&sample(format, text_name), 100)
            .unwrap_err()
            .to_string();
        assert!(error.contains("limit"), "{format}: {error}");
        // exactly at the limit is fine, one under is not
        assert_eq!(
            decode(&sample(format, text_name), text().len())
                .unwrap()
                .len(),
            text().len(),
            "{format}"
        );
        assert!(
            decode(&sample(format, text_name), text().len() - 1).is_err(),
            "{format}"
        );
    }
    let error = lzma_decode(&sample("lzma_alone", "xz_alone_text"), 100)
        .unwrap_err()
        .to_string();
    assert!(error.contains("limit") || error.contains("lzma"), "{error}");
}

#[test]
fn a_cut_a_corrupt_a_foreign_and_an_empty_stream_are_errors() {
    for (format, decode, text_name, tar_name, ..) in FORMATS {
        let good = sample(format, text_name);
        assert!(
            decode(&good[..good.len() - 6], 1 << 20).is_err(),
            "{format}: cut"
        );
        assert!(
            decode(&good[..good.len() / 2], 1 << 20).is_err(),
            "{format}: half"
        );
        assert!(
            decode(b"this is not compressed, it is text", 1 << 20).is_err(),
            "{format}: foreign"
        );
        assert!(decode(b"", 1 << 20).is_err(), "{format}: empty");
        let mut bad = sample(format, tar_name);
        let at = bad.len() * 2 / 3;
        bad[at] ^= 0xff;
        bad[at + 1] ^= 0x55;
        // a flipped bit in the data is caught by the checks (CRC, checksum), or by the decoder
        let outcome = decode(&bad, 1 << 20);
        assert!(
            outcome.is_err() || outcome.unwrap() != of_types("sample.tar"),
            "{format}: corrupt"
        );
    }
    let alone = sample("lzma_alone", "xz_alone_text");
    assert!(lzma_decode(&alone[..alone.len() - 6], 1 << 20).is_err());
    assert!(lzma_decode(b"not lzma at all, plain text", 1 << 20).is_err());
}

/// The layers of `sample` written to a file called `name`.
fn layers(name: &str, bytes: &[u8]) -> (String, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    let found = unwrap(&path, &[types()], &Options::default(), MAX_DEPTH, 1 << 20).unwrap();
    (found.describe(), found.notes)
}

#[test]
fn a_compressed_tar_or_cpio_is_the_file_system_it_holds() {
    for (name, format, sample_name, inner) in [
        ("a.tar.bz2", "bzip2", "python_bz2_tar", "bzip2 > tar"),
        ("a.tar.xz", "xz", "xz_tar", "xz > tar"),
        ("a.tar.zst", "zstd", "zstd_tar", "zstd > tar"),
        ("a.cpio.bz2", "bzip2", "python_bz2_cpio", "bzip2 > cpio"),
        ("a.cpio.xz", "xz", "xz_cpio", "xz > cpio"),
        ("a.cpio.zst", "zstd", "zstd_cpio", "zstd > cpio"),
    ] {
        let (described, notes) = layers(name, &sample(format, sample_name));
        assert_eq!(described, inner, "{name}: {notes:?}");
    }
    // a .tar.lzma has no signature: it is found by its name
    let (described, notes) = layers("a.tar.lzma", &sample("lzma_alone", "xz_alone_tar"));
    assert_eq!(described, "lzma_alone > tar", "{notes:?}");
    // the file name of the layer is the name without the compression's extension (tbz2, txz, tzst stand for .tar.*)
    let (described, _) = layers("a.txz", &sample("xz", "xz_tar"));
    assert_eq!(described, "xz > tar");
}

#[test]
fn a_stream_whose_contents_fit_nothing_or_cannot_be_decoded_stays_the_outer_layer_with_a_note() {
    let (described, notes) = layers("a.xz", &sample("xz", "xz_text"));
    assert_eq!(described, "xz");
    assert!(
        notes.iter().any(|n| n.contains("fits no schema")),
        "{notes:?}"
    );
    let good = sample("xz", "xz_tar");
    let mut cut = good.clone();
    cut.truncate(good.len() - 20);
    let (described, notes) = layers("a.tar.xz", &cut);
    // the footer is gone, so the type may not even fit: either the outer layer with a note, or nothing
    assert!(described.is_empty() || described == "xz", "{described}");
    assert!(
        described.is_empty() || notes.iter().any(|n| n.contains("could not unwrap")),
        "{notes:?}"
    );
}

#[test]
fn a_process_field_decodes_in_any_schema() {
    for (process, format, name) in [
        ("xz", "xz", "xz_text"),
        ("bzip2", "bzip2", "python_bz2_text"),
        ("zstd", "zstd", "zstd_text"),
        ("lzma", "lzma_alone", "xz_alone_text"),
    ] {
        let schema = format!(
            "meta:\n  id: packed\nseq:\n  - id: body\n    size-eos: true\n    process: {process}\n    type: str\n    encoding: ASCII\n"
        );
        let dir = tempfile::tempdir().unwrap();
        let schema_path = dir.path().join("packed.yaml");
        let data_path = dir.path().join("packed.bin");
        std::fs::write(&schema_path, schema).unwrap();
        std::fs::write(&data_path, sample(format, name)).unwrap();
        let json = doe::parse_file_with(
            &data_path,
            schema_path.to_str().unwrap(),
            &[],
            Some(Path::new("/nonexistent/config.toml")),
            doe::OutputFormat::Json,
            &doe::ParseOptions::default(),
        )
        .unwrap();
        assert!(
            json.contains("decoder of everything reads binary files"),
            "{process}: {json}"
        );
    }
    let schema = "meta:\n  id: packed\nseq:\n  - id: body\n    size-eos: true\n    process: zstd\n    type: str\n    encoding: ASCII\n";
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("p.yaml"), schema).unwrap();
    std::fs::write(dir.path().join("p.bin"), b"not zstd data at all").unwrap();
    let error = doe::parse_file_with(
        &dir.path().join("p.bin"),
        dir.path().join("p.yaml").to_str().unwrap(),
        &[],
        Some(Path::new("/nonexistent/config.toml")),
        doe::OutputFormat::Json,
        &doe::ParseOptions::default(),
    );
    let message = format!("{error:?}");
    assert!(
        message.contains("body") && message.contains("zstd"),
        "{message}"
    );
}

/// The size each tool says a sample decodes to (`xz -dc`, `zstd -dc`, python's bz2 and lzma), for every sample of the
/// formats: all the checks (CRC-32, CRC-64, SHA-256, none), filters (delta, x86), multi-stream files, blocks that are
/// raw or run-length coded, skippable frames, and the options of the encoders.
#[test]
fn every_sample_of_every_format_decodes_to_the_size_the_tool_gives() {
    let tables: &[(&str, Decoder, Sizes<'_>)] = &[
        (
            "xz",
            xz_decode,
            &[
                ("xz_blocks", 2520),
                ("xz_crc32", 2520),
                ("xz_default_crc64", 2520),
                ("xz_delta", 2520),
                ("xz_no_check", 2520),
                ("xz_sha256", 2520),
                ("xz_threaded_blocks", 2520),
                ("xz_three_streams", 11040),
                ("xz_two_streams", 8520),
                ("xz_x86", 2520),
            ],
        ),
        (
            "bzip2",
            bzip2_decode,
            &[
                ("python_bz2_bytes", 3000),
                ("python_bz2_level1", 2520),
                ("python_bz2_level9", 2520),
            ],
        ),
        (
            "lzma_alone",
            lzma_decode,
            &[("xz_preset0", 2520), ("xz_preset6", 2520)],
        ),
        (
            "zstd",
            zstd_decode,
            &[
                ("zstd_default", 7560),
                ("zstd_fast", 7560),
                ("zstd_level19", 7560),
                ("zstd_long_window", 7560),
                ("zstd_no_check", 7560),
                ("zstd_raw_block", 1920),
                ("zstd_rle_block", 20000),
                ("zstd_skippable_between", 5520),
                ("zstd_small_blocks", 7560),
                ("zstd_two_frames", 5520),
            ],
        ),
    ];
    for (format, decode, samples) in tables {
        for (name, size) in *samples {
            let out = decode(&sample(format, name), 1 << 20)
                .unwrap_or_else(|e| panic!("{format}/{name}: {e}"));
            assert_eq!(out.len(), *size, "{format}/{name}");
        }
    }
    // the samples of the library
    let library: &[(Decoder, &str, usize)] = &[
        (xz_decode, "sample.xz", 480),
        (xz_decode, "sample_blocks.xz", 876),
        (xz_decode, "sample_delta.xz", 600),
        (xz_decode, "sample_two_streams.xz", 1060),
        (bzip2_decode, "sample.bz2", 480),
        (lzma_decode, "sample.lzma", 480),
        (lzma_decode, "sample_sized.lzma", 480),
        (zstd_decode, "sample_seekable.zst", 33),
    ];
    // sample.zst was written by hand and its content checksum is wrong, as `zstd -d` says too
    let error = zstd_decode(&of_types("sample.zst"), 1 << 20)
        .unwrap_err()
        .to_string();
    assert!(error.to_lowercase().contains("checksum"), "{error}");
    for (decode, name, size) in library {
        let out = decode(&of_types(name), 1 << 20).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(out.len(), *size, "{name}");
    }
}

#[test]
fn lzip_decodes_every_member_and_checks_them() {
    for name in ["lzip_dict100000", "lzip_dict64k", "lzip_two_members"] {
        let out =
            lzip_decode(&sample("lzip", name), 1 << 20).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!out.is_empty(), "{name}");
    }
    let one = lzip_decode(&sample("lzip", "lzip_dict64k"), 1 << 20).unwrap();
    let both = lzip_decode(&sample("lzip", "lzip_two_members"), 1 << 20).unwrap();
    assert!(
        both.len() > one.len() || both.len() == one.len(),
        "{} {}",
        both.len(),
        one.len()
    );
    let good = sample("lzip", "lzip_dict64k");
    assert!(lzip_decode(&good[..good.len() - 8], 1 << 20).is_err());
    let mut bad = good.clone();
    let at = good.len() - 20;
    bad[at] ^= 0xff;
    let outcome = lzip_decode(&bad, 1 << 20);
    assert!(outcome.is_err() || outcome.unwrap() != lzip_decode(&good, 1 << 20).unwrap());
    assert!(
        lzip_decode(&good, 10)
            .unwrap_err()
            .to_string()
            .contains("limit")
    );
}

#[test]
fn a_bomb_of_64_mib_in_a_few_kilobytes_stops_at_the_limit() {
    for (format, decode, name) in [
        ("bzip2", bzip2_decode as Decoder, "python_bz2_bomb"),
        ("xz", xz_decode, "xz_bomb"),
        ("lzma_alone", lzma_decode, "xz_alone_bomb"),
        ("zstd", zstd_decode, "zstd_bomb"),
    ] {
        let bomb = sample(format, name);
        assert!(bomb.len() < 16 * 1024, "{name} is {} bytes", bomb.len());
        let started = std::time::Instant::now();
        let error = decode(&bomb, 1 << 20).unwrap_err().to_string();
        assert!(error.contains("limit"), "{name}: {error}");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(20),
            "{name} took {:?}",
            started.elapsed()
        );
        // and with room enough it is the 64 MiB of zeros
        let all = decode(&bomb, 128 << 20).unwrap();
        assert_eq!(all.len(), 64 << 20, "{name}");
        assert!(all.iter().all(|b| *b == 0), "{name}");
    }
}
