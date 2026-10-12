//! Command-line definition.

use std::path::PathBuf;

use clap::{ColorChoice, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
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
    disable_help_flag = true,
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true
)]
pub(crate) struct Cli {
    /// Binary file to parse.
    #[arg(required_unless_present = "help")]
    pub(crate) file: Option<PathBuf>,

    /// Type name (e.g. `png`) or path to a YAML schema file.
    #[arg(required_unless_present = "help")]
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

    /// Subcommand; without one, FILE is parsed.
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

/// Subcommands other than parsing a file.
#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Print a shell completion script to stdout.
    Completions {
        /// The shell to generate the script for.
        shell: Shell,
    },
    /// Print the roff man page to stdout.
    #[command(hide = true)]
    Man,
}

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
