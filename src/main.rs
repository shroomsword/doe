mod config;
mod error;
mod parser;
mod render;
mod schema;
mod value;

use std::path::PathBuf;
use std::process;

use clap::Parser as ClapParser;

use crate::config::{resolve_include_paths, Config};
use crate::error::DoeError;
use crate::parser::cursor::Cursor;
use crate::parser::engine::Engine;
use crate::schema::compiler::Compiler;
use crate::schema::raw::RawSchema;

// ─────────────────────────────────────────────────────────────────────────────
// CLI definition
// ─────────────────────────────────────────────────────────────────────────────

/// doe — binary file parser
///
/// Parses a binary FILE according to a schema and prints the result.
/// The schema can be supplied as either:
///   - a known type name (e.g. `png`) resolved from include paths, or
///   - a path to a YAML schema file (e.g. `./my_format.yaml`).
#[derive(ClapParser, Debug)]
#[command(name = "doe", version, about, long_about = None)]
struct Cli {
    /// Binary file to parse.
    file: PathBuf,

    /// Type name (e.g. `png`) or path to a YAML schema file.
    type_or_schema: String,

    /// Add a directory to the schema search path.
    /// May be specified multiple times; earlier entries take priority.
    #[arg(short = 'I', long = "include-path", value_name = "DIR")]
    include_paths: Vec<PathBuf>,

    /// Path to the configuration file.
    /// Defaults to ~/.doe/config/config.yaml.
    #[arg(short = 'c', long = "config", value_name = "FILE")]
    config: Option<PathBuf>,

    /// Emit JSON instead of indented text.
    #[arg(long)]
    json: bool,

    /// Pretty-print JSON output (implies --json).
    #[arg(long)]
    pretty: bool,

    /// Indentation string for text output (default: two spaces).
    #[arg(long, default_value = "  ", value_name = "STR")]
    indent: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("doe: error: {}", e);
        process::exit(1);
    }
}

fn run(cli: Cli) -> crate::error::Result<()> {
    // ── Load configuration ────────────────────────────────────────────────────

    let config_path = cli
        .config
        .clone()
        .or_else(Config::default_path)
        .unwrap_or_else(|| PathBuf::from(".doe_config.yaml"));

    let config = Config::load(&config_path)?;

    let include_paths = resolve_include_paths(&cli.include_paths, &config);

    // ── Compile the schema ────────────────────────────────────────────────────

    let mut compiler = Compiler::new(include_paths);

    // Determine whether the argument is a path to a YAML file or a bare type name.
    let type_name = if is_schema_path(&cli.type_or_schema) {
        // Explicit path to a YAML file — load it directly.
        let schema_path = PathBuf::from(&cli.type_or_schema);
        let schema = load_schema_file(&schema_path)?;
        let id = schema.id.clone();
        compiler.process_schema(&schema)?;
        id
    } else {
        // Bare type name — resolve through include paths.
        compiler.load(&cli.type_or_schema, None)?;
        cli.type_or_schema.clone()
    };

    let registry = compiler.registry;

    // ── Read the binary file ──────────────────────────────────────────────────

    let binary_data = std::fs::read(&cli.file).map_err(|e| DoeError::Io {
        path: cli.file.clone(),
        source: e,
    })?;

    // ── Parse ─────────────────────────────────────────────────────────────────

    let engine = Engine::new(&registry);
    let mut cursor = Cursor::new(&binary_data);
    let value = engine.parse_type(&type_name, &mut cursor)?;

    // ── Render ────────────────────────────────────────────────────────────────

    if cli.pretty || cli.json {
        if cli.pretty {
            print!("{}", render::json::render_pretty(&value));
        } else {
            print!("{}", render::json::render(&value));
        }
    } else {
        print!("{}", render::text::render_with_indent(&value, &cli.indent));
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Returns `true` if `s` looks like a path to a schema file rather than a
/// bare type name.  Heuristic: starts with `.`, `/`, or `~`, or ends with
/// `.yaml` / `.yml`.
fn is_schema_path(s: &str) -> bool {
    s.starts_with('.')
        || s.starts_with('/')
        || s.starts_with('~')
        || s.ends_with(".yaml")
        || s.ends_with(".yml")
}

/// Load and parse a YAML schema file, injecting its path.
fn load_schema_file(path: &std::path::Path) -> crate::error::Result<RawSchema> {
    let text = std::fs::read_to_string(path).map_err(|e| DoeError::Io {
        path: path.to_owned(),
        source: e,
    })?;
    let mut schema: RawSchema = serde_yaml::from_str(&text).map_err(|e| DoeError::Yaml {
        path: path.to_owned(),
        source: e,
    })?;
    schema.source_path = Some(path.to_owned());
    Ok(schema)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_schema_path ────────────────────────────────────────────────────────

    #[test]
    fn schema_path_dot_prefix()       { assert!(is_schema_path("./my.yaml")); }
    #[test]
    fn schema_path_absolute()         { assert!(is_schema_path("/usr/share/types/png.yaml")); }
    #[test]
    fn schema_path_tilde()            { assert!(is_schema_path("~/types/png.yaml")); }
    #[test]
    fn schema_path_yaml_extension()   { assert!(is_schema_path("some_file.yaml")); }
    #[test]
    fn schema_path_yml_extension()    { assert!(is_schema_path("some_file.yml")); }
    #[test]
    fn bare_type_name_not_a_path()    { assert!(!is_schema_path("png")); }
    #[test]
    fn bare_type_no_extension()       { assert!(!is_schema_path("my_format")); }

    // ── End-to-end: schema file path -> parse -> text output ─────────────────

    #[test]
    fn end_to_end_schema_file_text_output() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut schema_file = NamedTempFile::new().unwrap();
        writeln!(schema_file, "id: simple").unwrap();
        writeln!(schema_file, "seq:").unwrap();
        writeln!(schema_file, "  - id: a").unwrap();
        writeln!(schema_file, "    type: u8").unwrap();
        writeln!(schema_file, "  - id: b").unwrap();
        writeln!(schema_file, "    type: u16le").unwrap();

        let mut data_file = NamedTempFile::new().unwrap();
        data_file.write_all(&[0x07, 0x34, 0x12]).unwrap();

        let schema = load_schema_file(schema_file.path()).unwrap();
        let type_name = schema.id.clone();
        let mut compiler = Compiler::new(vec![]);
        compiler.process_schema(&schema).unwrap();

        let binary = std::fs::read(data_file.path()).unwrap();
        let engine = Engine::new(&compiler.registry);
        let mut cursor = Cursor::new(&binary);
        let value = engine.parse_type(&type_name, &mut cursor).unwrap();

        let out = render::text::render(&value);
        assert!(out.contains("simple\n"));
        assert!(out.contains("a  7"));
        assert!(out.contains("b  4660")); // 0x1234
    }

    #[test]
    fn end_to_end_schema_file_json_output() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut schema_file = NamedTempFile::new().unwrap();
        writeln!(schema_file, "id: pair").unwrap();
        writeln!(schema_file, "seq:").unwrap();
        writeln!(schema_file, "  - id: x").unwrap();
        writeln!(schema_file, "    type: u8").unwrap();
        writeln!(schema_file, "  - id: y").unwrap();
        writeln!(schema_file, "    type: u8").unwrap();

        let schema = load_schema_file(schema_file.path()).unwrap();
        let mut compiler = Compiler::new(vec![]);
        compiler.process_schema(&schema).unwrap();

        let binary = [0x01u8, 0x02];
        let engine = Engine::new(&compiler.registry);
        let mut cursor = Cursor::new(&binary);
        let value = engine.parse_type("pair", &mut cursor).unwrap();

        let json_str = render::json::render_pretty(&value);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["$type"], "pair");
        assert_eq!(parsed["x"], 1u64);
        assert_eq!(parsed["y"], 2u64);
    }

    #[test]
    fn end_to_end_include_path_resolution() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let schema_path = dir.path().join("mytype.yaml");
        std::fs::write(&schema_path,
            "id: mytype\nseq:\n  - id: val\n    type: u32le\n"
        ).unwrap();

        let mut compiler = Compiler::new(vec![dir.path().to_owned()]);
        compiler.load("mytype", None).unwrap();

        let binary = 0xdeadbeefu32.to_le_bytes();
        let engine = Engine::new(&compiler.registry);
        let mut cursor = Cursor::new(&binary);
        let value = engine.parse_type("mytype", &mut cursor).unwrap();

        let out = render::text::render(&value);
        assert!(out.contains("3735928559")); // 0xdeadbeef as decimal
    }

    #[test]
    fn end_to_end_with_subtype_and_repeat() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut f = NamedTempFile::new().unwrap();
        write!(f, concat!(
            "id: list\n",
            "seq:\n",
            "  - id: count\n",
            "    type: u8\n",
            "  - id: items\n",
            "    type: item_t\n",
            "    repeat: expr\n",
            "    repeat-expr: count\n",
            "types:\n",
            "  item_t:\n",
            "    seq:\n",
            "      - id: value\n",
            "        type: u16le\n",
        )).unwrap();

        let schema = load_schema_file(f.path()).unwrap();
        let mut compiler = Compiler::new(vec![]);
        compiler.process_schema(&schema).unwrap();

        let binary = [0x03u8, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00];
        let engine = Engine::new(&compiler.registry);
        let mut cursor = Cursor::new(&binary);
        let value = engine.parse_type("list", &mut cursor).unwrap();

        let out = render::text::render(&value);
        assert!(out.contains("count  3"));
        assert!(out.contains("items  [3 items]"));
        assert!(out.contains("[0]  list::item_t"));
    }

    #[test]
    fn missing_binary_file_is_error() {
        let cli = Cli {
            file: PathBuf::from("/nonexistent/file.bin"),
            type_or_schema: "png".into(),
            include_paths: vec![],
            config: Some(PathBuf::from("/nonexistent/config.yaml")),
            json: false,
            pretty: false,
            indent: "  ".into(),
        };
        let result = run(cli);
        assert!(result.is_err());
    }
}
