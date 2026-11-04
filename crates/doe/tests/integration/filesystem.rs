//! The list of files and directories of an archive, from the `filesystem:` key of a schema.

use std::path::{Path, PathBuf};

use doe::filesystem::{Kind, flag};

fn types() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../types")
}

/// Parse `file` of the samples with the library type `name`, as the web page does, and give the listing.
fn listing(name: &str, file: &str) -> doe::filesystem::Listing {
    let schema = std::fs::read_to_string(types().join(format!("{name}.yaml"))).unwrap();
    let bytes = std::fs::read(types().join("samples").join(file)).unwrap();
    let request = serde_json::json!({"schema": schema, "path": format!("{name}.yaml")});
    let response = doe::api::parse(&request.to_string(), &bytes);
    assert!(response.ok, "{:?}", response.error);
    response
        .filesystem
        .expect("the schema says it is a file system")
}

fn path_of(entry: &doe::filesystem::Entry) -> String {
    entry.path.join("/")
}

#[test]
fn a_tar_is_a_list_of_directories_and_files_with_sizes_modes_and_times() {
    let list = listing("tar", "sample.tar");
    // the block that ends the archive is not an entry
    let names: Vec<String> = list.entries.iter().map(path_of).collect();
    assert_eq!(names, ["docs", "docs/hello.txt", "data.bin"]);
    assert_eq!(list.entries[0].kind, Kind::Directory);
    assert_eq!(list.entries[1].kind, Kind::File);
    assert_eq!(list.entries[1].size, Some(6));
    assert_eq!(list.entries[1].mode, Some(0o644));
    assert_eq!(list.entries[1].mtime, Some(1_700_000_000));
    assert!(list.entries.iter().all(|e| e.flags.is_empty()));
    // the contents are a range of the input, and the node is a path in the trace (entries, then the third child)
    let (first, end) = list.entries[1].data.unwrap();
    assert_eq!(end - first, 6);
    let bytes = std::fs::read(types().join("samples/sample.tar")).unwrap();
    let start = usize::try_from(first).unwrap();
    assert_eq!(&bytes[start..start + 5], b"hello");
    assert_eq!(list.entries[1].trace_path.len(), 2);
    assert_eq!(list.entries[1].trace_path[1], 1);
}

#[test]
fn an_ar_has_members_and_a_zip_has_entries_without_a_preview() {
    let ar = listing("ar", "sample.a");
    assert_ne!(ar.entries.len(), 0);
    assert!(ar.entries.iter().all(|e| e.kind == Kind::File));
    assert!(ar.entries.iter().any(|e| e.data.is_some()));
    let zip = listing("zip_archive", "sample.zip");
    assert!(zip.entries.len() >= 2);
    assert!(zip.entries.iter().all(|e| e.data.is_none()), "{zip:?}");
    assert!(zip.entries.iter().any(|e| e.size.is_some()));
}

/// A schema of an archive of text lines `kind path`: all the cases that cannot be placed.
const ODD: &str = "meta:
  id: odd
filesystem:
  entries: entries
  path: path
  kind: type
  kinds:
    directory: [d]
    symlink: [l]
  link: target
seq:
  - id: entries
    type: entry
    repeat: eos
types:
  entry:
    seq:
      - id: type
        type: str
        size: 1
        encoding: ASCII
      - id: path
        type: str
        size: 8
        encoding: ASCII
      - id: target
        type: str
        size: 4
        encoding: ASCII
";

#[test]
fn entries_that_cannot_be_placed_are_listed_with_a_flag_and_never_resolved() {
    let rows: [(&str, &str, &str); 7] = [
        ("d", "a/./b   ", "    "),
        ("f", "a/b/c   ", "    "),
        ("f", "a/b/c   ", "    "),
        ("f", "../x    ", "    "),
        ("f", "/etc/pw ", "    "),
        ("f", "        ", "    "),
        ("l", "link    ", "/t  "),
    ];
    let mut bytes = Vec::new();
    for (kind, path, target) in rows {
        bytes.extend(kind.as_bytes());
        bytes.extend(path.as_bytes());
        bytes.extend(target.as_bytes());
    }
    let request = serde_json::json!({"schema": ODD});
    let response = doe::api::parse(&request.to_string(), &bytes);
    assert!(response.ok, "{:?}", response.error);
    let list = response.filesystem.unwrap();
    assert_eq!(list.entries.len(), 7);
    let by = |i: usize| {
        (
            path_of(&list.entries[i]),
            list.entries[i].flags.clone(),
            list.entries[i].kind,
        )
    };
    // trailing spaces are part of the text; only the slashes and dots are normalised
    assert_eq!(by(0).0, "a/b   ");
    assert_eq!(by(0).2, Kind::Directory);
    assert_eq!(by(2).1, [flag::REPEATED.to_owned()]);
    assert!(by(3).1.iter().any(|f| f == flag::PARENT));
    assert!(by(4).1.iter().any(|f| f == flag::ABSOLUTE));
    assert_eq!(by(6).2, Kind::Symlink);
    assert_eq!(list.entries[6].link.as_deref(), Some("/t  "));
}

#[test]
fn a_schema_without_the_key_has_no_listing() {
    let schema = std::fs::read_to_string(types().join("flac.yaml")).unwrap();
    let bytes = std::fs::read(types().join("samples/sample.flac")).unwrap();
    let request = serde_json::json!({"schema": schema});
    assert!(
        doe::api::parse(&request.to_string(), &bytes)
            .filesystem
            .is_none()
    );
}

#[test]
fn the_key_is_checked_like_the_others() {
    let bad = "meta:\n  id: t\nfilesystem:\n  entries: e\nseq: []\n";
    assert!(
        doe::schema::raw::RawSchema::from_yaml_str(bad).is_err(),
        "the path is required"
    );
    let unknown = "meta:\n  id: t\nfilesystem:\n  entries: e\n  path: p\n  colour: red\nseq: []\n";
    assert!(doe::schema::raw::RawSchema::from_yaml_str(unknown).is_err());
}
