//! The commands the binary runs.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::CommandFactory;
use doe::OutputFormat;
use doe::config::{Config, discover_types, resolve_include_paths};

use crate::cli::Cli;

/// Run whatever `cli` asks for. Logging has already been set up.
pub(crate) fn dispatch(cli: &Cli, config: &Config) -> anyhow::Result<()> {
    if let Some(shell) = cli.completions {
        completions(shell)
    } else if cli.man {
        man()
    } else if cli.help {
        print_help(&cli.include_paths, config)
    } else {
        run(cli)
    }
}

/// Write `bytes` to stdout, reporting failures (including a closed pipe).
fn write_stdout(bytes: &[u8]) -> anyhow::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(bytes)?;
    stdout.flush()?;
    Ok(())
}

fn completions(shell: clap_complete::Shell) -> anyhow::Result<()> {
    let mut script = Vec::new();
    clap_complete::generate(shell, &mut Cli::command(), "doe", &mut script);
    write_stdout(&script)
}

fn man() -> anyhow::Result<()> {
    let mut page = Vec::new();
    clap_mangen::Man::new(Cli::command()).render(&mut page)?;
    write_stdout(&page)
}

fn run(cli: &Cli) -> anyhow::Result<()> {
    let (Some(file), Some(type_or_schema)) = (&cli.file, &cli.type_or_schema) else {
        // clap requires both unless `--help`, `--completions` or `--man` was given.
        anyhow::bail!("a FILE and a TYPE_OR_SCHEMA are required");
    };

    let format = if cli.pretty {
        OutputFormat::JsonPretty
    } else if cli.json {
        OutputFormat::Json
    } else {
        OutputFormat::Text {
            indent: cli.indent.clone(),
        }
    };

    log::info!("parsing {} as {type_or_schema}", file.display());
    let output = doe::parse_file_with(
        file,
        type_or_schema,
        &cli.include_paths,
        cli.config.as_deref(),
        format,
        &doe::ParseOptions {
            instances: !cli.no_instances,
        },
    )
    .with_context(|| format!("could not parse {} as {type_or_schema}", file.display()))?;

    write_stdout(output.as_bytes())
}

/// Resolve include paths from CLI flags and the config file, then call
/// `discover_types`.  On duplicate errors, return an error describing every
/// conflict so the user knows their type library is broken before they waste
/// time trying to parse a file.
fn resolve_and_check_types(
    cli_include_paths: &[PathBuf],
    config: &Config,
) -> anyhow::Result<Vec<doe::AvailableType>> {
    let include_paths = resolve_include_paths(cli_include_paths, config);
    log::debug!("include paths: {include_paths:?}");

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
fn print_help(cli_include_paths: &[PathBuf], config: &Config) -> anyhow::Result<()> {
    let types = resolve_and_check_types(cli_include_paths, config)?;

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

/// The config file path for log messages.
pub(crate) fn describe_config(path: Option<&Path>) -> String {
    path.map(Path::to_owned)
        .or_else(Config::default_path)
        .map_or_else(|| "<none>".to_owned(), |p| p.display().to_string())
}
