//! The types of the library (`types/`) on the samples that `types/make_samples.py` makes.

use std::path::{Path, PathBuf};

use doe::{OutputFormat, parse_file};
use serde_json::Value as Json;

fn types() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../types")
}

/// The sample `file` parsed with the type `name` of the library, as JSON.
fn decode(name: &str, file: &str) -> Json {
    let text = parse_file(
        &types().join("samples").join(file),
        name,
        &[types()],
        Some(Path::new("/nonexistent/config.toml")),
        OutputFormat::Json,
    )
    .unwrap_or_else(|e| panic!("{file} as {name}: {e}"));
    serde_json::from_str(&text).unwrap()
}

fn name_of(v: &Json) -> &str {
    v["name"].as_str().unwrap_or("<unnamed>")
}

#[test]
fn tar_has_the_entries_with_their_headers_and_contents() {
    let v = decode("tar", "sample.tar");
    let entries = v["entries"].as_array().unwrap();
    // three entries, and the zeroed block that ends the archive
    assert_eq!(entries.len(), 4);
    let names: Vec<&str> = entries
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["docs/", "docs/hello.txt", "data.bin", ""]);
    assert_eq!(entries[0]["type_flag"], "5");
    assert_eq!(entries[1]["type_flag"], "0");
    assert_eq!(entries[1]["size_value"], 6);
    assert_eq!(entries[1]["mode_value"], 0o644);
    assert_eq!(entries[1]["mtime_value"], 1_700_000_000);
    assert_eq!(entries[1]["magic"], "ustar");
    assert_eq!(entries[1]["user_name"], "doe");
    assert_eq!(entries[1]["contents"]["hex"], "68656c6c6f0a");
    assert_eq!(entries[2]["size_value"], 10);
    assert_eq!(entries[2]["contents"]["len"], 10);
    // the contents are padded to a block
    assert_eq!(entries[2]["contents_padding"]["len"], 502);
}

#[test]
fn ar_has_the_members_and_pads_an_odd_size() {
    let v = decode("ar", "sample.a");
    let members = v["members"].as_array().unwrap();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0]["name"], "hello.txt/");
    assert_eq!(members[0]["size_value"], 5);
    assert_eq!(members[0]["data"]["hex"], "68656c6c6f");
    assert_eq!(members[0]["mode"], "100644");
    assert_eq!(
        members[0]["padding"]["len"], 1,
        "an odd size has a padding byte"
    );
    assert_eq!(members[1]["name"], "bye.txt/");
    assert_eq!(members[1]["size_value"], 8);
    assert!(members[1]["padding"].is_null(), "an even size has none");
}

#[test]
fn pcapng_has_the_section_the_interface_and_the_packets() {
    let v = decode("pcapng", "sample.pcapng");
    let blocks = v["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 4);
    let section = &blocks[0]["body"];
    assert_eq!(
        (
            section["major_version"].as_u64(),
            section["minor_version"].as_u64()
        ),
        (Some(1), Some(0))
    );
    assert_eq!(section["section_length"], -1);
    let interface = &blocks[1]["body"];
    assert_eq!(name_of(&interface["link_type"]), "ethernet");
    assert_eq!(interface["snap_len"], 65535);
    let packet = &blocks[2]["body"];
    assert_eq!(packet["interface_id"], 0);
    assert_eq!(packet["captured_len"], 29);
    assert_eq!(packet["original_len"], 29);
    assert_eq!(packet["timestamp"], (0x0005_E2F6_u64 << 32) | 0x1234_5678);
    assert_eq!(packet["packet"]["len"], 29);
    let simple = &blocks[3]["body"];
    assert_eq!(simple["original_len"], 4);
    assert_eq!(blocks[3]["block_length"], blocks[3]["block_length_again"]);
}

#[test]
fn wasm_has_the_sections_with_leb128_sizes() {
    let v = decode("wasm", "sample.wasm");
    assert_eq!(v["version"], 1);
    let sections = v["sections"].as_array().unwrap();
    let kinds: Vec<&str> = sections.iter().map(|s| name_of(&s["id"])).collect();
    assert_eq!(kinds, ["type", "function", "export", "code", "custom"]);
    let sizes: Vec<u64> = sections
        .iter()
        .map(|s| s["len_body"]["value"].as_u64().unwrap())
        .collect();
    assert_eq!(sizes, [7, 2, 7, 9, 200]);
    // 200 needs two bytes of LEB128
    assert_eq!(sections[4]["len_body"]["b0"], 0xc8);
    assert_eq!(sections[4]["len_body"]["b1"], 1);
    assert_eq!(sections[4]["body"]["name"], "producers");
}

#[test]
fn npy_has_the_header_text_and_the_data() {
    let v = decode("npy", "sample.npy");
    assert_eq!(
        (v["major_version"].as_u64(), v["minor_version"].as_u64()),
        (Some(1), Some(0))
    );
    let header = v["header"].as_str().unwrap();
    assert!(header.starts_with("{'descr': '<i2'"), "{header}");
    assert!(header.contains("'shape': (2, 3)"), "{header}");
    assert_eq!(v["len_header"].as_u64().map(|n| (10 + n) % 64), Some(0));
    assert_eq!(v["data"]["len"], 12);
    assert_eq!(v["data"]["hex"], "010002000300fcfffbfffaff");
}

#[test]
fn woff_has_the_header_and_the_table_directory() {
    let v = decode("woff", "sample.woff");
    assert_eq!(v["flavor"], 0x0001_0000);
    assert_eq!(v["num_tables"], 2);
    assert_eq!(
        (v["major_version"].as_u64(), v["minor_version"].as_u64()),
        (Some(1), Some(2))
    );
    let tables = v["tables"].as_array().unwrap();
    assert_eq!(tables[0]["tag"], "cmap");
    assert_eq!(tables[0]["offset"], 84);
    assert_eq!(
        (
            tables[0]["comp_length"].as_u64(),
            tables[0]["orig_length"].as_u64()
        ),
        (Some(20), Some(40))
    );
    assert_eq!(tables[0]["compressed"], true);
    assert_eq!(tables[1]["tag"], "head");
    assert_eq!(tables[1]["compressed"], false);
    assert_eq!(tables[1]["orig_checksum"], 0xDEAD_BEEF_u64);
}

#[test]
fn woff2_has_the_header() {
    let v = decode("woff2", "sample.woff2");
    assert_eq!(v["num_tables"], 3);
    assert_eq!(v["total_sfnt_size"], 4096);
    assert_eq!(v["total_compressed_size"], 20);
    assert_eq!(v["rest"]["len"], 16);
}

#[test]
fn flac_has_the_stream_info_the_tags_and_the_frames() {
    let v = decode("flac", "sample.flac");
    let blocks = v["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0]["last"], false);
    assert_eq!(name_of(&blocks[0]["block_type"]), "stream_info");
    let info = &blocks[0]["data"];
    assert_eq!(info["sample_rate"], 44100);
    assert_eq!(info["channels"], 2);
    assert_eq!(info["bits_per_sample"], 16);
    assert_eq!(info["total_samples"], 441_000);
    assert_eq!(info["md5"]["len"], 16);
    let tags = &blocks[1]["data"];
    assert_eq!(tags["vendor"], "doe test");
    assert_eq!(tags["comments"][0]["text"], "TITLE=Sample");
    assert_eq!(tags["comments"][1]["text"], "ARTIST=doe");
    assert_eq!(blocks[2]["last"], true);
    assert_eq!(name_of(&blocks[2]["block_type"]), "padding");
    assert_eq!(v["frames"]["len"], 9);
}

#[test]
fn flv_has_the_header_and_the_tags_with_their_times() {
    let v = decode("flv", "sample.flv");
    assert_eq!(
        (v["has_audio"].as_bool(), v["has_video"].as_bool()),
        (Some(true), Some(true))
    );
    assert_eq!(v["data_offset"], 9);
    let tags = v["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(name_of(&tags[0]["tag_type"]), "script_data");
    assert_eq!(tags[0]["data_size"], 13);
    assert_eq!(tags[0]["previous_tag_size"], 24);
    assert_eq!(name_of(&tags[1]["tag_type"]), "video");
    // the extension byte is the top eight bits of the 32-bit time
    assert_eq!(tags[1]["timestamp"], 0x0100_0010);
}

#[test]
fn tzfile_has_the_transitions_and_the_local_time_types() {
    let v = decode("tzfile", "sample.tzif");
    assert_eq!(v["count_transitions"], 2);
    assert_eq!(
        v["transition_times"],
        serde_json::json!([1_679_792_400_i64, 1_698_541_200_i64])
    );
    assert_eq!(v["transition_types"], serde_json::json!([1, 0]));
    let kinds = v["local_time_types"].as_array().unwrap();
    assert_eq!(
        (kinds[0]["utc_offset"].as_i64(), kinds[0]["is_dst"].as_u64()),
        (Some(3600), Some(0))
    );
    assert_eq!(
        (kinds[1]["utc_offset"].as_i64(), kinds[1]["is_dst"].as_u64()),
        (Some(7200), Some(1))
    );
    assert_eq!(v["abbreviation_chars"], "CET\u{0}CEST\u{0}");
}

#[test]
fn psd_has_the_header_the_resources_and_the_image_data() {
    let v = decode("psd", "sample.psd");
    assert_eq!(
        (v["width"].as_u64(), v["height"].as_u64()),
        (Some(4), Some(2))
    );
    assert_eq!(
        (v["num_channels"].as_u64(), v["depth"].as_u64()),
        (Some(3), Some(8))
    );
    assert_eq!(name_of(&v["color_mode"]), "rgb");
    let blocks = v["image_resources"]["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0]["id"], 1005);
    assert_eq!(blocks[0]["data"]["hex"], "00480048");
    assert_eq!(v["layer_and_mask"]["len"], 0);
    assert_eq!(name_of(&v["compression"]), "raw");
    assert_eq!(v["image_data"]["len"], 24);
}

#[test]
fn bzip2_has_the_level_and_the_first_block() {
    let v = decode("bzip2", "sample.bz2");
    assert_eq!(v["level"], "9");
    assert_eq!(v["block_size_kb"], 900);
    assert_eq!(v["first_block_magic"]["hex"], "314159265359");
}

#[test]
fn xz_has_the_stream_header_and_the_footer_read_from_the_end() {
    let v = decode("xz", "sample.xz");
    assert_eq!(v["stream_flags"], 1);
    assert_eq!(v["check_type"], 1);
    assert_eq!(v["check_name"], "crc32");
    let footer = &v["footer"];
    assert_eq!(footer["stream_flags"], 1);
    assert_eq!(footer["magic"]["hex"], "595a");
    assert_eq!(
        footer["index_size"].as_u64(),
        footer["backward_size"].as_u64().map(|n| (n + 1) * 4)
    );
}

#[test]
fn zstd_has_the_frames_with_their_flags_and_blocks() {
    let v = decode("zstd", "sample.zst");
    let frames = v["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0]["single_segment"], true);
    assert_eq!(frames[0]["has_content_checksum"], true);
    assert_eq!(frames[0]["frame_content_size"]["hex"], "05");
    assert_eq!(frames[0]["blocks"][0]["last"], true);
    assert_eq!(frames[0]["blocks"][0]["block_type"], 0);
    assert_eq!(frames[0]["blocks"][0]["data"]["hex"], "68656c6c6f");
    assert_eq!(frames[0]["content_checksum"], 0x0403_0201);
    assert_eq!(frames[1]["single_segment"], false);
    assert_eq!(frames[1]["window_descriptor"], 0x58);
    assert!(frames[1]["frame_content_size"].is_null());
    // a run-length block: a size of 100 is the length of the run, and one byte is stored
    assert_eq!(frames[1]["blocks"][0]["block_type"], 1);
    assert_eq!(frames[1]["blocks"][0]["block_size"], 100);
    assert_eq!(frames[1]["blocks"][0]["data"]["len"], 1);
}

#[test]
fn lz4_has_the_descriptor_the_blocks_and_the_checksum() {
    let v = decode("lz4", "sample.lz4");
    assert_eq!(v["version"], 1);
    assert_eq!(v["block_independence"], true);
    assert_eq!(v["has_content_size"], true);
    assert_eq!(v["content_size"], 5);
    assert_eq!(v["block_max_size_id"], 4);
    let blocks = v["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0]["uncompressed"], true);
    assert_eq!(blocks[0]["data"]["hex"], "68656c6c6f");
    assert_eq!(blocks[1]["size_field"], 0, "the end mark");
    assert_eq!(v["content_checksum"], 0x1234_5678);
}

#[test]
fn the_samples_are_the_ones_the_script_makes() {
    // the samples are in the repository so that the tests do not need Python; the script makes the same bytes
    let Ok(python) = std::process::Command::new("python3")
        .arg("--version")
        .output()
    else {
        return;
    };
    if !python.status.success() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let script = std::fs::read_to_string(types().join("make_samples.py")).unwrap();
    let copy = dir.path().join("types").join("make_samples.py");
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    std::fs::write(&copy, script).unwrap();
    let run = std::process::Command::new("python3")
        .arg("-I")
        .arg(&copy)
        .status()
        .unwrap();
    assert!(run.success());
    // made by the compressors of Python's libraries, whose output can differ between versions of them
    let compressed = ["sample.bz2", "sample.xz"];
    for entry in std::fs::read_dir(types().join("samples")).unwrap() {
        let entry = entry.unwrap();
        if compressed.contains(&entry.file_name().to_string_lossy().as_ref()) {
            continue;
        }
        let made = dir.path().join("types/samples").join(entry.file_name());
        assert_eq!(
            std::fs::read(&made).unwrap(),
            std::fs::read(entry.path()).unwrap(),
            "{:?} is not what make_samples.py makes",
            entry.file_name()
        );
    }
}
