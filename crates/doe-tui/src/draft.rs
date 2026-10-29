//! The schema that is being built in the viewer (`doe --tui FILE --build OUT`):
//! a list of fields that are read one after the other from the first byte of the
//! file, and the YAML that they come to.
//!
//! This is the model, with no terminal: the keys of the viewer change a
//! [`Draft`] with the methods below, each of which gives back the [`Edit`] that was
//! made so that it can be undone, and the viewer parses the file again with
//! [`Draft::to_yaml`].

use std::fmt::Write as _;

/// The most characters of a name.
pub const MAX_NAME: usize = 64;

/// The byte order of the integers and floats of the whole draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endian {
    /// Least significant byte first.
    Little,
    /// Most significant byte first.
    Big,
}

impl Endian {
    /// How the schema spells it (`meta.endian`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Endian::Little => "le",
            Endian::Big => "be",
        }
    }

    /// The other one.
    #[must_use]
    pub fn flipped(self) -> Endian {
        match self {
            Endian::Little => Endian::Big,
            Endian::Big => Endian::Little,
        }
    }
}

/// What a field is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// An unsigned integer of 1, 2, 4 or 8 bytes.
    Unsigned(u8),
    /// A signed integer of 1, 2, 4 or 8 bytes.
    Signed(u8),
    /// A float of 4 or 8 bytes.
    Float(u8),
    /// Bytes that are kept as they are.
    Bytes(u64),
    /// Text of this many bytes, in this encoding.
    Text {
        /// How many bytes.
        len: u64,
        /// The encoding (`ASCII`, `UTF-8`...).
        encoding: String,
    },
}

impl Kind {
    /// How many bytes the field takes.
    #[must_use]
    pub fn size(&self) -> u64 {
        match self {
            Kind::Unsigned(n) | Kind::Signed(n) | Kind::Float(n) => u64::from(*n),
            Kind::Bytes(len) | Kind::Text { len, .. } => *len,
        }
    }

    /// A short name for a list: `u2`, `f8`, `bytes`, `str`.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Kind::Unsigned(n) => format!("u{n}"),
            Kind::Signed(n) => format!("s{n}"),
            Kind::Float(n) => format!("f{n}"),
            Kind::Bytes(_) => "bytes".to_owned(),
            Kind::Text { .. } => "str".to_owned(),
        }
    }

    /// Whether the kind is one that the draft can write.
    ///
    /// # Errors
    ///
    /// What is wrong with the size or the encoding.
    pub fn check(&self) -> Result<(), DraftError> {
        match self {
            Kind::Unsigned(n) | Kind::Signed(n) if !matches!(n, 1 | 2 | 4 | 8) => Err(
                DraftError::Size(format!("an integer is 1, 2, 4 or 8 bytes, not {n}")),
            ),
            Kind::Float(n) if !matches!(n, 4 | 8) => Err(DraftError::Size(format!(
                "a float is 4 or 8 bytes, not {n}"
            ))),
            Kind::Bytes(0) | Kind::Text { len: 0, .. } => {
                Err(DraftError::Size("a field has at least one byte".to_owned()))
            }
            Kind::Text { encoding, .. }
                if encoding.is_empty()
                    || !encoding
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') =>
            {
                Err(DraftError::Encoding(encoding.clone()))
            }
            _ => Ok(()),
        }
    }
}

/// One field of the schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// Its name in the schema.
    pub id: String,
    /// What it is.
    pub kind: Kind,
}

/// Why the draft cannot be changed as asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftError {
    /// The name is not one a schema can have.
    Name(String),
    /// Another field has the name.
    Duplicate(String),
    /// The size does not fit the kind.
    Size(String),
    /// The encoding is not written like one.
    Encoding(String),
    /// There is no field at this place.
    NoSuchField(usize),
}

impl std::fmt::Display for DraftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DraftError::Duplicate(name) => write!(f, "there is a field called {name} already"),
            DraftError::Name(why) | DraftError::Size(why) => f.write_str(why),
            DraftError::Encoding(name) => write!(f, "{name:?} is not the name of an encoding"),
            DraftError::NoSuchField(index) => write!(f, "there is no field number {}", index + 1),
        }
    }
}

impl std::error::Error for DraftError {}

/// Whether `name` can be the id of a field or of the schema: lower case letters,
/// digits and `_`, starting with a letter.
///
/// # Errors
///
/// What is wrong with it.
pub fn check_name(name: &str) -> Result<(), DraftError> {
    let bad = |why: &str| Err(DraftError::Name(format!("{name:?}: {why}")));
    let mut chars = name.chars();
    match chars.next() {
        None => return Err(DraftError::Name("a name cannot be empty".to_owned())),
        Some(c) if !c.is_ascii_lowercase() => {
            return bad("a name starts with a lower case letter");
        }
        Some(_) => {}
    }
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return bad("a name has lower case letters, digits and _ only");
    }
    if name.len() > MAX_NAME {
        return bad("a name has at most 64 characters");
    }
    Ok(())
}

/// A change that was made to the draft, with what is needed to take it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// A field was put in at this place.
    Insert {
        /// Where.
        index: usize,
        /// The field.
        field: Field,
    },
    /// A field was taken out from this place.
    Remove {
        /// Where it was.
        index: usize,
        /// The field.
        field: Field,
    },
    /// A field was renamed.
    Rename {
        /// Which.
        index: usize,
        /// The name before.
        from: String,
        /// The name after.
        to: String,
    },
    /// A field was given another kind.
    Retype {
        /// Which.
        index: usize,
        /// The kind before.
        from: Kind,
        /// The kind after.
        to: Kind,
    },
    /// The byte order of the draft was changed.
    ByteOrder {
        /// Before.
        from: Endian,
        /// After.
        to: Endian,
    },
}

impl Edit {
    /// The change that takes this one back.
    #[must_use]
    pub fn inverse(&self) -> Edit {
        match self.clone() {
            Edit::Insert { index, field } => Edit::Remove { index, field },
            Edit::Remove { index, field } => Edit::Insert { index, field },
            Edit::Rename { index, from, to } => Edit::Rename {
                index,
                from: to,
                to: from,
            },
            Edit::Retype { index, from, to } => Edit::Retype {
                index,
                from: to,
                to: from,
            },
            Edit::ByteOrder { from, to } => Edit::ByteOrder { from: to, to: from },
        }
    }
}

/// The schema that is being built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    id: String,
    endian: Endian,
    fields: Vec<Field>,
}

impl Draft {
    /// An empty schema called `id`, with little-endian integers.
    ///
    /// # Errors
    ///
    /// The id is not a name a schema can have.
    pub fn new(id: &str) -> Result<Draft, DraftError> {
        check_name(id)?;
        Ok(Draft {
            id: id.to_owned(),
            endian: Endian::Little,
            fields: Vec::new(),
        })
    }

    /// The name of the schema.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The byte order.
    #[must_use]
    pub fn endian(&self) -> Endian {
        self.endian
    }

    /// The fields, in the order they are read.
    #[must_use]
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// Where the field at `index` starts, in bytes from the start of the file.
    #[must_use]
    pub fn offset_of(&self, index: usize) -> u64 {
        self.fields.iter().take(index).map(|f| f.kind.size()).sum()
    }

    /// Where the schema ends: the offset of the next field.
    #[must_use]
    pub fn end(&self) -> u64 {
        self.offset_of(self.fields.len())
    }

    /// Put a field at the end.
    ///
    /// # Errors
    ///
    /// The name or the kind is not right, or the name is taken.
    pub fn push(&mut self, field: Field) -> Result<Edit, DraftError> {
        self.insert(self.fields.len(), field)
    }

    /// Put a field in at `index` (the fields from there on move down).
    ///
    /// # Errors
    ///
    /// As [`push`](Self::push), or `index` is past the end.
    pub fn insert(&mut self, index: usize, field: Field) -> Result<Edit, DraftError> {
        if index > self.fields.len() {
            return Err(DraftError::NoSuchField(index));
        }
        self.check_new_name(&field.id, None)?;
        field.kind.check()?;
        let edit = Edit::Insert {
            index,
            field: field.clone(),
        };
        self.fields.insert(index, field);
        Ok(edit)
    }

    /// Take the field at `index` out.
    ///
    /// # Errors
    ///
    /// There is no such field.
    pub fn remove(&mut self, index: usize) -> Result<Edit, DraftError> {
        if index >= self.fields.len() {
            return Err(DraftError::NoSuchField(index));
        }
        let field = self.fields.remove(index);
        Ok(Edit::Remove { index, field })
    }

    /// Give the field at `index` another name.
    ///
    /// # Errors
    ///
    /// There is no such field, or the name is not right or is taken.
    pub fn rename(&mut self, index: usize, to: &str) -> Result<Edit, DraftError> {
        self.check_new_name(to, Some(index))?;
        let field = self
            .fields
            .get_mut(index)
            .ok_or(DraftError::NoSuchField(index))?;
        let from = std::mem::replace(&mut field.id, to.to_owned());
        Ok(Edit::Rename {
            index,
            from,
            to: to.to_owned(),
        })
    }

    /// Give the field at `index` another kind.
    ///
    /// # Errors
    ///
    /// There is no such field, or the kind is not right.
    pub fn retype(&mut self, index: usize, to: Kind) -> Result<Edit, DraftError> {
        to.check()?;
        let field = self
            .fields
            .get_mut(index)
            .ok_or(DraftError::NoSuchField(index))?;
        let from = std::mem::replace(&mut field.kind, to.clone());
        Ok(Edit::Retype { index, from, to })
    }

    /// Change the byte order of the draft.
    pub fn set_endian(&mut self, to: Endian) -> Edit {
        let from = std::mem::replace(&mut self.endian, to);
        Edit::ByteOrder { from, to }
    }

    /// Do (again) a change that was made before, or take one back with
    /// [`Edit::inverse`].
    ///
    /// # Errors
    ///
    /// The change does not fit the draft as it is now.
    pub fn apply(&mut self, edit: &Edit) -> Result<Edit, DraftError> {
        match edit {
            Edit::Insert { index, field } => self.insert(*index, field.clone()),
            Edit::Remove { index, .. } => self.remove(*index),
            Edit::Rename { index, to, .. } => self.rename(*index, to),
            Edit::Retype { index, to, .. } => self.retype(*index, to.clone()),
            Edit::ByteOrder { to, .. } => Ok(self.set_endian(*to)),
        }
    }

    /// A name that a field can have: right, and not another field's.
    fn check_new_name(&self, name: &str, except: Option<usize>) -> Result<(), DraftError> {
        check_name(name)?;
        if self
            .fields
            .iter()
            .enumerate()
            .any(|(i, f)| f.id == name && Some(i) != except)
        {
            return Err(DraftError::Duplicate(name.to_owned()));
        }
        Ok(())
    }

    /// The schema as text, which `doe` compiles. Each field has a comment with
    /// its place in the file.
    #[must_use]
    pub fn to_yaml(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "meta:");
        let _ = writeln!(out, "  id: {}", self.id);
        let _ = writeln!(out, "  endian: {}", self.endian.name());
        if self.fields.is_empty() {
            out.push_str("seq: []\n");
            return out;
        }
        out.push_str("seq:\n");
        let mut offset = 0_u64;
        for field in &self.fields {
            let size = field.kind.size();
            let _ = writeln!(out, "  # {offset:#x}..{:#x}", offset + size);
            let _ = writeln!(out, "  - id: {}", field.id);
            match &field.kind {
                Kind::Unsigned(n) => {
                    let _ = writeln!(out, "    type: u{n}");
                }
                Kind::Signed(n) => {
                    let _ = writeln!(out, "    type: s{n}");
                }
                Kind::Float(n) => {
                    let _ = writeln!(out, "    type: f{n}");
                }
                Kind::Bytes(len) => {
                    let _ = writeln!(out, "    size: {len}");
                }
                Kind::Text { len, encoding } => {
                    let _ = writeln!(out, "    type: str");
                    let _ = writeln!(out, "    size: {len}");
                    let _ = writeln!(out, "    encoding: {encoding}");
                }
            }
            offset += size;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "Panicking is permitted in tests (rust-practices)."
    )]

    use super::*;
    use doe::Value;
    use doe::parser::cursor::Cursor;
    use doe::parser::engine::Engine;
    use doe::schema::compiler::compile_schema;
    use doe::schema::raw::RawSchema;

    fn field(id: &str, kind: Kind) -> Field {
        Field {
            id: id.to_owned(),
            kind,
        }
    }

    fn text(len: u64) -> Kind {
        Kind::Text {
            len,
            encoding: "ASCII".to_owned(),
        }
    }

    /// Parse `data` with the draft, the way the viewer does.
    fn parse(draft: &Draft, data: &[u8]) -> Value {
        let schema = RawSchema::from_yaml_str(&draft.to_yaml()).unwrap();
        let registry = compile_schema(&schema).unwrap();
        Engine::new(&registry)
            .parse_type(&schema.id, &mut Cursor::new(data))
            .unwrap()
    }

    fn get<'a>(value: &'a Value, name: &str) -> &'a Value {
        let Value::Struct { fields, .. } = value else {
            panic!("not a struct: {value:?}")
        };
        fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v)
            .unwrap()
    }

    #[test]
    fn every_kind_is_written_as_a_schema_that_reads_a_file_with_the_sizes_it_says() {
        let mut draft = Draft::new("sample").unwrap();
        for (id, kind) in [
            ("a", Kind::Unsigned(1)),
            ("b", Kind::Unsigned(2)),
            ("c", Kind::Signed(4)),
            ("d", Kind::Unsigned(8)),
            ("e", Kind::Float(4)),
            ("f", Kind::Float(8)),
            ("g", Kind::Bytes(3)),
            ("h", text(2)),
            ("i", Kind::Signed(1)),
            ("j", Kind::Signed(2)),
            ("k", Kind::Signed(8)),
        ] {
            draft.push(field(id, kind)).unwrap();
        }
        assert_eq!(draft.end(), 1 + 2 + 4 + 8 + 4 + 8 + 3 + 2 + 1 + 2 + 8);
        let mut data = vec![7, 0x34, 0x12, 0xfe, 0xff, 0xff, 0xff];
        data.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
        data.extend_from_slice(&1.5_f32.to_le_bytes());
        data.extend_from_slice(&2.5_f64.to_le_bytes());
        data.extend_from_slice(&[9, 8, 7]);
        data.extend_from_slice(b"hi");
        data.push(0xff);
        data.extend_from_slice(&[0xfe, 0xff]);
        data.extend_from_slice(&[2, 0, 0, 0, 0, 0, 0, 0]);
        let value = parse(&draft, &data);
        assert_eq!(get(&value, "a"), &Value::UInt(7));
        assert_eq!(get(&value, "b"), &Value::UInt(0x1234));
        assert_eq!(get(&value, "c"), &Value::SInt(-2));
        assert_eq!(get(&value, "d"), &Value::UInt(1));
        assert_eq!(get(&value, "e"), &Value::Float(1.5));
        assert_eq!(get(&value, "f"), &Value::Float(2.5));
        assert_eq!(get(&value, "g"), &Value::Bytes(vec![9, 8, 7]));
        assert_eq!(get(&value, "h"), &Value::Str("hi".to_owned()));
        assert_eq!(get(&value, "i"), &Value::SInt(-1));
        assert_eq!(get(&value, "j"), &Value::SInt(-2));
        assert_eq!(get(&value, "k"), &Value::SInt(2));
    }

    #[test]
    fn the_byte_order_is_the_drafts() {
        let mut draft = Draft::new("t").unwrap();
        draft.push(field("n", Kind::Unsigned(2))).unwrap();
        assert_eq!(get(&parse(&draft, &[1, 2]), "n"), &Value::UInt(0x0201));
        draft.set_endian(Endian::Big);
        assert_eq!(draft.endian().name(), "be");
        assert_eq!(get(&parse(&draft, &[1, 2]), "n"), &Value::UInt(0x0102));
        assert_eq!(Endian::Big.flipped(), Endian::Little);
    }

    #[test]
    fn an_empty_draft_is_a_schema_and_the_fields_are_consecutive() {
        let mut draft = Draft::new("empty").unwrap();
        assert_eq!(
            draft.to_yaml(),
            "meta:\n  id: empty\n  endian: le\nseq: []\n"
        );
        parse(&draft, &[]);
        assert_eq!(draft.end(), 0);
        draft.push(field("a", Kind::Unsigned(2))).unwrap();
        draft.push(field("b", Kind::Bytes(5))).unwrap();
        draft.push(field("c", Kind::Unsigned(1))).unwrap();
        assert_eq!(
            [
                draft.offset_of(0),
                draft.offset_of(1),
                draft.offset_of(2),
                draft.end()
            ],
            [0, 2, 7, 8]
        );
        let yaml = draft.to_yaml();
        assert!(
            yaml.contains("# 0x0..0x2") && yaml.contains("# 0x2..0x7"),
            "{yaml}"
        );
    }

    #[test]
    fn names_and_sizes_are_checked() {
        let mut draft = Draft::new("t").unwrap();
        for bad in ["", "A", "1a", "a-b", "a b", &"a".repeat(65)] {
            assert!(
                matches!(check_name(bad), Err(DraftError::Name(_))),
                "{bad:?}"
            );
            assert!(draft.push(field(bad, Kind::Unsigned(1))).is_err());
        }
        assert!(Draft::new("Bad").is_err());
        draft.push(field("a", Kind::Unsigned(1))).unwrap();
        assert_eq!(
            draft.push(field("a", Kind::Unsigned(1))),
            Err(DraftError::Duplicate("a".to_owned()))
        );
        for bad in [
            Kind::Unsigned(3),
            Kind::Signed(0),
            Kind::Float(2),
            Kind::Bytes(0),
            text(0),
            Kind::Text {
                len: 1,
                encoding: "no good".to_owned(),
            },
            Kind::Text {
                len: 1,
                encoding: String::new(),
            },
        ] {
            assert!(draft.push(field("x", bad.clone())).is_err(), "{bad:?}");
        }
        assert_eq!(
            draft.fields().len(),
            1,
            "nothing was added by a refused push"
        );
        assert_eq!(draft.remove(5), Err(DraftError::NoSuchField(5)));
        assert!(draft.rename(0, "a").is_ok(), "a field may keep its name");
    }

    #[test]
    fn every_edit_can_be_taken_back_and_done_again() {
        let mut draft = Draft::new("t").unwrap();
        let start = draft.clone();
        let mut done = vec![
            draft.push(field("a", Kind::Unsigned(1))).unwrap(),
            draft.push(field("b", Kind::Bytes(4))).unwrap(),
            draft.insert(1, field("c", Kind::Float(4))).unwrap(),
        ];
        done.push(draft.rename(0, "first").unwrap());
        done.push(draft.retype(2, Kind::Unsigned(8)).unwrap());
        done.push(draft.set_endian(Endian::Big));
        done.push(draft.remove(1).unwrap());
        let finished = draft.clone();
        assert_eq!(
            draft
                .fields()
                .iter()
                .map(|f| f.id.as_str())
                .collect::<Vec<_>>(),
            ["first", "b"]
        );
        for edit in done.iter().rev() {
            draft.apply(&edit.inverse()).unwrap();
        }
        assert_eq!(draft, start);
        for edit in &done {
            draft.apply(edit).unwrap();
        }
        assert_eq!(draft, finished);
    }
}
