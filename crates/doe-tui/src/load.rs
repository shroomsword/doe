//! Parse the file and keep what was read, also when the parse fails.

use std::path::Path;

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

/// Compile the schema and parse the file with it.
///
/// # Errors
///
/// Fails when the schema cannot be found or compiled, or the file cannot be
/// read. A parse that fails is not an error here: it is in the outcome.
pub fn load(options: &Options) -> Result<Loaded, DoeError> {
    let mut compiler = Compiler::new(options.include_paths.clone());
    let type_name = if doe::is_schema_path(&options.type_or_schema) {
        let schema = doe::load_schema_file(Path::new(&options.type_or_schema))?;
        let id = schema.id.clone();
        compiler.process_schema(&schema)?;
        id
    } else {
        compiler.load(&options.type_or_schema, None)?;
        options.type_or_schema.clone()
    };

    let io_error = |source| DoeError::Io {
        path: options.file.clone(),
        source,
    };
    let file = std::fs::File::open(&options.file).map_err(io_error)?;
    let size = file.metadata().map_err(io_error)?.len();
    let engine = Engine::new(&compiler.registry);
    // A regular file is read as the schema asks for it, so its size does not
    // decide how much memory the viewer needs.
    let mut cursor = Cursor::from_file(file, &options.file)?;
    let outcome = engine.parse_type_partial(&type_name, &mut cursor)?;
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
