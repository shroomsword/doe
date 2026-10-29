//! `--list-types`: the types that can be used, for people and scripts.

use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use doe::config::{Config, discover_types, resolve_include_paths, unreadable_schemas};
use doe::detect::{magic_of, source_of};
use doe::inspect::inspect;
use doe::schema::compiler::Compiler;
use serde_json::{Value, json};

use crate::cli::Cli;

/// One line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    /// The id to parse with; none for a file that could not be read.
    pub(crate) id: Option<String>,
    /// The file: relative to its type path, or the schema itself for the types of a schema.
    pub(crate) path: PathBuf,
    /// The type path it was found in.
    pub(crate) type_path: Option<PathBuf>,
    /// The first line of its `doc`.
    pub(crate) doc: Option<String>,
    /// Whether it starts with a signature (only worked out for `--json`).
    pub(crate) signature: bool,
    /// Why it cannot be used.
    pub(crate) error: Option<String>,
}

/// Print the list. The schema, when `--list-types SCHEMA` gives one, is in `cli.file`.
pub(crate) fn run(cli: &Cli, config: &Config) -> anyhow::Result<()> {
    let json_output = cli.json || cli.pretty;
    let include_paths = resolve_include_paths(&cli.include_paths, config);
    let items = match &cli.file {
        Some(schema) => of_schema(schema, &include_paths, json_output)?,
        None => of_type_paths(&include_paths, json_output)?,
    };
    let text = if cli.pretty {
        let mut text = serde_json::to_string_pretty(&to_json(&items))?;
        text.push('\n');
        text
    } else if cli.json {
        let mut text = serde_json::to_string(&to_json(&items))?;
        text.push('\n');
        text
    } else {
        render(&items, std::io::stdout().is_terminal())
    };
    crate::commands::write_stdout(text.as_bytes())
}

/// The types of the type paths, and the files in them that could not be read.
pub(crate) fn of_type_paths(
    include_paths: &[PathBuf],
    with_signature: bool,
) -> anyhow::Result<Vec<Item>> {
    for dir in include_paths {
        anyhow::ensure!(
            dir.is_dir(),
            "the type path {} is not a directory",
            dir.display()
        );
    }
    let types = discover_types(include_paths).map_err(|duplicates| {
        let details: Vec<String> = duplicates.iter().map(ToString::to_string).collect();
        anyhow::anyhow!(
            "duplicate type ids found in include paths\n\n{}",
            details.join("\n\n")
        )
    })?;
    let mut items: Vec<Item> = types
        .iter()
        .map(|available| {
            let type_path = include_paths
                .iter()
                .find(|dir| available.source.starts_with(dir))
                .cloned();
            let path = type_path
                .as_ref()
                .and_then(|dir| available.source.strip_prefix(dir).ok())
                .unwrap_or(&available.source)
                .to_path_buf();
            let (signature, error) = if with_signature {
                let (source, error) = source_of(include_paths, available);
                (!source.magic.is_empty(), error)
            } else {
                (false, None)
            };
            Item {
                id: Some(available.id.clone()),
                path,
                type_path,
                doc: available
                    .doc
                    .as_deref()
                    .and_then(|doc| doc.lines().map(str::trim).find(|l| !l.is_empty()))
                    .map(str::to_owned),
                signature,
                error,
            }
        })
        .collect();
    for bad in unreadable_schemas(include_paths) {
        let type_path = include_paths
            .iter()
            .find(|dir| bad.path.starts_with(dir))
            .cloned();
        let path = type_path
            .as_ref()
            .and_then(|dir| bad.path.strip_prefix(dir).ok())
            .unwrap_or(&bad.path)
            .to_path_buf();
        items.push(Item {
            id: None,
            path,
            type_path,
            doc: None,
            signature: false,
            error: Some(bad.message),
        });
    }
    sort(&mut items);
    Ok(items)
}

/// The types a schema defines and the ones that it imports define.
pub(crate) fn of_schema(
    schema: &Path,
    include_paths: &[PathBuf],
    with_signature: bool,
) -> anyhow::Result<Vec<Item>> {
    let compiler = Compiler::new(include_paths.to_vec());
    let path = if schema.is_file() {
        schema.to_path_buf()
    } else {
        compiler
            .find_import(&schema.to_string_lossy(), None)
            .with_context(|| format!("could not find the schema {}", schema.display()))?
    };
    let found = inspect(&compiler, &path);
    let mut items = Vec::new();
    for info in &found.schemas {
        let registry = if with_signature && info.error.is_none() {
            let mut fresh = Compiler::new(include_paths.to_vec());
            fresh
                .load(&info.path.to_string_lossy(), None)
                .ok()
                .map(|()| fresh.registry)
        } else {
            None
        };
        if let Some(error) = &info.error {
            items.push(Item {
                id: None,
                path: info.path.clone(),
                type_path: None,
                doc: None,
                signature: false,
                error: Some(error.to_string()),
            });
        }
        for name in &info.types {
            items.push(Item {
                id: Some(name.clone()),
                path: info.path.clone(),
                type_path: None,
                doc: None,
                signature: registry
                    .as_ref()
                    .is_some_and(|r| !magic_of(r, name).is_empty()),
                error: None,
            });
        }
    }
    for missing in &found.missing {
        items.push(Item {
            id: None,
            path: missing.from.clone(),
            type_path: None,
            doc: None,
            signature: false,
            error: Some(format!("{}: {}", missing.name, missing.error)),
        });
    }
    sort(&mut items);
    Ok(items)
}

fn sort(items: &mut [Item]) {
    items.sort_by(|a, b| (&a.id, &a.path).cmp(&(&b.id, &b.path)));
}

fn to_json(items: &[Item]) -> Value {
    Value::Array(
        items
            .iter()
            .map(|item| {
                json!({
                    "id": item.id,
                    "path": item.path,
                    "type_path": item.type_path,
                    "doc": item.doc,
                    "signature": item.signature,
                    "error": item.error,
                })
            })
            .collect(),
    )
}

/// One type per line: the id, the file and the doc (or the error). Aligned
/// for a terminal; tabs between the columns otherwise, for scripts.
pub(crate) fn render(items: &[Item], aligned: bool) -> String {
    let rows: Vec<[String; 3]> = items
        .iter()
        .map(|item| {
            let last = match (&item.error, &item.doc) {
                (Some(error), _) => format!("error: {}", one_line(error)),
                (None, Some(doc)) => one_line(doc),
                (None, None) => String::new(),
            };
            [
                item.id.clone().unwrap_or_else(|| "-".to_owned()),
                item.path.display().to_string(),
                last,
            ]
        })
        .collect();
    let mut out = String::new();
    if aligned {
        let width = |column: usize| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|cell| cell.chars().count())
                .max()
                .unwrap_or(0)
        };
        let (first, second) = (width(0), width(1));
        for [id, path, last] in &rows {
            let line = format!("{id:<first$}  {path:<second$}  {last}");
            out.push_str(line.trim_end());
            out.push('\n');
        }
    } else {
        for [id, path, last] in &rows {
            out.push_str(id);
            out.push('\t');
            out.push_str(path);
            out.push('\t');
            out.push_str(last);
            out.push('\n');
        }
    }
    out
}

/// Text on one line, so a tab or a new line in a doc does not break the columns.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: Option<&str>, path: &str, doc: Option<&str>, error: Option<&str>) -> Item {
        Item {
            id: id.map(str::to_owned),
            path: PathBuf::from(path),
            type_path: None,
            doc: doc.map(str::to_owned),
            signature: false,
            error: error.map(str::to_owned),
        }
    }

    #[test]
    fn on_a_terminal_the_columns_are_aligned_and_in_a_pipe_they_are_tabs() {
        let items = [
            item(Some("a"), "a.yaml", Some("First"), None),
            item(Some("longer"), "sub/b.ksy", None, None),
            item(None, "bad.yaml", None, Some("it is\nbroken")),
        ];
        assert_eq!(
            render(&items, true),
            "a       a.yaml     First\nlonger  sub/b.ksy\n-       bad.yaml   error: it is broken\n"
        );
        assert_eq!(
            render(&items, false),
            "a\ta.yaml\tFirst\nlonger\tsub/b.ksy\t\n-\tbad.yaml\terror: it is broken\n"
        );
    }

    #[test]
    fn the_list_is_sorted_by_id_then_path_and_a_file_without_an_id_goes_first() {
        let mut items = vec![
            item(Some("b"), "x.yaml", None, None),
            item(Some("a"), "z.yaml", None, None),
            item(Some("a"), "y.yaml", None, None),
            item(None, "bad.yaml", None, None),
        ];
        sort(&mut items);
        let order: Vec<_> = items.iter().map(|i| i.path.display().to_string()).collect();
        assert_eq!(order, ["bad.yaml", "y.yaml", "z.yaml", "x.yaml"]);
    }
}
