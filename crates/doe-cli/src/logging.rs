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
                            rfc3339_seconds(SystemTime::now()),
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

/// A time as UTC to the second, as the log file writes it: `2026-11-18T04:16:02Z`. A time before 1970 is written as 1970.
fn rfc3339_seconds(time: SystemTime) -> String {
    let seconds = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    // the civil date of a day count (Howard Hinnant's algorithm, with days counted from 0000-03-01)
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod time_tests {
    use std::time::{Duration, SystemTime};

    use super::rfc3339_seconds;

    fn at(seconds: u64) -> String {
        rfc3339_seconds(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
    }

    #[test]
    fn a_time_is_written_in_utc_to_the_second() {
        // checked with `date -u -d @N`
        assert_eq!(at(0), "1970-01-01T00:00:00Z");
        assert_eq!(at(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(at(1_000_000_000), "2001-09-09T01:46:40Z");
        assert_eq!(at(1_709_164_800), "2024-02-29T00:00:00Z");
        assert_eq!(at(4_102_444_800), "2100-01-01T00:00:00Z");
        assert_eq!(at(253_402_300_799), "9999-12-31T23:59:59Z");
    }

    #[test]
    fn a_time_before_1970_is_written_as_1970() {
        let before = SystemTime::UNIX_EPOCH - Duration::from_secs(5);
        assert_eq!(rfc3339_seconds(before), "1970-01-01T00:00:00Z");
    }
}
