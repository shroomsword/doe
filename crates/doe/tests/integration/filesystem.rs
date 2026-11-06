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
fn an_ar_has_members_and_a_zip_has_entries_that_say_whether_their_data_is_the_file() {
    let ar = listing("ar", "sample.a");
    assert_ne!(ar.entries.len(), 0);
    assert!(ar.entries.iter().all(|e| e.kind == Kind::File));
    assert!(ar.entries.iter().any(|e| e.data.is_some()));
    assert!(ar.entries.iter().all(|e| e.data_is_file));
    let zip = listing("zip_archive", "sample.zip");
    assert!(zip.entries.len() >= 2);
    assert!(zip.entries.iter().any(|e| e.size.is_some()));
    // the stored entry's data are the file; the deflated ones are compressed, and are not
    let by_name = |name: &str| zip.entries.iter().find(|e| path_of(e) == name).unwrap();
    assert!(by_name("docs/hello.txt").data.is_some() && by_name("docs/hello.txt").data_is_file);
    assert!(by_name("data.bin").data.is_some() && !by_name("data.bin").data_is_file);
    assert!(by_name("stream.txt").data.is_some() && !by_name("stream.txt").data_is_file);
    // a directory has no data
    assert!(by_name("docs").data.is_none());
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

/// The MD5 of the file `name` of the sample `file`, from the entry's `data` range.
fn md5_of_entry(schema: &str, file: &str, name: &str) -> Option<String> {
    let list = listing(schema, file);
    let entry = list.entries.iter().find(|e| path_of(e) == name)?;
    let bytes = std::fs::read(types().join("samples").join(file)).unwrap();
    doe::filesystem::md5_of(&bytes, entry.data?)
}

#[test]
fn the_md5_of_a_file_is_that_of_md5sum_for_tar_ar_cpio_and_stored_zip_entries() {
    // the values are those of `md5sum` on the files that `tar`, `ar`, `cpio` and `unzip` extract
    let tar = |name| md5_of_entry("tar", "sample.tar", name);
    assert_eq!(
        tar("docs/hello.txt").as_deref(),
        Some("b1946ac92492d2347c6235b4d2611184")
    );
    assert_eq!(
        tar("data.bin").as_deref(),
        Some("c56bd5480f6e5413cb62a0ad9666613a")
    );
    // a directory has no contents
    assert_eq!(tar("docs"), None);
    let ar = |name| md5_of_entry("ar", "sample.a", name);
    assert_eq!(
        ar("hello.txt").as_deref(),
        Some("5d41402abc4b2a76b9719d911017c592")
    );
    assert_eq!(
        ar("bye.txt").as_deref(),
        Some("4bf24bc9ad9372c452f8f4588dee7f32")
    );
    let cpio = |name| md5_of_entry("cpio", "sample.cpio", name);
    assert_eq!(
        cpio("docs/hello.txt").as_deref(),
        Some("b1946ac92492d2347c6235b4d2611184")
    );
    assert_eq!(
        cpio("odd").as_deref(),
        Some("a2b6f2a6066ed8700d83335fc50a2b8e")
    );
    let zip = |name| md5_of_entry("zip_archive", "sample.zip", name);
    assert_eq!(
        zip("docs/hello.txt").as_deref(),
        Some("b1946ac92492d2347c6235b4d2611184")
    );
}

#[test]
fn a_checksum_is_computed_in_pieces_and_a_range_outside_the_input_has_none() {
    use doe::filesystem::{Algorithm, Hasher, digest_of};
    // RFC 1321, appendix A.5
    let cases: [(&[u8], &str); 4] = [
        (b"", "d41d8cd98f00b204e9800998ecf8427e"),
        (b"a", "0cc175b9c0f1b6a831c399e269772661"),
        (b"abc", "900150983cd24fb0d6963f7d28e17f72"),
        (b"message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
    ];
    for (bytes, hex) in cases {
        assert_eq!(
            digest_of(Algorithm::Md5, bytes, (0, bytes.len() as u64)).as_deref(),
            Some(hex)
        );
        let mut pieces = Hasher::new(Algorithm::Md5);
        for byte in bytes {
            pieces.update(&[*byte]);
        }
        assert_eq!(pieces.finish(), hex);
    }
    let input = vec![7_u8; 100_000];
    let mut chunked = Hasher::new(Algorithm::Md5);
    for chunk in input.chunks(4096) {
        chunked.update(chunk);
    }
    assert_eq!(
        Some(chunked.finish()),
        digest_of(Algorithm::Md5, &input, (0, 100_000))
    );
    assert_eq!(digest_of(Algorithm::Md5, b"abc", (1, 9)), None);
    assert_eq!(digest_of(Algorithm::Md5, b"abc", (2, 1)), None);
    assert_eq!(Algorithm::from_name("MD5"), Some(Algorithm::Md5));
    assert_eq!(Algorithm::from_name("crc"), None);
}

/// Two entries of four bytes, a name, a kind and two bytes of data: "a" has kind 1 and "b" kind 0.
const FLAGGED: &str = "meta:
  id: flagged
filesystem:
  entries: entries
  path: path
  data: body
  data_is_file: file
seq:
  - id: entries
    type: entry
    repeat: eos
types:
  entry:
    seq:
      - id: path
        type: str
        size: 1
        encoding: ASCII
      - id: kind
        type: u1
      - id: body
        size: 2
    instances:
      file:
        value: kind == 1
";

#[test]
fn data_is_file_follows_the_field_when_the_key_is_there_and_is_true_without_it() {
    let input = b"axxb yy";
    let list = |schema: &str| {
        let request = serde_json::json!({"schema": schema});
        let response = doe::api::parse(&request.to_string(), input);
        assert!(response.ok, "{:?}", response.error);
        response.filesystem.unwrap()
    };
    let with = list(FLAGGED);
    assert_eq!(with.entries.len(), 2);
    assert!(with.entries[0].data_is_file && with.entries[0].data == Some((2, 4)));
    assert!(!with.entries[1].data_is_file && with.entries[1].data == Some((6, 8)));
    let without = list(&FLAGGED.replace(
        "  data_is_file: file
",
        "",
    ));
    assert!(without.entries.iter().all(|e| e.data_is_file));
}

/// Run `program` with `args` in `dir` (feeding `stdin`); None when the program is not on this machine or fails.
#[cfg(unix)]
fn run(dir: &Path, program: &str, args: &[&str], stdin: Option<&str>) -> Option<Vec<u8>> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    if let Some(text) = stdin {
        child.stdin.take()?.write_all(text.as_bytes()).ok()?;
    }
    let output = child.wait_with_output().ok()?;
    output.status.success().then_some(output.stdout)
}

#[cfg(unix)]
#[test]
fn the_md5_of_every_file_of_archives_made_by_tar_ar_and_cpio_is_that_of_the_original() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // an empty file, one byte, a few blocks, and more than a megabyte, in a folder and not
    let files: [(&str, Vec<u8>); 5] = [
        ("empty.txt", Vec::new()),
        ("one.bin", vec![0x41]),
        (
            "text.txt",
            b"hello
world
"
            .repeat(500),
        ),
        (
            "sub/big.bin",
            (0..1_500_000_u32)
                .map(|i| ((i.wrapping_mul(2_654_435_761) >> 13) & 0xff) as u8)
                .collect(),
        ),
        ("sub/zeros.bin", vec![0; 1024]),
    ];
    std::fs::create_dir_all(root.join("sub")).unwrap();
    for (name, bytes) in &files {
        std::fs::write(root.join(name), bytes).unwrap();
    }
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    let mut checked = 0;
    // (the schema, the archive, the program that makes it)
    let tar = {
        let mut args = vec!["-cf", "-"];
        args.extend(&names);
        run(root, "tar", &args, None)
    };
    // ar members have no folders, and it writes its archive to a file
    let ar = run(
        root,
        "ar",
        &["rc", "a.a", "empty.txt", "one.bin", "text.txt"],
        None,
    )
    .and_then(|_| std::fs::read(root.join("a.a")).ok());
    let cpio = run(
        root,
        "cpio",
        &["-o", "-H", "newc", "--quiet"],
        Some(&names.join(
            "
",
        )),
    );
    for (schema, archive) in [("tar", tar), ("ar", ar), ("cpio", cpio)] {
        let Some(archive) = archive else {
            continue;
        };
        let request = serde_json::json!({"schema": std::fs::read_to_string(types().join(format!("{schema}.yaml"))).unwrap(), "path": format!("{schema}.yaml")});
        let response = doe::api::parse(&request.to_string(), &archive);
        assert!(response.ok, "{schema}: {:?}", response.error);
        let list = response.filesystem.unwrap();
        for entry in list.entries.iter().filter(|e| e.kind == Kind::File) {
            let name = path_of(entry);
            let original = files
                .iter()
                .find(|(n, _)| *n == name || n.rsplit('/').next() == Some(name.as_str()));
            let Some((_, original)) = original else {
                continue;
            };
            let range = entry.data.unwrap_or((0, 0));
            let digest = doe::filesystem::md5_of(&archive, range).unwrap();
            let wanted = doe::filesystem::md5_of(original, (0, original.len() as u64)).unwrap();
            assert_eq!(digest, wanted, "{schema}: {name}");
            checked += 1;
        }
    }
    // tar is on every machine this runs on; the others are skipped when their program is not
    assert!(checked >= 5, "only {checked} files were compared");
}

#[test]
fn a_tar_whose_numbers_end_in_spaces_as_bsdtar_writes_them_reads_the_same() {
    // one file "a.txt" with "hello"; the size, mode and time are octal text ended by a space, with no NUL
    let mut header = [0_u8; 512];
    let mut put =
        |at: usize, text: &str| header[at..at + text.len()].copy_from_slice(text.as_bytes());
    put(0, "a.txt");
    put(100, "000644  ");
    put(108, "0000000 ");
    put(116, "0000000 ");
    put(124, "00000000005 ");
    put(136, "14524770400 ");
    put(148, "        ");
    put(156, "0");
    put(257, "ustar ");
    put(263, "00");
    let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
    header[148..156].copy_from_slice(format!("{sum:06o}  ").as_bytes());
    let mut archive = header.to_vec();
    archive.extend_from_slice(b"hello");
    archive.resize(1024, 0);
    archive.resize(2048, 0);
    let request = serde_json::json!({"schema": std::fs::read_to_string(types().join("tar.yaml")).unwrap(), "path": "tar.yaml"});
    let response = doe::api::parse(&request.to_string(), &archive);
    assert!(response.ok, "{:?}", response.error);
    let list = response.filesystem.unwrap();
    assert_eq!(list.entries.len(), 1);
    assert_eq!(list.entries[0].size, Some(5));
    assert_eq!(list.entries[0].mode, Some(0o644));
    assert_eq!(list.entries[0].mtime, Some(0o14_524_770_400));
    assert_eq!(
        doe::filesystem::md5_of(&archive, list.entries[0].data.unwrap()).as_deref(),
        Some("5d41402abc4b2a76b9719d911017c592")
    );
}
