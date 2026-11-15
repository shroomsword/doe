//! The language of a schema, as the compiler defines it: the keys that go at each place, and what they and the built-in
//! types mean. An editor offers them while a schema is written.

use super::raw::RawSchema;

/// A place in a schema where keys are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Place {
    /// The top of the schema.
    Schema,
    /// `meta:`.
    Meta,
    /// An item of `seq:`.
    Field,
    /// An entry of `instances:`.
    Instance,
    /// An entry of `types:`.
    Type,
    /// The mapping that `type:` has to choose a type by a value (`switch-on`, `cases`).
    Switch,
    /// `payload:`.
    Payload,
    /// `filesystem:`.
    Filesystem,
    /// An item of `params:`.
    Param,
    /// `valid:`.
    Valid,
}

impl Place {
    /// Every place.
    pub const ALL: [Place; 10] = [
        Place::Schema,
        Place::Meta,
        Place::Field,
        Place::Instance,
        Place::Type,
        Place::Switch,
        Place::Payload,
        Place::Filesystem,
        Place::Param,
        Place::Valid,
    ];

    /// The name of the place, as the page asks for it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Place::Schema => "schema",
            Place::Meta => "meta",
            Place::Field => "field",
            Place::Instance => "instance",
            Place::Type => "type",
            Place::Switch => "switch",
            Place::Payload => "payload",
            Place::Filesystem => "filesystem",
            Place::Param => "param",
            Place::Valid => "valid",
        }
    }

    /// A schema with one key that the parser does not know, at this place.
    fn probe(self) -> &'static str {
        match self {
            Place::Schema => "bogus: 1\n",
            Place::Meta => "meta:\n  bogus: 1\n",
            Place::Field => "seq:\n  - id: a\n    bogus: 1\n",
            Place::Instance => "instances:\n  x:\n    bogus: 1\n",
            Place::Type => "types:\n  t:\n    bogus: 1\n",
            Place::Switch => "seq:\n  - id: a\n    type:\n      bogus: 1\n",
            Place::Payload => "payload:\n  bogus: 1\n",
            Place::Filesystem => "filesystem:\n  bogus: 1\n",
            Place::Param => "params:\n  - id: p\n    bogus: 1\n",
            Place::Valid => "seq:\n  - id: a\n    valid:\n      bogus: 1\n",
        }
    }
}

/// The keys that the parser accepts at `place`, in the order it lists them, found by asking it: a schema with a key that
/// it does not know is refused with a message that names the ones it does. Empty if the parser does not say.
#[must_use]
pub fn accepted_keys(place: Place) -> Vec<String> {
    let Err(error) = RawSchema::from_yaml_str(place.probe()) else {
        return Vec::new();
    };
    let text = error.to_string();
    let Some((_, rest)) = text.split_once("expected one of ") else {
        return Vec::new();
    };
    let line = rest.lines().next().unwrap_or_default();
    line.split(", ").map(str::to_owned).collect()
}

/// Whether a key is not for people to write: Kaitai Struct's own (`ks-*`, `doc-ref`), and the internal ones that start
/// with `-`. The parser accepts them; an editor does not offer them.
#[must_use]
pub fn is_hidden(key: &str) -> bool {
    key.starts_with('-') || key.starts_with("ks-") || key == "doc-ref"
}

/// What the keys at each place are for, one line each. A test keeps the table in step with the parser.
const KEYS: &[(Place, &str, &str)] = &[
    (
        Place::Schema,
        "id",
        "The name of the schema, and of its main type.",
    ),
    (Place::Schema, "doc", "What the format is, in words."),
    (
        Place::Schema,
        "meta",
        "The settings of the format: the byte order, the encoding, the imports.",
    ),
    (
        Place::Schema,
        "filesystem",
        "What a file of the format holds as a file system: where the entries are, and what their fields are called.",
    ),
    (
        Place::Schema,
        "payload",
        "What is inside a file of the format is another file, after a transform (gzip, zlib, …).",
    ),
    (
        Place::Schema,
        "imports",
        "Other schemas to use the types of, by name.",
    ),
    (Place::Schema, "params", "The parameters of the main type."),
    (
        Place::Schema,
        "seq",
        "The fields of the main type, in the order they are read.",
    ),
    (
        Place::Schema,
        "types",
        "The types that the fields can have, by name.",
    ),
    (
        Place::Schema,
        "instances",
        "Values at a place of the file, or computed: not in the order of the file.",
    ),
    (
        Place::Schema,
        "enums",
        "The names of the numbers that a field can have.",
    ),
    (Place::Meta, "id", "The name of the type."),
    (
        Place::Meta,
        "imports",
        "Other schemas to use the types of, by name (the same as the top-level imports).",
    ),
    (
        Place::Meta,
        "encoding",
        "The encoding of the strings that do not give one.",
    ),
    (
        Place::Meta,
        "endian",
        "The byte order of the numbers: le or be (or a choice by a value).",
    ),
    (
        Place::Meta,
        "bit-endian",
        "The order of the bits of a bit field: be takes the most significant bits of a byte first (the default), le the least.",
    ),
    (Place::Meta, "title", "A title for the format (not used)."),
    (
        Place::Meta,
        "application",
        "The applications that use the format (not used).",
    ),
    (
        Place::Meta,
        "file-extension",
        "The file extensions of the format (not used).",
    ),
    (
        Place::Meta,
        "xref",
        "Cross-references to other documentation (not used).",
    ),
    (
        Place::Meta,
        "license",
        "The license of the definition (not used).",
    ),
    (Place::Meta, "tags", "Tags for the format (not used)."),
    (
        Place::Type,
        "to-string",
        "An expression for the text that stands for a value of the type.",
    ),
    (Place::Type, "doc", "What the type is, in words."),
    (
        Place::Type,
        "meta",
        "The settings of the type: the byte order, the encoding.",
    ),
    (
        Place::Type,
        "params",
        "The parameters of the type, which a field that uses it gives.",
    ),
    (
        Place::Type,
        "seq",
        "The fields of the type, in the order they are read.",
    ),
    (
        Place::Type,
        "instances",
        "Values of the type at a place of the file, or computed.",
    ),
    (Place::Type, "types", "Types that only this type uses."),
    (
        Place::Type,
        "enums",
        "The names of numbers that fields of the type can have.",
    ),
    (
        Place::Switch,
        "switch-on",
        "The expression whose value chooses the type.",
    ),
    (
        Place::Switch,
        "cases",
        "For each value, the type to read; `_` is the type for any other.",
    ),
    (
        Place::Payload,
        "process",
        "The transform: zlib, deflate, gzip, brotli, bzip2, xz, lzma, lzip or zstd.",
    ),
    (
        Place::Payload,
        "from",
        "Where in the file the data starts, in bytes (0 by default).",
    ),
    (
        Place::Filesystem,
        "entries",
        "The list of entries, from the root of the value.",
    ),
    (
        Place::Filesystem,
        "path",
        "The path of an entry: a field, or several whose texts are joined with /.",
    ),
    (
        Place::Filesystem,
        "size",
        "The size of an entry in bytes (a number); several names: the first that an entry has.",
    ),
    (
        Place::Filesystem,
        "mode",
        "The mode: the permission bits, and the file type bits when the format has them.",
    ),
    (
        Place::Filesystem,
        "mtime",
        "The modification time, in seconds since 1970.",
    ),
    (
        Place::Filesystem,
        "kind",
        "The field that says what kind of entry it is.",
    ),
    (
        Place::Filesystem,
        "kinds",
        "For each kind (directory, symlink, hardlink, device, fifo, file), the values of `kind` that mean it.",
    ),
    (
        Place::Filesystem,
        "link",
        "The target of a symbolic or hard link.",
    ),
    (
        Place::Filesystem,
        "data",
        "The bytes of the file's contents, for a preview and for saving it.",
    ),
    (
        Place::Filesystem,
        "data_is_file",
        "Whether `data` is the file's own bytes (not, for a compressed entry); absent: it always is.",
    ),
    (
        Place::Filesystem,
        "directory_slash",
        "Whether a path that ends in / is a directory when nothing else says (true by default).",
    ),
    (
        Place::Filesystem,
        "ignore",
        "Names of entries that are not files and are left out (cpio's TRAILER!!!).",
    ),
    (Place::Param, "id", "The name of the parameter."),
    (
        Place::Param,
        "type",
        "The type of the parameter: a number, a string, a boolean or bytes.",
    ),
    (
        Place::Param,
        "enum",
        "The enum that names the values of the parameter.",
    ),
    (Place::Param, "doc", "What the parameter is, in words."),
    (Place::Valid, "eq", "The value must equal this expression."),
    (Place::Valid, "min", "The value must be at least this."),
    (Place::Valid, "max", "The value must be at most this."),
    (Place::Valid, "any-of", "The value must equal one of these."),
    (
        Place::Valid,
        "expr",
        "An expression over `_`, the value, that must be true.",
    ),
    (
        Place::Valid,
        "in-enum",
        "The value must be a member of the field's enum.",
    ),
];

/// What the keys of a field (an item of `seq:`) are for; an instance has them too (see [`describe`]).
const FIELD_KEYS: &[(&str, &str)] = &[
    (
        "parent",
        "_parent (or _parent._parent): the type that this field reads gets the parent of the type it is in, as its _parent.",
    ),
    ("id", "The name of the field."),
    (
        "type",
        "What is read: a number type (u2, s4le, f8), bN for bits, str, strz, bytes, or a type of the schema; or a choice by a value (switch-on and cases).",
    ),
    (
        "contents",
        "The bytes that must be there (a magic number, a signature).",
    ),
    (
        "size",
        "How many bytes: for bytes and str, or the window a type is read in. A number or an expression.",
    ),
    (
        "size-eos",
        "true: the rest of the window or the input is the size.",
    ),
    ("terminator", "The byte that ends bytes or a str."),
    (
        "include",
        "With terminator: keep the terminator in the value.",
    ),
    (
        "consume",
        "With terminator: go on after the terminator (true by default).",
    ),
    (
        "eos-error",
        "With terminator: false takes the rest of the input when there is none, instead of an error.",
    ),
    (
        "pad-right",
        "With size: strip trailing bytes of this value from the result.",
    ),
    (
        "process",
        "A transform of the bytes of the size window before they are read: xor(key), rol(n), ror(n), zlib, deflate, gzip, brotli, bzip2, xz, lzma, lzip, zstd.",
    ),
    (
        "encoding",
        "The encoding of a string (utf8, ascii, latin1, utf16le, …).",
    ),
    ("bit_size", "With the type bits: how many bits (1 to 64)."),
    (
        "repeat",
        "How the field repeats: eos (to the end), expr (a number of times) or until (until a condition).",
    ),
    ("repeat-expr", "With repeat: expr: how many times."),
    (
        "repeat-until",
        "With repeat: until: the condition that ends it; `_` is the last value read.",
    ),
    ("if", "An expression: the field is read only if it is true."),
    (
        "pos",
        "Read the field at this byte offset instead of where the last one ended.",
    ),
    (
        "points-to",
        "The field or instance that this number points at, for an offset or a size that pos and size do not read as it is; the viewers jump there.",
    ),
    (
        "valid",
        "Rules that the value must satisfy, or the parse fails.",
    ),
    (
        "enum",
        "Name the number that is read, by an enum of the type.",
    ),
    ("doc", "What the field is, in words."),
];

/// The keys of an instance that a field does not have.
const INSTANCE_KEYS: &[(&str, &str)] = &[
    (
        "io",
        "The stream to read from (`_root._io`, or the io of a value read earlier).",
    ),
    (
        "value",
        "An expression: the instance is computed, not read.",
    ),
];

/// The one line that says what `key` is for at `place`, if there is one.
#[must_use]
pub fn describe(place: Place, key: &str) -> Option<&'static str> {
    let found = KEYS.iter().find(|(p, k, _)| *p == place && *k == key);
    if let Some((_, _, doc)) = found {
        return Some(doc);
    }
    if !matches!(place, Place::Field | Place::Instance) {
        return None;
    }
    if place == Place::Instance
        && let Some((_, doc)) = INSTANCE_KEYS.iter().find(|(k, _)| *k == key)
    {
        return Some(doc);
    }
    FIELD_KEYS
        .iter()
        .find(|(k, _)| *k == key && (place == Place::Field || *k != "id"))
        .map(|(_, d)| *d)
}

/// The keys that have a line for `place`, whether the parser accepts them or not (a test says that it does).
#[must_use]
pub fn documented_keys(place: Place) -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = KEYS
        .iter()
        .filter(|(p, _, _)| *p == place)
        .map(|(_, k, _)| *k)
        .collect();
    if matches!(place, Place::Field | Place::Instance) {
        // an instance is named by the key it is under, not by an `id`
        keys.extend(
            FIELD_KEYS
                .iter()
                .map(|(k, _)| *k)
                .filter(|k| place == Place::Field || *k != "id"),
        );
    }
    if place == Place::Instance {
        keys.extend(INSTANCE_KEYS.iter().map(|(k, _)| *k));
    }
    keys
}

/// A key that goes at a place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInfo {
    /// The key.
    pub key: String,
    /// What it is for, in a line; empty for a hidden key.
    pub doc: String,
    /// Whether an editor should not offer it (see [`is_hidden`]).
    pub hidden: bool,
}

/// The keys of a place, with what each is for, in the order the parser lists them.
#[must_use]
pub fn keys(place: Place) -> Vec<KeyInfo> {
    accepted_keys(place)
        .into_iter()
        .map(|key| KeyInfo {
            hidden: is_hidden(&key),
            doc: describe(place, &key).unwrap_or_default().to_owned(),
            key,
        })
        .collect()
}

/// A name that a key takes as its value, or a type, with what it means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    /// The name, as it is written.
    pub name: String,
    /// What it means.
    pub doc: String,
}

fn named(name: &str, doc: &str) -> Named {
    Named {
        name: name.to_owned(),
        doc: doc.to_owned(),
    }
}

/// The built-in types of a field: the numbers, the bits, the strings and the bytes.
#[must_use]
pub fn builtin_types() -> Vec<Named> {
    let mut out = Vec::new();
    for (prefix, what) in [("u", "an unsigned integer"), ("s", "a signed integer")] {
        for bytes in [1, 2, 4, 8] {
            out.push(named(
                &format!("{prefix}{bytes}"),
                &format!(
                    "{what} of {bytes} byte{}, in the byte order of the schema",
                    if bytes == 1 { "" } else { "s" }
                ),
            ));
            if bytes > 1 {
                for (order, name) in [("le", "little-endian"), ("be", "big-endian")] {
                    out.push(named(
                        &format!("{prefix}{bytes}{order}"),
                        &format!("{what} of {bytes} bytes, {name}"),
                    ));
                }
            }
        }
    }
    for bytes in [4, 8] {
        out.push(named(
            &format!("f{bytes}"),
            &format!("an IEEE 754 floating-point number of {bytes} bytes, in the byte order of the schema"),
        ));
        for (order, name) in [("le", "little-endian"), ("be", "big-endian")] {
            out.push(named(
                &format!("f{bytes}{order}"),
                &format!("an IEEE 754 floating-point number of {bytes} bytes, {name}"),
            ));
        }
    }
    out.push(named(
        "bytes",
        "a sequence of bytes; needs size (or terminator, or size-eos)",
    ));
    out.push(named("str", "a string; needs size, terminator or size-eos"));
    out.push(named("strz", "a string that ends with a zero byte"));
    out.push(named("bits", "a bit field of the size in bit_size"));
    for width in 1..=64 {
        out.push(named(
            &format!("b{width}"),
            &format!(
                "a bit field of {width} bit{} (b1 is a boolean)",
                if width == 1 { "" } else { "s" }
            ),
        ));
    }
    out
}

/// The names that these keys take, as the parser reads them.
#[must_use]
pub fn values(key: &str) -> Vec<Named> {
    match key {
        "endian" | "bit-endian" => vec![
            named(
                "le",
                "little-endian: the least significant byte (or bit) first",
            ),
            named("be", "big-endian: the most significant byte (or bit) first"),
        ],
        "repeat" => vec![
            named("eos", "until the end of the input"),
            named("expr", "as many times as repeat-expr says"),
            named("until", "until repeat-until is true"),
        ],
        "encoding" => ENCODINGS.iter().map(|name| named(name, "")).collect(),
        "process" | "payload-process" => PROCESSES
            .iter()
            .map(|(name, doc)| named(name, doc))
            .collect(),
        _ => Vec::new(),
    }
}

/// The encodings that `encoding:` takes.
pub const ENCODINGS: &[&str] = &[
    "UTF-8",
    "ASCII",
    "ISO-8859-1",
    "UTF-16LE",
    "UTF-16BE",
    "UTF-32LE",
    "UTF-32BE",
    "CP437",
    "CP1250",
    "CP1251",
    "CP1252",
    "CP1253",
    "CP1254",
    "CP1255",
    "CP1256",
    "CP1257",
    "CP1258",
    "CP874",
    "ISO-8859-2",
    "ISO-8859-3",
    "ISO-8859-4",
    "ISO-8859-5",
    "ISO-8859-6",
    "ISO-8859-7",
    "ISO-8859-8",
    "ISO-8859-9",
    "ISO-8859-10",
    "ISO-8859-11",
    "ISO-8859-13",
    "ISO-8859-14",
    "ISO-8859-15",
    "ISO-8859-16",
    "Shift_JIS",
    "GBK",
    "GB18030",
    "Big5",
    "EUC-JP",
    "EUC-KR",
    "KOI8-R",
    "KOI8-U",
];

/// The transforms of `process:`, as they are written (a transform with an argument shows it).
const PROCESSES: &[(&str, &str)] = &[
    (
        "xor(key)",
        "xor with a key: an integer, or a sequence of bytes, repeated",
    ),
    ("rol(n)", "rotate each byte left by n bits"),
    ("ror(n)", "rotate each byte right by n bits"),
    ("zlib", "a zlib stream (RFC 1950)"),
    ("deflate", "a raw deflate stream (RFC 1951)"),
    (
        "gzip",
        "gzip data (RFC 1952), every member, with its CRC-32 and size checked",
    ),
    ("brotli", "a Brotli stream (RFC 7932)"),
    ("bzip2", "bzip2 data, every stream, the CRCs checked"),
    ("xz", "xz data, every stream, the checks verified"),
    ("lzma", "a .lzma file"),
    ("lzip", "lzip data, every member"),
    ("zstd", "Zstandard data, every frame, the checksum verified"),
];
