//! Parse the file and keep what was read, also when the parse fails.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use doe::DoeError;
use doe::Value;
use doe::parser::cursor::Cursor;
use doe::parser::engine::Engine;
use doe::parser::engine::ParseOutcome;
use doe::parser::trace::TraceNode;
use doe::schema::compiler::Compiler;
use doe::schema::source::{FsSource, SchemaSource};

use crate::Options;

/// A parsed file.
#[derive(Debug)]
pub struct Loaded {
    /// The type that was parsed.
    pub type_name: String,
    /// The size of the file in bytes.
    pub size: u64,
    /// What was read: the value, or as much of it as there was when the parse failed.
    pub value: Option<Value>,
    /// Where each part of it came from.
    pub trace: TraceNode,
    /// Why the parse stopped, if it did.
    pub error: Option<DoeError>,
    /// The schema files that were read: the schema and what it imports.
    pub schema_files: Vec<PathBuf>,
}

/// Where loading stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The schema could not be found, read or compiled.
    Schema,
    /// The file could not be opened or read.
    File,
}

/// Why nothing could be parsed.
#[derive(Debug)]
pub struct LoadFailure {
    /// What went wrong.
    pub error: DoeError,
    /// Where.
    pub stage: Stage,
    /// The type, as it was asked for.
    pub type_name: String,
    /// The size of the file, if it could be read.
    pub size: u64,
    /// The schema files that were read before it stopped.
    pub schema_files: Vec<PathBuf>,
}

/// Reads the filesystem, and remembers which files it read.
#[derive(Debug)]
struct Recording {
    seen: Arc<Mutex<Vec<PathBuf>>>,
}

impl SchemaSource for Recording {
    fn exists(&self, path: &Path) -> bool {
        FsSource.exists(path)
    }

    fn read(&self, path: &Path) -> doe::error::Result<String> {
        if let Ok(mut seen) = self.seen.lock()
            && !seen.iter().any(|p| p == path)
        {
            seen.push(path.to_owned());
        }
        FsSource.read(path)
    }
}

impl LoadFailure {
    /// What is said when the thread that parses ended without an answer (it
    /// panicked, or could not be started).
    #[must_use]
    pub fn stopped(options: &Options) -> LoadFailure {
        LoadFailure {
            error: DoeError::Schema(
                "the thread that parses stopped without an answer (this is a bug in doe)"
                    .to_owned(),
            ),
            stage: Stage::Schema,
            type_name: options.type_or_schema.clone(),
            size: 0,
            schema_files: Vec::new(),
        }
    }
}

/// The file the schema is in, when it is known: the one named on the command
/// line, or the one that declares the type with that name on the include paths.
#[must_use]
pub fn schema_file(options: &Options) -> Option<PathBuf> {
    let (schema, _) = doe::split_type_arg(&options.type_or_schema);
    if doe::is_schema_path(schema) {
        return Some(PathBuf::from(schema));
    }
    let types = doe::config::discover_types(&options.include_paths).ok()?;
    types.into_iter().find(|t| t.id == schema).map(|t| t.source)
}

/// Compile the schema and parse the file with it.
///
/// # Errors
///
/// Fails when the schema cannot be found or compiled, or the file cannot be
/// read. A parse that fails is not an error here: it is in the outcome.
pub fn load(options: &Options) -> Result<Loaded, DoeError> {
    try_load(options).map_err(|failure| failure.error)
}

/// Like [`load`], saying where it stopped.
///
/// # Errors
///
/// The reason nothing could be parsed, and at what stage.
pub fn try_load(options: &Options) -> Result<Loaded, Box<LoadFailure>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let files =
        |seen: &Arc<Mutex<Vec<PathBuf>>>| seen.lock().map(|s| s.clone()).unwrap_or_default();
    let failed = |error: DoeError, stage: Stage, type_name: &str, size: u64| {
        Box::new(LoadFailure {
            error,
            stage,
            type_name: type_name.to_owned(),
            size,
            schema_files: files(&seen),
        })
    };
    let asked = options.type_or_schema.as_str();
    let mut compiler = Compiler::with_source(
        options.include_paths.clone(),
        Box::new(Recording {
            seen: Arc::clone(&seen),
        }),
    );
    // `schema.yaml::type` names a type inside a schema
    let (schema_arg, wanted) = doe::split_type_arg(asked);
    let id = if doe::is_schema_path(schema_arg) {
        let schema = compiler
            .read_schema(Path::new(schema_arg))
            .map_err(|e| failed(e, Stage::Schema, asked, 0))?;
        let id = schema.id.clone();
        compiler
            .process_schema(&schema)
            .map_err(|e| failed(e, Stage::Schema, &id, 0))?;
        id
    } else {
        compiler
            .load(schema_arg, None)
            .map_err(|e| failed(e, Stage::Schema, asked, 0))?;
        schema_arg.to_owned()
    };
    let type_name = doe::type_in_schema(&compiler.registry, &id, wanted)
        .map_err(|e| failed(e, Stage::Schema, asked, 0))?;

    let io_error = |source| DoeError::Io {
        path: options.file.clone(),
        source,
    };
    let file = std::fs::File::open(&options.file)
        .map_err(|e| failed(io_error(e), Stage::File, &type_name, 0))?;
    let size = file
        .metadata()
        .map_err(|e| failed(io_error(e), Stage::File, &type_name, 0))?
        .len();
    let engine = Engine::new(&compiler.registry);
    // A regular file is read as the schema asks for it, so its size does not
    // decide how much memory the viewer needs.
    let mut cursor = Cursor::from_file(file, &options.file)
        .map_err(|e| failed(e, Stage::File, &type_name, size))?;
    let outcome = engine
        .parse_type_partial(&type_name, &mut cursor)
        .map_err(|e| failed(e, Stage::Schema, &type_name, size))?;
    let ParseOutcome {
        value,
        trace,
        error,
        ..
    } = outcome;
    let schema_files = files(&seen);
    Ok(Loaded {
        type_name,
        size,
        value,
        trace,
        error,
        schema_files,
    })
}
