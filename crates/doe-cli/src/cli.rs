//! Command-line definition.

use std::path::PathBuf;

use clap::{ColorChoice, Parser, ValueEnum};
use clap_complete::Shell;

use crate::manpage::Page;
use clap_verbosity_flag::{Verbosity, WarnLevel};

/// doe — binary file parser
///
/// Parses a binary FILE according to a schema and prints the result.
/// The schema can be supplied as either a known type name resolved from the
/// include paths, or a path to a YAML schema file.
#[derive(Parser, Debug)]
#[command(
    name = "doe",
    version,
    about,
    long_about = None,
    disable_help_flag = true
)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each bool is a command-line flag; grouping them would only change how clap names them"
)]
pub(crate) struct Cli {
    /// Binary file to parse.
    #[arg(required_unless_present_any = ["help", "completions", "man", "ide"])]
    pub(crate) file: Option<PathBuf>,

    /// Type name (e.g. `png`) or path to a YAML schema file.
    #[arg(required_unless_present_any = ["help", "completions", "man", "ide"])]
    pub(crate) type_or_schema: Option<String>,

    /// Add a directory to the schema search path (repeatable).
    #[arg(short = 'I', long = "include-path", value_name = "DIR")]
    pub(crate) include_paths: Vec<PathBuf>,

    /// Config file path [default: ~/.config/doe/config.toml].
    #[arg(short = 'c', long = "config", value_name = "FILE")]
    pub(crate) config: Option<PathBuf>,

    /// Emit compact JSON output.
    #[arg(long)]
    pub(crate) json: bool,

    /// Emit pretty-printed JSON output.
    #[arg(long)]
    pub(crate) pretty: bool,

    /// Leave out the values a schema's `instances` compute or read elsewhere.
    #[arg(long)]
    pub(crate) no_instances: bool,

    /// Indentation string for text output.
    #[arg(long, default_value = "  ", value_name = "STR")]
    pub(crate) indent: String,

    /// Print help, including available types from the include paths.
    #[arg(long, short = 'h', action = clap::ArgAction::SetTrue)]
    pub(crate) help: bool,

    /// When to use color on stderr.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto, value_name = "WHEN", global = true)]
    pub(crate) color: ColorChoice,

    /// Verbosity of the log file [default: debug].
    #[arg(long, value_enum, value_name = "LEVEL", global = true)]
    pub(crate) log_level: Option<LogLevelArg>,

    /// Write the log file here [default: the XDG state directory].
    #[arg(long, value_name = "FILE", global = true)]
    pub(crate) log_file: Option<PathBuf>,

    /// Verbosity of messages on stderr.
    #[command(flatten)]
    pub(crate) verbosity: Verbosity<WarnLevel>,

    /// Print a shell completion script to stdout and exit.
    #[arg(
        long,
        value_enum,
        value_name = "SHELL",
        conflicts_with_all = PARSING_ARGS
    )]
    pub(crate) completions: Option<Shell>,

    /// Print a manual page (roff) to stdout and exit: `doe` (the default) or
    /// `schema`, the schema format.
    #[arg(
        long,
        value_enum,
        value_name = "PAGE",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "doe",
        conflicts_with_all = PARSING_ARGS,
        conflicts_with = "completions"
    )]
    pub(crate) man: Option<Page>,

    /// Serve the web IDE on this machine and print its address, instead of
    /// parsing a file. Only the loopback address is used.
    #[arg(
        long,
        conflicts_with_all = PARSING_ARGS,
        conflicts_with_all = ["completions", "man"]
    )]
    pub(crate) ide: bool,

    /// With `--ide`: the port to serve on (0 for any free one) [default: 8765].
    #[arg(long, value_name = "PORT", requires = "ide")]
    pub(crate) port: Option<u16>,

    /// With `--ide`: open the page in the browser.
    #[arg(long, requires = "ide")]
    pub(crate) open: bool,
}

/// The port that `--ide` serves on when `--port` is not given.
#[cfg(feature = "ide")]
pub(crate) const DEFAULT_IDE_PORT: u16 = 8765;

/// The arguments that only mean something when a file is parsed: they conflict
/// with `--completions` and `--man`. The global ones (`--color`, the logging
/// options) are left out, so they can be given with either.
const PARSING_ARGS: [&str; 8] = [
    "file",
    "type_or_schema",
    "include_paths",
    "config",
    "json",
    "pretty",
    "no_instances",
    "help",
];

/// Log verbosity as spelled on the command line.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogLevelArg {
    /// Log nothing.
    Off,
    /// Errors only.
    Error,
    /// Errors and warnings.
    Warn,
    /// Also informational messages.
    Info,
    /// Also debug messages.
    Debug,
    /// Also trace messages.
    Trace,
}

impl From<LogLevelArg> for doe::config::LogLevel {
    fn from(level: LogLevelArg) -> Self {
        match level {
            LogLevelArg::Off => Self::Off,
            LogLevelArg::Error => Self::Error,
            LogLevelArg::Warn => Self::Warn,
            LogLevelArg::Info => Self::Info,
            LogLevelArg::Debug => Self::Debug,
            LogLevelArg::Trace => Self::Trace,
        }
    }
}
