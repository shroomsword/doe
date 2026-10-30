#![cfg_attr(feature = "fs", doc = include_str!("../../../README.md"))]
#![cfg_attr(
    not(feature = "fs"),
    doc = "Binary file parser driven by YAML type definitions, built without the `fs` feature: \
           no filesystem and no configuration file. Schemas come from a \
           [`schema::source::MemorySource`], the input from a byte slice, and the parse is \
           run with [`parser::engine::Engine`]. The full documentation is in the README and \
           in the docs of the default build."
)]
//!
//! ---
//!
//! ## API cross-references
//!
//! Key types and functions mentioned in the documentation above:
//!
//! - `parse_file` — the primary high-level entry point.
//! - [`OutputFormat`] — selects text, compact JSON, or pretty JSON output.
//! - [`Value`] — the parsed value tree produced by the engine.
//! - [`DoeError`] / [`Result`] — the unified error and result types.
//! - `config::Config` — configuration file structure.
//! - [`schema::compiler::Compiler`] — stateful schema compiler.
//! - [`schema::ir::TypeRegistry`] — the compiled type registry consumed by the parser.
//! - [`parser::engine::Engine`] — the binary parse engine.
//! - [`parser::cursor::Cursor`] — zero-copy byte cursor.
//! - [`render::text::render_with_indent`] — indented text renderer.
//! - [`render::json::render_pretty`] — pretty-printed JSON renderer.

pub mod api;
#[cfg(feature = "fs")]
pub mod config;
pub mod detect;
pub mod error;
pub mod inspect;
pub mod parser;
pub mod radix;
pub mod render;
pub mod schema;
pub mod value;

use std::path::Path;
#[cfg(feature = "fs")]
use std::path::PathBuf;

#[cfg(feature = "fs")]
pub use config::{AvailableType, DuplicateTypeError, discover_types};
pub use error::{DoeError, Result};
pub use value::Value;

// ─────────────────────────────────────────────────────────────────────────────
// High-level API
// ─────────────────────────────────────────────────────────────────────────────

/// Selects the output format produced by `parse_file`.
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
#[cfg(feature = "fs")]
pub fn parse_file(
    binary_file: &Path,
    type_or_schema: &str,
    include_paths: &[PathBuf],
    config_path: Option<&Path>,
    format: OutputFormat,
) -> Result<String> {
    parse_file_with(
        binary_file,
        type_or_schema,
        include_paths,
        config_path,
        format,
        &ParseOptions::default(),
    )
}

/// What `parse_file_with` does beyond `parse_file`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseOptions {
    /// Whether the `instances` of a type are part of the output (the
    /// default). Those that its `seq` fields use are computed either way.
    pub instances: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        ParseOptions { instances: true }
    }
}

/// Like `parse_file`, with options.
///
/// # Errors
///
/// Returns the same errors as `parse_file`.
#[cfg(feature = "fs")]
pub fn parse_file_with(
    binary_file: &Path,
    type_or_schema: &str,
    include_paths: &[PathBuf],
    config_path: Option<&Path>,
    format: OutputFormat,
    options: &ParseOptions,
) -> Result<String> {
    use config::{Config, resolve_include_paths, with_default_types};
    use parser::{cursor::Cursor, engine::Engine};
    use schema::compiler::Compiler;

    // ── Config ────────────────────────────────────────────────────────────────

    let config = Config::load_or_default(config_path)?;
    let all_include_paths =
        with_default_types(resolve_include_paths(include_paths, &config), &config);

    // ── Schema ────────────────────────────────────────────────────────────────

    let mut compiler = Compiler::new(all_include_paths);

    let (schema_arg, wanted) = split_type_arg(type_or_schema);
    let id = if is_schema_path(schema_arg) {
        let schema = load_schema_file(Path::new(schema_arg))?;
        let id = schema.id.clone();
        compiler.process_schema(&schema)?;
        id
    } else {
        compiler.load(schema_arg, None)?;
        schema_arg.to_owned()
    };
    let type_name = type_in_schema(&compiler.registry, &id, wanted)?;

    // ── Parse ─────────────────────────────────────────────────────────────────

    let io_error = |source| DoeError::Io {
        path: binary_file.to_owned(),
        source,
    };
    let mut file = std::fs::File::open(binary_file).map_err(io_error)?;
    let engine = Engine::new(&compiler.registry).with_instances(options.instances);
    let value = if file.metadata().map_err(io_error)?.is_file() {
        // A regular file is read as the schema asks for it, so its size does
        // not decide how much memory the parse needs.
        let mut cursor = Cursor::from_file(file, binary_file)?;
        engine.parse_type(&type_name, &mut cursor)?
    } else {
        // A pipe or a device cannot seek back, so it is read in full.
        log::debug!(
            "{} is not a regular file; reading it into memory",
            binary_file.display()
        );
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut data).map_err(io_error)?;
        engine.parse_type(&type_name, &mut Cursor::new(&data))?
    };

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
/// as `"png"` or `"custom_format"` return `false` and are resolved through the
/// configured include paths instead.
///
/// # Examples
///
/// ```
/// use doe::is_schema_path;
///
/// assert!(is_schema_path("./custom_format.yaml"));
/// assert!(is_schema_path("/usr/share/doe/types/elf.yaml"));
/// assert!(is_schema_path("~/types/png.yaml"));
/// assert!(is_schema_path("custom.yml"));
///
/// assert!(!is_schema_path("png"));
/// assert!(!is_schema_path("custom_format"));
/// ```
pub fn is_schema_path(s: &str) -> bool {
    s.starts_with(['.', '/', '\\', '~']) || Path::new(s).is_absolute() || has_schema_extension(s)
}

/// Split `SCHEMA::TYPE`, the way to name a type inside a schema on the command
/// line (`custom_format.yaml::chunk`, or `png::chunk` for a type of the library):
/// the schema (a path or a name) and the type, if there is one.
///
/// ```
/// use doe::split_type_arg;
///
/// assert_eq!(split_type_arg("custom_format.yaml::chunk"), ("custom_format.yaml", Some("chunk")));
/// assert_eq!(split_type_arg("png::chunk::data"), ("png", Some("chunk::data")));
/// assert_eq!(split_type_arg("png"), ("png", None));
/// assert_eq!(split_type_arg("C:\\types\\elf.yaml"), ("C:\\types\\elf.yaml", None));
/// ```
#[must_use]
pub fn split_type_arg(arg: &str) -> (&str, Option<&str>) {
    match arg.split_once("::") {
        Some((schema, rest)) if !schema.is_empty() && !rest.is_empty() => (schema, Some(rest)),
        _ => (arg, None),
    }
}

/// The name to parse with when `SCHEMA::TYPE` was given: `TYPE` in the schema
/// whose id is `id` (`id::TYPE`), or else a type of another schema that was
/// imported, named in full (`other::type`).
///
/// # Errors
///
/// Fails when the schema has no such type, and lists the ones it has.
pub fn type_in_schema(
    registry: &schema::ir::TypeRegistry,
    id: &str,
    wanted: Option<&str>,
) -> Result<String> {
    let Some(wanted) = wanted else {
        return Ok(id.to_owned());
    };
    for candidate in [format!("{id}::{wanted}"), wanted.to_owned()] {
        if registry.types.contains_key(&candidate) {
            return Ok(candidate);
        }
    }
    let prefix = format!("{id}::");
    let own: Vec<&str> = registry
        .types
        .keys()
        .filter_map(|name| name.strip_prefix(&prefix))
        .collect();
    Err(DoeError::Schema(format!(
        "the schema {id} has no type {wanted}{}",
        if own.is_empty() {
            " (it defines no types besides itself)".to_owned()
        } else {
            format!(": it has {}", own.join(", "))
        }
    )))
}

/// The file extensions of schemas: `.yaml` and `.yml`, and `.ksy`, which Kaitai
/// Struct files have.
pub(crate) const SCHEMA_EXTENSIONS: [&str; 3] = ["yaml", "yml", "ksy"];

/// Returns `true` if `s` ends in a schema extension, ignoring ASCII case.
pub(crate) fn has_schema_extension(s: &str) -> bool {
    Path::new(s).extension().is_some_and(|ext| {
        SCHEMA_EXTENSIONS
            .iter()
            .any(|known| ext.eq_ignore_ascii_case(known))
    })
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
#[cfg(feature = "fs")]
pub fn load_schema_file(path: &Path) -> Result<schema::raw::RawSchema> {
    let text = std::fs::read_to_string(path).map_err(|e| DoeError::Io {
        path: path.to_owned(),
        source: e,
    })?;
    let mut s: schema::raw::RawSchema =
        schema::raw::RawSchema::from_yaml_str(&text).map_err(|e| DoeError::Yaml {
            path: path.to_owned(),
            source: Box::new(e),
        })?;
    s.source_path = Some(path.to_owned());
    Ok(s)
}
