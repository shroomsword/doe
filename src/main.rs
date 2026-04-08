//! `doe` binary — CLI entry point.
//!
//! All logic lives in the `doe` library crate.  This file is responsible
//! only for argument parsing, delegating to `doe::parse_file`, and writing
//! output / exit codes.
//!
//! ## Help generation
//!
//! `--help` output is augmented with the list of types available on the
//! configured include paths.  This requires a two-pass strategy:
//!
//! 1. Parse flags only (no positional args) to discover `-I` / `-c` values.
//! 2. Load the config, resolve include paths, and scan for available types.
//! 3. If `--help` was requested, print standard clap help followed by the
//!    type list, then exit.  Otherwise, proceed with normal argument parsing.

use std::path::PathBuf;
use std::process;

use clap::{CommandFactory, Parser as ClapParser};

use doe::config::{resolve_include_paths, Config};
use doe::{discover_types, OutputFormat};

// ─────────────────────────────────────────────────────────────────────────────
// CLI definition
// ─────────────────────────────────────────────────────────────────────────────

/// doe — binary file parser
///
/// Parses a binary FILE according to a schema and prints the result.
/// The schema can be supplied as either a known type name resolved from the
/// include paths, or a path to a YAML schema file.
#[derive(ClapParser, Debug)]
#[command(name = "doe", version, about, long_about = None, disable_help_flag = true)]
struct Cli {
    /// Binary file to parse.
    file: PathBuf,

    /// Type name (e.g. `png`) or path to a YAML schema file.
    type_or_schema: String,

    /// Add a directory to the schema search path (repeatable).
    #[arg(short = 'I', long = "include-path", value_name = "DIR")]
    include_paths: Vec<PathBuf>,

    /// Config file path [default: ~/.doe/config/config.yaml].
    #[arg(short = 'c', long = "config", value_name = "FILE")]
    config: Option<PathBuf>,

    /// Emit compact JSON output.
    #[arg(long)]
    json: bool,

    /// Emit pretty-printed JSON output.
    #[arg(long)]
    pretty: bool,

    /// Indentation string for text output [default: two spaces].
    #[arg(long, default_value = "  ", value_name = "STR")]
    indent: String,

    /// Print help, including available types from the include paths.
    #[arg(long, short = 'h', action = clap::ArgAction::SetTrue)]
    help: bool,
}

/// Flags-only subset of the CLI, used in the first pass to extract `-I` / `-c`
/// before positional arguments are required.
#[derive(ClapParser, Debug)]
#[command(disable_help_flag = true, ignore_errors = true)]
struct EarlyArgs {
    #[arg(short = 'I', long = "include-path", value_name = "DIR")]
    include_paths: Vec<PathBuf>,

    #[arg(short = 'c', long = "config", value_name = "FILE")]
    config: Option<PathBuf>,

    #[arg(long, short = 'h', action = clap::ArgAction::SetTrue)]
    help: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

fn main() {
    // ── Pass 1: extract flags without requiring positional args ───────────────
    let early = EarlyArgs::parse();

    if early.help {
        print_help(&early.include_paths, early.config.as_deref());
        process::exit(0);
    }

    // ── Pass 2: full parse now that we know --help was not requested ──────────
    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("doe: error: {}", e);
        process::exit(1);
    }
}

fn run(cli: Cli) -> doe::Result<()> {
    let format = if cli.pretty {
        OutputFormat::JsonPretty
    } else if cli.json {
        OutputFormat::Json
    } else {
        OutputFormat::Text {
            indent: cli.indent.clone(),
        }
    };

    let output = doe::parse_file(
        &cli.file,
        &cli.type_or_schema,
        &cli.include_paths,
        cli.config.as_deref(),
        format,
    )?;

    print!("{}", output);
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Help output
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve include paths from CLI flags and config file, then call
/// `discover_types`.  On duplicate errors, print every conflict to stderr
/// and exit with a non-zero code so the user knows their type library is
/// broken before they waste time trying to parse a file.
fn resolve_and_check_types(
    cli_include_paths: &[PathBuf],
    config_path: Option<&std::path::Path>,
) -> Vec<doe::AvailableType> {
    let cfg_path = config_path
        .map(std::path::Path::to_owned)
        .or_else(Config::default_path)
        .unwrap_or_else(|| PathBuf::from(".doe_config.yaml"));

    let config = Config::load(&cfg_path).unwrap_or_default();
    let include_paths = resolve_include_paths(cli_include_paths, &config);

    match discover_types(&include_paths) {
        Ok(types) => types,
        Err(duplicates) => {
            eprintln!("doe: error: duplicate type ids found in include paths\n");
            for dup in &duplicates {
                eprintln!("{}\n", dup);
            }
            process::exit(1);
        }
    }
}

/// Return the first non-empty, non-whitespace-only line of `s`, trimmed,
/// with any trailing sentence-ending punctuation (`.`, `!`, `?`) removed.
/// Returns `None` if the string contains no such line.
fn first_line(s: &str) -> Option<&str> {
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| l.trim_end_matches(|c| matches!(c, '.' | '!' | '?')))
}

/// Print the standard clap help text followed by a list of types discovered
/// on the resolved include paths.
fn print_help(cli_include_paths: &[PathBuf], config_path: Option<&std::path::Path>) {
    // Print clap's generated help text for the main Cli struct.
    let mut cmd = Cli::command();
    let mut help_text = Vec::new();
    cmd.write_help(&mut help_text).unwrap_or(());
    print!("{}", String::from_utf8_lossy(&help_text));

    let types = resolve_and_check_types(cli_include_paths, config_path);

    if types.is_empty() {
        if cli_include_paths.is_empty() {
            println!("\nAvailable types: none (no include paths configured)");
        } else {
            println!("\nAvailable types: none found on include paths");
        }
        return;
    }

    println!("\nAvailable types:");

    // Align the doc column: pad each id to the width of the longest one.
    let max_id_len = types.iter().map(|t| t.id.len()).max().unwrap_or(0);

    for t in &types {
        match t.doc.as_deref().and_then(first_line) {
            Some(line) => println!("  {:<width$}  {}", t.id, line, width = max_id_len),
            None => println!("  {}", t.id),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
            help: false,
        };
        assert!(run(cli).is_err());
    }

    #[test]
    fn pretty_implies_json_pretty_format() {
        let cli = Cli {
            file: PathBuf::from("ignored"),
            type_or_schema: "ignored".into(),
            include_paths: vec![],
            config: None,
            json: false,
            pretty: true,
            indent: "  ".into(),
            help: false,
        };
        assert!(matches!(
            if cli.pretty {
                OutputFormat::JsonPretty
            } else if cli.json {
                OutputFormat::Json
            } else {
                OutputFormat::Text {
                    indent: cli.indent.clone(),
                }
            },
            OutputFormat::JsonPretty
        ));
    }

    #[test]
    fn json_flag_selects_compact_format() {
        let cli = Cli {
            file: PathBuf::from("ignored"),
            type_or_schema: "ignored".into(),
            include_paths: vec![],
            config: None,
            json: true,
            pretty: false,
            indent: "  ".into(),
            help: false,
        };
        assert!(matches!(
            if cli.pretty {
                OutputFormat::JsonPretty
            } else if cli.json {
                OutputFormat::Json
            } else {
                OutputFormat::Text {
                    indent: cli.indent.clone(),
                }
            },
            OutputFormat::Json
        ));
    }

    #[test]
    fn is_schema_path_reexported() {
        use doe::is_schema_path;
        assert!(is_schema_path("./foo.yaml"));
        assert!(!is_schema_path("png"));
    }

    #[test]
    fn print_help_no_include_paths_does_not_panic() {
        print_help(&[], None);
    }

    #[test]
    fn print_help_with_types_does_not_panic() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("mytype.yaml"),
            "id: mytype\ndoc: A test type\nseq: []\n",
        )
        .unwrap();
        print_help(&[dir.path().to_owned()], None);
    }

    // ── first_line ────────────────────────────────────────────────────────────

    #[test]
    fn first_line_single_line() {
        assert_eq!(first_line("hello"), Some("hello"));
    }

    #[test]
    fn first_line_multiline_returns_first() {
        assert_eq!(first_line("first\nsecond\nthird"), Some("first"));
    }

    #[test]
    fn first_line_trims_whitespace() {
        assert_eq!(first_line("  trimmed  \nsecond"), Some("trimmed"));
    }

    #[test]
    fn first_line_skips_leading_blank_lines() {
        assert_eq!(first_line("\n\nactual line\nmore"), Some("actual line"));
    }

    #[test]
    fn first_line_empty_string_is_none() {
        assert_eq!(first_line(""), None);
    }

    #[test]
    fn first_line_only_whitespace_is_none() {
        assert_eq!(first_line("   \n   \n"), None);
    }

    #[test]
    fn first_line_strips_trailing_period() {
        assert_eq!(first_line("A PNG image file."), Some("A PNG image file"));
    }

    #[test]
    fn first_line_strips_trailing_exclamation() {
        assert_eq!(first_line("Watch out!"), Some("Watch out"));
    }

    #[test]
    fn first_line_strips_trailing_question_mark() {
        assert_eq!(first_line("Is this ELF?"), Some("Is this ELF"));
    }

    #[test]
    fn first_line_strips_multiple_trailing_punctuation() {
        // e.g. "Really?!" — strips all consecutive sentence-enders
        assert_eq!(first_line("Really?!"), Some("Really"));
    }

    #[test]
    fn first_line_no_punctuation_unchanged() {
        assert_eq!(first_line("No punctuation here"), Some("No punctuation here"));
    }

    #[test]
    fn first_line_block_scalar_style() {
        // Simulates a YAML block scalar like the ELF doc field.
        let doc = "Executable and Linkable Format (ELF).\n\nCovers 32-bit and 64-bit ELF files.";
        assert_eq!(
            first_line(doc),
            Some("Executable and Linkable Format (ELF)")
        );
    }
}
