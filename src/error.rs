use std::path::PathBuf;
use thiserror::Error;

/// Errors produced during schema loading, compilation, or binary parsing.
#[derive(Debug, Error)]
pub enum DoeError {
    // ── I/O ──────────────────────────────────────────────────────────────────
    #[error("could not read file {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    // ── YAML ─────────────────────────────────────────────────────────────────
    #[error("YAML parse error in {path}: {source}")]
    Yaml {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    // ── Schema resolution ────────────────────────────────────────────────────
    #[error("schema not found: '{name}' (searched: {searched:?})")]
    SchemaNotFound { name: String, searched: Vec<PathBuf> },

    #[error("import cycle detected: {cycle:?}")]
    ImportCycle { cycle: Vec<String> },

    #[allow(dead_code)]
    #[error("duplicate type name '{name}' defined in {first} and {second}")]
    DuplicateType {
        name: String,
        first: String,
        second: String,
    },

    // ── Expression ───────────────────────────────────────────────────────────
    #[error("expression parse error in '{expr}': {message}")]
    ExprParse { expr: String, message: String },

    #[error("expression eval error in '{expr}': {message}")]
    ExprEval { expr: String, message: String },

    // ── IR compilation ───────────────────────────────────────────────────────
    #[error("unknown type '{type_name}' referenced in {context}")]
    UnknownType { type_name: String, context: String },

    #[error("field '{field}' in {context}: {message}")]
    FieldError {
        field: String,
        context: String,
        message: String,
    },

    #[error("unknown enum '{enum_name}' referenced in {context}")]
    UnknownEnum { enum_name: String, context: String },

    #[error("{0}")]
    Schema(String),
}

pub type Result<T> = std::result::Result<T, DoeError>;
