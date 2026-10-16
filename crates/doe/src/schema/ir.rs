//! Compiled Intermediate Representation.
//!
//! The IR is what the binary parser consumes.  All type references are
//! resolved, all expression strings are pre-parsed into ASTs, and all
//! enum keys are decoded from their raw YAML string form into `u64`.
//!
//! Types are stored in a flat registry keyed by a fully-qualified name:
//! - Top-level types use their bare id, e.g. `"png"`.
//! - Inline types are prefixed by their parent: `"png::chunk"`.
//! - Doubly-nested: `"png::chunk::segment"`.

use std::collections::HashMap;

use indexmap::IndexMap;

use crate::schema::expr::Expr;

// ─────────────────────────────────────────────────────────────────────────────
// Type registry
// ─────────────────────────────────────────────────────────────────────────────

/// The compiled, resolved representation of all loaded schemas.
#[derive(Debug, Default)]
pub struct TypeRegistry {
    /// All known types, keyed by fully-qualified name.
    pub types: IndexMap<String, CompiledType>,
}

impl TypeRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a type by fully-qualified name.
    pub fn get(&self, name: &str) -> Option<&CompiledType> {
        self.types.get(name)
    }

    /// Insert a type, replacing any existing type with the same name.
    pub fn insert(&mut self, name: String, ty: CompiledType) {
        self.types.insert(name, ty);
    }

    /// Returns `true` if a type with this fully-qualified name exists.
    pub fn contains(&self, name: &str) -> bool {
        self.types.contains_key(name)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Compiled type
// ─────────────────────────────────────────────────────────────────────────────

/// A fully compiled struct-like type.
#[derive(Debug, Clone)]
pub struct CompiledType {
    /// Fully-qualified name, e.g. `"png"` or `"png::chunk"`.
    pub name: String,

    /// Optional human-readable description.
    pub doc: Option<String>,

    /// Default byte order for this type's fields.
    pub endian: Endian,

    /// Default bit order for this type's bit fields: [`Endian::Big`] takes the
    /// most significant bits of a byte first, [`Endian::Little`] the least.
    pub bit_endian: Endian,

    /// The parameters the type takes, in order.
    pub params: Vec<CompiledParam>,

    /// Ordered list of fields.
    pub fields: Vec<CompiledField>,

    /// Values computed or read on demand, in schema order.
    pub instances: Vec<CompiledInstance>,

    /// Enum definitions visible within this type (merged from imports +
    /// inline `enums:` blocks).
    pub enums: HashMap<String, CompiledEnum>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Compiled field
// ─────────────────────────────────────────────────────────────────────────────

/// A parameter of a [`CompiledType`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledParam {
    /// The name the type's expressions use.
    pub id: String,
    /// Human-readable description.
    pub doc: Option<String>,
}

/// A single compiled field of a [`CompiledType`].
#[derive(Debug, Clone)]
pub struct CompiledField {
    /// The field's identifier within its type.
    pub id: String,
    /// Human-readable description; used by renderers (verbose mode).
    pub doc: Option<String>,
    /// What the field contains.
    pub kind: FieldKind,
    /// How many times the field repeats.
    pub repeat: RepeatMode,
    /// Compiled `if:` predicate, if present.
    pub if_expr: Option<Expr>,
    /// Compiled `pos:` expression, if present: where to read the field.
    pub pos: Option<Expr>,
    /// Name of the enum to apply, if present.
    pub enum_ref: Option<String>,
    /// Instances of the same type that the field's expressions use, which are
    /// computed before the field is.
    pub uses_instances: Vec<String>,
    /// What the value read may be, if constrained (`valid:`).
    pub valid: Option<CompiledValid>,
}

/// The rules a field's value must satisfy; all that are given must hold.
#[derive(Debug, Clone, Default)]
pub struct CompiledValid {
    /// The value must equal this.
    pub eq: Option<Expr>,
    /// The value must be at least this.
    pub min: Option<Expr>,
    /// The value must be at most this.
    pub max: Option<Expr>,
    /// The value must equal one of these; empty when not given.
    pub any_of: Vec<Expr>,
    /// An expression over `_`, the value, that must be true.
    pub expr: Option<Expr>,
    /// The value must be a member of the field's enum.
    pub in_enum: bool,
}

/// A value of a type that is computed or read on demand (`instances:`).
#[derive(Debug, Clone)]
pub struct CompiledInstance {
    /// The instance's name within its type.
    pub id: String,
    /// What it is.
    pub kind: InstanceKind,
}

/// How an instance gets its value.
#[derive(Debug, Clone)]
pub enum InstanceKind {
    /// Read like a `seq` field, usually at a `pos:`.
    Read(Box<CompiledField>),
    /// Computed from other fields (`value:`).
    Value {
        /// Human-readable description.
        doc: Option<String>,
        /// The expression.
        expr: Expr,
        /// Compiled `if:` predicate, if present.
        if_expr: Option<Expr>,
        /// Name of the enum to apply, if present.
        enum_ref: Option<String>,
        /// Instances of the same type that the expression uses.
        uses_instances: Vec<String>,
    },
}

/// The kind of data a field holds.
#[derive(Debug, Clone)]
pub enum FieldKind {
    /// A fixed-width unsigned integer.
    UInt {
        /// Width of the integer.
        width: IntWidth,
        /// Byte-order override for this field.
        endian: EndianOverride,
    },
    /// A fixed-width signed integer.
    SInt {
        /// Width of the integer.
        width: IntWidth,
        /// Byte-order override for this field.
        endian: EndianOverride,
    },
    /// IEEE 754 float.
    Float {
        /// Width of the float.
        width: FloatWidth,
        /// Byte-order override for this field.
        endian: EndianOverride,
    },
    /// Raw byte sequence.
    Bytes {
        /// Number of bytes to read.
        size: SizeExpr,
    },
    /// Raw byte sequence that ends at a terminator or is cut from a window.
    BytesCut {
        /// Where the value ends.
        cut: Cut,
    },
    /// String with optional terminator or explicit size.
    Str {
        /// How the string's extent is determined.
        size: StrSize,
        /// Character encoding of the bytes.
        encoding: Encoding,
    },
    /// Bit field of 1 to 64 bits; a single bit reads as a boolean.
    Bits {
        /// Number of bits to read.
        bit_size: u8,
        /// Bit-order override for this field.
        endian: EndianOverride,
    },
    /// A reference to another compiled type by its fully-qualified name.
    TypeRef {
        /// Fully-qualified name of the referenced type.
        type_name: String,
        /// If set, the type is parsed in a window of this many bytes, and the
        /// parent continues after the whole window.
        size: Option<SizeExpr>,
        /// The values of the type's parameters, as expressions of the field's
        /// own type.
        args: Vec<Expr>,
    },
    /// A type chosen by the value of an expression (`switch-on` / `cases`).
    Switch {
        /// The selector.
        on: Expr,
        /// If set, a window of this many bytes around the chosen type; a case
        /// that is `bytes` or `str` fills it.
        size: Option<SizeExpr>,
        /// The cases in the order they are tried: a key expression compared
        /// with the selector, and the type to read when they are equal.
        cases: Vec<(Expr, FieldKind)>,
        /// The type to read when no case matches (the `_` case).
        default: Option<Box<FieldKind>>,
    },
    /// Fixed literal content — parser reads and asserts exact bytes.
    Contents(Vec<u8>),
}

/// Integer byte widths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntWidth {
    /// 8 bits.
    W8,
    /// 16 bits.
    W16,
    /// 32 bits.
    W32,
    /// 64 bits.
    W64,
}

impl IntWidth {
    /// The width in bytes.
    pub fn bytes(self) -> usize {
        match self {
            IntWidth::W8 => 1,
            IntWidth::W16 => 2,
            IntWidth::W32 => 4,
            IntWidth::W64 => 8,
        }
    }
}

/// Float byte widths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatWidth {
    /// 32-bit float.
    F32,
    /// 64-bit float.
    F64,
}

impl FloatWidth {
    /// The width in bytes.
    pub fn bytes(self) -> usize {
        match self {
            FloatWidth::F32 => 4,
            FloatWidth::F64 => 8,
        }
    }
}

/// Endianness selection for a specific field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndianOverride {
    /// Use the type's (or schema's) default.
    Inherit,
    /// Force little-endian.
    Little,
    /// Force big-endian.
    Big,
}

/// Byte order resolved to a concrete value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Endian {
    #[default]
    /// Least significant byte first.
    Little,
    /// Most significant byte first.
    Big,
}

/// How many bytes to consume for a `bytes` or `str` field.
#[derive(Debug, Clone)]
pub enum SizeExpr {
    /// A compile-time constant.
    Literal(usize),
    /// An expression evaluated at parse time.
    Dynamic(Expr),
    /// Everything left in the enclosing window or the input (`size-eos`).
    Eos,
}

/// String termination/size strategy.
#[derive(Debug, Clone)]
pub enum StrSize {
    /// Read until this byte value (the terminator is consumed but not included
    /// in the string value); the end of the input before it is an error.
    Terminator(u8),
    /// Read exactly this many bytes.
    Fixed(SizeExpr),
    /// A terminator with options, or a window cut at a terminator or stripped.
    Cut(Cut),
}

/// How a `bytes` or `str` value ends when it is more than a plain size or a
/// terminator that is consumed and not kept.
#[derive(Debug, Clone)]
pub struct Cut {
    /// Read exactly this many bytes, then cut the value; without it the value
    /// runs to the terminator.
    pub size: Option<SizeExpr>,
    /// The byte that ends the value.
    pub terminator: Option<u8>,
    /// Keep the terminator in the value.
    pub include: bool,
    /// Continue after the terminator (without `size`).
    pub consume: bool,
    /// Without `size`: no terminator is an error and not the rest of the stream.
    pub eos_error: bool,
    /// With `size`: strip trailing bytes of this value before cutting.
    pub pad_right: Option<u8>,
}

/// String character encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    /// UTF-8.
    #[default]
    Utf8,
    /// 7-bit ASCII.
    Ascii,
    /// ISO 8859-1.
    Latin1,
}

/// Repetition mode for a field.
#[derive(Debug, Clone)]
pub enum RepeatMode {
    /// Appear exactly once (default).
    Once,
    /// Repeat until end of the containing stream.
    Eos,
    /// Repeat a count determined at parse time.
    Count(Expr),
    /// Repeat until the most recently parsed value satisfies a predicate.
    /// The value is bound to `_` within the expression.
    Until(Expr),
}

// ─────────────────────────────────────────────────────────────────────────────
// Compiled enum
// ─────────────────────────────────────────────────────────────────────────────

/// An enum: maps integer discriminant → variant name.
#[derive(Debug, Clone, Default)]
pub struct CompiledEnum {
    /// Variant names keyed by discriminant.
    pub variants: HashMap<u64, String>,
}

impl CompiledEnum {
    /// The discriminant of the variant called `name`, if there is one.
    pub fn value_of(&self, name: &str) -> Option<u64> {
        self.variants
            .iter()
            .find_map(|(value, variant)| (variant == name).then_some(*value))
    }

    /// The variant name for discriminant `v`, if any.
    pub fn lookup(&self, v: u64) -> Option<&str> {
        self.variants.get(&v).map(String::as_str)
    }
}
