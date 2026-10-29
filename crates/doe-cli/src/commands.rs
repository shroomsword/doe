//! The commands the binary runs.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::CommandFactory;
use doe::OutputFormat;
use doe::config::{Config, discover_types, resolve_include_paths, with_default_types};

use crate::cli::Cli;
use crate::manpage::{self, Page};

/// Run whatever `cli` asks for. Logging has already been set up.
pub(crate) fn dispatch(cli: &Cli, config: &Config) -> anyhow::Result<()> {
    if let Some(shell) = cli.completions {
        completions(shell)
    } else if let Some(page) = cli.man {
        man(page)
    } else if cli.ide {
        ide(cli, config)
    } else if cli.tui {
        tui(cli, config)
    } else if let Some(dir) = &cli.install_types {
        install_types(dir.as_deref(), cli.force)
    } else if cli.list_types {
        crate::list_types::run(cli, config)
    } else if cli.help {
        print_help(&cli.include_paths, config)
    } else {
        run(cli, config)
    }
}

/// The directories searched for types: `-I`, then the config's, then the
/// installed types (`doe --install-types`) when there are some.
pub(crate) fn include_paths(cli_paths: &[PathBuf], config: &Config) -> Vec<PathBuf> {
    with_default_types(resolve_include_paths(cli_paths, config), config)
}

/// What to say when there is nowhere to look for types: how to get some.
fn no_types_hint() -> Option<String> {
    if crate::install_types::TYPES.is_empty() {
        return None;
    }
    let dir = doe::config::default_types_dir()?;
    Some(format!(
        "There are no type paths: install the built-in types in {} with `doe --install-types`, or give a directory with -I.",
        dir.display()
    ))
}

/// `doe --install-types[=DIR]`: write the built-in types to `DIR`, or to the
/// default directory when none is named.
fn install_types(dir: Option<&Path>, force: bool) -> anyhow::Result<()> {
    let dir = match dir {
        Some(dir) => dir.to_owned(),
        None => doe::config::default_types_dir()
            .context("the home directory is not known: name the directory, --install-types=DIR")?,
    };
    let mut report = Vec::new();
    let result = crate::install_types::run(&dir, force, &mut report);
    write_stdout(&report)?;
    result.map(|_| ())
}

/// Write `bytes` to stdout, reporting failures (including a closed pipe).
pub(crate) fn write_stdout(bytes: &[u8]) -> anyhow::Result<()> {
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

fn man(page: Page) -> anyhow::Result<()> {
    write_stdout(&manpage::render(page)?)
}

#[cfg(feature = "ide")]
fn ide(cli: &Cli, config: &Config) -> anyhow::Result<()> {
    let include_paths = include_paths(&cli.include_paths, config);
    if !crate::ide::BUNDLED {
        log::warn!(
            "this build of doe has no web IDE in it (it was built without web/dist): the page will say so"
        );
    }
    let local = crate::ide_api::Local::new(&include_paths, cli.file.as_deref())?;
    let requested = cli.port.unwrap_or(crate::cli::DEFAULT_IDE_PORT);
    let (listener, port) = crate::ide::bind(requested)
        .with_context(|| format!("could not listen on 127.0.0.1:{requested}"))?;
    let url = format!("http://127.0.0.1:{port}/");
    write_stdout(format!("doe IDE: {url}  (Ctrl-C to stop)\n").as_bytes())?;
    if cli.open {
        crate::ide::open_browser(&url);
    }
    crate::ide::serve(&listener, port, local);
    Ok(())
}

#[cfg(not(feature = "ide"))]
fn ide(_cli: &Cli, _config: &Config) -> anyhow::Result<()> {
    anyhow::bail!("--ide is not in this build of doe: it was built without the `ide` feature")
}

/// The schema of `file`, found among the types of the include paths: the id of
/// the first type that parses it. What was tried is logged (`-v`), and when
/// nothing fits it is the error.
fn detect_type(cli: &Cli, config: &Config, file: &std::path::Path) -> anyhow::Result<String> {
    let include_paths = include_paths(&cli.include_paths, config);
    let options = doe::detect::Options {
        all: cli.detect.as_deref() == Some("all"),
        ..doe::detect::Options::default()
    };
    let found = doe::detect::detect_file(file, &include_paths, &options)
        .with_context(|| format!("could not look for the schema of {}", file.display()))?;
    for attempt in &found.attempts {
        log::info!(
            "tried {} ({}): {}",
            attempt.name,
            attempt.why.describe(),
            attempt.outcome.describe()
        );
    }
    let Some(chosen) = found.chosen() else {
        if include_paths.is_empty()
            && let Some(hint) = no_types_hint()
        {
            anyhow::bail!("no schema fits {}: {hint}", file.display());
        }
        let hint = if options.all {
            ""
        } else {
            "\nOnly the types whose signature the file starts with, and those named like the file, were tried; \
             --detect all tries the rest."
        };
        anyhow::bail!(
            "no schema fits {}: {} tried, {} known\n{}{hint}\nName the schema: doe {} TYPE_OR_SCHEMA",
            file.display(),
            found.attempts.len(),
            found.known,
            found.report().trim_end(),
            file.display()
        );
    };
    log::info!("{} is {}", file.display(), chosen.name);
    Ok(chosen.name.clone())
}

#[cfg(feature = "tui")]
fn tui(cli: &Cli, config: &Config) -> anyhow::Result<()> {
    let Some(file) = &cli.file else {
        anyhow::bail!("a FILE is required");
    };
    let type_or_schema = match (&cli.type_or_schema, &cli.build) {
        (Some(given), _) => given.clone(),
        // the schema is the one that is being built
        (None, Some(_)) => String::new(),
        (None, None) => detect_type(cli, config, file)?,
    };
    // `--color never`, and `NO_COLOR` unless `--color always`, mean no colour.
    let color = match cli.color {
        clap::ColorChoice::Always => true,
        clap::ColorChoice::Never => false,
        clap::ColorChoice::Auto => std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()),
    };
    let options = doe_tui::Options {
        file: file.clone(),
        type_or_schema,
        include_paths: include_paths(&cli.include_paths, config),
        color,
        watch: cli.watch,
        theme: cli.theme.clone().or_else(|| config.theme.clone()),
        radix: match cli.radix {
            Some(radix) => Some(radix),
            None => config.radix.as_deref().map(str::parse).transpose()?,
        },
        theme_dir: doe_tui::theme::user_dir(
            cli.config.clone().or_else(Config::default_path).as_deref(),
        ),
    };
    match &cli.build {
        Some(out) => doe_tui::run_build(&options, out),
        None => doe_tui::run(&options),
    }
}

#[cfg(not(feature = "tui"))]
fn tui(_cli: &Cli, _config: &Config) -> anyhow::Result<()> {
    anyhow::bail!("--tui is not in this build of doe: it was built without the `tui` feature")
}

fn run(cli: &Cli, config: &Config) -> anyhow::Result<()> {
    let Some(file) = &cli.file else {
        // clap requires it unless `--help`, `--completions` or `--man` was given.
        anyhow::bail!("a FILE is required");
    };
    let type_or_schema = &match &cli.type_or_schema {
        Some(given) => given.clone(),
        None => detect_type(cli, config, file)?,
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
    let include_paths = include_paths(cli_include_paths, config);
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
            if let Some(hint) = no_types_hint() {
                writeln!(stdout, "{hint}")?;
            }
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
    writeln!(
        stdout,
        "\n`doe --list-types` prints them with their files, for scripts."
    )?;
    Ok(())
}

/// The config file path for log messages.
pub(crate) fn describe_config(path: Option<&Path>) -> String {
    path.map(Path::to_owned)
        .or_else(Config::default_path)
        .map_or_else(|| "<none>".to_owned(), |p| p.display().to_string())
}
