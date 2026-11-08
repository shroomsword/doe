//! The descriptions that a schema gives to its fields: the `doc:` key of a field or an instance.
//!
//! A viewer shows a field's description next to the field. The trace has a node for every field of every element, so it
//! does not carry the text of each one: the descriptions are a small table, by the fully-qualified name of the type
//! (the `type` of a trace node) and the name of the field, made for the types that the trace has. A node's description is
//! the one of its name in the table of the type of its parent; the elements of a repeated field (`[0]`, `[1]`) have none,
//! the field has it once.

use std::collections::{BTreeMap, HashSet};

use crate::parser::trace::TraceNode;
use crate::schema::ir::{InstanceKind, TypeRegistry};

/// The descriptions of the fields: by the name of the type, then by the name of the field or instance. Only those that have
/// one are in it.
pub type FieldDocs = BTreeMap<String, BTreeMap<String, String>>;

/// The descriptions of the fields of the types that `trace` has.
#[must_use]
pub fn field_docs(registry: &TypeRegistry, trace: &TraceNode) -> FieldDocs {
    // the distinct types of the trace, found without recursion: a trace can be deep
    let mut used: HashSet<&str> = HashSet::new();
    let mut stack = vec![trace];
    while let Some(node) = stack.pop() {
        if let Some(name) = node.type_name.as_deref() {
            used.insert(name);
        }
        stack.extend(node.children.iter());
    }
    let mut out = FieldDocs::new();
    for name in used {
        let Some(ty) = registry.types.get(name) else {
            continue;
        };
        let mut docs = BTreeMap::new();
        let mut add = |id: &str, doc: Option<&str>| {
            if let Some(doc) = doc.map(str::trim).filter(|d| !d.is_empty()) {
                docs.insert(id.to_owned(), doc.to_owned());
            }
        };
        for field in &ty.fields {
            add(&field.id, field.doc.as_deref());
        }
        for instance in &ty.instances {
            match &instance.kind {
                InstanceKind::Read(field) => add(&instance.id, field.doc.as_deref()),
                InstanceKind::Value { doc, .. } => add(&instance.id, doc.as_deref()),
            }
        }
        if !docs.is_empty() {
            out.insert(name.to_owned(), docs);
        }
    }
    out
}

/// The description of a node: `parent_type` is the type of the node that holds it, `name` its name.
#[must_use]
pub fn doc_of<'a>(docs: &'a FieldDocs, parent_type: Option<&str>, name: &str) -> Option<&'a str> {
    docs.get(parent_type?)?.get(name).map(String::as_str)
}

/// The first line of a description, for the row of a field: up to the first blank line or line end, trimmed.
#[must_use]
pub fn first_line(doc: &str) -> &str {
    doc.trim().lines().next().unwrap_or("").trim()
}
