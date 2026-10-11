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

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Context;
use clap::{CommandFactory, Parser as ClapParser};

use doe::config::{Config, resolve_include_paths};
use doe::{OutputFormat, discover_types};

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

fn main() -> ExitCode {
    match try_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) if is_broken_pipe(&err) => ExitCode::SUCCESS,
        Err(err) => {
            report(&err);
            ExitCode::FAILURE
        }
    }
}

fn try_main() -> anyhow::Result<()> {
    // ── Pass 1: extract flags without requiring positional args ───────────────
    let early = EarlyArgs::parse();

    if early.help {
        return print_help(&early.include_paths, early.config.as_deref());
    }

    // ── Pass 2: full parse now that we know --help was not requested ──────────
    let cli = Cli::parse();
    run(&cli)
}

/// Returns `true` if `err` was caused by the reader of stdout going away.
fn is_broken_pipe(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|cause| cause.downcast_ref::<io::Error>())
        .any(|e| e.kind() == io::ErrorKind::BrokenPipe)
}

/// Print `error: <message>` and each cause to stderr.
fn report(err: &anyhow::Error) {
    let mut stderr = io::stderr().lock();
    // If stderr itself is unwritable there is nowhere left to report to.
    let _ = writeln!(stderr, "error: {err}");
    for cause in err.chain().skip(1) {
        let _ = writeln!(stderr, "  caused by: {cause}");
    }
}

fn run(cli: &Cli) -> anyhow::Result<()> {
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
    )
    .with_context(|| {
        format!(
            "could not parse {} as {}",
            cli.file.display(),
            cli.type_or_schema
        )
    })?;

    let mut stdout = io::stdout().lock();
    write!(stdout, "{output}")?;
    stdout.flush()?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Help output
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve include paths from CLI flags and config file, then call
/// `discover_types`.  On duplicate errors, return an error describing every
/// conflict so the user knows their type library is broken before they waste
/// time trying to parse a file.
fn resolve_and_check_types(
    cli_include_paths: &[PathBuf],
    config_path: Option<&Path>,
) -> anyhow::Result<Vec<doe::AvailableType>> {
    let cfg_path = config_path
        .map(Path::to_owned)
        .or_else(Config::default_path)
        .unwrap_or_else(|| PathBuf::from(".doe_config.yaml"));

    let config = Config::load(&cfg_path).unwrap_or_default();
    let include_paths = resolve_include_paths(cli_include_paths, &config);

    discover_types(&include_paths).map_err(|duplicates| {
        let details: Vec<String> = duplicates.iter().map(ToString::to_string).collect();
        anyhow::anyhow!(
            "duplicate type ids found in include paths\n\n{}",
            details.join("\n\n")
        )
    })
}

/// Print the standard clap help text followed by a list of types discovered
/// on the resolved include paths.
fn print_help(cli_include_paths: &[PathBuf], config_path: Option<&Path>) -> anyhow::Result<()> {
    let types = resolve_and_check_types(cli_include_paths, config_path)?;

    let mut stdout = io::stdout().lock();

    // Print clap's generated help text for the main Cli struct.
    Cli::command().write_help(&mut stdout)?;

    if types.is_empty() {
        if cli_include_paths.is_empty() {
            writeln!(
                stdout,
                "\nAvailable types: none (no include paths configured)"
            )?;
        } else {
            writeln!(stdout, "\nAvailable types: none found on include paths")?;
        }
        return Ok(());
    }

    writeln!(stdout, "\nAvailable types:")?;

    // Align the doc column: pad each id to the width of the longest one.
    let max_id_len = types.iter().map(|t| t.id.len()).max().unwrap_or(0);

    for t in &types {
        match &t.doc {
            Some(doc) => writeln!(stdout, "  {:<max_id_len$}  {doc}", t.id)?,
            None => writeln!(stdout, "  {}", t.id)?,
        }
    }
    Ok(())
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
        assert!(run(&cli).is_err());
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
        print_help(&[], None).unwrap();
    }

    #[test]
    fn print_help_with_types_does_not_panic() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("mytype.yaml"),
            "id: mytype\ndoc: A test type\nseq: []\n",
        )
        .unwrap();
        print_help(&[dir.path().to_owned()], None).unwrap();
    }
}
