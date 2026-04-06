//! Error types for the `doe` library.
//!
//! All fallible operations in this crate return [`Result<T>`], which is an
//! alias for `std::result::Result<T, DoeError>`.  [`DoeError`] covers every
//! failure mode from config loading through binary parsing.

use std::path::PathBuf;
use thiserror::Error;

/// The unified error type for all `doe` operations.
///
/// Errors are grouped by the phase in which they occur:
///
/// - **I/O** — file system failures when reading config, schema, or binary files.
/// - **YAML** — malformed or structurally invalid schema files.
/// - **Schema resolution** — missing schemas, import cycles, duplicate names.
/// - **Expression** — syntax or evaluation errors in field-size / repeat /
///   conditional expressions.
/// - **IR compilation** — references to unknown types or enums, invalid field
///   definitions caught at compile time.
///
/// All variants implement [`std::error::Error`] and carry enough context
/// (file path, field name, expression string, etc.) to produce actionable
/// diagnostic messages.
#[derive(Debug, Error)]
pub enum DoeError {
    // ── I/O ──────────────────────────────────────────────────────────────────

    /// A file could not be opened or read.
    #[error("could not read file {path}: {source}")]
    Io {
        /// The path that was being accessed.
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    // ── YAML ─────────────────────────────────────────────────────────────────

    /// A YAML file exists but could not be parsed or does not match the
    /// expected schema structure.
    #[error("YAML parse error in {path}: {source}")]
    Yaml {
        /// The schema file that triggered the error.
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    // ── Schema resolution ────────────────────────────────────────────────────

    /// A bare type name (e.g. `"png"`) could not be found on any include path.
    #[error("schema not found: '{name}' (searched: {searched:?})")]
    SchemaNotFound {
        /// The type name that was being looked up.
        name: String,
        /// Every directory that was searched.
        searched: Vec<PathBuf>,
    },

    /// A schema's `imports:` list creates a dependency cycle.
    ///
    /// The `cycle` field contains the chain of schema ids leading back to the
    /// repeated id, e.g. `["a", "b", "c", "a"]`.
    #[error("import cycle detected: {cycle:?}")]
    ImportCycle {
        /// The import chain, with the repeated id at both ends.
        cycle: Vec<String>,
    },

    /// The same type name is defined in two separate schema files.
    #[error("duplicate type name '{name}' defined in {first} and {second}")]
    DuplicateType {
        /// The conflicting type name.
        name: String,
        /// The schema that defined the name first.
        first: String,
        /// The schema that attempted to redefine it.
        second: String,
    },

    // ── Expression ───────────────────────────────────────────────────────────

    /// The expression string in a `size:`, `repeat-expr:`, or `if:` field
    /// could not be parsed.
    #[error("expression parse error in '{expr}': {message}")]
    ExprParse {
        /// The raw expression string from the schema.
        expr: String,
        /// A human-readable description of the syntax error.
        message: String,
    },

    /// A syntactically valid expression failed at evaluation time (e.g.
    /// division by zero, or a field reference that resolved to no value).
    #[error("expression eval error in '{expr}': {message}")]
    ExprEval {
        /// The raw expression string from the schema.
        expr: String,
        /// A description of the evaluation failure.
        message: String,
    },

    // ── IR compilation ───────────────────────────────────────────────────────

    /// A field's `type:` value does not match any primitive name or known
    /// user-defined type.
    #[error("unknown type '{type_name}' referenced in {context}")]
    UnknownType {
        /// The unresolved type name as written in the schema.
        type_name: String,
        /// The fully-qualified field path where the reference appeared.
        context: String,
    },

    /// A field definition is invalid (e.g. `bytes` without `size:`, `bits`
    /// without `bit_size:`, a `size:` expression that forward-references a
    /// later field).
    #[error("field '{field}' in {context}: {message}")]
    FieldError {
        /// The field id.
        field: String,
        /// The fully-qualified type name containing the field.
        context: String,
        /// A description of what is wrong with the field definition.
        message: String,
    },

    /// A field's `enum:` value names an enum that is not defined in any
    /// enclosing scope.
    #[error("unknown enum '{enum_name}' referenced in {context}")]
    UnknownEnum {
        /// The enum name as written in the field's `enum:` attribute.
        enum_name: String,
        /// The fully-qualified field path where the reference appeared.
        context: String,
    },

    /// A catch-all for internal errors that do not fit a more specific
    /// variant (e.g. buffer overruns during binary parsing).
    #[error("{0}")]
    Schema(String),
}

/// Alias for `std::result::Result<T, DoeError>`.
///
/// Every fallible function in the `doe` library returns this type.
pub type Result<T> = std::result::Result<T, DoeError>;
