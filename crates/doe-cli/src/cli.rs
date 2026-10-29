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
    #[arg(required_unless_present_any = ["help", "completions", "man", "ide", "list_types", "install_types"])]
    pub(crate) file: Option<PathBuf>,

    /// Type name (e.g. `png`) or path to a YAML schema file; add `::TYPE` to
    /// parse with a type inside it. Without it, the schema is looked for among
    /// the types of the include paths (see `--detect`).
    #[arg()]
    pub(crate) type_or_schema: Option<String>,

    /// When no `TYPE_OR_SCHEMA` is given: `all` also tries the types that have no
    /// signature and whose name the file does not carry (slow for a big
    /// library). Without it the types whose signature the file starts with, and
    /// those named like the file, are tried; `-v` says which.
    #[arg(long, value_name = "WHAT", value_parser = ["all"], conflicts_with = "type_or_schema")]
    pub(crate) detect: Option<String>,

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

    /// Print the types that can be used and exit: one per line, with the file
    /// each is in (relative to its type path) and the first line of its doc,
    /// aligned on a terminal and separated by tabs otherwise; with `--json` or
    /// `--pretty`, an array of objects with `id`, `path`, `type_path`, `doc`,
    /// `signature` and `error`. The type paths are searched in their
    /// subdirectories too. Give a SCHEMA instead of a FILE to list the types
    /// it defines and imports.
    #[arg(
        long,
        conflicts_with_all = ["type_or_schema", "detect", "no_instances", "help", "completions", "man", "ide", "tui"]
    )]
    pub(crate) list_types: bool,

    /// Write the type library and the Kaitai Struct gallery that are built into
    /// this binary to DIR (default: `~/.doe/types`, or `$DOE_TYPES_DIR`) and
    /// exit. `doe` searches that directory after the `-I` and config paths.
    /// Run it again after an upgrade: files that were not changed are
    /// replaced by the new version's, files that were are kept (see --force).
    #[arg(
        long,
        value_name = "DIR",
        num_args = 0..=1,
        require_equals = true,
        conflicts_with_all = ["file", "type_or_schema", "include_paths", "json", "pretty", "no_instances", "help", "completions", "man", "ide", "tui", "list_types", "detect"]
    )]
    #[allow(
        clippy::option_option,
        reason = "clap's way to tell the flag without a value (the default directory) from no flag"
    )]
    pub(crate) install_types: Option<Option<PathBuf>>,

    /// With --install-types, replace the files that were changed too.
    #[arg(long, requires = "install_types")]
    pub(crate) force: bool,

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
    /// parsing a file. Only the loopback address is used. A FILE is offered
    /// to the page as the input, however large it is; the types found through
    /// the include paths and the config are offered too.
    #[arg(
        long,
        conflicts_with_all = ["type_or_schema", "json", "pretty", "no_instances", "help", "completions", "man", "tui"]
    )]
    pub(crate) ide: bool,

    /// Show FILE decoded with the given type or schema in an interactive viewer in the
    /// terminal, instead of printing it. Needs a terminal on stdin and stdout.
    #[arg(
        long,
        conflicts_with_all = ["json", "pretty", "no_instances", "indent", "help", "completions", "man", "ide"]
    )]
    pub(crate) tui: bool,

    /// With `--tui`: build a schema one field at a time from the hex view of
    /// FILE, to be saved to OUT with `w`: a file that is not there yet (the
    /// schema is named for it), or a schema that this made, to go on with.
    /// Instead of a `TYPE_OR_SCHEMA`.
    #[arg(
        long,
        value_name = "OUT",
        requires = "tui",
        conflicts_with_all = ["type_or_schema", "watch"]
    )]
    pub(crate) build: Option<PathBuf>,

    /// With `--tui`: read the schema and the file again when either changes
    /// on disk (the modification times are looked at twice a second).
    #[arg(long, requires = "tui")]
    pub(crate) watch: bool,

    /// With `--tui`: the colour theme, a built-in one by its id (`nord`,
    /// `solarized-dark`, `catppuccin-mocha`...), a file `NAME.toml` in the
    /// `themes` directory next to the config file, or the path of a theme file;
    /// `terminal` is the colours of the terminal itself, which is the default
    /// (the `theme` key of the config file changes that).
    #[arg(long, value_name = "NAME|FILE", requires = "tui")]
    pub(crate) theme: Option<String>,

    /// With `--tui`: the radix in which integers are first shown, `dec`
    /// (the default), `hex`, `bin` or `oct`; `x` in the viewer goes through
    /// them, and `X` chooses one for the selected field (the `radix` key of
    /// the config file changes the default).
    #[arg(long, value_name = "RADIX", requires = "tui", value_parser = clap::value_parser!(doe::radix::Radix))]
    pub(crate) radix: Option<doe::radix::Radix>,

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
