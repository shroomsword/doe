#![doc = include_str!("../../../README.md")]
//!
//! ---
//!
//! ## API cross-references
//!
//! Key types and functions mentioned in the documentation above:
//!
//! - [`parse_file`] — the primary high-level entry point.
//! - [`OutputFormat`] — selects text, compact JSON, or pretty JSON output.
//! - [`Value`] — the parsed value tree produced by the engine.
//! - [`DoeError`] / [`Result`] — the unified error and result types.
//! - [`config::Config`] — configuration file structure.
//! - [`schema::compiler::Compiler`] — stateful schema compiler.
//! - [`schema::ir::TypeRegistry`] — the compiled type registry consumed by the parser.
//! - [`parser::engine::Engine`] — the binary parse engine.
//! - [`parser::cursor::Cursor`] — zero-copy byte cursor.
//! - [`render::text::render_with_indent`] — indented text renderer.
//! - [`render::json::render_pretty`] — pretty-printed JSON renderer.

pub mod config;
pub mod error;
pub mod parser;
pub mod render;
pub mod schema;
pub mod value;

use std::path::{Path, PathBuf};

pub use config::{AvailableType, DuplicateTypeError, discover_types};
pub use error::{DoeError, Result};
pub use value::Value;

// ─────────────────────────────────────────────────────────────────────────────
// High-level API
// ─────────────────────────────────────────────────────────────────────────────

/// Selects the output format produced by [`parse_file`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputFormat {
    /// Indented human-readable text.
    ///
    /// Each level of nesting is indented by one copy of `indent`.  Two spaces
    /// (`"  "`) is the conventional default.
    Text {
        /// The string prepended once per nesting level.
        indent: String,
    },
    /// Compact single-line JSON.
    Json,
    /// Pretty-printed JSON with four-space indentation.
    JsonPretty,
}

/// Parse `binary_file` according to `type_or_schema` and return the rendered
/// output as a `String`.
///
/// This is the primary entry point for the library.  It composes config
/// loading, schema compilation, binary parsing, and rendering into a single
/// convenient call.
///
/// # Arguments
///
/// * `binary_file` — path to the binary file to parse.
/// * `type_or_schema` — either a **bare type name** (e.g. `"png"`) resolved
///   through `include_paths` and the config file, or a **path to a YAML
///   schema file** (detected by a leading `.`, `/`, `~`, or a `.yaml`/`.yml`
///   extension).
/// * `include_paths` — additional directories to prepend to the schema search
///   path, in priority order.  These take precedence over any paths in the
///   config file.
/// * `config_path` — override the config file location.  Pass `None` to use
///   the default (`$XDG_CONFIG_HOME/doe/config.toml`).  A missing file is silently
///   treated as an empty config.
/// * `format` — the desired output representation.
///
/// # Errors
///
/// Returns [`DoeError`] if any of the following occur:
/// - The config file exists but cannot be read or parsed.
/// - The schema cannot be found on any include path.
/// - The schema YAML is invalid.
/// - An import cycle is detected.
/// - A field references an unknown type or enum.
/// - The binary file cannot be read.
/// - The binary data does not match the schema (e.g. buffer overrun, bad
///   magic bytes).
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use doe::{parse_file, OutputFormat};
///
/// let text = parse_file(
///     Path::new("sample.bin"),
///     "./sample.yaml",
///     &[],
///     None,
///     OutputFormat::Text { indent: "  ".into() },
/// ).unwrap();
///
/// print!("{}", text);
/// ```
pub fn parse_file(
    binary_file: &Path,
    type_or_schema: &str,
    include_paths: &[PathBuf],
    config_path: Option<&Path>,
    format: OutputFormat,
) -> Result<String> {
    use config::{Config, resolve_include_paths};
    use parser::{cursor::Cursor, engine::Engine};
    use schema::compiler::Compiler;

    // ── Config ────────────────────────────────────────────────────────────────

    let config = Config::load_or_default(config_path)?;
    let all_include_paths = resolve_include_paths(include_paths, &config);

    // ── Schema ────────────────────────────────────────────────────────────────

    let mut compiler = Compiler::new(all_include_paths);

    let type_name = if is_schema_path(type_or_schema) {
        let schema = load_schema_file(Path::new(type_or_schema))?;
        let id = schema.id.clone();
        compiler.process_schema(&schema)?;
        id
    } else {
        compiler.load(type_or_schema, None)?;
        type_or_schema.to_owned()
    };

    // ── Parse ─────────────────────────────────────────────────────────────────

    let binary_data = std::fs::read(binary_file).map_err(|e| DoeError::Io {
        path: binary_file.to_owned(),
        source: e,
    })?;

    let engine = Engine::new(&compiler.registry);
    let mut cursor = Cursor::new(&binary_data);
    let value = engine.parse_type(&type_name, &mut cursor)?;

    // ── Render ────────────────────────────────────────────────────────────────

    Ok(match format {
        OutputFormat::Text { indent } => render::text::render_with_indent(&value, &indent),
        OutputFormat::Json => render::json::render(&value),
        OutputFormat::JsonPretty => render::json::render_pretty(&value),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared helpers (also used by main.rs)
// ─────────────────────────────────────────────────────────────────────────────

/// Returns `true` if `s` looks like a file-system path to a schema rather
/// than a bare type name.
///
/// The heuristic checks for a leading `.`, `/`, `\`, or `~` (relative,
/// absolute, or home-relative paths), a platform-absolute path such as
/// `C:\types\elf` on Windows, or a `.yaml`/`.yml` extension.  Bare names such
/// as `"png"` or `"my_format"` return `false` and are resolved through the
/// configured include paths instead.
///
/// # Examples
///
/// ```
/// use doe::is_schema_path;
///
/// assert!(is_schema_path("./my_format.yaml"));
/// assert!(is_schema_path("/usr/share/doe/types/elf.yaml"));
/// assert!(is_schema_path("~/types/png.yaml"));
/// assert!(is_schema_path("custom.yml"));
///
/// assert!(!is_schema_path("png"));
/// assert!(!is_schema_path("my_format"));
/// ```
pub fn is_schema_path(s: &str) -> bool {
    s.starts_with(['.', '/', '\\', '~']) || Path::new(s).is_absolute() || has_schema_extension(s)
}

/// Returns `true` if `s` ends in `.yaml` or `.yml`, ignoring ASCII case.
pub(crate) fn has_schema_extension(s: &str) -> bool {
    Path::new(s)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml"))
}

/// Load and deserialise a YAML schema file from `path`.
///
/// The resolved path is injected into [`schema::raw::RawSchema::source_path`]
/// so that relative imports within the schema are resolved correctly.
///
/// # Errors
///
/// Returns [`DoeError::Io`] if the file cannot be read, or
/// [`DoeError::Yaml`] if the content is not valid YAML or does not conform
/// to the schema format.
pub fn load_schema_file(path: &Path) -> Result<schema::raw::RawSchema> {
    let text = std::fs::read_to_string(path).map_err(|e| DoeError::Io {
        path: path.to_owned(),
        source: e,
    })?;
    let mut s: schema::raw::RawSchema =
        serde_yaml::from_str(&text).map_err(|e| DoeError::Yaml {
            path: path.to_owned(),
            source: e,
        })?;
    s.source_path = Some(path.to_owned());
    Ok(s)
}
