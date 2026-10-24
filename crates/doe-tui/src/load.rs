//! Parse the file and keep what was read, also when the parse fails.

use std::path::{Path, PathBuf};

use doe::DoeError;
use doe::Value;
use doe::parser::cursor::Cursor;
use doe::parser::engine::Engine;
use doe::parser::engine::ParseOutcome;
use doe::parser::trace::TraceNode;
use doe::schema::compiler::Compiler;

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
}

/// The file the schema is in, when it is known: the one named on the command
/// line, or the one that declares the type with that name on the include paths.
#[must_use]
pub fn schema_file(options: &Options) -> Option<PathBuf> {
    if doe::is_schema_path(&options.type_or_schema) {
        return Some(PathBuf::from(&options.type_or_schema));
    }
    let types = doe::config::discover_types(&options.include_paths).ok()?;
    types
        .into_iter()
        .find(|t| t.id == options.type_or_schema)
        .map(|t| t.source)
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
    let failed = |error: DoeError, stage: Stage, type_name: &str, size: u64| {
        Box::new(LoadFailure {
            error,
            stage,
            type_name: type_name.to_owned(),
            size,
        })
    };
    let asked = options.type_or_schema.as_str();
    let mut compiler = Compiler::new(options.include_paths.clone());
    let type_name = if doe::is_schema_path(asked) {
        let schema = doe::load_schema_file(Path::new(asked))
            .map_err(|e| failed(e, Stage::Schema, asked, 0))?;
        let id = schema.id.clone();
        compiler
            .process_schema(&schema)
            .map_err(|e| failed(e, Stage::Schema, &id, 0))?;
        id
    } else {
        compiler
            .load(asked, None)
            .map_err(|e| failed(e, Stage::Schema, asked, 0))?;
        asked.to_owned()
    };

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
    Ok(Loaded {
        type_name,
        size,
        value,
        trace,
        error,
    })
}
