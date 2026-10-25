//! Finding the schema of a file that was not told which one it is.
//!
//! The schemas that are known (the type library, the schemas a person has
//! opened) are put in an order for the file, and tried in it; the first that
//! parses the file without an error is the one:
//!
//! 1. **By signature.** A schema whose first fields are `contents:` (or fixed
//!    sizes before one) can only describe a file that starts with those bytes.
//!    This is read from the compiled schema, without parsing anything
//!    ([`magic_of`]). One whose signature is not at the start of the file is
//!    left out: it cannot fit.
//! 2. **By name.** The extension of the file against the schema's `id`, the
//!    name of its file and its `meta: file-extension`.
//! 3. **By trying.** The ones that are left are tried in that order, each with
//!    a limit of work and of time, and a panic caught, so that one bad schema
//!    does not stop the others. A parse that stops with an error is a schema
//!    that did not fit, and the error is kept in the report.
//!
//! A format without a signature, and whose name the file does not carry, is
//! only found when asked for ([`Options::all`]): trying the whole library takes
//! time.
//!
//! [`rank`] is the order, over plain data, which the web page uses with
//! definitions it has not fetched yet; [`detect_file`] does all of it for a
//! file and the schemas of the include paths.

#[cfg(feature = "fs")]
use std::path::Path;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::schema::expr::{EvalContext, ExprValue, eval};
use crate::schema::ir::{
    CompiledField, Encoding, Endian, EndianOverride, EndianRule, FieldKind, FloatWidth, IntWidth,
    RepeatMode, SizeExpr, StrSize, TypeRegistry,
};

/// The most bytes from the start of the file that the signatures are compared
/// with.
pub const HEAD_BYTES: usize = 64 * 1024;

/// Bytes that a file of a schema has at a place: `offset` bytes from the start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Magic {
    /// Where, in bytes from the start of the file.
    pub offset: u64,
    /// What is there.
    pub bytes: Vec<u8>,
}

/// The signature of the type `type_name` of a compiled schema: what every file
/// of the type starts with, from the `contents:` and the `valid:` (an exact
/// value) of its first fields, with their offsets. The fields before one must
/// have a size that is always the same; a field that is a type is looked
/// into. It stops at the first field that is conditional, placed with `pos:`,
/// counted, or of a size that is not known, and after a field that repeats to
/// the end (whose first element is read, and which may have none). Empty when
/// the type starts with something else, as one whose magic number is an enum
/// without a `valid:`.
#[must_use]
pub fn magic_of(registry: &TypeRegistry, type_name: &str) -> Vec<Magic> {
    let mut out: Vec<Magic> = Vec::new();
    let mut offset: u64 = 0;
    walk(registry, type_name, 0, &mut out, &mut offset);
    out
}

/// How deep a type is looked into for a signature.
const SIGNATURE_DEPTH: usize = 8;

fn push(out: &mut Vec<Magic>, offset: u64, bytes: &[u8]) {
    match out.last_mut() {
        Some(last) if last.offset + last.bytes.len() as u64 == offset => {
            last.bytes.extend_from_slice(bytes);
        }
        _ => out.push(Magic {
            offset,
            bytes: bytes.to_vec(),
        }),
    }
}

/// The constant that a field has to equal (`valid: x`), when that is all it
/// says.
fn exact(field: &CompiledField) -> Option<ExprValue> {
    let valid = field.valid.as_ref()?;
    if valid.min.is_some()
        || valid.max.is_some()
        || !valid.any_of.is_empty()
        || valid.expr.is_some()
    {
        return None;
    }
    eval(valid.eq.as_ref()?, &EvalContext::new(), "<signature>").ok()
}

/// Add what the fields of `type_name` start with at `offset`, which moves on
/// over them. Whether the fields after the type can be looked at: false after
/// a field of a size that is not known.
fn walk(
    registry: &TypeRegistry,
    type_name: &str,
    depth: usize,
    out: &mut Vec<Magic>,
    offset: &mut u64,
) -> bool {
    let Some(ty) = registry.get(type_name) else {
        return false;
    };
    if depth > SIGNATURE_DEPTH || !ty.params.is_empty() {
        return false;
    }
    let order = match ty.endian_rule {
        EndianRule::Fixed => Some(ty.endian),
        _ => None,
    };
    for field in &ty.fields {
        if field.pos.is_some() || field.if_expr.is_some() {
            return false;
        }
        let to_end = match field.repeat {
            RepeatMode::Once => false,
            RepeatMode::Eos => true,
            RepeatMode::Count(_) | RepeatMode::Until(_) => return false,
        };
        let int = |width: &IntWidth, endian: &EndianOverride, signed: bool| -> Option<Vec<u8>> {
            let ExprValue::Int(value) = exact(field)? else {
                return None;
            };
            let bytes = match width {
                IntWidth::W8 => 1,
                IntWidth::W16 => 2,
                IntWidth::W32 => 4,
                IntWidth::W64 => 8,
            };
            let big = match endian {
                EndianOverride::Little => false,
                EndianOverride::Big => true,
                EndianOverride::Inherit => order? == Endian::Big,
            };
            let fits = if signed {
                let half = 1_i128 << (bytes * 8 - 1);
                (-half..half).contains(&value)
            } else {
                (0..1_i128 << (bytes * 8)).contains(&value)
            };
            fits.then(|| {
                let le = value.to_le_bytes();
                let mut taken = le.get(..bytes)?.to_vec();
                if big {
                    taken.reverse();
                }
                Some(taken)
            })?
        };
        let size = match &field.kind {
            FieldKind::Contents(bytes) => {
                push(out, *offset, bytes);
                bytes.len() as u64
            }
            FieldKind::UInt { width, endian } | FieldKind::SInt { width, endian } => {
                let signed = matches!(field.kind, FieldKind::SInt { .. });
                if let Some(bytes) = int(width, endian, signed) {
                    push(out, *offset, &bytes);
                }
                match width {
                    IntWidth::W8 => 1,
                    IntWidth::W16 => 2,
                    IntWidth::W32 => 4,
                    IntWidth::W64 => 8,
                }
            }
            FieldKind::Float { width, .. } => match width {
                FloatWidth::F32 => 4,
                FloatWidth::F64 => 8,
            },
            FieldKind::Str {
                size: StrSize::Fixed(SizeExpr::Literal(n)),
                encoding,
            } => {
                if let Some(ExprValue::Str(text)) = exact(field)
                    && matches!(encoding, Encoding::Utf8 | Encoding::Ascii)
                    && text.len() == *n
                {
                    push(out, *offset, text.as_bytes());
                }
                *n as u64
            }
            FieldKind::Bytes {
                size: SizeExpr::Literal(n),
            } => {
                if let Some(ExprValue::Bytes(bytes)) = exact(field)
                    && bytes.len() == *n
                {
                    push(out, *offset, &bytes);
                }
                *n as u64
            }
            FieldKind::TypeRef {
                type_name,
                size,
                args,
            } if args.is_empty() => {
                let start = *offset;
                let whole = walk(registry, type_name, depth + 1, out, offset);
                match size {
                    Some(SizeExpr::Literal(n)) => *offset = start + *n as u64,
                    Some(_) => return false,
                    None if !whole => return false,
                    None => {}
                }
                *offset - start
            }
            _ => return false,
        };
        if !matches!(field.kind, FieldKind::TypeRef { size: None, .. }) {
            *offset += size;
        }
        if to_end {
            return false;
        }
    }
    true
}

/// Whether the start of a file (`head`) has all of a signature.
#[must_use]
pub fn matches_magic(magic: &[Magic], head: &[u8]) -> bool {
    magic.iter().all(|m| {
        let Ok(start) = usize::try_from(m.offset) else {
            return false;
        };
        start
            .checked_add(m.bytes.len())
            .and_then(|end| head.get(start..end))
            .is_some_and(|found| found == m.bytes.as_slice())
    })
}

/// What is known of a schema before it is tried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// How it is called: its `id`, or the file or the gallery name.
    pub name: String,
    /// What the file starts with, if the schema says; see [`magic_of`].
    #[serde(default)]
    pub magic: Vec<Magic>,
    /// The names of the format, in lower case: the extensions of its files,
    /// its `id`, the name of its file.
    #[serde(default)]
    pub names: Vec<String>,
}

/// Why a schema is tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Why {
    /// The file starts with its signature, and its name fits too.
    SignatureAndName,
    /// The file starts with its signature.
    Signature,
    /// It has no signature, and the name of the file fits.
    Name,
    /// It has no signature and the name does not fit: only when all are tried.
    Remaining,
}

impl Why {
    /// A few words.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Why::SignatureAndName => "the signature and the name fit",
            Why::Signature => "the file starts with its signature",
            Why::Name => "the name of the file fits",
            Why::Remaining => "no signature to check, tried because all were asked for",
        }
    }
}

/// A schema to try, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Ranked {
    /// The place in the list of sources.
    pub index: usize,
    /// Why it is tried.
    pub why: Why,
}

/// The extension of a file name, in lower case, without the dot.
fn extension(file_name: &str) -> Option<String> {
    let (stem, ext) = file_name.rsplit_once('.')?;
    (!stem.is_empty() && !ext.is_empty()).then(|| ext.to_ascii_lowercase())
}

/// The order in which to try `sources` on a file that starts with `head` and is
/// called `file_name`; with `all`, the ones that have no signature and no name
/// that fits are at the end instead of left out. The ones whose signature the
/// file does not have are always left out.
#[must_use]
pub fn rank(sources: &[Source], head: &[u8], file_name: Option<&str>, all: bool) -> Vec<Ranked> {
    let ext = file_name.and_then(extension);
    let mut ranked: Vec<(Ranked, usize)> = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let named = ext
            .as_deref()
            .is_some_and(|ext| source.names.iter().any(|name| name == ext));
        let length: usize = source.magic.iter().map(|m| m.bytes.len()).sum();
        let why = if source.magic.is_empty() {
            if named {
                Why::Name
            } else if all {
                Why::Remaining
            } else {
                continue;
            }
        } else if !matches_magic(&source.magic, head) {
            continue;
        } else if named {
            Why::SignatureAndName
        } else {
            Why::Signature
        };
        ranked.push((Ranked { index, why }, length));
    }
    let tier = |why: Why| match why {
        Why::SignatureAndName => 0,
        Why::Signature => 1,
        Why::Name => 2,
        Why::Remaining => 3,
    };
    // the longer the signature, the less likely it is chance; the order of the sources otherwise
    ranked.sort_by_key(|(r, length)| (tier(r.why), std::cmp::Reverse(*length), r.index));
    ranked.into_iter().map(|(r, _)| r).collect()
}

/// What came of trying a schema on the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// It parsed the file; it read this many bytes of it.
    Fit {
        /// Where the parse ended.
        consumed: u64,
    },
    /// The parse stopped with this error.
    Failed(String),
    /// The schema could not be compiled.
    Unusable(String),
    /// It used more fields or time than it was allowed.
    OutOfLimits(String),
    /// The parser panicked.
    Panicked,
}

impl Outcome {
    /// Whether the schema fits the file.
    #[must_use]
    pub fn fits(&self) -> bool {
        matches!(self, Outcome::Fit { .. })
    }

    /// A line for a person.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Outcome::Fit { consumed } => format!("parsed ({consumed} bytes read)"),
            Outcome::Failed(error) => format!("did not fit: {error}"),
            Outcome::Unusable(error) => format!("could not be used: {error}"),
            Outcome::OutOfLimits(what) => format!("stopped: {what}"),
            Outcome::Panicked => "stopped: the parser failed on it".to_owned(),
        }
    }
}

/// A schema that was tried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    /// Its name.
    pub name: String,
    /// The file it was read from.
    pub path: Option<PathBuf>,
    /// Why it was tried.
    pub why: Why,
    /// What came of it.
    pub outcome: Outcome,
}

/// What detection found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Detection {
    /// The schema that fits: the first that parsed the file, an index in
    /// `attempts`.
    pub chosen: Option<usize>,
    /// The schemas that were tried, in order, up to the one that fits.
    pub attempts: Vec<Attempt>,
    /// How many schemas there were.
    pub known: usize,
    /// How many were left out because the file does not start with their
    /// signature.
    pub excluded: usize,
}

impl Detection {
    /// The attempt that fits.
    #[must_use]
    pub fn chosen(&self) -> Option<&Attempt> {
        self.chosen.and_then(|i| self.attempts.get(i))
    }

    /// The report, one line for each schema tried.
    #[must_use]
    pub fn report(&self) -> String {
        use std::fmt::Write as _;

        let mut out = String::new();
        for attempt in &self.attempts {
            let _ = writeln!(
                out,
                "  {} ({}): {}",
                attempt.name,
                attempt.why.describe(),
                attempt.outcome.describe()
            ); // writing to a String cannot fail
        }
        let _ = writeln!(
            out,
            "  {} schemas known, {} left out because the file does not start with their signature",
            self.known, self.excluded
        );
        out
    }
}

/// How detection goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Also try the schemas that have no signature and whose name the file
    /// does not carry (slow for a big library).
    pub all: bool,
    /// The most fields one try may read.
    pub max_fields: u64,
    /// The longest one try may take.
    pub max_time: std::time::Duration,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            all: false,
            max_fields: 2_000_000,
            max_time: std::time::Duration::from_secs(10),
        }
    }
}

/// The extensions of a format as `meta: file-extension` gives them: one, or a list.
#[must_use]
pub fn extensions_of(value: Option<&serde_json::Value>) -> Vec<String> {
    match value {
        Some(serde_json::Value::String(s)) => vec![s.to_ascii_lowercase()],
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_ascii_lowercase))
            .collect(),
        _ => Vec::new(),
    }
}

/// A schema of the include paths, compiled for trying.
#[cfg(feature = "fs")]
struct Loaded {
    source: Source,
    path: PathBuf,
    id: String,
    compiled: Result<crate::schema::compiler::Compiler, String>,
}

/// The types of some include paths, compiled once so that many files can be
/// looked at without reading them again.
#[cfg(feature = "fs")]
pub struct Library {
    loaded: Vec<Loaded>,
}

#[cfg(feature = "fs")]
impl std::fmt::Debug for Library {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.loaded.iter().map(|l| &l.source.name))
            .finish()
    }
}

#[cfg(feature = "fs")]
impl Library {
    /// Compile the types of the include paths, to see their signatures. One
    /// that cannot be compiled is kept, and reported when it is tried.
    ///
    /// # Errors
    ///
    /// Fails when two types of the include paths have the same id.
    pub fn load(include_paths: &[PathBuf]) -> crate::Result<Library> {
        use crate::error::DoeError;
        use crate::schema::compiler::Compiler;

        let found = crate::config::discover_types(include_paths).map_err(|duplicates| {
            DoeError::Schema(
                duplicates
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        })?;
        let mut loaded: Vec<Loaded> = Vec::new();
        for available in found {
            let mut fresh = Compiler::new(include_paths.to_vec());
            let compiled = fresh
                .load(&available.id, None)
                .map(|()| fresh)
                .map_err(|e| e.to_string());
            let mut names = vec![available.id.to_ascii_lowercase()];
            if let Some(stem) = available.source.file_stem().and_then(|s| s.to_str()) {
                names.push(stem.to_ascii_lowercase());
            }
            if let Ok(raw) = Compiler::new(include_paths.to_vec()).read_schema(&available.source) {
                names.extend(extensions_of(raw.meta.file_extension.as_ref()));
            }
            let magic = compiled
                .as_ref()
                .map(|c| magic_of(&c.registry, &available.id))
                .unwrap_or_default();
            loaded.push(Loaded {
                source: Source {
                    name: available.id.clone(),
                    magic,
                    names,
                },
                path: available.source,
                id: available.id,
                compiled,
            });
        }
        Ok(Library { loaded })
    }

    /// What is known of each type: its signature and names.
    #[must_use]
    pub fn sources(&self) -> Vec<Source> {
        self.loaded.iter().map(|l| l.source.clone()).collect()
    }

    /// Find the schema of `file` among the types: the first, in the order
    /// described at the top of this module, that parses it.
    ///
    /// # Errors
    ///
    /// Fails only when the file cannot be read; a schema that cannot be used,
    /// or that does not fit, is part of the result.
    pub fn detect(&self, file: &Path, options: &Options) -> crate::Result<Detection> {
        use std::io::Read as _;

        use crate::error::DoeError;
        use crate::parser::cursor::Cursor;
        use crate::parser::engine::{Engine, Limits};

        let io_error = |source| DoeError::Io {
            path: file.to_owned(),
            source,
        };
        let mut head = Vec::new();
        std::fs::File::open(file)
            .map_err(io_error)?
            .take(HEAD_BYTES as u64)
            .read_to_end(&mut head)
            .map_err(io_error)?;

        let sources = self.sources();
        let file_name = file.file_name().and_then(|n| n.to_str());
        let order = rank(&sources, &head, file_name, options.all);
        let mut detection = Detection {
            known: self.loaded.len(),
            excluded: self
                .loaded
                .iter()
                .filter(|l| !l.source.magic.is_empty() && !matches_magic(&l.source.magic, &head))
                .count(),
            ..Detection::default()
        };
        for ranked in order {
            let Some(entry) = self.loaded.get(ranked.index) else {
                continue;
            };
            let outcome = match &entry.compiled {
                Err(error) => Outcome::Unusable(error.clone()),
                Ok(compiler) => {
                    let limits = Limits {
                        max_fields: Some(options.max_fields),
                        max_time: Some(options.max_time),
                    };
                    // A schema that makes the parser panic is a schema that did not fit.
                    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let handle = std::fs::File::open(file).map_err(io_error)?;
                        let mut cursor = Cursor::from_file(handle, file)?;
                        let engine = Engine::new(&compiler.registry).with_limits(limits);
                        engine.parse_type(&entry.id, &mut cursor)?;
                        Ok::<u64, DoeError>(cursor.bit_position() / 8)
                    }));
                    match run {
                        Ok(Ok(consumed)) => Outcome::Fit { consumed },
                        Ok(Err(DoeError::LimitExceeded(what))) => Outcome::OutOfLimits(what),
                        Ok(Err(error)) => Outcome::Failed(error.to_string()),
                        Err(_) => Outcome::Panicked,
                    }
                }
            };
            let fits = outcome.fits();
            detection.attempts.push(Attempt {
                name: entry.source.name.clone(),
                path: Some(entry.path.clone()),
                why: ranked.why,
                outcome,
            });
            if fits {
                detection.chosen = Some(detection.attempts.len() - 1);
                break;
            }
        }
        Ok(detection)
    }
}

/// Find the schema of `file` among the types of the include paths; see
/// [`Library`] to look at many files.
///
/// # Errors
///
/// Fails when the file cannot be read, or two types of the include paths have
/// the same id; a schema that cannot be used, or that does not fit, is part of
/// the result.
#[cfg(feature = "fs")]
pub fn detect_file(
    file: &Path,
    include_paths: &[PathBuf],
    options: &Options,
) -> crate::Result<Detection> {
    Library::load(include_paths)?.detect(file, options)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(name: &str, magic: &[(u64, &[u8])], names: &[&str]) -> Source {
        Source {
            name: name.to_owned(),
            magic: magic
                .iter()
                .map(|(offset, bytes)| Magic {
                    offset: *offset,
                    bytes: bytes.to_vec(),
                })
                .collect(),
            names: names.iter().map(|n| (*n).to_owned()).collect(),
        }
    }

    fn order(sources: &[Source], head: &[u8], name: Option<&str>, all: bool) -> Vec<(String, Why)> {
        rank(sources, head, name, all)
            .into_iter()
            .map(|r| (sources[r.index].name.clone(), r.why))
            .collect()
    }

    #[test]
    fn a_signature_that_is_not_there_leaves_the_schema_out() {
        let sources = [
            source("png", &[(0, b"\x89PNG")], &["png"]),
            source("gif", &[(0, b"GIF")], &["gif"]),
        ];
        let got = order(&sources, b"\x89PNG\r\n", Some("a.gif"), true);
        // a name that says gif does not make the file one
        assert_eq!(got, [("png".to_owned(), Why::Signature)]);
    }

    #[test]
    fn the_order_is_signature_and_name_then_signature_then_name_then_the_rest() {
        let sources = [
            source("plain", &[], &[]),
            source("named", &[], &["dat"]),
            source("short", &[(0, b"AB")], &[]),
            source("long", &[(0, b"ABCD")], &[]),
            source("both", &[(0, b"AB")], &["dat"]),
        ];
        let head = b"ABCDEF";
        assert_eq!(
            order(&sources, head, Some("x.DAT"), true),
            [
                ("both".to_owned(), Why::SignatureAndName),
                ("long".to_owned(), Why::Signature),
                ("short".to_owned(), Why::Signature),
                ("named".to_owned(), Why::Name),
                ("plain".to_owned(), Why::Remaining),
            ]
        );
        // without all, what has no signature and no name that fits is not tried
        assert_eq!(
            order(&sources, head, Some("x.dat"), false)
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>(),
            ["both", "long", "short", "named"]
        );
    }

    #[test]
    fn a_signature_at_an_offset_is_compared_there() {
        let sources = [source("tar", &[(4, b"ustar")], &[])];
        assert_eq!(order(&sources, b"....ustar", None, false).len(), 1);
        assert_eq!(order(&sources, b"ustar....", None, false), []);
        // a file shorter than the signature does not have it
        assert_eq!(order(&sources, b"..", None, false), []);
    }

    #[test]
    fn extensions_are_a_string_or_a_list() {
        let one = serde_json::json!("PNG");
        let many = serde_json::json!(["jpg", "JPEG", 3]);
        assert_eq!(extensions_of(Some(&one)), ["png"]);
        assert_eq!(extensions_of(Some(&many)), ["jpg", "jpeg"]);
        assert_eq!(extensions_of(None), Vec::<String>::new());
    }

    #[test]
    fn the_extension_of_a_name_has_something_before_it() {
        assert_eq!(extension("a.PNG").as_deref(), Some("png"));
        assert_eq!(extension("a.tar.gz").as_deref(), Some("gz"));
        assert_eq!(extension(".bashrc"), None);
        assert_eq!(extension("noext"), None);
        assert_eq!(extension("a."), None);
    }
}
