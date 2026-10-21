//! Where in the text of a schema an error is.
//!
//! The errors of loading and compiling a schema name what is wrong (a field,
//! a type, an expression) but not where it is written. A [`SchemaMap`] is made
//! from the text of the schema and answers that: [`SchemaMap::locate`] gives
//! the [`Position`] of the field, type, instance, import or attribute that an
//! error is about. An editor can underline it.
//!
//! The map is a second reading of the text, with its own small set of types
//! that keep only the positions (the types of [`RawSchema`](super::raw::RawSchema)
//! use `flatten`, which loses them). It does not check anything, and a text it
//! cannot read gives no map. [`DoeError`] does not change: an error from a
//! schema that is not the one the map was made from, or about something the
//! map does not know, has no position.

use std::fmt;
use std::marker::PhantomData;

use indexmap::IndexMap;
use serde::Deserialize;
use serde::de::{MapAccess, Visitor};
use serde_saphyr::{Location, Spanned};

use crate::error::DoeError;

/// A place in the text of a schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Position {
    /// The line, from 1.
    pub line: u32,
    /// The column, in characters, from 1.
    pub column: u32,
    /// The byte offset of the start from the start of the text.
    pub offset: Option<usize>,
    /// The length in bytes of what is meant (the value, the name), when known.
    pub len: Option<usize>,
}

impl Position {
    fn from_location(location: &Location) -> Self {
        let span = location.span();
        Position {
            line: u32::try_from(location.line()).unwrap_or(u32::MAX),
            column: u32::try_from(location.column()).unwrap_or(u32::MAX),
            offset: span
                .byte_offset()
                .and_then(|offset| usize::try_from(offset).ok()),
            len: span.byte_len().and_then(|len| usize::try_from(len).ok()),
        }
    }
}

// ── What is read from the text ───────────────────────────────────────────────

/// A mapping read as a list of its entries, so that the keys can carry their
/// positions (a `Spanned` key cannot be the key of a map).
struct Pairs<K, V>(Vec<(K, V)>);

impl<K, V> Default for Pairs<K, V> {
    fn default() -> Self {
        Pairs(Vec::new())
    }
}

impl<'de, K: Deserialize<'de>, V: Deserialize<'de>> Deserialize<'de> for Pairs<K, V> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Entries<K, V>(PhantomData<(K, V)>);

        impl<'de, K: Deserialize<'de>, V: Deserialize<'de>> Visitor<'de> for Entries<K, V> {
            type Value = Pairs<K, V>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a mapping")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry::<K, V>()? {
                    entries.push(entry);
                }
                Ok(Pairs(entries))
            }
        }

        deserializer.deserialize_map(Entries(PhantomData))
    }
}

/// The attributes of a field or instance, each with the position of its value.
type Attrs = IndexMap<String, Spanned<serde_json::Value>>;

#[derive(Deserialize, Default)]
struct MetaText {
    id: Option<Spanned<String>>,
    #[serde(default)]
    imports: Vec<Spanned<String>>,
}

#[derive(Deserialize, Default)]
struct TypeText {
    #[serde(default)]
    seq: Vec<Attrs>,
    #[serde(default)]
    types: Pairs<Spanned<String>, TypeText>,
    #[serde(default)]
    instances: Pairs<Spanned<String>, Attrs>,
}

#[derive(Deserialize)]
struct SchemaText {
    id: Option<Spanned<String>>,
    #[serde(default)]
    meta: MetaText,
    #[serde(default)]
    imports: Vec<Spanned<String>>,
    #[serde(default)]
    seq: Vec<Attrs>,
    #[serde(default)]
    types: Pairs<Spanned<String>, TypeText>,
    #[serde(default)]
    instances: Pairs<Spanned<String>, Attrs>,
}

// ── The map ──────────────────────────────────────────────────────────────────

/// One attribute of a field or instance: where its value is, and its text when
/// it is a string or a number.
#[derive(Debug)]
struct Attr {
    position: Position,
    text: Option<String>,
}

/// A field of `seq` or an instance, with the position of its name.
#[derive(Debug)]
struct Item {
    name: String,
    position: Position,
    attrs: Vec<(String, Attr)>,
}

impl Item {
    fn from_attrs(name: String, position: Position, attrs: Attrs) -> Self {
        let attrs = attrs
            .into_iter()
            .map(|(key, value)| {
                let text = match &value.value {
                    serde_json::Value::String(text) => Some(text.clone()),
                    serde_json::Value::Number(number) => Some(number.to_string()),
                    serde_json::Value::Bool(flag) => Some(flag.to_string()),
                    _ => None,
                };
                let position = Position::from_location(&value.referenced);
                (key, Attr { position, text })
            })
            .collect();
        Item {
            name,
            position,
            attrs,
        }
    }

    /// The position of the attribute that holds `needle` in its text, if one
    /// does. The `id` is not an attribute for this: an error that mentions the
    /// name of a field is about the field.
    fn attr_with(&self, needle: &str) -> Option<Position> {
        self.attrs
            .iter()
            .filter(|(key, _)| key != "id")
            .filter(|(_, attr)| {
                attr.text
                    .as_deref()
                    .is_some_and(|text| text.contains(needle))
            })
            .max_by_key(|(_, attr)| attr.text.as_ref().map_or(0, String::len))
            .map(|(_, attr)| attr.position)
    }

    /// The position of the attribute called `key`.
    fn attr_named(&self, key: &str) -> Option<Position> {
        self.attrs
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, attr)| attr.position)
    }

    /// The attribute that an error message quotes: a key (`'value': ...`) or
    /// the text of a value (`size expression 'a + b' ...`).
    fn attr_quoted(&self, message: &str) -> Option<Position> {
        quotes(message).find_map(|text| self.attr_named(text).or_else(|| self.attr_with(text)))
    }

    /// The position of the attribute called `key` whose text is `value`.
    fn attr_is(&self, key: &str, value: &str) -> Option<Position> {
        self.attrs
            .iter()
            .find(|(name, attr)| name == key && attr.text.as_deref() == Some(value))
            .map(|(_, attr)| attr.position)
    }
}

/// A schema or a type declared in it: the fields and instances it has and the
/// types declared inside it.
#[derive(Debug)]
struct Scope {
    position: Position,
    fields: Vec<Item>,
    instances: Vec<Item>,
    types: Vec<(String, Scope)>,
}

impl Scope {
    fn new(position: Position, seq: Vec<Attrs>, instances: Pairs<Spanned<String>, Attrs>) -> Self {
        let fields = seq
            .into_iter()
            .enumerate()
            .map(|(index, attrs)| {
                // A field without an id is named by where it is in `seq`.
                let (name, position) = match attrs.get("id") {
                    Some(id) => (
                        match &id.value {
                            serde_json::Value::String(text) => text.clone(),
                            other => other.to_string(),
                        },
                        Position::from_location(&id.referenced),
                    ),
                    None => (format!("_unnamed{index}"), position),
                };
                Item::from_attrs(name, position, attrs)
            })
            .collect();
        let instances = instances
            .0
            .into_iter()
            .map(|(name, attrs)| {
                Item::from_attrs(name.value, Position::from_location(&name.referenced), attrs)
            })
            .collect();
        Scope {
            position,
            fields,
            instances,
            types: Vec::new(),
        }
    }

    fn from_type(position: Position, text: TypeText) -> Self {
        let mut scope = Scope::new(position, text.seq, text.instances);
        scope.types = declared(text.types);
        scope
    }

    fn item(&self, name: &str) -> Option<&Item> {
        self.fields
            .iter()
            .chain(&self.instances)
            .find(|item| item.name == name)
    }

    fn items(&self) -> impl Iterator<Item = &Item> {
        self.fields.iter().chain(&self.instances)
    }

    /// Every scope below this one, this one first.
    fn walk<'a>(&'a self, out: &mut Vec<&'a Scope>) {
        out.push(self);
        for (_, scope) in &self.types {
            scope.walk(out);
        }
    }
}

fn declared(types: Pairs<Spanned<String>, TypeText>) -> Vec<(String, Scope)> {
    types
        .0
        .into_iter()
        .map(|(name, text)| {
            let position = Position::from_location(&name.referenced);
            (name.value, Scope::from_type(position, text))
        })
        .collect()
}

/// Where the things that a schema declares are written in its text.
#[derive(Debug)]
pub struct SchemaMap {
    root_id: String,
    root: Scope,
    /// Where the id is written.
    id: Position,
    imports: Vec<(String, Position)>,
}

impl SchemaMap {
    /// Read the positions in the text of a schema. `None` when the text is not
    /// something that can be read as one (the error of reading it is the
    /// [`DoeError::Yaml`] that loading it gives).
    pub fn parse(text: &str) -> Option<SchemaMap> {
        let schema: SchemaText = serde_saphyr::from_str(text).ok()?;
        // The id is `id:` or, in a Kaitai Struct file, `meta.id:`.
        let id = schema.id.or(schema.meta.id)?;
        let id_position = Position::from_location(&id.referenced);
        let mut root = Scope::new(id_position, schema.seq, schema.instances);
        root.types = declared(schema.types);
        let imports = schema
            .imports
            .into_iter()
            .chain(schema.meta.imports)
            .map(|import| {
                let position = Position::from_location(&import.referenced);
                (import.value, position)
            })
            .collect();
        Some(SchemaMap {
            root_id: id.value,
            root,
            id: id_position,
            imports,
        })
    }

    /// The scope a fully-qualified type name (`schema::outer::inner`) names,
    /// if it is a type of this schema.
    fn scope(&self, fqn: &str) -> Option<&Scope> {
        let mut parts = fqn.split("::");
        if parts.next()? != self.root_id {
            return None;
        }
        let mut scope = &self.root;
        for name in parts {
            scope = &scope.types.iter().find(|(declared, _)| declared == name)?.1;
        }
        Some(scope)
    }

    /// Every scope of the schema.
    fn scopes(&self) -> Vec<&Scope> {
        let mut all = Vec::new();
        self.root.walk(&mut all);
        all
    }

    /// The position in the text of what `error` is about, if it is about this
    /// schema and the map knows where it is.
    ///
    /// - A field's or instance's error: its attribute, if the message quotes
    ///   one (`size` for a `size` expression), else its name.
    /// - An unknown type or enum: the `type:` or `enum:` that names it.
    /// - An expression that does not parse or evaluate: the attribute with
    ///   that expression.
    /// - A missing import: its entry in `imports`.
    /// - A duplicate type: the declaration, or the schema's id.
    pub fn locate(&self, error: &DoeError) -> Option<Position> {
        match error {
            DoeError::FieldError {
                field,
                context,
                message,
            } => {
                let scope = self.scope(context)?;
                // The error names the field it is about, or, for a reference
                // to a field that is not there yet, that other field: then the
                // expression in the message is looked for in the whole scope.
                let found = scope
                    .item(field)
                    .and_then(|item| item.attr_quoted(message))
                    .or_else(|| scope.items().find_map(|item| item.attr_quoted(message)));
                Some(
                    found
                        .or_else(|| scope.item(field).map(|item| item.position))
                        .unwrap_or(scope.position),
                )
            }
            DoeError::UnknownType { type_name, context } => {
                self.with_attr("type", type_name, context)
            }
            DoeError::UnknownEnum { enum_name, context } => {
                self.with_attr("enum", enum_name, context)
            }
            DoeError::ExprParse { expr, .. } | DoeError::ExprEval { expr, .. } => {
                let expr = expr.trim_end_matches('…');
                self.scopes()
                    .into_iter()
                    .flat_map(Scope::items)
                    .find_map(|item| item.attr_with(expr))
            }
            DoeError::SchemaNotFound { name, .. } => self
                .imports
                .iter()
                .find(|(import, _)| import == name)
                .map(|(_, position)| *position),
            DoeError::DuplicateType { name, .. } => {
                if *name == self.root_id {
                    return Some(self.id);
                }
                self.scopes()
                    .into_iter()
                    .flat_map(|scope| scope.types.iter())
                    .find(|(declared, _)| declared == name)
                    .map(|(_, scope)| scope.position)
            }
            DoeError::Yaml { source, .. } => source.location().map(|l| Position::from_location(&l)),
            _ => None,
        }
    }

    /// The `key:` with the value `value` of the field `context` names: a type
    /// and a field (`schema::outer.field`), as the errors give it. The field,
    /// or its type, when it has no such attribute.
    fn with_attr(&self, key: &str, value: &str, context: &str) -> Option<Position> {
        let (scope_name, field) = context
            .rsplit_once('.')
            .map_or((context, None), |(scope, field)| (scope, Some(field)));
        let scope = self.scope(scope_name)?;
        let item = field.and_then(|field| scope.item(field));
        Some(
            item.and_then(|item| item.attr_is(key, value))
                .or_else(|| scope.items().find_map(|item| item.attr_is(key, value)))
                .or_else(|| item.map(|item| item.position))
                .unwrap_or(scope.position),
        )
    }
}

/// The texts between single quotes in `message`, in order, which is how the
/// errors quote the key, the expression or the name they are about.
fn quotes(message: &str) -> impl Iterator<Item = &str> {
    message
        .split('\'')
        .skip(1)
        .step_by(2)
        .filter(|text| !text.is_empty())
}

/// The position of `error` in `text`, the text of the schema that was being
/// loaded. A YAML syntax error has its own position; any other is looked for
/// in a [`SchemaMap`] of the text.
pub fn locate_error(text: &str, error: &DoeError) -> Option<Position> {
    if let DoeError::Yaml { source, .. } = error {
        return source.location().map(|l| Position::from_location(&l));
    }
    SchemaMap::parse(text)?.locate(error)
}
