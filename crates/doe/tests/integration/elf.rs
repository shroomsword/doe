//! Tests for `types/elf.yaml`, run against ELF files built in memory.

use std::path::{Path, PathBuf};

use doe::{DoeError, OutputFormat, parse_file};
use serde_json::Value as Json;
use tempfile::TempDir;

fn spec() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../types/elf.yaml")
}

struct Segment {
    kind: u32,
    flags: u32,
    offset: u64,
    filesz: u64,
}

struct Section {
    name: u32,
    kind: u32,
    flags: u64,
    offset: u64,
    size: u64,
}

fn push_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn push_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Build an ELF64 little-endian file: header, program headers, `body`, then
/// section headers, which is the layout linkers produce.
fn elf64(
    e_type: u16,
    machine: u16,
    segments: &[Segment],
    body: &[u8],
    sections: &[Section],
) -> Vec<u8> {
    let phoff = if segments.is_empty() { 0 } else { 64 };
    let shoff = if sections.is_empty() {
        0
    } else {
        64 + 56 * u64::try_from(segments.len()).unwrap() + u64::try_from(body.len()).unwrap()
    };
    let mut out = vec![0x7f, b'E', b'L', b'F', 2, 1, 1, 3, 0];
    out.extend_from_slice(&[0; 7]);
    push_u16(&mut out, e_type);
    push_u16(&mut out, machine);
    push_u32(&mut out, 1);
    push_u64(&mut out, 0x1040); // e_entry
    push_u64(&mut out, phoff);
    push_u64(&mut out, shoff);
    push_u32(&mut out, 0x5); // e_flags
    push_u16(&mut out, 64);
    push_u16(&mut out, if segments.is_empty() { 0 } else { 56 });
    push_u16(&mut out, u16::try_from(segments.len()).unwrap());
    push_u16(&mut out, 64);
    push_u16(&mut out, u16::try_from(sections.len()).unwrap());
    push_u16(&mut out, 0);
    assert_eq!(out.len(), 64);
    for s in segments {
        push_u32(&mut out, s.kind);
        push_u32(&mut out, s.flags);
        push_u64(&mut out, s.offset);
        push_u64(&mut out, 0x40_0000 + s.offset); // p_vaddr
        push_u64(&mut out, 0x40_0000 + s.offset); // p_paddr
        push_u64(&mut out, s.filesz);
        push_u64(&mut out, s.filesz); // p_memsz
        push_u64(&mut out, 0x1000);
    }
    out.extend_from_slice(body);
    for s in sections {
        push_u32(&mut out, s.name);
        push_u32(&mut out, s.kind);
        push_u64(&mut out, s.flags);
        push_u64(&mut out, 0);
        push_u64(&mut out, s.offset);
        push_u64(&mut out, s.size);
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_u64(&mut out, 8);
        push_u64(&mut out, 0);
    }
    out
}

fn decode(bytes: &[u8]) -> Result<Json, DoeError> {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("sample.elf");
    std::fs::write(&file, bytes).unwrap();
    let text = parse_file(
        &file,
        spec().to_str().unwrap(),
        &[],
        Some(Path::new("/nonexistent/config.toml")),
        OutputFormat::Json,
    )?;
    Ok(serde_json::from_str(&text).unwrap())
}

fn name_of(v: &Json) -> &str {
    v["name"].as_str().unwrap_or("<unnamed>")
}

fn executable() -> Vec<u8> {
    elf64(
        3,
        62,
        &[
            Segment {
                kind: 6,
                flags: 4,
                offset: 64,
                filesz: 112,
            },
            Segment {
                kind: 1,
                flags: 5,
                offset: 0,
                filesz: 400,
            },
            Segment {
                kind: 0x6474_e551,
                flags: 6,
                offset: 0,
                filesz: 0,
            },
        ],
        &[0xcc; 100],
        &[
            Section {
                name: 0,
                kind: 0,
                flags: 0,
                offset: 0,
                size: 0,
            },
            Section {
                name: 1,
                kind: 1,
                flags: 6,
                offset: 232,
                size: 100,
            },
            Section {
                name: 7,
                kind: 3,
                flags: 0,
                offset: 332,
                size: 8,
            },
        ],
    )
}

#[test]
fn header_fields_and_enums_are_decoded() {
    let v = decode(&executable()).unwrap();
    assert_eq!(v["$type"], "elf");
    assert_eq!(name_of(&v["header"]["os_abi"]), "gnu_linux");
    assert_eq!(name_of(&v["header"]["e_type"]), "shared_object");
    assert_eq!(name_of(&v["header"]["e_machine"]), "x86_64");
    assert_eq!(v["header"]["e_entry"], 0x1040);
    assert_eq!(v["header"]["e_phoff"], 64);
    assert_eq!(v["header"]["e_flags"], 5);
    assert_eq!(v["header"]["e_phentsize"], 56);
    assert_eq!(v["header"]["e_phnum"], 3);
    assert_eq!(v["header"]["e_shnum"], 3);
}

#[test]
fn program_headers_are_decoded_in_order() {
    let v = decode(&executable()).unwrap();
    let headers = v["program_headers"].as_array().unwrap();
    assert_eq!(headers.len(), 3);
    assert_eq!(name_of(&headers[0]["p_type"]), "phdr");
    assert_eq!(headers[0]["p_offset"], 64);
    assert_eq!(headers[0]["p_filesz"], 112);
    assert_eq!(name_of(&headers[1]["p_type"]), "load");
    assert_eq!(headers[1]["p_flags"], 5);
    assert_eq!(headers[1]["p_align"], 0x1000);
    assert_eq!(name_of(&headers[2]["p_type"]), "gnu_stack");
}

#[test]
fn section_headers_are_read_from_e_shoff() {
    let v = decode(&executable()).unwrap();
    assert!(v.get("contents").is_none());
    let sections = v["section_headers"].as_array().unwrap();
    assert_eq!(sections.len(), 3);
    assert_eq!(name_of(&sections[0]["sh_type"]), "null");
    assert_eq!(name_of(&sections[1]["sh_type"]), "progbits");
    assert_eq!(sections[1]["sh_flags"], 6);
    assert_eq!(sections[1]["sh_offset"], 232);
    assert_eq!(sections[1]["sh_size"], 100);
    assert_eq!(name_of(&sections[2]["sh_type"]), "strtab");
    assert_eq!(sections[2]["sh_name"], 7);
}

#[test]
fn relocatable_object_without_program_headers_is_decoded() {
    let bytes = elf64(
        1,
        183,
        &[],
        &[0x90; 40],
        &[
            Section {
                name: 0,
                kind: 0,
                flags: 0,
                offset: 0,
                size: 0,
            },
            Section {
                name: 1,
                kind: 1,
                flags: 6,
                offset: 64,
                size: 40,
            },
        ],
    );
    let v = decode(&bytes).unwrap();
    assert_eq!(name_of(&v["header"]["e_type"]), "relocatable");
    assert_eq!(name_of(&v["header"]["e_machine"]), "aarch64");
    // The field is skipped (e_phoff is 0), not an empty array.
    assert!(v["program_headers"].is_null());
    assert_eq!(v["section_headers"].as_array().unwrap().len(), 2);
}

#[test]
fn file_without_section_headers_is_decoded() {
    let bytes = elf64(
        2,
        243,
        &[Segment {
            kind: 1,
            flags: 5,
            offset: 0,
            filesz: 64,
        }],
        &[1, 2, 3],
        &[],
    );
    let v = decode(&bytes).unwrap();
    assert_eq!(name_of(&v["header"]["e_machine"]), "risc_v");
    assert_eq!(v["header"]["e_shnum"], 0);
    // The field is skipped (rendered as null), not an empty array.
    assert!(v["section_headers"].is_null(), "{}", v["section_headers"]);
}

#[test]
fn unknown_enum_values_are_kept_with_no_name() {
    let bytes = elf64(
        3,
        0x1234,
        &[],
        &[],
        &[Section {
            name: 0,
            kind: 0x7000_0003,
            flags: 0,
            offset: 64,
            size: 0,
        }],
    );
    let v = decode(&bytes).unwrap();
    assert_eq!(v["header"]["e_machine"]["value"], 0x1234);
    assert!(v["header"]["e_machine"]["name"].is_null());
    assert_eq!(v["section_headers"][0]["sh_type"]["value"], 0x7000_0003u64);
}

#[test]
fn elf32_big_endian_and_non_elf_input_are_rejected() {
    let good = executable();

    let mut elf32 = good.clone();
    elf32[4] = 1;
    let mut big_endian = good.clone();
    big_endian[5] = 2;
    let mut bad_magic = good.clone();
    bad_magic[1] = b'X';

    for (what, bytes) in [
        ("ELF32", elf32),
        ("big-endian", big_endian),
        ("bad magic", bad_magic),
    ] {
        let err = decode(&bytes).unwrap_err();
        assert!(
            matches!(&err, DoeError::Schema(m) if m.contains("contents mismatch")),
            "{what}: {err:?}"
        );
    }
}

#[test]
fn empty_and_truncated_files_are_buffer_errors() {
    let good = executable();
    for len in [0, 5, 40, 63, 100, good.len() - 1] {
        let err = decode(&good[..len]).unwrap_err();
        assert!(
            matches!(&err, DoeError::Schema(m) if m.contains("buffer overrun")),
            "len {len}: {err:?}"
        );
    }
}

#[test]
fn tables_are_found_wherever_the_header_points() {
    // Section headers in the middle of the file, with bytes after them, and
    // program headers after those: nothing assumes the usual layout.
    let mut bytes = executable();
    let sh_start = 64 + 3 * 56 + 100;
    let table = bytes[sh_start..].to_vec();
    bytes.truncate(sh_start);
    bytes.extend_from_slice(&[0xee; 16]);
    let new_shoff = 64 + 3 * 56 + 100 + 16;
    bytes.extend_from_slice(&table);
    bytes[40..48].copy_from_slice(&u64::try_from(new_shoff).unwrap().to_le_bytes());
    let v = decode(&bytes).unwrap();
    assert_eq!(v["section_headers"].as_array().unwrap().len(), 3);
    assert_eq!(name_of(&v["section_headers"][1]["sh_type"]), "progbits");
    assert_eq!(v["program_headers"].as_array().unwrap().len(), 3);
}

#[test]
fn larger_entry_sizes_are_honored() {
    let mut bytes = elf64(
        2,
        62,
        &[],
        &[],
        &[
            Section {
                name: 0,
                kind: 0,
                flags: 0,
                offset: 0,
                size: 0,
            },
            Section {
                name: 1,
                kind: 1,
                flags: 6,
                offset: 64,
                size: 40,
            },
        ],
    );
    // Stretch each 64-byte entry to 72 bytes.
    let table = bytes.split_off(64);
    for entry in table.chunks(64) {
        bytes.extend_from_slice(entry);
        bytes.extend_from_slice(&[0xaa; 8]);
    }
    bytes[58..60].copy_from_slice(&72u16.to_le_bytes());
    let v = decode(&bytes).unwrap();
    let sections = v["section_headers"].as_array().unwrap();
    assert_eq!(sections.len(), 2);
    assert_eq!(name_of(&sections[1]["sh_type"]), "progbits");
    assert_eq!(sections[1]["sh_size"], 40);
}

#[test]
fn table_offsets_outside_the_file_are_errors_not_panics() {
    for (at, value) in [
        (32, 70_000u64),
        (40, 70_000),
        (40, u64::MAX),
        (32, u64::MAX),
    ] {
        let mut bytes = executable();
        bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
        let err = decode(&bytes).unwrap_err();
        assert!(
            matches!(err, DoeError::FieldError { .. }),
            "{at} {value}: {err:?}"
        );
    }
}
