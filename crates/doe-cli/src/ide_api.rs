//! What `doe --ide` offers beyond the page: the things of this machine that a
//! page in a browser cannot see. All of it is read-only, and what it can reach
//! is decided here, not by the request:
//!
//! - the **types** found through the include paths and the configuration (the
//!   schema files that `discover_types` lists), and each with the files it
//!   imports;
//! - the **input file** named on the command line, which can be larger than a
//!   browser takes: its bytes by range, and a parse of it that reads it as the
//!   schema asks for it.
//!
//! No request names a path to read except a schema, and that one must be one of
//! the files that `discover_types` lists now (compared as the real file, so `..`
//! and links do not change which file it is).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Context;
use doe::config::discover_types;
use doe::schema::compiler::Compiler;
use doe::schema::source::{FsSource, SchemaSource};
use serde_json::{Value, json};
use std::borrow::Cow;

use crate::ide::{Response, error};

/// The most bytes of the input that one request for bytes gives.
const MAX_RANGE: u64 = 1024 * 1024;
/// The most bytes of the answer to a parse.
const MAX_ANSWER: usize = 64 * 1024 * 1024;

/// The file named on the command line.
#[derive(Debug)]
struct InputFile {
    path: PathBuf,
    name: String,
    size: u64,
}

/// What the server may show of this machine.
#[derive(Debug)]
pub(crate) struct Local {
    include_paths: Vec<PathBuf>,
    input: Option<InputFile>,
    /// Whether a parse is running: one at a time.
    busy: AtomicBool,
}

fn json_response(status: u16, value: &Value) -> Response {
    match serde_json::to_vec(value) {
        Ok(bytes) => Response {
            status,
            content_type: "application/json",
            body: Cow::Owned(bytes),
            head_only: false,
        },
        Err(_) => error(500, "could not make the answer\n"),
    }
}

/// The value of `name` in a query string, with `%` escapes decoded.
fn query_param(query: &str, name: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        (key == name).then(|| percent_decode(value))?
    })
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        match b {
            b'%' => {
                let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            _ => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// A source that reads the filesystem and remembers what it read.
#[derive(Debug)]
struct Recording {
    seen: Arc<Mutex<BTreeMap<PathBuf, String>>>,
}

impl SchemaSource for Recording {
    fn exists(&self, path: &Path) -> bool {
        FsSource.exists(path)
    }

    fn read(&self, path: &Path) -> doe::error::Result<String> {
        let text = FsSource.read(path)?;
        if let Ok(mut seen) = self.seen.lock() {
            seen.insert(path.to_owned(), text.clone());
        }
        Ok(text)
    }
}

impl Local {
    /// `include_paths` are where types are looked for; `input` is the file that
    /// the page may parse natively.
    pub(crate) fn new(include_paths: &[PathBuf], input: Option<&Path>) -> anyhow::Result<Self> {
        let include_paths = include_paths
            .iter()
            .map(|p| std::path::absolute(p).unwrap_or_else(|_| p.clone()))
            .collect();
        let input = input
            .map(|path| -> anyhow::Result<InputFile> {
                let real = std::fs::canonicalize(path)
                    .with_context(|| format!("could not open {}", path.display()))?;
                let meta = std::fs::metadata(&real)
                    .with_context(|| format!("could not open {}", path.display()))?;
                anyhow::ensure!(meta.is_file(), "{} is not a regular file", path.display());
                let name = path
                    .file_name()
                    .map_or_else(|| "input".to_owned(), |n| n.to_string_lossy().into_owned());
                Ok(InputFile {
                    path: real,
                    name,
                    size: meta.len(),
                })
            })
            .transpose()?;
        Ok(Local {
            include_paths,
            input,
            busy: AtomicBool::new(false),
        })
    }

    /// The answer to a request for `/api/<api>`.
    pub(crate) fn handle(&self, method: &str, api: &str, query: &str, body: &[u8]) -> Response {
        match (method, api) {
            ("GET" | "HEAD", "local") => self.info(),
            ("GET" | "HEAD", "types") => self.types(),
            ("GET" | "HEAD", "schema") => self.schema(query),
            ("GET" | "HEAD", "input") => self.bytes(query),
            ("GET" | "HEAD", "detect") => self.detect(query),
            ("POST", "parse") => self.parse(body),
            ("POST", "export") => self.export(query, body),
            _ => error(404, "not found\n"),
        }
    }

    fn info(&self) -> Response {
        json_response(
            200,
            &json!({
                "version": env!("CARGO_PKG_VERSION"),
                "include_paths": self.include_paths,
                "input": self.input.as_ref().map(|i| json!({"name": i.name, "size": i.size})),
            }),
        )
    }

    fn types(&self) -> Response {
        match discover_types(&self.include_paths) {
            Ok(types) => json_response(
                200,
                &Value::Array(
                    types
                        .iter()
                        .map(|t| json!({"id": t.id, "doc": t.doc, "path": t.source}))
                        .collect(),
                ),
            ),
            Err(duplicates) => {
                log::warn!("duplicate type ids on the include paths: {duplicates:?}");
                error(500, "duplicate type ids on the include paths\n")
            }
        }
    }

    /// A schema that `discover_types` lists, and the files it imports.
    fn schema(&self, query: &str) -> Response {
        let Some(wanted) = query_param(query, "path") else {
            return error(400, "path is needed\n");
        };
        let Ok(wanted) = std::fs::canonicalize(&wanted) else {
            return error(403, "not one of the types\n");
        };
        let Ok(types) = discover_types(&self.include_paths) else {
            return error(500, "duplicate type ids on the include paths\n");
        };
        let Some(found) = types
            .iter()
            .find(|t| std::fs::canonicalize(&t.source).is_ok_and(|real| real == wanted))
        else {
            return error(403, "not one of the types\n");
        };

        let seen = Arc::new(Mutex::new(BTreeMap::new()));
        let mut compiler = Compiler::with_source(
            self.include_paths.clone(),
            Box::new(Recording {
                seen: Arc::clone(&seen),
            }),
        );
        // A schema that does not compile still has the files it read: the
        // page reports the error.
        if let Ok(schema) = compiler.read_schema(&found.source) {
            let _ = compiler.process_schema(&schema);
        }
        let mut files = seen.lock().map(|m| m.clone()).unwrap_or_default();
        let Some(text) = files.remove(&found.source) else {
            return error(500, "could not read the schema\n");
        };
        let files: BTreeMap<String, String> = files
            .into_iter()
            .map(|(path, text)| (path.to_string_lossy().into_owned(), text))
            .collect();
        json_response(
            200,
            &json!({
                "path": found.source,
                "text": text,
                "files": files,
                "include_paths": self.include_paths,
            }),
        )
    }

    /// A range of the bytes of the input file.
    fn bytes(&self, query: &str) -> Response {
        let Some(input) = &self.input else {
            return error(404, "no input file was named\n");
        };
        let number = |name| query_param(query, name).and_then(|v| v.parse::<u64>().ok());
        let (Some(offset), Some(len)) = (number("offset"), number("len")) else {
            return error(400, "offset and len are needed\n");
        };
        let len = len.min(MAX_RANGE).min(input.size.saturating_sub(offset));
        let read = || -> std::io::Result<Vec<u8>> {
            let mut file = File::open(&input.path)?;
            file.seek(SeekFrom::Start(offset))?;
            let mut bytes = Vec::new();
            file.take(len).read_to_end(&mut bytes)?;
            Ok(bytes)
        };
        match read() {
            Ok(bytes) => Response {
                status: 200,
                content_type: "application/octet-stream",
                body: Cow::Owned(bytes),
                head_only: false,
            },
            Err(e) => {
                log::warn!("could not read the input file: {e}");
                error(500, "could not read the input file\n")
            }
        }
    }

    /// The whole value of the input file as `doe` prints it (`format=text`,
    /// `json` or `pretty`).
    fn export(&self, query: &str, body: &[u8]) -> Response {
        let Some(input) = &self.input else {
            return error(404, "no input file was named\n");
        };
        let Ok(request) = std::str::from_utf8(body) else {
            return error(400, "the request is not UTF-8\n");
        };
        let format = query_param(query, "format").unwrap_or_default();
        if self.busy.swap(true, Ordering::SeqCst) {
            return error(503, "a parse is already running\n");
        }
        let answer = doe::api::export_file(request, &input.path, &format);
        self.busy.store(false, Ordering::SeqCst);
        // The output is the text of the file, which can be large: no cap here
        // beyond what the machine has, as with the command line.
        match serde_json::to_vec(&answer) {
            Ok(bytes) => Response {
                status: 200,
                content_type: "application/json",
                body: Cow::Owned(bytes),
                head_only: false,
            },
            Err(_) => error(500, "could not make the answer\n"),
        }
    }

    /// Find the type of the input file among the types of the include paths
    /// (`doe::detect`), and say what was tried.
    fn detect(&self, query: &str) -> Response {
        let Some(input) = &self.input else {
            return error(404, "no input file was named\n");
        };
        if self.busy.swap(true, Ordering::SeqCst) {
            return error(503, "a parse is already running\n");
        }
        let options = doe::detect::Options {
            all: query_param(query, "all").is_some_and(|v| v == "1"),
            ..doe::detect::Options::default()
        };
        let found = doe::detect::detect_file(&input.path, &self.include_paths, &options);
        self.busy.store(false, Ordering::SeqCst);
        match found {
            Ok(found) => json_response(
                200,
                &json!({
                    "chosen": found.chosen().map(|a| json!({"name": a.name, "path": a.path})),
                    "attempts": found.attempts.iter().map(|a| json!({
                        "name": a.name,
                        "why": a.why.describe(),
                        "fits": a.outcome.fits(),
                        "outcome": a.outcome.describe(),
                    })).collect::<Vec<_>>(),
                    "known": found.known,
                    "excluded": found.excluded,
                }),
            ),
            Err(e) => {
                log::warn!("detection failed: {e}");
                error(500, "could not look for the schema\n")
            }
        }
    }

    /// Parse the input file with the schema of the request.
    fn parse(&self, body: &[u8]) -> Response {
        let Some(input) = &self.input else {
            return error(404, "no input file was named\n");
        };
        let Ok(request) = std::str::from_utf8(body) else {
            return error(400, "the request is not UTF-8\n");
        };
        if self.busy.swap(true, Ordering::SeqCst) {
            return error(503, "a parse is already running\n");
        }
        let answer = doe::api::parse_file(request, &input.path);
        self.busy.store(false, Ordering::SeqCst);
        match serde_json::to_vec(&answer) {
            Ok(bytes) if bytes.len() <= MAX_ANSWER => Response {
                status: 200,
                content_type: "application/json",
                body: Cow::Owned(bytes),
                head_only: false,
            },
            Ok(_) => error(413, "the result is too large to send to the page\n"),
            Err(_) => error(500, "could not make the answer\n"),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use std::fmt::Write as _;

    use super::*;

    const SCHEMA: &str = "meta:\n  id: pair\nimports:\n  - common\nseq:\n  - id: a\n    type: u1\n  - id: b\n    type: common\n";
    const COMMON: &str = "meta:\n  id: common\nseq:\n  - id: x\n    type: u1\n";

    /// A directory of types, a file outside it, and a server over them.
    struct Fixture {
        dir: tempfile::TempDir,
        local: Local,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let types = dir.path().join("types");
            std::fs::create_dir(&types).unwrap();
            std::fs::write(types.join("pair.ksy"), SCHEMA).unwrap();
            std::fs::write(types.join("common.ksy"), COMMON).unwrap();
            std::fs::write(types.join("notes.txt"), "not a schema").unwrap();
            std::fs::write(
                dir.path().join("outside.ksy"),
                COMMON.replace("common", "outside"),
            )
            .unwrap();
            std::fs::write(dir.path().join("secret.txt"), "secret").unwrap();
            std::fs::write(dir.path().join("input.bin"), [1_u8, 2, 3, 4, 5]).unwrap();
            let local = Local::new(&[types], Some(&dir.path().join("input.bin"))).unwrap();
            Fixture { dir, local }
        }

        fn path(&self, name: &str) -> String {
            self.dir.path().join(name).to_string_lossy().into_owned()
        }

        fn get(&self, api: &str, query: &str) -> Response {
            self.local.handle("GET", api, query, b"")
        }

        fn schema(&self, path: &str) -> Response {
            let mut escaped = String::new();
            for b in path.bytes() {
                write!(escaped, "%{b:02x}").unwrap();
            }
            self.get("schema", &format!("path={escaped}"))
        }
    }

    fn json(response: &Response) -> Value {
        serde_json::from_slice(&response.body).unwrap()
    }

    #[test]
    fn the_types_of_the_include_paths_are_listed() {
        let f = Fixture::new();
        let r = f.get("types", "");
        assert_eq!(r.status, 200);
        let ids: Vec<_> = json(&r)
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(ids, ["common", "pair"]);
    }

    #[test]
    fn a_type_comes_with_the_files_it_imports_and_compiles_from_them() {
        let f = Fixture::new();
        let r = f.schema(&f.path("types/pair.ksy"));
        assert_eq!(r.status, 200);
        let answer = json(&r);
        assert_eq!(answer["text"], SCHEMA);
        assert_eq!(answer["files"].as_object().unwrap().len(), 1);
        // What the page sends back to the module is enough to compile it.
        let request = json!({
            "schema": answer["text"],
            "path": answer["path"],
            "files": answer["files"],
            "include_paths": answer["include_paths"],
        });
        let checked = doe::api::check(&request.to_string());
        assert!(checked.ok, "{:?}", checked.diagnostics);
    }

    #[test]
    fn only_listed_schemas_are_given() {
        let f = Fixture::new();
        for path in [
            f.path("outside.ksy"),
            f.path("secret.txt"),
            f.path("types/notes.txt"),
            f.path("types/../outside.ksy"),
            f.path("types/../secret.txt"),
            "/etc/passwd".to_owned(),
            f.path("types/missing.ksy"),
        ] {
            assert_eq!(f.schema(&path).status, 403, "{path}");
        }
        // The same real file by another name is the same file.
        assert_eq!(f.schema(&f.path("types/../types/pair.ksy")).status, 200);
        assert_eq!(f.get("schema", "").status, 400);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_does_not_make_a_file_available() {
        let f = Fixture::new();
        // A link in the include path to a file that is not a type…
        std::os::unix::fs::symlink(f.path("secret.txt"), f.path("types/link.ksy")).unwrap();
        assert_eq!(f.schema(&f.path("types/link.ksy")).status, 403);
        // …and a link to a directory outside, with a schema in it.
        std::os::unix::fs::symlink(f.dir.path(), f.path("types/up")).unwrap();
        assert_eq!(f.schema(&f.path("types/up/secret.txt")).status, 403);
        assert_eq!(f.schema(&f.path("types/up/outside.ksy")).status, 403);
    }

    #[test]
    fn the_input_is_read_by_range_and_nothing_else_is() {
        let f = Fixture::new();
        let info = json(&f.get("local", ""));
        assert_eq!(info["input"]["name"], "input.bin");
        assert_eq!(info["input"]["size"], 5);
        assert_eq!(&*f.get("input", "offset=1&len=3").body, &[2, 3, 4]);
        assert_eq!(&*f.get("input", "offset=3&len=100").body, &[4, 5]);
        assert!(f.get("input", "offset=9&len=4").body.is_empty());
        // there is no way to name a path
        assert_eq!(
            &*f.get("input", "path=/etc/passwd&file=/etc/passwd&offset=0&len=4")
                .body,
            &[1, 2, 3, 4]
        );
        assert_eq!(f.get("input", "offset=0").status, 400);
        assert_eq!(f.get("nothing", "").status, 404);
        assert_eq!(f.local.handle("POST", "types", "", b"").status, 404);
    }

    #[test]
    fn the_input_is_parsed_natively() {
        let f = Fixture::new();
        let request = json!({
            "schema": "meta:\n  id: t\nseq:\n  - id: a\n    type: u1\n  - id: rest\n    size-eos: true\n",
        });
        let r = f
            .local
            .handle("POST", "parse", "", request.to_string().as_bytes());
        assert_eq!(r.status, 200);
        let answer = json(&r);
        assert_eq!(answer["ok"], true, "{answer}");
        assert_eq!(answer["value"]["a"], 1);
        assert_eq!(answer["trace"]["end"], 40);
        assert_eq!(f.local.handle("POST", "parse", "", b"\xff").status, 400);
    }

    #[test]
    fn the_type_of_the_input_is_looked_for_among_the_types() {
        let f = Fixture::new();
        // no signature and no name that fits: nothing is tried unless all are asked for
        let r = f.get("detect", "");
        assert_eq!(r.status, 200);
        let answer = json(&r);
        assert_eq!(answer["chosen"], Value::Null, "{answer}");
        assert_eq!(answer["attempts"].as_array().unwrap().len(), 0);
        assert_eq!(answer["known"], 2);

        let answer = json(&f.get("detect", "all=1"));
        assert_eq!(answer["chosen"]["name"], "common", "{answer}");
        assert_eq!(answer["attempts"][0]["fits"], true);
        assert!(
            answer["chosen"]["path"]
                .as_str()
                .unwrap()
                .ends_with("common.yaml")
                || answer["chosen"]["path"]
                    .as_str()
                    .unwrap()
                    .ends_with("common.ksy"),
            "{answer}"
        );
    }

    #[test]
    fn without_an_input_file_nothing_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let local = Local::new(&[dir.path().to_owned()], None).unwrap();
        assert_eq!(local.handle("GET", "detect", "", b"").status, 404);
    }

    #[test]
    fn the_input_is_exported_as_the_command_line_prints_it() {
        let f = Fixture::new();
        let schema =
            "meta:\n  id: t\nseq:\n  - id: a\n    type: u1\n  - id: rest\n    size-eos: true\n";
        let request = json!({ "schema": schema }).to_string();
        let schema_file = f.path("t.ksy");
        std::fs::write(&schema_file, schema).unwrap();
        for (format, wanted) in [
            (
                "text",
                doe::OutputFormat::Text {
                    indent: "  ".into(),
                },
            ),
            ("json", doe::OutputFormat::Json),
            ("pretty", doe::OutputFormat::JsonPretty),
        ] {
            let r = f.local.handle(
                "POST",
                "export",
                &format!("format={format}"),
                request.as_bytes(),
            );
            assert_eq!(r.status, 200);
            let cli = doe::parse_file(
                Path::new(&f.path("input.bin")),
                schema_file.as_str(),
                &[],
                None,
                wanted,
            )
            .unwrap();
            assert_eq!(json(&r)["output"], cli, "{format}");
        }
        let r = f
            .local
            .handle("POST", "export", "format=xml", request.as_bytes());
        assert_eq!(json(&r)["ok"], false);
    }

    #[test]
    fn without_an_input_file_there_is_nothing_to_read() {
        let local = Local::new(&[], None).unwrap();
        assert_eq!(
            local.handle("GET", "input", "offset=0&len=1", b"").status,
            404
        );
        assert_eq!(local.handle("POST", "parse", "", b"{}").status, 404);
        assert!(Local::new(&[], Some(Path::new("/nonexistent/file"))).is_err());
        assert!(Local::new(&[], Some(Path::new("/"))).is_err());
    }

    #[test]
    fn query_values_are_decoded() {
        assert_eq!(
            query_param("a=1&path=%2Fx%20y+z", "path").as_deref(),
            Some("/x y z")
        );
        assert_eq!(query_param("a=1", "path"), None);
        assert_eq!(query_param("path=%zz", "path"), None);
        assert_eq!(query_param("path=%f", "path"), None);
    }
}
