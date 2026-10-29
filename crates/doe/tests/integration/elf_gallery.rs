//! The Kaitai Struct gallery's `elf.ksy`, which is what to use for ELF files (the library has no type of its
//! own), run against ELF files built in memory: the same files, and the same facts about them, that the
//! hand-written type of the library was tested with.

use std::path::{Path, PathBuf};

use doe::{DoeError, OutputFormat, parse_file};
use serde_json::Value as Json;
use tempfile::TempDir;

fn spec() -> PathBuf {
    super::conformance::gallery_dir().join("executable/elf.ksy")
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
    // the string table of the names of the sections is the last one
    push_u16(
        &mut out,
        u16::try_from(sections.len().saturating_sub(1)).unwrap(),
    );
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
        &[[0xcc; 100].as_slice(), b"\0.text\0\0"].concat(),
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

fn name_of_enum(v: &Json) -> &str {
    v["name"].as_str().unwrap_or("<unnamed>")
}

#[test]
fn header_fields_and_enums_are_decoded() {
    let v = decode(&executable()).unwrap();
    assert_eq!(name_of_enum(&v["abi"]), "gnu");
    assert_eq!(v["bits"]["name"], "b64");
    assert_eq!(v["endian"]["name"], "le");
    let h = &v["header"];
    assert_eq!(name_of_enum(&h["e_type"]), "shared");
    assert_eq!(name_of_enum(&h["machine"]), "x86_64");
    assert_eq!(h["entry_point"], 0x1040);
    assert_eq!(h["ofs_program_headers"], 64);
    assert_eq!(h["program_header_size"], 56);
    assert_eq!(h["num_program_headers"], 3);
    assert_eq!(h["num_section_headers"], 3);
    assert_eq!(h["flags"]["hex"], "05000000");
}

#[test]
fn program_headers_are_decoded_in_order() {
    let v = decode(&executable()).unwrap();
    let headers = v["header"]["program_headers"].as_array().unwrap();
    assert_eq!(headers.len(), 3);
    assert_eq!(name_of_enum(&headers[0]["type"]), "phdr");
    assert_eq!(headers[0]["ofs_body"], 64);
    assert_eq!(headers[0]["len_body"], 112);
    assert_eq!(name_of_enum(&headers[1]["type"]), "load");
    assert_eq!(headers[1]["flags64"], 5);
    assert_eq!(headers[1]["align"], 0x1000);
    assert_eq!(name_of_enum(&headers[2]["type"]), "gnu_stack");
}

#[test]
fn section_headers_are_read_from_where_the_header_points() {
    let v = decode(&executable()).unwrap();
    let sections = v["header"]["section_headers"].as_array().unwrap();
    assert_eq!(sections.len(), 3);
    assert_eq!(name_of_enum(&sections[0]["type"]), "null_type");
    assert_eq!(name_of_enum(&sections[1]["type"]), "progbits");
    assert_eq!(sections[1]["flags"], 6);
    assert_eq!(sections[1]["ofs_body"], 232);
    assert_eq!(sections[1]["len_body"], 100);
    assert_eq!(name_of_enum(&sections[2]["type"]), "strtab");
    assert_eq!(sections[2]["ofs_name"], 7);
    assert_eq!(sections[1]["name"], ".text");
}

#[test]
fn a_relocatable_object_without_program_headers_is_decoded() {
    let bytes = elf64(
        1,
        183,
        &[],
        &[[0x90; 40].as_slice(), b"\0.text\0\0"].concat(),
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
            Section {
                name: 0,
                kind: 3,
                flags: 0,
                offset: 104,
                size: 8,
            },
        ],
    );
    let v = decode(&bytes).unwrap();
    assert_eq!(name_of_enum(&v["header"]["e_type"]), "relocatable");
    assert_eq!(name_of_enum(&v["header"]["machine"]), "aarch64");
    let sections = v["header"]["section_headers"].as_array().unwrap();
    assert_eq!(sections.len(), 3);
    assert_eq!(sections[1]["name"], ".text");
}

#[test]
fn a_file_without_section_headers_is_decoded() {
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
    assert_eq!(name_of_enum(&v["header"]["machine"]), "riscv");
    assert_eq!(v["header"]["num_section_headers"], 0);
}

/// A difference from the hand-written type, which kept a machine it did not know: the gallery's definition
/// says a machine is one of its enum, and a file with another is not read.
#[test]
fn a_machine_that_the_definition_does_not_know_is_refused() {
    let mut bytes = executable();
    // e_machine, at byte 18
    bytes[18..20].copy_from_slice(&0x1234_u16.to_le_bytes());
    let error = decode(&bytes).unwrap_err().to_string();
    assert!(error.contains("not a member of the enum"), "{error}");
}

#[test]
fn input_that_is_not_an_elf_file_and_empty_or_truncated_files_are_errors() {
    let mut not_elf = executable();
    not_elf[0] = b'X';
    assert!(decode(&not_elf).is_err());
    assert!(decode(&[]).is_err());
    for len in [1, 3, 10, 20] {
        assert!(decode(&executable()[..len]).is_err(), "{len} bytes");
    }
}

#[test]
fn a_table_that_points_outside_the_file_is_an_error_and_not_a_panic() {
    let mut bytes = executable();
    // e_shoff, at byte 40, far past the end
    bytes[40..48].copy_from_slice(&0xffff_ffff_u64.to_le_bytes());
    // either the value is read lazily and the table is missing, or the parse fails: not a panic
    let _ = decode(&bytes);
}
