#![doc = include_str!("../README.md")]
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

pub use error::{DoeError, Result};
pub use value::Value;
pub use config::{AvailableType, DuplicateTypeError, discover_types};

// ─────────────────────────────────────────────────────────────────────────────
// High-level API
// ─────────────────────────────────────────────────────────────────────────────

/// Selects the output format produced by [`parse_file`].
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
///   the default (`~/.doe/config/config.yaml`).  A missing file is silently
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
    use config::{resolve_include_paths, Config};
    use parser::{cursor::Cursor, engine::Engine};
    use schema::compiler::Compiler;

    // ── Config ────────────────────────────────────────────────────────────────

    let cfg_path = config_path
        .map(Path::to_owned)
        .or_else(Config::default_path)
        .unwrap_or_else(|| PathBuf::from(".doe_config.yaml"));

    let config = Config::load(&cfg_path)?;
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
        OutputFormat::Json            => render::json::render(&value),
        OutputFormat::JsonPretty      => render::json::render_pretty(&value),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared helpers (also used by main.rs)
// ─────────────────────────────────────────────────────────────────────────────

/// Returns `true` if `s` looks like a file-system path to a schema rather
/// than a bare type name.
///
/// The heuristic checks for a leading `.`, `/`, or `~` (relative, absolute,
/// or home-relative paths), or a `.yaml`/`.yml` extension.  Bare names such
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
    s.starts_with('.')
        || s.starts_with('/')
        || s.starts_with('~')
        || s.ends_with(".yaml")
        || s.ends_with(".yml")
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

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_schema_path ────────────────────────────────────────────────────────

    #[test]
    fn schema_path_dot_prefix()     { assert!(is_schema_path("./my.yaml")); }
    #[test]
    fn schema_path_absolute()       { assert!(is_schema_path("/usr/share/types/png.yaml")); }
    #[test]
    fn schema_path_tilde()          { assert!(is_schema_path("~/types/png.yaml")); }
    #[test]
    fn schema_path_yaml_extension() { assert!(is_schema_path("some_file.yaml")); }
    #[test]
    fn schema_path_yml_extension()  { assert!(is_schema_path("some_file.yml")); }
    #[test]
    fn bare_type_name_not_a_path()  { assert!(!is_schema_path("png")); }
    #[test]
    fn bare_type_no_extension()     { assert!(!is_schema_path("my_format")); }

    // ── load_schema_file ──────────────────────────────────────────────────────

    #[test]
    fn load_schema_file_ok() {
        use std::io::Write;
        use tempfile::NamedTempFile;
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "id: t\nseq:\n  - id: x\n    type: u8").unwrap();
        let s = load_schema_file(f.path()).unwrap();
        assert_eq!(s.id, "t");
        assert!(s.source_path.is_some());
    }

    #[test]
    fn load_schema_file_missing_is_error() {
        assert!(load_schema_file(Path::new("/nonexistent.yaml")).is_err());
    }

    #[test]
    fn load_schema_file_bad_yaml_is_error() {
        use std::io::Write;
        use tempfile::NamedTempFile;
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "unknown_key: true").unwrap();
        assert!(load_schema_file(f.path()).is_err());
    }

    // ── parse_file end-to-end ─────────────────────────────────────────────────

    #[test]
    fn parse_file_text_output() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut schema_f = NamedTempFile::new().unwrap();
        writeln!(schema_f, "id: simple\nseq:\n  - id: a\n    type: u8\n  - id: b\n    type: u16le").unwrap();

        let mut bin_f = NamedTempFile::new().unwrap();
        bin_f.write_all(&[0x07, 0x34, 0x12]).unwrap();

        let out = parse_file(
            bin_f.path(),
            schema_f.path().to_str().unwrap(),
            &[],
            Some(Path::new("/nonexistent")),
            OutputFormat::Text { indent: "  ".into() },
        ).unwrap();

        assert!(out.contains("simple\n"));
        assert!(out.contains("a  7"));
        assert!(out.contains("b  4660"));
    }

    #[test]
    fn parse_file_json_output() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut schema_f = NamedTempFile::new().unwrap();
        writeln!(schema_f, "id: pair\nseq:\n  - id: x\n    type: u8\n  - id: y\n    type: u8").unwrap();

        let mut bin_f = NamedTempFile::new().unwrap();
        bin_f.write_all(&[0x01, 0x02]).unwrap();

        let out = parse_file(
            bin_f.path(),
            schema_f.path().to_str().unwrap(),
            &[],
            Some(Path::new("/nonexistent")),
            OutputFormat::JsonPretty,
        ).unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["$type"], "pair");
        assert_eq!(parsed["x"], 1u64);
        assert_eq!(parsed["y"], 2u64);
    }

    #[test]
    fn parse_file_via_include_path() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("mytype.yaml"),
            "id: mytype\nseq:\n  - id: val\n    type: u32le\n",
        ).unwrap();

        let mut bin_f = tempfile::NamedTempFile::new().unwrap();
        use std::io::Write;
        bin_f.write_all(&0xdeadbeefu32.to_le_bytes()).unwrap();

        let out = parse_file(
            bin_f.path(),
            "mytype",
            &[dir.path().to_owned()],
            Some(Path::new("/nonexistent")),
            OutputFormat::Text { indent: "  ".into() },
        ).unwrap();

        assert!(out.contains("3735928559"));
    }

    #[test]
    fn parse_file_missing_binary_is_error() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut schema_f = NamedTempFile::new().unwrap();
        writeln!(schema_f, "id: t\nseq:\n  - id: x\n    type: u8").unwrap();

        let result = parse_file(
            Path::new("/nonexistent/file.bin"),
            schema_f.path().to_str().unwrap(),
            &[],
            Some(Path::new("/nonexistent")),
            OutputFormat::Text { indent: "  ".into() },
        );
        assert!(result.is_err());
    }
}
