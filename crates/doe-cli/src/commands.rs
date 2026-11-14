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

/// What a viewer opens for a FILE whose schema is looked for: the file itself, or, when it holds files (a `.tar.gz`),
/// the layer that is wanted (the innermost by default, `--layer N`, `--no-unwrap`) written to a temporary file named as
/// the layer would be (`x.tar`), so that the viewers, which read a file by its path, show it. The guard removes the
/// file when it is dropped.
#[cfg(any(feature = "tui", feature = "ide"))]
struct Viewed {
    /// The file to open.
    path: PathBuf,
    /// The type that fits it, when it was looked for.
    type_name: Option<String>,
    /// The directory of the temporary file, removed when the viewer is left.
    _guard: Option<tempfile::TempDir>,
}

#[cfg(any(feature = "tui", feature = "ide"))]
fn viewed(
    cli: &Cli,
    config: &Config,
    file: &Path,
    find_type: bool,
    given: Option<&str>,
) -> anyhow::Result<Viewed> {
    use std::io::Write as _;

    let own = |type_name: Option<String>| Viewed {
        path: file.to_owned(),
        type_name,
        _guard: None,
    };
    let layers = detect_layers(cli, config, file, given)?;
    for note in &layers.notes {
        log::info!("{note}");
    }
    let Some(inner) = layers.layers.last() else {
        // nothing fits: the error says what was tried (the viewer cannot look for a type of its own)
        return if find_type {
            Ok(own(Some(detect_type(cli, config, file)?)))
        } else {
            Ok(own(None))
        };
    };
    if layers.layers.len() == 1 {
        return Ok(own(Some(inner.type_name.clone())));
    }
    let Some(data) = layers.data.last() else {
        return Ok(own(Some(inner.type_name.clone())));
    };
    let mut name = file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("layer")
        .to_owned();
    for _ in 1..layers.layers.len() {
        name = doe::layers::payload_name(&name);
    }
    let dir = tempfile::Builder::new().prefix("doe-layer-").tempdir()?;
    let path = dir
        .path()
        .join(if name.is_empty() { "layer" } else { &name });
    std::fs::File::create(&path)?.write_all(data)?;
    let _ = writeln!(
        io::stderr(),
        "{} is {}: showing the last layer, from a temporary file {} (--no-unwrap shows the outer layer only)",
        file.display(),
        layers.describe(),
        path.display()
    );
    Ok(Viewed {
        path,
        type_name: Some(inner.type_name.clone()),
        _guard: Some(dir),
    })
}

/// Whether `given` names a type of the type paths (not a schema file, and not `SCHEMA::TYPE`).
#[cfg(feature = "tui")]
fn is_library_type(given: &str) -> bool {
    !doe::is_schema_path(given) && !given.contains("::")
}

#[cfg(feature = "ide")]
fn ide(cli: &Cli, config: &Config) -> anyhow::Result<()> {
    let paths = include_paths(&cli.include_paths, config);
    // a file that holds files is offered as the layer that is wanted
    let viewed = cli
        .file
        .as_deref()
        .map(|file| viewed(cli, config, file, false, None))
        .transpose()?;
    let offered = viewed.as_ref().map(|v| v.path.as_path());
    if !crate::ide::BUNDLED {
        log::warn!(
            "this build of doe has no web IDE in it (it was built without web/dist): the page will say so"
        );
    }
    let config_path = cli.config.clone().or_else(Config::default_path);
    let cli_paths = cli.include_paths.clone();
    let reading = {
        let (config_path, cli_paths) = (config_path.clone(), cli_paths.clone());
        move || -> anyhow::Result<Vec<PathBuf>> {
            let config = Config::load_or_default(config_path.as_deref())?;
            Ok(include_paths(&cli_paths, &config))
        }
    };
    let local = crate::ide_api::Local::new(&paths, offered)?.reading_from(reading.clone());
    // The libraries follow the files: the include paths, and the configuration.
    if let Err(why) = crate::ide_watch::start(local.libraries(), config_path, Box::new(reading)) {
        log::warn!("{why}: use Reload in the page");
        local.libraries().set_unwatched(Some(why));
    }
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
    let mut viewed_file = None;
    let type_or_schema = match (&cli.type_or_schema, &cli.build) {
        // a type of the library that holds another file (`gzip_file`) is shown as what it holds, as when it is found
        (Some(given), None) if is_library_type(given) => {
            let found = viewed(cli, config, file, false, Some(given))?;
            let type_name = found.type_name.clone().unwrap_or_else(|| given.clone());
            viewed_file = Some(found);
            type_name
        }
        (Some(given), _) => given.clone(),
        // the schema is the one that is being built
        (None, Some(_)) => String::new(),
        (None, None) => {
            let found = viewed(cli, config, file, true, None)?;
            let type_name = found.type_name.clone().unwrap_or_default();
            viewed_file = Some(found);
            type_name
        }
    };
    let outer_file = viewed_file
        .as_ref()
        .filter(|v| v.path != *file)
        .map(|_| file.clone());
    let file: &Path = viewed_file
        .as_ref()
        .map_or(file.as_path(), |v| v.path.as_path());
    // `--color never`, and `NO_COLOR` unless `--color always`, mean no colour.
    let color = match cli.color {
        clap::ColorChoice::Always => true,
        clap::ColorChoice::Never => false,
        clap::ColorChoice::Auto => std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()),
    };
    let options = doe_tui::Options {
        file: file.to_owned(),
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
        config_file: cli.config.clone().or_else(Config::default_path),
        outer_file,
        theme_on_command_line: cli.theme.is_some(),
        transparent: transparent_setting(cli, config),
        mouse: !cli.no_mouse && config.mouse.unwrap_or(true),
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

/// The `transparent` setting of the viewer: `--transparent` or `--no-transparent`, else the config file's key. None
/// leaves it to the theme (a theme file may say `transparent = true`).
#[cfg(feature = "tui")]
pub(crate) fn transparent_setting(cli: &Cli, config: &Config) -> Option<bool> {
    if cli.transparent {
        Some(true)
    } else if cli.no_transparent {
        Some(false)
    } else {
        config.transparent
    }
}

/// When a parse fails, the types of the library whose signature the file starts with, if it is not the type that was
/// asked for: a `.pcapng` file that is a libpcap capture, for example.
fn other_start_hint(cli: &Cli, config: &Config, file: &Path, tried: &str) -> Option<String> {
    use std::io::Read as _;

    let mut head = Vec::new();
    std::fs::File::open(file)
        .ok()?
        .take(doe::detect::HEAD_BYTES as u64)
        .read_to_end(&mut head)
        .ok()?;
    let library = doe::detect::Library::load(&include_paths(&cli.include_paths, config)).ok()?;
    let names: Vec<String> = library
        .sources()
        .into_iter()
        .filter(|s| s.name != tried && s.starts_like(&head).is_some())
        .map(|s| s.name)
        .collect();
    let first = names.first()?;
    Some(format!(
        "  the file starts like {} (a signature of the type), not like {tried}: try `doe {} {first}`",
        names
            .iter()
            .map(|n| format!("`{n}`"))
            .collect::<Vec<_>>()
            .join(", "),
        file.display()
    ))
}

/// The most bytes of a file that are read to unwrap it (what `process:` allows to be produced).
const UNWRAP_LIMIT: usize = 256 * 1024 * 1024;

/// The layers of the file: what fits it, and what it holds.
fn detect_layers(
    cli: &Cli,
    config: &Config,
    file: &Path,
    given: Option<&str>,
) -> anyhow::Result<doe::layers::Unwrapped> {
    let options = doe::detect::Options {
        all: cli.detect.as_deref() == Some("all"),
        ..doe::detect::Options::default()
    };
    let wanted = if cli.no_unwrap {
        1
    } else {
        cli.layer.map_or(doe::layers::MAX_DEPTH, usize::from)
    };
    doe::layers::unwrap_as(
        file,
        &include_paths(&cli.include_paths, config),
        &options,
        wanted,
        UNWRAP_LIMIT,
        given,
    )
    .with_context(|| format!("could not look for the schema of {}", file.display()))
}

/// Show a file that holds files: the innermost layer (or layer `--layer`) as text, and every layer as JSON.
fn run_layers(cli: &Cli, file: &Path, layers: &doe::layers::Unwrapped) -> anyhow::Result<()> {
    use std::io::Write as _;

    let count = layers.layers.len();
    if let Some(wanted) = cli.layer
        && usize::from(wanted) > count
    {
        anyhow::bail!(
            "{} has {count} {} ({}), not {wanted}",
            file.display(),
            if count == 1 { "layer" } else { "layers" },
            layers.describe()
        );
    }
    let _ = writeln!(
        io::stderr(),
        "{} is {} (--no-unwrap shows the outer layer only)",
        file.display(),
        layers.describe()
    );
    let parse = |index: usize, format: OutputFormat| -> anyhow::Result<doe::ParseReport> {
        let name = layers
            .layers
            .get(index)
            .map_or("", |l| l.type_name.as_str());
        log::info!(
            "parsing layer {} of {} as {name}",
            index + 1,
            file.display()
        );
        let options = doe::ParseOptions {
            instances: !cli.no_instances,
        };
        let report = match index.checked_sub(1).and_then(|i| layers.data.get(i)) {
            None => doe::parse_file_report(
                file,
                name,
                &cli.include_paths,
                cli.config.as_deref(),
                format,
                &options,
            ),
            Some(data) => doe::parse_bytes_report(
                data,
                name,
                &cli.include_paths,
                cli.config.as_deref(),
                format,
                &options,
            ),
        };
        report.with_context(|| {
            format!(
                "could not parse layer {} of {} as {name}",
                index + 1,
                file.display()
            )
        })
    };
    let json = cli.json || cli.pretty;
    let (output, problems) = if json && cli.layer.is_none() {
        // every layer, outermost first
        let mut parts = Vec::new();
        let mut problems = Vec::new();
        for (index, layer) in layers.layers.iter().enumerate() {
            let report = parse(index, OutputFormat::Json)?;
            parts.push(format!(
                "{{\"type\":{},\"value\":{}}}",
                serde_json::to_string(&layer.type_name)?,
                report.output
            ));
            problems.extend(report.text_problems);
        }
        let text = format!("{{\"layers\":[{}]}}", parts.join(","));
        let text = if cli.pretty {
            let value: serde_json::Value = serde_json::from_str(&text)?;
            serde_json::to_string_pretty(&value)?
        } else {
            text
        };
        (text, problems)
    } else {
        let index = cli.layer.map_or(count, usize::from) - 1;
        let format = if cli.pretty {
            OutputFormat::JsonPretty
        } else if cli.json {
            OutputFormat::Json
        } else {
            OutputFormat::Text {
                indent: cli.indent.clone(),
            }
        };
        let report = parse(index, format)?;
        (report.output, report.text_problems)
    };
    write_stdout(output.as_bytes())?;
    if problems.is_empty() {
        return Ok(());
    }
    let list: Vec<String> = problems.iter().map(|p| format!("  {p}")).collect();
    anyhow::bail!(
        "{} string{} in {} not valid in {} encoding:\n{}",
        problems.len(),
        if problems.len() == 1 { "" } else { "s" },
        file.display(),
        if problems.len() == 1 { "its" } else { "their" },
        list.join("\n")
    )
}

fn run(cli: &Cli, config: &Config) -> anyhow::Result<()> {
    let Some(file) = &cli.file else {
        // clap requires it unless `--help`, `--completions` or `--man` was given.
        anyhow::bail!("a FILE is required");
    };
    if cli.type_or_schema.is_none() {
        // the schema is looked for, and a file that holds another file is shown as the one it holds
        let layers = detect_layers(cli, config, file, None)?;
        for note in &layers.notes {
            log::info!("{note}");
        }
        if layers.layers.len() > 1 || cli.layer.is_some() {
            return run_layers(cli, file, &layers);
        }
    }
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
    let report = doe::parse_file_report(
        file,
        type_or_schema,
        &cli.include_paths,
        cli.config.as_deref(),
        format,
        &doe::ParseOptions {
            instances: !cli.no_instances,
        },
    )
    .with_context(|| {
        let mut message = format!("could not parse {} as {type_or_schema}", file.display());
        if let Some(hint) = other_start_hint(cli, config, file, type_or_schema) {
            message.push('\n');
            message.push_str(&hint);
        }
        message
    })?;

    write_stdout(report.output.as_bytes())?;
    if report.text_problems.is_empty() {
        return Ok(());
    }
    // The parse went on, and the output is whole: the strings that were not valid are the error, at the end.
    let list: Vec<String> = report
        .text_problems
        .iter()
        .map(|problem| format!("  {problem}"))
        .collect();
    let count = report.text_problems.len();
    anyhow::bail!(
        "{count} {} in {} {} not valid in {} encoding:\n{}",
        if count == 1 { "string" } else { "strings" },
        file.display(),
        if count == 1 { "is" } else { "are" },
        if count == 1 { "its" } else { "their" },
        list.join("\n")
    )
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

#[cfg(test)]
#[cfg(feature = "tui")]
mod transparent_tests {
    use clap::Parser;

    use super::transparent_setting;
    use crate::cli::Cli;
    use doe::config::Config;

    fn cli(args: &[&str]) -> Cli {
        let mut all = vec!["doe", "--tui"];
        all.extend_from_slice(args);
        all.push("file.bin");
        Cli::try_parse_from(all).unwrap()
    }

    fn config(transparent: Option<bool>) -> Config {
        Config {
            transparent,
            ..Config::default()
        }
    }

    #[test]
    fn the_flags_win_over_the_config_file_and_the_config_file_over_the_theme() {
        let unset = config(None);
        let on = config(Some(true));
        let off = config(Some(false));
        // nothing said: it is left to the theme
        assert_eq!(transparent_setting(&cli(&[]), &unset), None);
        // the config file
        assert_eq!(transparent_setting(&cli(&[]), &on), Some(true));
        assert_eq!(transparent_setting(&cli(&[]), &off), Some(false));
        // the flags
        assert_eq!(
            transparent_setting(&cli(&["--transparent"]), &unset),
            Some(true)
        );
        assert_eq!(
            transparent_setting(&cli(&["--transparent"]), &off),
            Some(true)
        );
        assert_eq!(
            transparent_setting(&cli(&["--no-transparent"]), &on),
            Some(false)
        );
        // the last of the two flags is the one that counts
        assert_eq!(
            transparent_setting(&cli(&["--transparent", "--no-transparent"]), &on),
            Some(false)
        );
        assert_eq!(
            transparent_setting(&cli(&["--no-transparent", "--transparent"]), &off),
            Some(true)
        );
    }

    #[test]
    fn the_flags_need_the_viewer() {
        assert!(Cli::try_parse_from(["doe", "--transparent", "file.bin"]).is_err());
        assert!(Cli::try_parse_from(["doe", "--no-transparent", "file.bin"]).is_err());
    }
}

#[cfg(test)]
#[cfg(any(feature = "tui", feature = "ide"))]
mod viewed_tests {
    use clap::Parser;

    use super::viewed;
    use crate::cli::Cli;
    use doe::config::Config;

    fn types() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../types")
    }

    fn cli(file: &std::path::Path, extra: &[&str]) -> Cli {
        let types = types();
        let mut all = vec![
            "doe",
            "-c",
            "/nonexistent/doe-config.yaml",
            "-I",
            types.to_str().unwrap(),
        ];
        all.extend_from_slice(extra);
        all.push(file.to_str().unwrap());
        Cli::try_parse_from(all).unwrap()
    }

    #[test]
    fn a_viewer_opens_the_innermost_layer_from_a_temporary_file_that_goes_away() {
        let file = types().join("samples/sample.tar.gz");
        let found = viewed(&cli(&file, &[]), &Config::default(), &file, true, None).unwrap();
        assert_eq!(found.type_name.as_deref(), Some("tar"));
        assert_eq!(found.path.file_name().unwrap(), "sample.tar");
        assert_eq!(
            std::fs::read(&found.path).unwrap(),
            std::fs::read(types().join("samples/sample.tar")).unwrap()
        );
        let dir = found.path.parent().unwrap().to_owned();
        drop(found);
        assert!(!dir.exists(), "the temporary file is removed");
    }

    #[test]
    fn the_outer_layer_is_opened_as_the_file_itself_with_no_unwrap_or_for_one_layer() {
        let file = types().join("samples/sample.tar.gz");
        let found = viewed(
            &cli(&file, &["--no-unwrap"]),
            &Config::default(),
            &file,
            true,
            None,
        )
        .unwrap();
        assert_eq!(found.path, file);
        assert_eq!(found.type_name.as_deref(), Some("gzip_file"));
        let plain = types().join("samples/sample.txt.gz");
        let found = viewed(&cli(&plain, &[]), &Config::default(), &plain, true, None).unwrap();
        assert_eq!(found.path, plain);
        assert_eq!(found.type_name.as_deref(), Some("gzip_file"));
    }
}
