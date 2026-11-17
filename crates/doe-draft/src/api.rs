//! The builder as one call over JSON, for a page: the text of a schema, one change to it, and the bytes of the file go in;
//! the new text and where each field is come out.
//!
//! There is no state between calls: the text is the schema. A text that the builder wrote is read back with
//! [`Draft::from_yaml`], changed, written again and checked against the file, so a person can edit the text by hand
//! between two calls; a text that the builder did not write is refused, with the line it does not understand, and is
//! never rewritten.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use doe::parser::cursor::Cursor;
use doe::parser::engine::Engine;
use doe::schema::compiler::Compiler;
use doe::schema::source::MemorySource;
use serde::{Deserialize, Serialize};

use crate::draft::{Container, Draft, Endian, Field, Kind};
use crate::field_dialog::{self, Answers};

/// The id of a schema that is begun from nothing.
const NEW_ID: &str = "built";

/// Where the schema is for the compiler.
const SCHEMA_PATH: &str = "built.ksy";

/// What a page asks for.
#[derive(Debug, Deserialize)]
pub struct Request {
    /// The text of the schema: what the builder wrote, or nothing (blank) to begin.
    pub text: String,
    /// The id of the schema when the text is blank.
    #[serde(default)]
    pub id: Option<String>,
    /// The schemas that the draft imports (a type of the library), by the name it imports them with.
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    /// The change to make; none to only read the text.
    #[serde(default)]
    pub op: Option<Op>,
}

/// What a field is, as a dialog collects it: the kind as it is typed (`u4`, `bytes`, `str`, `strz`, `magic`, `b3`,
/// `type`) and what that kind needs.
#[derive(Debug, Default, Deserialize)]
pub struct KindSpec {
    /// The kind as it is typed.
    pub kind: String,
    /// The length of `bytes` and `str`.
    #[serde(default)]
    pub length: Option<u64>,
    /// The encoding of `str` and `strz` (`ASCII` when none).
    #[serde(default)]
    pub encoding: Option<String>,
    /// The bytes of `magic`.
    #[serde(default)]
    pub magic: Vec<u8>,
    /// The byte that ends `strz`.
    #[serde(default)]
    pub terminator: u8,
    /// The size of the window that a `type` field is read from.
    #[serde(default)]
    pub size: Option<u64>,
    /// The id of the type of a `type` field.
    #[serde(default)]
    pub type_id: Option<String>,
    /// How often it is repeated, as typed: a number, the name of an earlier integer field, or `eos`; nothing for once.
    #[serde(default)]
    pub repeat: Option<String>,
}

/// One change to the schema.
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Op {
    /// Put a field in (at the end when there is no `index`).
    Add {
        /// The type to put it in; the schema itself when none.
        #[serde(default)]
        at: Option<String>,
        /// Where; the end when none.
        #[serde(default)]
        index: Option<usize>,
        /// Its name; one is proposed when none.
        #[serde(default)]
        name: Option<String>,
        /// What it is.
        #[serde(flatten)]
        spec: KindSpec,
    },
    /// Take a field out.
    Remove {
        /// The type, or none for the schema.
        #[serde(default)]
        at: Option<String>,
        /// Which field.
        index: usize,
    },
    /// Give a field another name.
    Rename {
        /// The type, or none for the schema.
        #[serde(default)]
        at: Option<String>,
        /// Which field.
        index: usize,
        /// The new name.
        to: String,
    },
    /// Give a field another kind.
    Retype {
        /// The type, or none for the schema.
        #[serde(default)]
        at: Option<String>,
        /// Which field.
        index: usize,
        /// What it is now.
        #[serde(flatten)]
        spec: KindSpec,
    },
    /// Set the byte order of the schema.
    Endian {
        /// `le` or `be`.
        to: String,
    },
    /// Make the fields from `start` a new type, with a field of that type where they were.
    Group {
        /// The type the fields are in, or none for the schema.
        #[serde(default)]
        at: Option<String>,
        /// The first field.
        start: usize,
        /// How many.
        count: usize,
        /// The id of the new type.
        type_id: String,
        /// The name of the field that has it.
        field_id: String,
    },
    /// Put the fields of a field's type back in its place.
    Ungroup {
        /// The type the field is in, or none for the schema.
        #[serde(default)]
        at: Option<String>,
        /// The field that has the type.
        start: usize,
    },
    /// Say what a dialog should begin with for a selection of this many bytes (and for this kind, once it is chosen).
    Propose {
        /// How many bytes are selected.
        #[serde(default)]
        selection: Option<u64>,
        /// The kind that was chosen, if one was.
        #[serde(default)]
        kind: Option<String>,
        /// The id of the type, for a field of a type.
        #[serde(default)]
        type_id: Option<String>,
    },
}

/// Where a field is.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Slot {
    /// Its name.
    pub id: String,
    /// What it is, short (`u2`, `bytes`, `str[]`, the id of a type).
    pub kind: String,
    /// Where it starts, in bits from the start of the schema (or of the type).
    pub start_bit: u64,
    /// Where it ends.
    pub end_bit: u64,
}

/// The fields of a type of the schema.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct TypeSlots {
    /// The id of the type.
    pub id: String,
    /// Its fields.
    pub fields: Vec<Slot>,
}

/// The schema as the builder has it.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Layout {
    /// The id of the schema.
    pub id: String,
    /// `le` or `be`.
    pub endian: &'static str,
    /// The schemas that it imports.
    pub imports: Vec<String>,
    /// Its fields.
    pub fields: Vec<Slot>,
    /// Its types.
    pub types: Vec<TypeSlots>,
}

/// What a dialog should begin with.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Proposal {
    /// The kind: the integer of the size of the selection, `bytes` for another size, or the one chosen.
    pub kind: String,
    /// A name that no field has.
    pub name: String,
}

/// What the call answers.
#[derive(Debug, Serialize)]
pub struct Response {
    /// Whether the change was made (or none was asked) and the text is the builder's.
    pub ok: bool,
    /// Whether the text is one that the builder wrote and can go on with.
    pub buildable: bool,
    /// The schema as text: the new one, or the one that was given when nothing was changed.
    pub text: String,
    /// Why nothing was changed, or why the text is not the builder's (with its line).
    pub error: Option<String>,
    /// Where each field is, when the text is the builder's.
    pub layout: Option<Layout>,
    /// What a dialog should begin with, for `propose`.
    pub proposal: Option<Proposal>,
}

/// Do what a request says (JSON), against the bytes of the file, and answer in JSON.
#[must_use]
pub fn draft(request: &str, bytes: &[u8]) -> String {
    let response = match serde_json::from_str::<Request>(request) {
        Ok(request) => run(&request, bytes),
        Err(e) => Response {
            ok: false,
            buildable: false,
            text: String::new(),
            error: Some(format!("the request is not valid: {e}")),
            layout: None,
            proposal: None,
        },
    };
    serde_json::to_string(&response).unwrap_or_else(|e| {
        format!(r#"{{"ok":false,"buildable":false,"text":"","error":"{e}","layout":null,"proposal":null}}"#)
    })
}

/// [`draft`] without the JSON.
#[must_use]
pub fn run(request: &Request, bytes: &[u8]) -> Response {
    let blank = request.text.trim().is_empty();
    let mut draft = if blank {
        match Draft::new(request.id.as_deref().unwrap_or(NEW_ID)) {
            Ok(draft) => draft,
            Err(e) => return refused(&request.text, e.to_string()),
        }
    } else {
        match Draft::from_yaml(&request.text) {
            Ok(draft) => draft,
            Err(why) => {
                return Response {
                    ok: false,
                    buildable: false,
                    text: request.text.clone(),
                    error: Some(why),
                    layout: None,
                    proposal: None,
                };
            }
        }
    };
    // the lengths that depend on the file; the file not fitting is said, but does not stop the text from being read
    let fitted = fit(&mut draft, &request.files, bytes);
    let before = draft.clone();
    let Some(op) = &request.op else {
        return Response {
            ok: fitted.is_ok(),
            buildable: true,
            text: if blank {
                draft.to_yaml()
            } else {
                request.text.clone()
            },
            error: fitted.err(),
            layout: Some(layout(&draft)),
            proposal: None,
        };
    };
    if let Op::Propose {
        selection,
        kind,
        type_id,
    } = op
    {
        let kind = kind
            .clone()
            .unwrap_or_else(|| field_dialog::propose_kind(*selection));
        let base = field_dialog::name_base(&kind, type_id.as_deref().unwrap_or_default());
        let name = field_dialog::propose_name(&draft, &base);
        return Response {
            ok: true,
            buildable: true,
            text: request.text.clone(),
            error: None,
            layout: Some(layout(&draft)),
            proposal: Some(Proposal { kind, name }),
        };
    }
    // a change that makes a file that fitted not fit is refused; when it did not fit before, a change is how to fix it
    let changed =
        apply(&mut draft, op).and_then(|()| match fit(&mut draft, &request.files, bytes) {
            Err(why) if fitted.is_ok() => Err(why),
            other => Ok(other.err()),
        });
    match changed {
        Ok(note) => Response {
            ok: true,
            buildable: true,
            text: draft.to_yaml(),
            error: note,
            layout: Some(layout(&draft)),
            proposal: None,
        },
        Err(why) => Response {
            ok: false,
            buildable: true,
            text: if blank {
                before.to_yaml()
            } else {
                request.text.clone()
            },
            error: Some(why),
            layout: Some(layout(&before)),
            proposal: None,
        },
    }
}

fn refused(text: &str, why: String) -> Response {
    Response {
        ok: false,
        buildable: false,
        text: text.to_owned(),
        error: Some(why),
        layout: None,
        proposal: None,
    }
}

fn container(at: Option<&String>) -> Container {
    at.map_or(Container::Top, |id| Container::Type(id.clone()))
}

/// The kind that a dialog's answers make; what depends on the file is set by [`fit`] after the change.
fn kind_of(spec: &KindSpec) -> Result<Kind, String> {
    let answers = Answers {
        length: spec.length,
        encoding: spec
            .encoding
            .clone()
            .unwrap_or_else(|| field_dialog::DEFAULT_ENCODING.to_owned()),
        magic: spec.magic.clone(),
        terminator: spec.terminator,
        text_sample: 1,
        type_id: spec.type_id.clone().unwrap_or_default(),
        size: spec.size,
    };
    let base = field_dialog::kind_of(&spec.kind, &answers)?;
    match field_dialog::parse_repeat(spec.repeat.as_deref().unwrap_or_default())? {
        None => Ok(base),
        Some(count) => Ok(Kind::Repeat {
            inner: Box::new(base),
            count,
            sample: 1,
        }),
    }
}

fn apply(draft: &mut Draft, op: &Op) -> Result<(), String> {
    let done = match op {
        Op::Add {
            at,
            index,
            name,
            spec,
        } => {
            let at = container(at.as_ref());
            let kind = kind_of(spec)?;
            let id = name.clone().unwrap_or_else(|| {
                field_dialog::propose_name(
                    draft,
                    &field_dialog::name_base(
                        &spec.kind,
                        spec.type_id.as_deref().unwrap_or_default(),
                    ),
                )
            });
            let index = index.unwrap_or_else(|| draft.fields_in(&at).len());
            draft.insert_in(&at, index, Field { id, kind })
        }
        Op::Remove { at, index } => draft.remove_in(&container(at.as_ref()), *index),
        Op::Rename { at, index, to } => draft.rename_in(&container(at.as_ref()), *index, to),
        Op::Retype { at, index, spec } => {
            draft.retype_in(&container(at.as_ref()), *index, kind_of(spec)?)
        }
        Op::Endian { to } => {
            let to = match to.as_str() {
                "le" => Endian::Little,
                "be" => Endian::Big,
                other => return Err(format!("{other:?} is not a byte order: le or be")),
            };
            draft.set_endian(to);
            return Ok(());
        }
        Op::Group {
            at,
            start,
            count,
            type_id,
            field_id,
        } => draft.group_in(&container(at.as_ref()), *start, *count, type_id, field_id),
        Op::Ungroup { at, start } => draft.ungroup_in(&container(at.as_ref()), *start),
        Op::Propose { .. } => return Ok(()),
    };
    done.map(|_| ()).map_err(|e| e.to_string())
}

/// Parse the file with the draft as it is, and set the lengths that the file decides. Fails when the schema does not
/// compile or the file does not fit it.
fn fit(draft: &mut Draft, files: &BTreeMap<String, String>, bytes: &[u8]) -> Result<(), String> {
    let mut source = MemorySource::new();
    for (path, text) in files {
        source.insert(path, text.as_str());
    }
    let text = draft.to_yaml();
    source.insert(SCHEMA_PATH, text.as_str());
    let mut compiler = Compiler::with_source(Vec::<PathBuf>::new(), Box::new(source));
    let schema = compiler
        .read_schema(Path::new(SCHEMA_PATH))
        .map_err(|e| e.to_string())?;
    compiler
        .process_schema(&schema)
        .map_err(|e| e.to_string())?;
    let mut cursor = Cursor::new(bytes);
    let outcome = Engine::new(&compiler.registry)
        .parse_type_partial(&schema.id, &mut cursor)
        .map_err(|e| e.to_string())?;
    if let Some(error) = &outcome.error {
        return Err(format!("the file does not fit the schema: {error}"));
    }
    draft.fit_to_trace(&outcome.trace);
    Ok(())
}

fn slots(draft: &Draft, at: &Container) -> Vec<Slot> {
    draft
        .fields_in(at)
        .iter()
        .zip(draft.layout_in(at))
        .map(|(field, (start_bit, end_bit))| Slot {
            id: field.id.clone(),
            kind: field.kind.label(),
            start_bit,
            end_bit,
        })
        .collect()
}

fn layout(draft: &Draft) -> Layout {
    Layout {
        id: draft.id().to_owned(),
        endian: draft.endian().name(),
        imports: draft.imports(),
        fields: slots(draft, &Container::Top),
        types: draft
            .types()
            .iter()
            .map(|def| TypeSlots {
                id: def.id.clone(),
                fields: slots(draft, &Container::Type(def.id.clone())),
            })
            .collect(),
    }
}
