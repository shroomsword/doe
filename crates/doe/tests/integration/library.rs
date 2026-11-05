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
fn coff_has_the_sections_the_symbols_and_the_string_table() {
    let v = decode("coff", "sample.obj");
    assert_eq!(name_of(&v["machine"]), "amd64");
    assert_eq!(
        (v["num_sections"].as_u64(), v["num_symbols"].as_u64()),
        (Some(1), Some(2))
    );
    let section = &v["sections"][0];
    assert_eq!(section["name"], ".text");
    assert_eq!(
        (
            section["len_raw_data"].as_u64(),
            section["ofs_raw_data"].as_u64()
        ),
        (Some(4), Some(60))
    );
    assert_eq!(section["characteristics"], 0x6000_0020_u64);
    let symbols = v["symbols"].as_array().unwrap();
    assert_eq!(symbols.len(), 2);
    assert_eq!(
        symbols[0]["name_or_offset"]["hex"], "6d61696e00000000",
        "main"
    );
    assert_eq!(symbols[0]["storage_class"], 2);
    assert_eq!(symbols[0]["section_number"], 1);
    // a name of more than eight bytes: four zero bytes and an offset into the string table
    assert_eq!(symbols[1]["name_or_offset"]["hex"], "0000000004000000");
    assert_eq!(symbols[1]["value"], 4);
    assert_eq!(v["string_table"]["len"], 4);
}

#[test]
fn mpegts_has_the_packets_with_their_pids_and_fields() {
    let v = decode("mpegts", "sample.ts");
    let packets = v["packets"].as_array().unwrap();
    let pids: Vec<u64> = packets.iter().map(|p| p["pid"].as_u64().unwrap()).collect();
    assert_eq!(pids, [0, 0x100, 0x1fff]);
    assert_eq!(packets[0]["payload_unit_start"], true);
    assert_eq!(packets[0]["adaptation_field_control"], 1);
    assert_eq!(packets[0]["payload"]["len"], 184);
    assert_eq!(packets[1]["adaptation_field_control"], 3);
    assert_eq!(packets[1]["adaptation_field"]["len_adaptation_field"], 7);
    assert_eq!(packets[1]["continuity_counter"], 5);
    assert_eq!(packets[1]["payload"]["len"], 176);
    assert_eq!(packets[2]["is_null"], true);
    assert_eq!(packets[0]["is_null"], false);
}

#[test]
fn mp3_has_the_frames_with_their_bit_rate_sample_rate_and_length() {
    let v = decode("mp3", "sample.mp3");
    let frames = v["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 3);
    for frame in frames {
        assert_eq!(frame["sync"], 0x7ff);
        assert_eq!(name_of(&frame["version"]), "v1");
        assert_eq!(frame["bitrate_kbps"], 128);
        assert_eq!(frame["sample_rate"], 44100);
        assert_eq!(name_of(&frame["channel_mode"]), "stereo");
    }
    let lengths: Vec<u64> = frames
        .iter()
        .map(|f| f["len_frame"].as_u64().unwrap())
        .collect();
    assert_eq!(lengths, [417, 418, 417]);
    assert_eq!(frames[1]["padding"], true);
    assert_eq!(frames[1]["body"]["len"], 414);
}

#[test]
fn cab_has_the_folders_the_files_and_the_data_blocks() {
    let v = decode("cab", "sample.cab");
    assert_eq!(
        (v["version_major"].as_u64(), v["version_minor"].as_u64()),
        (Some(1), Some(3))
    );
    assert_eq!(
        (v["num_folders"].as_u64(), v["num_files"].as_u64()),
        (Some(1), Some(2))
    );
    assert_eq!(v["set_id"], 1234);
    assert_eq!(v["len_cabinet"], 124);
    let folder = &v["folders"][0];
    assert_eq!(folder["method"], 0, "stored");
    assert_eq!(folder["blocks"][0]["len_compressed"], 22);
    assert_eq!(folder["blocks"][0]["data"]["len"], 22);
    let files = v["files"].as_array().unwrap();
    assert_eq!(files[0]["name"], "hello.txt");
    assert_eq!(files[0]["len_file"], 14);
    assert_eq!(files[1]["name"], "bye.txt");
    assert_eq!(
        (
            files[1]["ofs_in_folder"].as_u64(),
            files[1]["len_file"].as_u64()
        ),
        (Some(14), Some(8))
    );
}

#[test]
fn sevenzip_has_the_signature_header_and_the_next_header() {
    let v = decode("sevenzip", "sample.7z");
    assert_eq!(
        (v["version_major"].as_u64(), v["version_minor"].as_u64()),
        (Some(0), Some(4))
    );
    assert_eq!(v["ofs_next_header"], 0);
    assert_eq!(v["len_next_header"], 2);
    assert_eq!(v["next_header"]["hex"], "0100");
    assert_eq!(v["next_header_crc"], 1_489_118_142_u64);
    assert_eq!(v["start_header_crc"], 3_090_458_632_u64);
    assert_eq!(v["packed_streams"]["len"], 0);
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

#[test]
fn zip_archive_is_read_from_the_central_directory() {
    let v = decode("zip_archive", "sample.zip");
    let end = &v["end_of_central_directory"];
    assert_eq!(end["num_entries"], 4);
    assert_eq!(end["ofs_central_directory"], 320);
    assert_eq!(end["len_comment"], 0);
    let entries = v["entries"].as_array().unwrap();
    let names: Vec<&str> = entries.iter().map(name_of).collect();
    assert_eq!(names, ["docs/", "docs/hello.txt", "data.bin", "stream.txt"]);
    assert_eq!(entries[0]["is_directory"], true);
    assert_eq!(entries[1]["is_directory"], false);
    assert_eq!(entries[0]["unix_mode"], 0o040_755);
    assert_eq!(entries[1]["unix_mode"], 0o100_644);
    // a stored entry: its data is found through the local header
    assert_eq!(entries[1]["method"]["name"], "stored");
    assert_eq!(entries[1]["crc32"], 0x363a_3020_u32);
    assert_eq!(entries[1]["data"]["hex"], "68656c6c6f0a");
    assert_eq!(entries[1]["local_header"]["name"], "docs/hello.txt");
    // a deflated one
    assert_eq!(entries[2]["method"]["name"], "deflated");
    assert_eq!(entries[2]["len_uncompressed"], 200);
    assert_eq!(entries[2]["data"]["len"], entries[2]["len_compressed"]);
    // a streamed one has zeros in its local header and its sizes in the central directory
    assert_eq!(entries[3]["has_data_descriptor"], true);
    assert_eq!(entries[3]["local_header"]["len_compressed"], 0);
    assert_eq!(entries[3]["local_header"]["crc32"], 0);
    assert_ne!(entries[3]["len_compressed"], 0);
    assert_eq!(entries[3]["data"]["len"], entries[3]["len_compressed"]);
    // the extended timestamp has both times in the local header and only the modification time in the
    // central directory, where its flags still say both: the extra fields are bytes, so both read
    let central = &entries[1]["extra"]["fields"];
    assert_eq!(central[0]["code"], 0x5455);
    assert_eq!(central[0]["len_body"], 5);
    assert_eq!(central[1]["code"], 0x7875);
    let local = &entries[1]["local_header"]["extra"]["fields"];
    assert_eq!(local[0]["len_body"], 9);
}

#[test]
fn zip_archive_is_found_for_a_file_named_like_a_zip() {
    let found = doe::detect::detect_file(
        &types().join("samples").join("sample.zip"),
        &[types()],
        &doe::detect::Options::default(),
    )
    .unwrap();
    assert_eq!(found.chosen().map(|c| c.name.as_str()), Some("zip_archive"));
}

#[test]
fn tga_image_has_the_colour_map_the_data_and_what_the_footer_points_to() {
    let v = decode("tga_image", "sample.tga");
    assert_eq!(v["image_type"]["name"], "color_mapped");
    assert_eq!(
        (v["width"].clone(), v["height"].clone()),
        (2.into(), 2.into())
    );
    assert_eq!(v["image_id"]["hex"], "646f65");
    assert_eq!(v["color_map"]["len"], 6);
    assert_eq!(v["image_data"]["hex"], "00010100");
    assert_eq!(v["top_to_bottom"], true);
    assert_eq!(v["is_version_2"], true);
    let extension = &v["extension_area"];
    assert_eq!(extension["author_name"], "doe authors");
    assert_eq!(extension["comments"][1], "second line");
    assert_eq!(extension["software_version_number"], 301);
    assert_eq!(extension["attributes_type"]["name"], "useful_alpha");
    let entry = &v["developer_directory"]["entries"][0];
    assert_eq!(entry["tag"], 0x0DE0);
    assert_eq!(entry["data"]["hex"], "646f652d64617461");
}

fn decode_real(type_name: &str, sample: &str) -> Json {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/samples_without_reference")
        .join(sample);
    let text = parse_file(
        &path,
        type_name,
        &[types()],
        Some(Path::new("/nonexistent/config.toml")),
        OutputFormat::Json,
    )
    .unwrap_or_else(|e| panic!("{sample}: {e}"));
    serde_json::from_str(&text).unwrap()
}

/// The first box of a type among `boxes` (the `items` of a container, or the `boxes` of a file).
fn mp4_box<'a>(boxes: &'a Json, kind: &str) -> &'a Json {
    boxes
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["type"] == kind)
        .unwrap_or_else(|| panic!("no {kind} box in {boxes}"))
}

#[test]
fn mp4_has_the_movie_the_tracks_the_sample_tables_and_the_tags() {
    let v = decode("mp4", "sample.mp4");
    assert_eq!(v["ftyp"]["body"]["major_brand"], "isom");
    assert_eq!(v["ftyp"]["body"]["minor_version"], 512);
    assert_eq!(
        v["ftyp"]["body"]["compatible_brands"],
        serde_json::json!(["isom", "iso2", "avc1", "mp41"])
    );
    let kinds: Vec<&str> = v["boxes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["moov", "free", "wide", "uuid", "zzzz", "mdat"]);
    let moov = &mp4_box(&v["boxes"], "moov")["body"]["items"];
    let movie = &mp4_box(moov, "mvhd")["body"];
    assert_eq!(
        (movie["timescale"].as_u64(), movie["duration"].as_u64()),
        (Some(1000), Some(5000))
    );
    assert_eq!(movie["duration_seconds"], 5.0);
    assert_eq!(movie["next_track_id"], 3);
    let tracks: Vec<&Json> = moov
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["type"] == "trak")
        .collect();
    assert_eq!(tracks.len(), 2);
    // the video track
    let video = &tracks[0]["body"]["items"];
    let tkhd = &mp4_box(video, "tkhd")["body"];
    assert_eq!(
        (
            tkhd["track_id"].as_u64(),
            tkhd["width"].as_f64(),
            tkhd["height"].as_f64()
        ),
        (Some(1), Some(320.0), Some(240.0))
    );
    assert_eq!(
        (tkhd["enabled"].as_bool(), tkhd["in_movie"].as_bool()),
        (Some(true), Some(true))
    );
    let elst = &mp4_box(&mp4_box(video, "edts")["body"]["items"], "elst")["body"];
    assert_eq!(elst["entries"][0]["segment_duration_v0"], 5000);
    let mdia = &mp4_box(video, "mdia")["body"]["items"];
    assert_eq!(mp4_box(mdia, "mdhd")["body"]["duration_seconds"], 5.0);
    let hdlr = &mp4_box(mdia, "hdlr")["body"];
    assert_eq!(
        (hdlr["handler_type"].as_str(), hdlr["name"].as_str()),
        (Some("vide"), Some("VideoHandler"))
    );
    let stbl = &mp4_box(&mp4_box(mdia, "minf")["body"]["items"], "stbl")["body"]["items"];
    let entry = &mp4_box(stbl, "stsd")["body"]["entries"][0];
    assert_eq!(entry["format"], "avc1");
    assert_eq!(
        (
            entry["body"]["width"].as_u64(),
            entry["body"]["height"].as_u64(),
            entry["body"]["depth"].as_u64()
        ),
        (Some(320), Some(240), Some(24))
    );
    assert_eq!(entry["body"]["boxes"][0]["type"], "avcC");
    assert_eq!(
        mp4_box(stbl, "stts")["body"]["entries"][0]["sample_count"],
        5
    );
    assert_eq!(
        mp4_box(stbl, "stss")["body"]["sample_numbers"],
        serde_json::json!([1])
    );
    assert_eq!(mp4_box(stbl, "stsz")["body"]["sample_count"], 5);
    assert_eq!(
        mp4_box(stbl, "stsz")["body"]["entry_sizes"],
        serde_json::json!([10, 4, 4, 4, 4])
    );
    assert_eq!(
        mp4_box(stbl, "stsc")["body"]["entries"][0]["samples_per_chunk"],
        5
    );
    // the chunk offset points at the media data, which starts after the 8 bytes of the header of mdat
    let offset = mp4_box(stbl, "stco")["body"]["chunk_offsets"][0]
        .as_u64()
        .unwrap();
    let mdat = mp4_box(&v["boxes"], "mdat");
    assert_eq!(mdat["size32"], 66);
    let data = std::fs::read(types().join("samples/sample.mp4")).unwrap();
    let at = usize::try_from(offset).unwrap();
    assert_eq!(&data[at..at + 4], [0, 1, 2, 3]);
    // the audio track
    let audio = &tracks[1]["body"]["items"];
    let mdia = &mp4_box(audio, "mdia")["body"]["items"];
    assert_eq!(mp4_box(mdia, "hdlr")["body"]["handler_type"], "soun");
    assert_eq!(mp4_box(mdia, "mdhd")["body"]["timescale"], 44100);
    let stbl = &mp4_box(&mp4_box(mdia, "minf")["body"]["items"], "stbl")["body"]["items"];
    let entry = &mp4_box(stbl, "stsd")["body"]["entries"][0];
    assert_eq!(entry["format"], "mp4a");
    assert_eq!(
        (
            entry["body"]["channel_count"].as_u64(),
            entry["body"]["sample_size"].as_u64(),
            entry["body"]["sample_rate"].as_f64()
        ),
        (Some(2), Some(16), Some(44100.0))
    );
    assert_eq!(entry["body"]["boxes"][0]["type"], "esds");
    // the tag in udta > meta > ilst > (c)nam > data
    let meta = &mp4_box(&mp4_box(moov, "udta")["body"]["items"], "meta")["body"]["boxes"];
    let ilst = &mp4_box(meta, "ilst")["body"]["items"];
    let name = &mp4_box(ilst, "\u{a9}nam")["body"]["items"];
    assert_eq!(mp4_box(name, "data")["body"]["value"]["len"], 6);
    // the odd boxes are read and skipped: a 64-bit size, a uuid, one nobody knows
    let wide = mp4_box(&v["boxes"], "wide");
    assert_eq!(
        (
            wide["size32"].as_u64(),
            wide["largesize"].as_u64(),
            wide["body_len"].as_u64()
        ),
        (Some(1), Some(20), Some(4))
    );
    let uuid = mp4_box(&v["boxes"], "uuid");
    assert_eq!(uuid["usertype"]["len"], 16);
    assert_eq!(uuid["body_len"], 7);
    assert_eq!(mp4_box(&v["boxes"], "zzzz")["body"]["bytes"]["len"], 7);
}

#[test]
fn mp4_reads_the_fragments_of_a_fragmented_file() {
    let v = decode("mp4", "sample_fragmented.mp4");
    let kinds: Vec<&str> = v["boxes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["moov", "moof", "mdat", "mfra"]);
    let header = &mp4_box(&v["boxes"], "moov")["body"]["items"];
    let mvex = &mp4_box(header, "mvex")["body"]["items"];
    assert_eq!(mp4_box(mvex, "mehd")["body"]["fragment_duration_v0"], 4000);
    assert_eq!(
        mp4_box(mvex, "trex")["body"]["default_sample_duration"],
        1000
    );
    let moof = &mp4_box(&v["boxes"], "moof")["body"]["items"];
    assert_eq!(mp4_box(moof, "mfhd")["body"]["sequence_number"], 7);
    let traf = &mp4_box(moof, "traf")["body"]["items"];
    let tfhd = &mp4_box(traf, "tfhd")["body"];
    assert_eq!(
        (
            tfhd["track_id"].as_u64(),
            tfhd["default_sample_duration"].as_u64()
        ),
        (Some(1), Some(1000))
    );
    assert_eq!(tfhd["default_base_is_moof"], true);
    assert_eq!(
        mp4_box(traf, "tfdt")["body"]["base_media_decode_time_v1"],
        90000
    );
    let trun = &mp4_box(traf, "trun")["body"];
    assert_eq!(trun["sample_count"], 3);
    let sizes: Vec<u64> = trun["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["size"].as_u64().unwrap())
        .collect();
    assert_eq!(sizes, [4, 5, 6]);
    assert_eq!(
        mp4_box(&v["boxes"], "mfra")["body"]["items"][0]["type"],
        "mfro"
    );
}

/// Files written by `ffmpeg` are `QuickTime` `.mov` (the gallery's `quicktime_mov` reads them too): the boxes are the same.
#[test]
fn mp4_reads_the_quicktime_files_that_ffmpeg_writes() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/samples/gallery/media/quicktime_mov/ffmpeg_mjpeg.bin");
    if !path.exists() {
        return;
    }
    let text = parse_file(
        &path,
        "mp4",
        &[types()],
        Some(Path::new("/nonexistent/config.toml")),
        OutputFormat::Json,
    )
    .unwrap();
    let v: Json = serde_json::from_str(&text).unwrap();
    assert_eq!(v["ftyp"]["body"]["major_brand"], "qt  ");
    let moov = &mp4_box(&v["boxes"], "moov")["body"]["items"];
    let trak = mp4_box(moov, "trak");
    let mdia = &mp4_box(&trak["body"]["items"], "mdia")["body"]["items"];
    assert_eq!(mp4_box(mdia, "hdlr")["body"]["handler_type"], "vide");
}

#[test]
fn mp4_is_found_by_ftyp_at_offset_four_and_a_truncated_file_reads_as_far_as_it_goes() {
    // detection: the signature is the type `ftyp` after the size of the first box
    let found = doe::detect::detect_file(
        &types().join("samples/sample.mp4"),
        &[types()],
        &doe::detect::Options::default(),
    )
    .unwrap();
    assert_eq!(
        found.chosen().map(|a| a.name.as_str()),
        Some("mp4"),
        "{}",
        found.report()
    );
    // a file cut inside the movie: the boxes before the cut are read
    let data = std::fs::read(types().join("samples/sample.mp4")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let cut = dir.path().join("cut.mp4");
    std::fs::write(&cut, &data[..32 + 8 + 108 + 8 + 92]).unwrap();
    let text = parse_file(
        &cut,
        "mp4",
        &[types()],
        Some(Path::new("/nonexistent/config.toml")),
        OutputFormat::Json,
    )
    .unwrap();
    let v: Json = serde_json::from_str(&text).unwrap();
    let moov = mp4_box(&v["boxes"], "moov");
    assert_eq!(moov["size32"], 1143);
    assert_eq!(moov["body_len"], 108 + 8 + 92);
    let items = moov["body"]["items"].as_array().unwrap();
    assert_eq!(items[0]["type"], "mvhd");
    // the track that is cut is read as far as it goes: its header box
    assert_eq!(items[1]["type"], "trak");
    assert_eq!(items[1]["body"]["items"][0]["type"], "tkhd");
}

/// The text of a token (a word, a name, a string) of the `pdf` type, or the type of the value when it has none.
fn pdf_text(token: &Json) -> String {
    let value = &token["value"];
    for key in ["text", "hex"] {
        if let Some(text) = value[key].as_str() {
            return text.to_owned();
        }
    }
    value["$type"]
        .as_str()
        .unwrap()
        .rsplit("::")
        .next()
        .unwrap()
        .to_owned()
}

/// The tokens of a dictionary (or an array) in order, as texts.
fn pdf_dictionary(token: &Json) -> Vec<String> {
    let value = &token["value"];
    let items = value["dictionary"]
        .as_array()
        .or_else(|| value["items"].as_array())
        .unwrap();
    items.iter().map(pdf_text).collect()
}

#[test]
fn pdf_has_the_header_the_end_the_table_the_trailer_and_the_objects_as_tokens() {
    let v = decode("pdf", "sample.pdf");
    assert_eq!(v["header"]["version"], "1.7");
    // the end of the file, read from the last 1024 bytes as lines
    assert_eq!(v["tail"]["startxref"], 551);
    assert_eq!(v["startxref"], 551);
    let xref = &v["xref"];
    assert_eq!(xref["keyword"], "xref");
    let entries = xref["subsections"][0]["entries"].as_array().unwrap();
    let rows: Vec<(u64, u64, bool)> = entries
        .iter()
        .map(|e| {
            (
                e["offset"].as_u64().unwrap(),
                e["generation"].as_u64().unwrap(),
                e["in_use"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(rows[0], (0, 65535, false));
    assert_eq!(rows.len(), 7);
    assert!(rows[1..].iter().all(|r| r.2 && r.1 == 0));
    assert_eq!(pdf_text(&xref["trailer_keyword"]), "trailer");
    assert_eq!(
        pdf_dictionary(&xref["trailer"]),
        [
            "Size",
            "7",
            "Root",
            "1",
            "0",
            "R",
            "Info",
            "6",
            "0",
            "R",
            "dict_close"
        ]
    );
    // the objects, through the offsets of the table: the number, the generation and obj, then the value
    for (n, e) in entries.iter().enumerate().skip(1) {
        let object = &e["object"];
        assert_eq!(pdf_text(&object["number"]), n.to_string());
        assert_eq!(pdf_text(&object["obj_keyword"]), "obj");
    }
    assert_eq!(
        pdf_dictionary(&entries[1]["object"]["value"]),
        ["Type", "Catalog", "Pages", "2", "0", "R", "dict_close"]
    );
    assert_eq!(pdf_text(&entries[1]["object"]["closing"]), "endobj");
    // an array and a nested dictionary of the page
    let page = pdf_dictionary(&entries[3]["object"]["value"]);
    assert!(
        page.starts_with(&["Type".to_owned(), "Page".to_owned()]),
        "{page:?}"
    );
    assert!(
        page.contains(&"MediaBox".to_owned()) && page.contains(&"array".to_owned()),
        "{page:?}"
    );
    // the object with a stream is read up to the `stream` keyword: its data is not read
    assert_eq!(pdf_text(&entries[4]["object"]["closing"]), "stream");
    assert_eq!(
        pdf_dictionary(&entries[4]["object"]["value"]),
        ["Length", "36", "dict_close"]
    );
    // strings of every kind, numbers and keywords in the info dictionary
    let info = pdf_dictionary(&entries[6]["object"]["value"]);
    assert_eq!(
        info,
        [
            "Title",
            "A sample",
            "Author",
            "646f65",
            "Producer",
            "doe tests",
            "CreationDate",
            "D:20261104120000Z",
            "Pi",
            "3.14159",
            "Yes",
            "true",
            "Nothing",
            "null",
            "dict_close"
        ]
    );
    let author = &entries[6]["object"]["value"]["value"]["dictionary"][3];
    assert_eq!(author["value"]["hex"], "646f65");
}

#[test]
fn pdf_reads_a_file_whose_cross_reference_is_a_stream_as_far_as_it_can_and_says_so() {
    let v = decode("pdf", "sample_xrefstream.pdf");
    assert_eq!(v["header"]["version"], "1.5");
    assert!(v["startxref"].as_u64().unwrap() > 0);
    // the section is not an `xref` keyword: it is a stream, which is not read (a note field of size 0)
    assert_eq!(v["xref"]["keyword"], "3 0 ");
    assert!(
        v["xref"].get("cross_reference_stream_not_read").is_some(),
        "{}",
        v["xref"]
    );
    assert!(v["xref"]["subsections"].is_null() || v["xref"].get("subsections").is_none());
}

#[test]
fn pdf_reads_an_updated_file_at_its_last_section_only() {
    let v = decode("pdf", "sample_updated.pdf");
    let xref = &v["xref"];
    let entries = xref["subsections"][0]["entries"].as_array().unwrap();
    // the update has one object, number 5, and the trailer has /Prev (the chain is not followed)
    assert_eq!(entries.len(), 1);
    assert_eq!(pdf_text(&entries[0]["object"]["number"]), "5");
    assert_eq!(
        pdf_dictionary(&xref["trailer"]),
        [
            "Size",
            "7",
            "Root",
            "1",
            "0",
            "R",
            "Prev",
            "551",
            "dict_close"
        ]
    );
}

#[test]
fn pdf_is_found_by_its_header_and_shows_what_it_can_when_the_end_is_missing() {
    let found = doe::detect::detect_file(
        &types().join("samples/sample.pdf"),
        &[types()],
        &doe::detect::Options::default(),
    )
    .unwrap();
    assert_eq!(
        found.chosen().map(|a| a.name.as_str()),
        Some("pdf"),
        "{}",
        found.report()
    );
    // a file with only a header: there is no startxref, and what can be read is shown (no table, no error)
    let dir = tempfile::tempdir().unwrap();
    let cut = dir.path().join("cut.pdf");
    std::fs::write(&cut, b"%PDF-1.4\n%binary\n").unwrap();
    let text = parse_file(
        &cut,
        "pdf",
        &[types()],
        Some(Path::new("/nonexistent/config.toml")),
        OutputFormat::Json,
    )
    .unwrap();
    let v: Json = serde_json::from_str(&text).unwrap();
    assert_eq!(v["header"]["version"], "1.4");
    assert_eq!(v["tail"]["has_startxref"], false);
    assert!(v["xref"].is_null() && v["startxref"].is_null(), "{v}");
}

#[test]
fn pdf_reads_the_first_300_objects_and_gives_the_offsets_of_the_rest() {
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for n in 1..=305_u32 {
        offsets.push(out.len());
        out.extend(format!("{n} 0 obj\n<< /N {n} >>\nendobj\n").bytes());
    }
    let start = out.len();
    out.extend(b"xref\n0 306\n0000000000 65535 f \n");
    for offset in &offsets {
        out.extend(format!("{offset:010} 00000 n \n").bytes());
    }
    out.extend(format!("trailer\n<< /Size 306 >>\nstartxref\n{start}\n%%EOF\n").bytes());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("many.pdf");
    std::fs::write(&path, out).unwrap();
    let text = parse_file(
        &path,
        "pdf",
        &[types()],
        Some(Path::new("/nonexistent/config.toml")),
        OutputFormat::Json,
    )
    .unwrap();
    let v: Json = serde_json::from_str(&text).unwrap();
    let entries = v["xref"]["subsections"][0]["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 306);
    assert!(!entries[300]["object"].is_null());
    assert!(entries[301]["object"].is_null());
    assert_eq!(
        entries[305]["offset"].as_u64().unwrap(),
        offsets[304] as u64
    );
}

/// The entries of a `cpio` value as `(name, size, mode, type)`; the entry's own fields are in `body` once or twice.
fn cpio_rows(v: &Json) -> Vec<(String, u64, u64, String)> {
    v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let mut body = &entry["body"];
            if body["body"].is_object() {
                body = &body["body"];
            }
            (
                body["name"].as_str().unwrap().to_owned(),
                body["filesize_value"].as_u64().unwrap(),
                body["mode_value"].as_u64().unwrap(),
                body["file_type"]["name"].as_str().unwrap_or("").to_owned(),
            )
        })
        .collect()
}

#[test]
fn cpio_reads_every_format_to_the_trailer() {
    let wanted = vec![
        ("docs".to_owned(), 0, 0o040_755, "directory".to_owned()),
        (
            "docs/hello.txt".to_owned(),
            6,
            0o100_644,
            "regular".to_owned(),
        ),
        ("odd".to_owned(), 3, 0o100_600, "regular".to_owned()),
        ("link".to_owned(), 14, 0o120_777, "symlink".to_owned()),
        ("TRAILER!!!".to_owned(), 0, 0, String::new()),
    ];
    for (file, magic) in [
        ("sample.cpio", "ascii"),
        ("sample_crc.cpio", "ascii"),
        ("sample_odc.cpio", "ascii"),
        ("sample_bin_le.cpio", "binary_little_endian"),
        ("sample_bin_be.cpio", "binary_big_endian"),
    ] {
        let v = decode("cpio", file);
        assert_eq!(name_of(&v["magic"]), magic, "{file}");
        assert_eq!(cpio_rows(&v), wanted, "{file}");
        let last = v["entries"].as_array().unwrap().last().unwrap();
        assert_eq!(last["is_end"], true, "{file}");
    }
    // the details of each: the header numbers, the padding of newc, the check of crc, the data of a link
    let newc = decode("cpio", "sample.cpio");
    let hello = &newc["entries"][1]["body"]["body"];
    assert_eq!(hello["magic"], "070701");
    assert_eq!(hello["mtime_value"], 1_700_000_000);
    assert_eq!(hello["data"]["len"], 6);
    assert_eq!(hello["data_padding"]["len"], 2);
    assert_eq!(
        newc["entries"][3]["body"]["body"]["data"]["hex"],
        "646f63732f68656c6c6f2e747874"
    );
    let crc = decode("cpio", "sample_crc.cpio");
    let hello = &crc["entries"][1]["body"]["body"];
    assert_eq!(
        (hello["is_crc"].as_bool(), hello["check_value"].as_u64()),
        (
            Some(true),
            Some(b"hello\n".iter().map(|b| u64::from(*b)).sum())
        )
    );
    let odc = decode("cpio", "sample_odc.cpio");
    assert_eq!(odc["entries"][1]["body"]["body"]["nlink_value"], 1);
    // the zeros after the trailer are not read as entries
    assert_eq!(newc["entries"].as_array().unwrap().len(), 5);
}

/// The archives that GNU `cpio` writes, in the four formats it has.
#[test]
fn cpio_reads_what_gnu_cpio_writes() {
    for file in ["gnu_newc", "gnu_crc", "gnu_odc", "gnu_binary"] {
        let v = decode_real("cpio", &format!("archive/cpio/{file}.bin"));
        let rows = cpio_rows(&v);
        let names: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
        assert_eq!(
            names,
            ["b", "dir", "dir/a.txt", "dir/link", "TRAILER!!!"],
            "{file}"
        );
        assert_eq!(rows[0].1, 3, "{file}");
        assert_eq!(rows[1].3, "directory", "{file}");
        assert_eq!(rows[2].1, 6, "{file}");
        assert_eq!(rows[3].3, "symlink", "{file}");
    }
}

#[test]
fn cpio_is_found_by_its_magic_in_every_format_and_lists_files_and_directories() {
    for file in [
        "sample.cpio",
        "sample_crc.cpio",
        "sample_odc.cpio",
        "sample_bin_le.cpio",
        "sample_bin_be.cpio",
    ] {
        let found = doe::detect::detect_file(
            &types().join("samples").join(file),
            &[types()],
            &doe::detect::Options::default(),
        )
        .unwrap();
        assert_eq!(
            found.chosen().map(|a| a.name.as_str()),
            Some("cpio"),
            "{file}: {}",
            found.report()
        );
    }
    // the files of the archive, for the Files view: the trailer is left out
    let schema = std::fs::read_to_string(types().join("cpio.yaml")).unwrap();
    let bytes = std::fs::read(types().join("samples/sample.cpio")).unwrap();
    let request = serde_json::json!({"schema": schema});
    let response = doe::api::parse(&request.to_string(), &bytes);
    let listing = response.filesystem.expect("cpio says it is a file system");
    let names: Vec<String> = listing.entries.iter().map(|e| e.path.join("/")).collect();
    assert_eq!(names, ["docs", "docs/hello.txt", "odd", "link"]);
    assert_eq!(listing.entries[0].kind, doe::filesystem::Kind::Directory);
    assert_eq!(listing.entries[3].kind, doe::filesystem::Kind::Symlink);
    assert_eq!(listing.entries[1].size, Some(6));
    let (from, end) = listing.entries[1].data.unwrap();
    assert_eq!(
        &bytes[usize::try_from(from).unwrap()..usize::try_from(end).unwrap()],
        b"hello\n"
    );
}

/// The value of `squashfs` for these bytes, as the API gives it (the parse may stop at an error: what was read is there).
fn squashfs_of(bytes: &[u8]) -> Json {
    let schema = std::fs::read_to_string(types().join("squashfs.yaml")).unwrap();
    let request = serde_json::json!({"schema": schema});
    let response = doe::api::parse(&request.to_string(), bytes);
    assert!(
        response.ok,
        "{:?} {:?}",
        response.error, response.diagnostics
    );
    response.value.expect("a value")
}

#[test]
fn squashfs_reads_the_superblock_the_tables_and_the_root_directory() {
    for (file, zlib) in [
        ("sample.squashfs", false),
        ("sample_gzip.squashfs", true),
        ("sample_extended.squashfs", false),
    ] {
        let v = decode("squashfs", file);
        assert_eq!(
            (
                v["inode_count"].as_u64(),
                v["block_size"].as_u64(),
                v["block_log"].as_u64(),
                v["version_major"].as_u64(),
                v["version_minor"].as_u64()
            ),
            (Some(5), Some(131_072), Some(17), Some(4), Some(0)),
            "{file}"
        );
        assert_eq!(name_of(&v["compression"]), "gzip", "{file}");
        assert_eq!(
            (v["is_supported"].as_bool(), v["is_complete"].as_bool()),
            (Some(true), Some(true)),
            "{file}"
        );
        assert_eq!(v["is_exportable"], true, "{file}");
        assert_eq!(v["no_xattrs"], true, "{file}");
        // stored images say so in the flags, zlib ones do not
        assert_eq!(v["uncompressed_inodes"], !zlib, "{file}");
        // the id table: two ids, read from a stored or a zlib block
        let ids = &v["id_blocks"][0]["block"]["body"]["content"]["ids"];
        assert_eq!(ids, &serde_json::json!([0, 1000]), "{file}");
        // the fragment table: where the one fragment is, its size and that it is stored
        let fragment = &v["fragment_blocks"][0]["block"]["body"]["content"]["fragments"][0];
        assert_eq!(
            (
                fragment["start"].as_u64(),
                fragment["size"].as_u64(),
                fragment["is_stored"].as_bool()
            ),
            (Some(96), Some(26), Some(true)),
            "{file}"
        );
        // the export table: five inode references
        let exports = &v["export_blocks"][0]["block"]["body"]["content"]["inodes"];
        assert_eq!(exports.as_array().map(Vec::len), Some(5), "{file}");
        // the root inode: a directory with three entries and the listing's size
        let root = &v["root_inode_value"];
        let directory = if file == "sample_extended.squashfs" {
            8
        } else {
            1
        };
        assert_eq!(root["kind"], directory, "{file}");
        assert_eq!(root["inode_number"], 1, "{file}");
        assert_eq!(root["body"]["link_count"], 3, "{file}");
        assert_eq!(root["body"]["listing_size"], 52, "{file}");
        // the root directory: the names, their types and inode numbers (the numbers are the base plus a signed delta)
        let run = &v["root_directory"]["body"]["runs"][0];
        assert_eq!(run["base_inode"], 3, "{file}");
        let entries: Vec<(String, String, i64)> = run["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["name"].as_str().unwrap().to_owned(),
                    match e["kind"].as_u64().unwrap() {
                        1 => "directory",
                        2 => "file",
                        3 => "symlink",
                        _ => "other",
                    }
                    .to_owned(),
                    e["inode_number"].as_i64().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            entries,
            [
                ("dir".to_owned(), "directory".to_owned(), 3),
                ("hello.txt".to_owned(), "file".to_owned(), 2),
                ("link".to_owned(), "symlink".to_owned(), 5),
            ],
            "{file}"
        );
    }
}

#[test]
fn squashfs_of_another_version_or_cut_short_shows_the_superblock_only() {
    let whole = std::fs::read(types().join("samples/sample.squashfs")).unwrap();
    // version 3.0: the superblock is read, and nothing that depends on the version 4 layout
    let mut old = whole.clone();
    old[28] = 3;
    let v = squashfs_of(&old);
    assert_eq!(v["is_supported"], false);
    assert!(v["id_blocks"].is_null() && v["root_inode_value"].is_null());
    // the tables are past the end of the file: the bytes used are not all there
    let v = squashfs_of(&whole[..whole.len() - 20]);
    assert_eq!(v["is_complete"], false);
    assert_eq!(v["is_supported"], true);
    assert_eq!(v["inode_count"], 5);
    assert!(v["id_blocks"].is_null() && v["root_directory"].is_null());
}

#[test]
fn squashfs_of_another_compression_has_its_blocks_as_bytes() {
    let mut xz = std::fs::read(types().join("samples/sample.squashfs")).unwrap();
    // compression 4 (xz): the metadata blocks of this image are stored, but the type reads only what it can decode
    xz[20] = 4;
    let v = squashfs_of(&xz);
    assert_eq!(name_of(&v["compression"]), "xz");
    assert_eq!(v["uses_zlib"], false);
    // a block with the stored bit is decoded whatever the compression says
    assert_eq!(
        v["root_directory"]["body"]["runs"][0]["entries"][0]["name"],
        "dir"
    );
}

#[test]
fn squashfs_is_found_by_its_magic() {
    for file in [
        "sample.squashfs",
        "sample_gzip.squashfs",
        "sample_extended.squashfs",
    ] {
        let found = doe::detect::detect_file(
            &types().join("samples").join(file),
            &[types()],
            &doe::detect::Options::default(),
        )
        .unwrap();
        assert_eq!(
            found.chosen().map(|a| a.name.as_str()),
            Some("squashfs"),
            "{file}: {}",
            found.report()
        );
    }
}

/// The value of `ubi` for these bytes, as the API gives it.
fn ubi_of(bytes: &[u8]) -> Json {
    let schema = std::fs::read_to_string(types().join("ubi.yaml")).unwrap();
    let request = serde_json::json!({"schema": schema});
    let response = doe::api::parse(&request.to_string(), bytes);
    assert!(
        response.ok,
        "{:?} {:?}",
        response.error, response.diagnostics
    );
    response.value.expect("a value")
}

#[test]
fn ubi_reads_every_eraseblock_and_the_layout_volume() {
    let v = decode("ubi", "sample.ubi");
    assert_eq!(
        (v["peb_size"].as_u64(), v["peb_count"].as_u64()),
        (Some(16384), Some(4))
    );
    let blocks: Vec<&Json> = v["pebs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| &p["block"])
        .collect();
    assert_eq!(blocks.len(), 4);
    // the erase counters and the image sequence number
    let counters: Vec<u64> = blocks
        .iter()
        .map(|b| b["ec"]["erase_counter"].as_u64().unwrap())
        .collect();
    assert_eq!(counters, [2, 3, 7, 1]);
    assert_eq!(blocks[0]["ec"]["image_seq"], 0x1234_ABCD_u64);
    assert_eq!(
        (
            blocks[0]["ec"]["vid_hdr_offset"].as_u64(),
            blocks[0]["ec"]["data_offset"].as_u64()
        ),
        (Some(64), Some(128))
    );
    // the volume ids and the logical eraseblock numbers: two copies of the layout volume, a block of volume 0, a free one
    let ids: Vec<(Option<u64>, Option<u64>)> = blocks
        .iter()
        .map(|b| (b["vid"]["vol_id"].as_u64(), b["vid"]["lnum"].as_u64()))
        .collect();
    assert_eq!(
        ids,
        [
            (Some(0x7FFF_EFFF), Some(0)),
            (Some(0x7FFF_EFFF), Some(1)),
            (Some(0), Some(0)),
            (None, None)
        ]
    );
    assert_eq!(name_of(&blocks[2]["vid"]["vol_type"]), "dynamic");
    assert_eq!(name_of(&blocks[0]["vid"]["compat"]), "reject");
    assert_eq!(
        (
            blocks[2]["vid"]["data_size"].as_u64(),
            blocks[2]["vid"]["sqnum"].as_u64()
        ),
        (Some(96), Some(3))
    );
    assert_eq!(blocks[3]["has_vid"], false);
    // the layout volume: the table of volumes, in both copies, up to the empty record
    for b in &blocks[..2] {
        assert_eq!(b["is_layout"], true);
        let records = b["volume_table"]["records"].as_array().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["name"], "rootfs");
        assert_eq!(
            (
                records[0]["reserved_pebs"].as_u64(),
                name_of(&records[0]["vol_type"])
            ),
            (Some(3), "dynamic")
        );
        assert_eq!(records[1]["is_empty"], true);
    }
    assert!(blocks[2]["volume_table"].is_null());
}

#[test]
fn ubi_finds_the_eraseblock_size_and_survives_erased_and_cut_blocks() {
    let whole = std::fs::read(types().join("samples/sample.ubi")).unwrap();
    // eraseblocks of 32 KiB: the 16 KiB blocks padded with erased flash
    let mut wide = Vec::new();
    for chunk in whole.chunks(16384) {
        wide.extend_from_slice(chunk);
        wide.extend(std::iter::repeat_n(0xFF_u8, 16384));
    }
    let v = ubi_of(&wide);
    assert_eq!(
        (v["peb_size"].as_u64(), v["peb_count"].as_u64()),
        (Some(32768), Some(4))
    );
    // a block that is erased (all 0xFF) has no headers
    let mut erased = whole.clone();
    erased[16384 * 3..].fill(0xFF);
    let v = ubi_of(&erased);
    assert_eq!(v["pebs"][3]["block"]["has_ec"], false);
    assert!(v["pebs"][3]["block"]["ec"].is_null());
    assert_eq!(v["pebs"][2]["block"]["ec"]["erase_counter"], 7);
    // cut in the middle of the last block: the whole ones are shown
    let v = ubi_of(&whole[..16384 * 3 + 5000]);
    assert_eq!(v["peb_count"], 3);
    // one block only (nothing at a later multiple): its size is the file's
    let v = ubi_of(&whole[..16384]);
    assert_eq!(
        (v["peb_size"].as_u64(), v["peb_count"].as_u64()),
        (Some(16384), Some(1))
    );
}

#[test]
fn ubi_is_found_by_its_magic() {
    let found = doe::detect::detect_file(
        &types().join("samples/sample.ubi"),
        &[types()],
        &doe::detect::Options::default(),
    )
    .unwrap();
    assert_eq!(
        found.chosen().map(|a| a.name.as_str()),
        Some("ubi"),
        "{}",
        found.report()
    );
}

#[test]
fn wav_audio_reads_the_format_the_samples_and_the_tags_as_utf8() {
    let v = decode("wav_audio", "sample.wav");
    let chunks = v["chunks"].as_array().unwrap();
    let ids: Vec<&str> = chunks.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["fmt ", "fact", "bext", "data", "LIST"]);
    let format = &chunks[0]["body"];
    assert_eq!(name_of(&format["format_tag"]), "pcm");
    assert_eq!(
        (
            format["channels"].as_u64(),
            format["samples_per_sec"].as_u64(),
            format["bits_per_sample"].as_u64()
        ),
        (Some(1), Some(8000), Some(16))
    );
    assert_eq!(chunks[1]["body"]["sample_count"], 8);
    assert_eq!(chunks[2]["body"]["originator"], "doe");
    assert_eq!(chunks[2]["body"]["origination_date"], "2026-11-04");
    assert_eq!(chunks[3]["body"]["bytes"]["len"], 16);
    // the tags: the second has an em dash and an odd size, so it is padded; read as UTF-8 there is no error (#329)
    let entries = chunks[4]["body"]["entries"].as_array().unwrap();
    assert_eq!(entries[0]["text"]["value"], "Sample");
    assert_eq!(entries[1]["text"]["value"], "Made by doe \u{2014} partial");
    assert_eq!(entries[0]["pad"]["len"], 1);
}

#[test]
fn mar_archive_reads_the_form_with_signatures() {
    let v = decode("mar_archive", "sample.mar");
    assert_eq!(v["is_newer_form"], true);
    assert_eq!(v["newer_header"]["num_signatures"], 1);
    assert_eq!(
        v["newer_header"]["signatures"][0]["algorithm"]["name"],
        "rsa_pkcs1_sha1"
    );
    let block = &v["newer_header"]["additional_sections"][0]["body"];
    assert_eq!(block["mar_channel_name"], "release");
    assert_eq!(block["product_version"], "1.2.3");
    let entries = &v["index"]["entries"]["entries"];
    assert_eq!(entries[1]["file_name"], "dir/b.bin");
    assert_eq!(entries[0]["body"]["hex"], "68656c6c6f206d61720a");
}

/// The MAR files of the Python package `mar` have only the magic and the offset of the index (#278): the gallery's
/// `mozilla_mar` reads the start of the first file as the size and the number of signatures.
#[test]
fn mar_archive_reads_the_file_that_mar_writes_without_signatures() {
    let v = decode_real("mar_archive", "archive/mozilla_mar/mar_two_files.bin");
    assert_eq!(v["is_newer_form"], false);
    assert!(v["newer_header"].is_null());
    let entries = &v["index"]["entries"]["entries"];
    assert_eq!(entries.as_array().unwrap().len(), 2);
    assert_eq!(entries[0]["file_name"], "a.txt");
    assert_eq!(entries[0]["ofs_content"], 8);
    assert_eq!(entries[0]["len_content"], 400);
    assert_eq!(entries[1]["file_name"], "b.bin");
    assert_eq!(entries[1]["body"]["len"], 700);
}

/// The list that `cert-to-efi-sig-list` writes has no 4 bytes of attributes at its start (#278): the gallery's
/// `efivar_signature_list` is of the file of an EFI variable, which has them.
#[test]
fn efi_signature_list_reads_the_list_that_efitools_writes() {
    let v = decode_real(
        "efi_signature_list",
        "security/efivar_signature_list/efitools_x509.bin",
    );
    let list = &v["lists"][0];
    assert_eq!(list["is_x509"], true);
    assert_eq!(list["len_signature_list"], 827);
    assert_eq!(list["len_signature_header"], 0);
    assert_eq!(
        list["signatures"][0]["owner"]["hex"],
        "11111111222233334444555555555555"
    );
    // a certificate in DER starts with a sequence
    assert!(
        list["signatures"][0]["data"]["hex"]
            .as_str()
            .unwrap()
            .starts_with("30")
    );
}

/// The files that Pillow writes have the footer and no extension area (the offset is zero): the gallery's definition
/// reads one at offset 0 (#275); this type does not, and has all the pixels as the image data.
#[test]
fn tga_image_reads_the_files_pillow_writes_that_have_no_extension_area() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/samples/gallery/image/tga");
    for (name, pixels) in [
        ("pillow_rgb_raw.bin", Some(16 * 16 * 3)),
        ("pillow_rgb_rle.bin", None),
    ] {
        let text = parse_file(
            &dir.join(name),
            "tga_image",
            &[types()],
            Some(Path::new("/nonexistent/config.toml")),
            OutputFormat::Json,
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let v: Json = serde_json::from_str(&text).unwrap();
        assert_eq!(v["is_version_2"], true, "{name}");
        assert_eq!(v["footer"]["ofs_extension_area"], 0, "{name}");
        assert!(v["extension_area"].is_null(), "{name}");
        assert!(v["developer_directory"].is_null(), "{name}");
        assert_eq!(v["width"], 16, "{name}");
        // the image data ends where the footer starts; a run-length encoded image has fewer or more bytes than pixels
        let size = std::fs::metadata(dir.join(name)).unwrap().len();
        assert_eq!(v["image_data"]["len"], size - 18 - 26, "{name}");
        if let Some(pixels) = pixels {
            assert_eq!(v["image_data"]["len"], pixels, "{name}");
        }
    }
}
