//! Logger setup: terse messages on stderr, a detailed log file on disk.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use anstream::AutoStream;
use anstyle::{AnsiColor, Style};
use clap::ColorChoice;
use doe::config::LogLevel;
use etcetera::BaseStrategy;
use log::{Level, LevelFilter};

/// The log-file verbosity used when neither a flag nor the config file sets one.
const DEFAULT_FILE_LEVEL: LogLevel = LogLevel::Debug;

static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Returns `true` once [`init`] has installed the logger.
pub(crate) fn installed() -> bool {
    INSTALLED.load(Ordering::Relaxed)
}

/// How the logger should be set up.
#[derive(Debug)]
pub(crate) struct LogSettings<'a> {
    /// Threshold for messages on stderr.
    pub(crate) stderr_level: LevelFilter,
    /// When to use color on stderr.
    pub(crate) color: ColorChoice,
    /// Verbosity of the log file from `--log-level`.
    pub(crate) file_level_flag: Option<LogLevel>,
    /// Verbosity of the log file from the config file.
    pub(crate) file_level_config: Option<LogLevel>,
    /// The log file from `--log-file`, or `None` for the default location.
    pub(crate) file_path: Option<&'a Path>,
}

/// Convert a configured level to a `log` filter.
pub(crate) fn filter_for(level: LogLevel) -> LevelFilter {
    match level {
        LogLevel::Off => LevelFilter::Off,
        LogLevel::Error => LevelFilter::Error,
        LogLevel::Warn => LevelFilter::Warn,
        LogLevel::Info => LevelFilter::Info,
        LogLevel::Debug => LevelFilter::Debug,
        LogLevel::Trace => LevelFilter::Trace,
    }
}

/// The default log file: `$XDG_STATE_HOME/doe/doe.log`, falling back to
/// `~/.local/state/doe/doe.log`, or the local data directory on Windows.
pub(crate) fn default_log_path() -> Option<PathBuf> {
    let strategy = etcetera::choose_base_strategy().ok()?;
    let base = strategy.state_dir().unwrap_or_else(|| strategy.data_dir());
    Some(base.join("doe").join("doe.log"))
}

/// Wrap `stream` so ANSI sequences follow `choice` (and `NO_COLOR` and TTY
/// detection when the choice is `auto`).
pub(crate) fn color_stream<S: anstream::stream::RawStream>(
    stream: S,
    choice: ColorChoice,
) -> AutoStream<S> {
    let choice = match choice {
        ColorChoice::Auto => anstream::ColorChoice::Auto,
        ColorChoice::Always => anstream::ColorChoice::Always,
        ColorChoice::Never => anstream::ColorChoice::Never,
    };
    AutoStream::new(stream, choice)
}

fn label_style(level: Level) -> Style {
    let color = match level {
        Level::Error => AnsiColor::Red,
        Level::Warn => AnsiColor::Yellow,
        Level::Info => AnsiColor::Green,
        Level::Debug | Level::Trace => AnsiColor::Cyan,
    };
    Style::new().fg_color(Some(color.into())).bold()
}

fn label(level: Level) -> &'static str {
    match level {
        Level::Error => "error",
        Level::Warn => "warning",
        Level::Info => "info",
        Level::Debug => "debug",
        Level::Trace => "trace",
    }
}

/// Install the global logger.
///
/// Failure to open the log file is reported as a warning and does not stop the
/// program: logging to a file is a convenience, not part of the command.
///
/// # Errors
///
/// Returns an error if a global logger is already installed.
pub(crate) fn init(settings: &LogSettings<'_>) -> Result<(), log::SetLoggerError> {
    let stderr_dispatch = fern::Dispatch::new()
        .level(settings.stderr_level)
        .format(|out, message, record| {
            let style = label_style(record.level());
            out.finish(format_args!(
                "{style}{}{style:#}: {message}",
                label(record.level())
            ));
        })
        .chain(Box::new(color_stream(io::stderr(), settings.color)) as Box<dyn Write + Send>);

    let file_level = settings
        .file_level_flag
        .or(settings.file_level_config)
        .unwrap_or(DEFAULT_FILE_LEVEL);
    let mut warning = None;
    let file_dispatch = settings
        .file_path
        .map(Path::to_owned)
        .or_else(default_log_path)
        .and_then(|path| match open_log_file(&path) {
            Ok(file) => Some(
                fern::Dispatch::new()
                    .level(filter_for(file_level))
                    .format(|out, message, record| {
                        out.finish(format_args!(
                            "{} {:<5} {} {message}",
                            humantime::format_rfc3339_seconds(SystemTime::now()),
                            record.level(),
                            record.target()
                        ));
                    })
                    .chain(file),
            ),
            Err(err) => {
                warning = Some(format!("could not open log file {}: {err}", path.display()));
                None
            }
        });

    let mut root = fern::Dispatch::new().chain(stderr_dispatch);
    if let Some(file_dispatch) = file_dispatch {
        root = root.chain(file_dispatch);
    }
    root.apply()?;
    INSTALLED.store(true, Ordering::Relaxed);
    if let Some(message) = warning {
        log::warn!("{message}");
    }
    Ok(())
}

fn open_log_file(path: &Path) -> io::Result<std::fs::File> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}
