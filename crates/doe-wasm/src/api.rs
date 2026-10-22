//! The calls of the web IDE, in plain Rust: JSON in, JSON out.
//!
//! Everything the page needs is one of two calls, [`check`] (compile a schema
//! and say what is wrong with it, with the positions in its text) and [`parse`]
//! (check, then parse bytes with it: the value, where each part came from, and
//! the error and what was read before it if the parse failed). The shape of the
//! JSON is described in `API.md` and tested.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use doe::DoeError;
use doe::Value;
use doe::parser::cursor::Cursor;
use doe::parser::engine::Engine;
use doe::parser::trace::TraceNode;
use doe::schema::compiler::Compiler;
use doe::schema::locate::{Position, locate_error};
use doe::schema::source::MemorySource;
use serde::{Deserialize, Serialize};

/// The most bytes [`parse`] takes: the memory of a WebAssembly module is 4 GiB
/// at most, and the page has the file and a copy of it in it.
pub const MAX_INPUT_BYTES: usize = 256 * 1024 * 1024;

/// The longest summary of a value in the trace; longer ones are cut.
const MAX_SUMMARY_CHARS: usize = 200;

fn default_path() -> String {
    "main.ksy".to_owned()
}

/// What the page asks for.
#[derive(Debug, Deserialize)]
pub struct Request {
    /// The text of the schema.
    pub schema: String,
    /// The other files (imports, by path) that the schema can use.
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    /// Where the schema is, for imports that are next to it. Default `main.ksy`.
    #[serde(default = "default_path")]
    pub path: String,
    /// The places that imports are looked for besides next to the schema.
    #[serde(default)]
    pub include_paths: Vec<String>,
    /// The type to parse with; the schema's own by default.
    #[serde(default, rename = "type")]
    pub type_name: Option<String>,
}

/// A place in the text of the schema.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PositionOut {
    /// The line, from 1.
    pub line: u32,
    /// The column, from 1.
    pub column: u32,
    /// The byte offset in the text.
    pub offset: Option<usize>,
    /// The length in bytes of what is meant.
    pub len: Option<usize>,
}

impl From<Position> for PositionOut {
    fn from(position: Position) -> Self {
        PositionOut {
            line: position.line,
            column: position.column,
            offset: position.offset,
            len: position.len,
        }
    }
}

/// Something wrong, for the page to show.
#[derive(Debug, Serialize)]
pub struct Diagnostic {
    /// Always `error` for now.
    pub severity: &'static str,
    /// `request` (the call was malformed), `yaml` (the schema is not valid
    /// YAML), `schema` (it does not compile), `input` (the bytes cannot be
    /// parsed at all) or `parse` (the parse failed).
    pub kind: &'static str,
    /// What is wrong.
    pub message: String,
    /// Where in the text of the schema, when that is known.
    pub position: Option<PositionOut>,
}

impl Diagnostic {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Diagnostic {
            severity: "error",
            kind,
            message: message.into(),
            position: None,
        }
    }
}

/// The answer to [`check`].
#[derive(Debug, Serialize)]
pub struct CheckResponse {
    /// Whether the schema compiled.
    pub ok: bool,
    /// The id of the schema.
    pub id: Option<String>,
    /// What is wrong with it.
    pub diagnostics: Vec<Diagnostic>,
}

/// A part of the input and what was read from it.
#[derive(Debug, Serialize)]
pub struct TraceOut {
    /// The name of the field, `[i]` for an element, the type for the root.
    pub name: String,
    /// The user type read here.
    #[serde(rename = "type")]
    pub type_name: Option<String>,
    /// The first bit, from the start of the input.
    pub start: Option<u64>,
    /// The bit after the last one.
    pub end: Option<u64>,
    /// Whether it is an instance.
    pub instance: bool,
    /// Whether the parse failed here.
    pub failed: bool,
    /// Whether the offsets are in the bytes a `process:` produced.
    pub processed: bool,
    /// The value as text, for one that is not a type or a list; cut when long.
    pub summary: Option<String>,
    /// The parts inside.
    pub children: Vec<TraceOut>,
}

/// The answer to [`parse`].
#[derive(Debug, Serialize)]
pub struct ParseResponse {
    /// Whether the schema compiled and the parse worked.
    pub ok: bool,
    /// The id of the schema.
    pub id: Option<String>,
    /// What is wrong with the schema.
    pub diagnostics: Vec<Diagnostic>,
    /// The value, as `doe --json` prints it; what was read before the failure
    /// when the parse failed.
    pub value: Option<serde_json::Value>,
    /// Where each part of the value came from.
    pub trace: Option<TraceOut>,
    /// Why the parse failed.
    pub error: Option<Diagnostic>,
}

/// A schema and the compiler that has it.
struct Compiled {
    id: String,
    compiler: Compiler,
}

fn diagnostic_of(error: &DoeError, text: &str) -> Diagnostic {
    let kind = if matches!(error, DoeError::Yaml { .. }) {
        "yaml"
    } else {
        "schema"
    };
    let mut diagnostic = Diagnostic::new(kind, error.to_string());
    diagnostic.position = locate_error(text, error).map(PositionOut::from);
    diagnostic
}

fn compile(request: &Request) -> Result<Compiled, Vec<Diagnostic>> {
    let mut files = MemorySource::new();
    for (path, text) in &request.files {
        files.insert(path, text.as_str());
    }
    files.insert(&request.path, request.schema.as_str());
    let include = request.include_paths.iter().map(PathBuf::from).collect();
    let mut compiler = Compiler::with_source(include, Box::new(files));
    let one = |error: DoeError| vec![diagnostic_of(&error, &request.schema)];
    let schema = compiler
        .read_schema(Path::new(&request.path))
        .map_err(one)?;
    compiler.process_schema(&schema).map_err(one)?;
    Ok(Compiled {
        id: schema.id,
        compiler,
    })
}

fn request_from(json: &str) -> Result<Request, Diagnostic> {
    serde_json::from_str(json)
        .map_err(|e| Diagnostic::new("request", format!("the request is not valid: {e}")))
}

/// Compile the schema of a request (a JSON string) and say what is wrong.
pub fn check(request: &str) -> CheckResponse {
    let request = match request_from(request) {
        Ok(request) => request,
        Err(diagnostic) => {
            return CheckResponse {
                ok: false,
                id: None,
                diagnostics: vec![diagnostic],
            };
        }
    };
    match compile(&request) {
        Ok(compiled) => CheckResponse {
            ok: true,
            id: Some(compiled.id),
            diagnostics: Vec::new(),
        },
        Err(diagnostics) => CheckResponse {
            ok: false,
            id: None,
            diagnostics,
        },
    }
}

/// Compile the schema of a request and parse `bytes` with it.
pub fn parse(request: &str, bytes: &[u8]) -> ParseResponse {
    let failed = |diagnostics: Vec<Diagnostic>, id: Option<String>| ParseResponse {
        ok: false,
        id,
        diagnostics,
        value: None,
        trace: None,
        error: None,
    };
    let request = match request_from(request) {
        Ok(request) => request,
        Err(diagnostic) => return failed(vec![diagnostic], None),
    };
    let compiled = match compile(&request) {
        Ok(compiled) => compiled,
        Err(diagnostics) => return failed(diagnostics, None),
    };
    let id = Some(compiled.id.clone());
    if bytes.len() > MAX_INPUT_BYTES {
        let message = format!(
            "the input is {} bytes; the most that can be parsed here is {MAX_INPUT_BYTES}",
            bytes.len()
        );
        return ParseResponse {
            error: Some(Diagnostic::new("input", message)),
            ..failed(Vec::new(), id)
        };
    }
    let type_name = request.type_name.as_deref().unwrap_or(&compiled.id);
    let engine = Engine::new(&compiled.compiler.registry);
    let outcome = match engine.parse_type_partial(type_name, &mut Cursor::new(bytes)) {
        Ok(outcome) => outcome,
        Err(error) => {
            return ParseResponse {
                error: Some(Diagnostic::new("input", error.to_string())),
                ..failed(Vec::new(), id)
            };
        }
    };
    let value_json = outcome
        .value
        .as_ref()
        .and_then(|value| serde_json::from_str(&doe::render::json::render(value)).ok());
    let mut trace = trace_out(&outcome.trace);
    annotate(&mut trace, outcome.value.as_ref());
    ParseResponse {
        ok: outcome.error.is_none(),
        id,
        diagnostics: Vec::new(),
        value: value_json,
        trace: Some(trace),
        error: outcome
            .error
            .map(|error| Diagnostic::new("parse", error.to_string())),
    }
}

fn trace_out(node: &TraceNode) -> TraceOut {
    TraceOut {
        name: node.name.clone(),
        type_name: node.type_name.clone(),
        start: node.span.map(|span| span.start),
        end: node.span.map(|span| span.end),
        instance: node.is_instance,
        failed: node.failed,
        processed: node.processed,
        summary: None,
        children: node.children.iter().map(trace_out).collect(),
    }
}

/// Put the text of each value on the node it belongs to: a node's children are
/// the fields of its struct (by name) or the elements of its list (`[i]`).
fn annotate(node: &mut TraceOut, value: Option<&Value>) {
    let value = match value {
        Some(Value::Instance { value, .. }) => Some(value.as_ref()),
        other => other,
    };
    node.summary = value.and_then(summary);
    for child in &mut node.children {
        let inner = match value {
            Some(Value::Struct { fields, .. }) => fields
                .iter()
                .find(|(name, _)| *name == child.name)
                .map(|(_, field)| field),
            Some(Value::Array(items)) => child
                .name
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
                .and_then(|index| index.parse::<usize>().ok())
                .and_then(|index| items.get(index)),
            _ => None,
        };
        annotate(child, inner);
    }
}

fn summary(value: &Value) -> Option<String> {
    match value {
        Value::Struct { .. } | Value::Array(_) | Value::Absent => None,
        Value::Instance { value, .. } => summary(value),
        other => {
            let text = other.to_string();
            Some(if text.chars().count() > MAX_SUMMARY_CHARS {
                let cut: String = text.chars().take(MAX_SUMMARY_CHARS).collect();
                format!("{cut}…")
            } else {
                text
            })
        }
    }
}
