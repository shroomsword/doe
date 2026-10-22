//! `doe` binary — CLI entry point.
//!
//! All logic lives in the `doe` library crate.  This file parses arguments,
//! sets up logging, runs the requested command and maps the result to an
//! exit code.

mod cli;
mod commands;
#[cfg(feature = "ide")]
mod ide;
mod logging;
mod manpage;

use std::io::{self, Write};
use std::process::ExitCode;

use clap::Parser;
use doe::config::Config;

use crate::cli::Cli;
use crate::logging::LogSettings;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match try_main(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) if is_broken_pipe(&err) => ExitCode::SUCCESS,
        Err(err) => {
            report(&cli, &err);
            ExitCode::FAILURE
        }
    }
}

fn try_main(cli: &Cli) -> anyhow::Result<()> {
    // Generating completions or the man page needs neither config nor logs.
    if cli.completions.is_some() || cli.man.is_some() {
        return commands::dispatch(cli, &Config::default());
    }

    // A broken config file is reported as the command's error, but logging is
    // set up first so the error reaches the log file and honours `-q`.
    let config = Config::load_or_default(cli.config.as_deref());
    let loaded = config.as_ref().ok();

    logging::init(&LogSettings {
        stderr_level: cli.verbosity.log_level_filter(),
        color: cli.color,
        file_level_flag: cli.log_level.map(Into::into),
        file_level_config: loaded.and_then(|c| c.log.level),
        file_path: cli.log_file.as_deref(),
    })?;
    log::debug!(
        "config file: {}",
        commands::describe_config(cli.config.as_deref())
    );

    commands::dispatch(cli, &config?)
}

/// Returns `true` if `err` was caused by the reader of stdout going away.
fn is_broken_pipe(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|cause| cause.downcast_ref::<io::Error>())
        .any(|e| e.kind() == io::ErrorKind::BrokenPipe)
}

/// Report a failure: `error: <message>` and each cause, through the logger.
fn report(cli: &Cli, err: &anyhow::Error) {
    let mut message = err.to_string();
    for cause in err.chain().skip(1) {
        message.push_str("\n  caused by: ");
        message.push_str(&cause.to_string());
    }
    if logging::installed() {
        log::error!("{message}");
    } else if cli.verbosity.log_level_filter() >= log::LevelFilter::Error {
        let mut stderr = logging::color_stream(io::stderr(), cli.color);
        // If stderr itself is unwritable there is nowhere left to report to.
        let _ = writeln!(stderr, "error: {message}");
    }
}
