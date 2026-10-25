//! Which schemas a schema is made of: the file itself and the files it imports
//! (and those import), the imports that could not be found, and the types they
//! define, any of which can be parsed with.
//!
//! This reads the files but does not compile them, so it also says what there
//! is when the schema does not compile; a file that cannot be read (a mistake in
//! its YAML, say) is in the list with its error.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::error::DoeError;
use crate::schema::compiler::Compiler;
use crate::schema::raw::{RawSchema, RawTypeDecl};

/// One schema file.
#[derive(Debug)]
pub struct SchemaEntry {
    /// Where the file is, as imports found it.
    pub path: PathBuf,
    /// Its id, when the file could be read.
    pub id: Option<String>,
    /// The imports it names, as it writes them.
    pub imports: Vec<String>,
    /// The types it defines, as `parse` takes them: the id of the schema, and
    /// `id::name` for the types in `types:` (`id::name::inner` for those in them).
    pub types: Vec<String>,
    /// Why the file could not be read, if it could not.
    pub error: Option<DoeError>,
}

/// An import that was not found.
#[derive(Debug)]
pub struct MissingImport {
    /// The name that was asked for, as the importing file writes it.
    pub name: String,
    /// The file that asked for it.
    pub from: PathBuf,
    /// What was searched and where.
    pub error: DoeError,
}

/// The schema files of a schema.
#[derive(Debug, Default)]
pub struct Inspection {
    /// The main schema first, then the files it imports in the order they are
    /// first named, each once.
    pub schemas: Vec<SchemaEntry>,
    /// The imports that could not be found.
    pub missing: Vec<MissingImport>,
}

fn nested(prefix: &str, types: &indexmap::IndexMap<String, RawTypeDecl>, out: &mut Vec<String>) {
    for (name, decl) in types {
        let fqn = format!("{prefix}::{name}");
        out.push(fqn.clone());
        nested(&fqn, &decl.types, out);
    }
}

/// The types of a schema that was read.
#[must_use]
pub fn type_names(schema: &RawSchema) -> Vec<String> {
    let mut out = vec![schema.id.clone()];
    nested(&schema.id, &schema.types, &mut out);
    out
}

/// A path without its `.` parts and with the `..` parts applied, so that a
/// file that is reached in two ways is one entry.
fn normalized(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir
                if out
                    .components()
                    .next_back()
                    .is_some_and(|last| matches!(last, Component::Normal(_))) =>
            {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// List the schema at `main` and everything it imports, found the way
/// compiling would find them (`compiler` has the files and the include paths).
#[must_use]
pub fn inspect(compiler: &Compiler, main: &Path) -> Inspection {
    let mut result = Inspection::default();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut queue: std::collections::VecDeque<PathBuf> = std::collections::VecDeque::new();
    seen.insert(main.to_path_buf());
    queue.push_back(main.to_path_buf());
    while let Some(path) = queue.pop_front() {
        match compiler.read_schema(&path) {
            Ok(schema) => {
                let imports: Vec<String> = schema
                    .imports
                    .iter()
                    .chain(&schema.meta.imports)
                    .cloned()
                    .collect();
                for import in &imports {
                    match compiler.find_import(import, path.parent()) {
                        Ok(found) => {
                            let found = normalized(&found);
                            if seen.insert(found.clone()) {
                                queue.push_back(found);
                            }
                        }
                        Err(error) => result.missing.push(MissingImport {
                            name: import.clone(),
                            from: path.clone(),
                            error,
                        }),
                    }
                }
                result.schemas.push(SchemaEntry {
                    id: (!schema.id.is_empty()).then(|| schema.id.clone()),
                    types: if schema.id.is_empty() {
                        Vec::new()
                    } else {
                        type_names(&schema)
                    },
                    imports,
                    path,
                    error: None,
                });
            }
            Err(error) => result.schemas.push(SchemaEntry {
                path,
                id: None,
                imports: Vec::new(),
                types: Vec::new(),
                error: Some(error),
            }),
        }
    }
    result
}
