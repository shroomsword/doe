//! Schema compiler: transforms `RawSchema` trees into a `TypeRegistry`.
//!
//! Compilation proceeds in four passes:
//!
//! 1. **Load** — read YAML from disk (or accept an already-parsed `RawSchema`),
//!    following imports recursively and detecting cycles.
//! 2. **Collect** — walk every schema and register all type names (fully-qualified)
//!    in the registry without compiling their fields yet.
//! 3. **Resolve** — compile each type's fields: look up type references, parse
//!    expressions, decode enum keys.
//! 4. **Validate** — check that `size` expressions only reference fields that
//!    appear earlier in the same `seq`.
//!
//! Passes 2–4 are implemented here as a single traversal for simplicity; the
//! separation is logical rather than structural.  We do two physical passes:
//! first a name-collection pass (so forward references within a schema work),
//! then a compilation pass.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use indexmap::IndexMap;

use crate::error::{DoeError, Result};
use crate::schema::expr;
use crate::schema::ir::{
    CompiledEnum, CompiledField, CompiledType, Encoding, Endian, EndianOverride, FieldKind,
    FloatWidth, IntWidth, RepeatMode, SizeExpr, StrSize, TypeRegistry,
};
use crate::schema::raw::{
    RawContents, RawEncoding, RawEndian, RawField, RawRepeat, RawSchema, RawTypeDecl, StringOrInt,
};

// ─────────────────────────────────────────────────────────────────────────────
// Compiler entry points
// ─────────────────────────────────────────────────────────────────────────────

/// Compile a single in-memory `RawSchema` (and its inline sub-types) into a
/// `TypeRegistry`.  Imports are not followed — use `Compiler` for that.
///
/// # Errors
///
/// Returns an error if the schema is invalid or references unknown types,
/// enums, or fields.
pub fn compile_schema(schema: &RawSchema) -> Result<TypeRegistry> {
    let mut compiler = Compiler::new(vec![]);
    compiler.process_schema(schema)?;
    Ok(compiler.registry)
}

// ─────────────────────────────────────────────────────────────────────────────
// Compiler state
// ─────────────────────────────────────────────────────────────────────────────

/// Stateful compiler that accumulates types across multiple schema files.
#[derive(Debug)]
pub struct Compiler {
    /// Directories to search for bare schema names (e.g. `"png"` → `<dir>/png.yaml`).
    include_paths: Vec<PathBuf>,
    /// The registry being built.
    pub registry: TypeRegistry,
    /// Set of schema ids currently being loaded (for cycle detection).
    loading_stack: Vec<String>,
    /// Set of schema ids that have already been fully processed.
    processed: HashSet<String>,
    /// Maps schema id → the source path it was loaded from, for duplicate
    /// detection across separately loaded schemas.
    source_paths: HashMap<String, String>,
}

impl Compiler {
    /// Create a compiler that resolves bare schema names against
    /// `include_paths`, in priority order.
    pub fn new(include_paths: Vec<PathBuf>) -> Self {
        Compiler {
            include_paths,
            registry: TypeRegistry::new(),
            loading_stack: Vec::new(),
            processed: HashSet::new(),
            source_paths: HashMap::new(),
        }
    }

    // ── Public API ───────────────────────────────────────────────────────────

    /// Load and compile a schema by name (resolved through include paths) or
    /// by explicit path.
    ///
    /// # Errors
    ///
    /// Returns an error if the schema cannot be found, read, or compiled.
    pub fn load(&mut self, name_or_path: &str, relative_to: Option<&Path>) -> Result<()> {
        let path = self.resolve_path(name_or_path, relative_to)?;
        log::debug!("loading schema {name_or_path} from {}", path.display());
        let schema = load_yaml(&path)?;
        self.process_schema(&schema)
    }

    /// Compile an already-parsed schema.
    ///
    /// # Errors
    ///
    /// Returns an error if the schema is invalid, conflicts with an already
    /// loaded id, or has an import cycle.
    pub fn process_schema(&mut self, schema: &RawSchema) -> Result<()> {
        // If we have already processed this exact id, check whether it came
        // from the same source.  An id being loaded again via a shared import
        // is fine (idempotent); the same id from a *different* file is an error.
        if self.processed.contains(&schema.id) {
            let incoming = schema
                .source_path
                .as_deref()
                .map_or_else(|| "<in-memory>".to_owned(), |p| p.display().to_string());
            let existing = self
                .source_paths
                .get(&schema.id)
                .cloned()
                .unwrap_or_else(|| "<unknown>".to_owned());
            if incoming != existing {
                return Err(DoeError::DuplicateType {
                    name: schema.id.clone(),
                    first: existing,
                    second: incoming,
                });
            }
            return Ok(());
        }

        // Cycle detection
        if self.loading_stack.contains(&schema.id) {
            let mut cycle = self.loading_stack.clone();
            cycle.push(schema.id.clone());
            return Err(DoeError::ImportCycle { cycle });
        }

        self.loading_stack.push(schema.id.clone());

        // Process imports first (depth-first)
        for import in &schema.imports {
            let rel = schema.source_path.as_deref().and_then(|p| p.parent());
            self.load(import, rel)?;
        }

        // Collect all type names defined in this schema (pass 1)
        self.collect_type_names(schema);

        // Compile all types in this schema (pass 2)
        let endian = raw_endian_to_ir(schema.meta.endian.unwrap_or(RawEndian::Le));
        let bit_endian = raw_endian_to_ir(schema.meta.bit_endian.unwrap_or(RawEndian::Be));
        self.compile_type_decl(
            &schema.id,
            &schema.seq,
            &schema.types,
            &schema.enums,
            &HashMap::new(),
            (endian, bit_endian),
        )?;

        self.loading_stack.pop();
        self.processed.insert(schema.id.clone());
        // Record source for future duplicate detection.
        let source = schema
            .source_path
            .as_deref()
            .map_or_else(|| "<in-memory>".to_owned(), |p| p.display().to_string());
        self.source_paths.insert(schema.id.clone(), source);
        Ok(())
    }

    // ── Name collection (pass 1) ─────────────────────────────────────────────

    /// Walk the schema tree and register every type name we'll produce, so
    /// that forward references within the same schema work during compilation.
    fn collect_type_names(&mut self, schema: &RawSchema) {
        // The root type
        self.registry
            .types
            .entry(schema.id.clone())
            .or_insert_with(|| placeholder(&schema.id));

        let prefix = schema.id.clone();
        self.collect_nested_names(&prefix, &schema.types);
    }

    fn collect_nested_names(&mut self, prefix: &str, types: &IndexMap<String, RawTypeDecl>) {
        for (name, decl) in types {
            let fqn = format!("{prefix}::{name}");
            self.registry
                .types
                .entry(fqn.clone())
                .or_insert_with(|| placeholder(&fqn));
            self.collect_nested_names(&fqn, &decl.types);
        }
    }

    // ── Compilation (pass 2) ─────────────────────────────────────────────────

    /// Compile one type (the root or an inline sub-type) and insert it into
    /// the registry.  Recursively compiles nested `types:` declarations.
    ///
    /// Enums are lexically scoped: a type sees the enums declared by every
    /// type that encloses it, and an enum declared on the type itself shadows
    /// an enclosing one of the same name. Sibling and nested types' enums are
    /// not visible.
    fn compile_type_decl(
        &mut self,
        fqn: &str, // fully-qualified name for this type
        seq: &[RawField],
        types: &IndexMap<String, RawTypeDecl>,
        enums: &IndexMap<String, HashMap<String, String>>,
        outer_enums: &HashMap<String, CompiledEnum>,
        (endian, bit_endian): (Endian, Endian),
    ) -> Result<()> {
        // Enums visible in this type: the enclosing ones, then its own.
        let mut compiled_enums = outer_enums.clone();
        compiled_enums.extend(compile_enums(enums, fqn)?);

        // Compile each field
        let mut fields = Vec::with_capacity(seq.len());
        let mut seen_ids: Vec<String> = Vec::new(); // for forward-ref validation

        for raw_field in seq {
            let ctx = FieldCtx {
                parent_fqn: fqn,
                seen_ids: &seen_ids,
                enums: &compiled_enums,
            };
            let compiled = self.compile_field(raw_field, &ctx)?;
            seen_ids.push(raw_field.id.clone());
            fields.push(compiled);
        }

        let ct = CompiledType {
            name: fqn.to_owned(),
            doc: None, // doc not propagated here; add if needed
            endian,
            bit_endian,
            fields,
            enums: compiled_enums.clone(),
        };
        self.registry.insert(fqn.to_owned(), ct);

        // Recurse into nested types
        for (name, decl) in types {
            let child_fqn = format!("{fqn}::{name}");
            // Nested types inherit the byte and bit order from the parent
            // (RawTypeDecl has no meta; both are purely inherited)
            self.compile_type_decl(
                &child_fqn,
                &decl.seq,
                &decl.types,
                &decl.enums,
                &compiled_enums,
                (endian, bit_endian),
            )?;
        }

        Ok(())
    }

    fn compile_field(&self, raw: &RawField, ctx: &FieldCtx<'_>) -> Result<CompiledField> {
        let kind = if let Some(contents) = &raw.contents {
            compile_contents(contents, &raw.id, ctx.parent_fqn)?
        } else {
            let type_name = raw
                .type_ref
                .as_deref()
                .ok_or_else(|| DoeError::FieldError {
                    field: raw.id.clone(),
                    context: ctx.parent_fqn.to_owned(),
                    message: "field has neither 'type' nor 'contents'".to_owned(),
                })?;
            self.compile_type_ref(type_name, raw, ctx)?
        };

        let repeat = compile_repeat(raw, ctx)?;
        let if_expr = raw.if_expr.as_deref().map(expr::parse).transpose()?;
        let pos = raw
            .pos
            .as_ref()
            .map(|p| expr::parse(&p.as_str()))
            .transpose()?;

        // Validate enum reference if present
        if let Some(enum_name) = &raw.enum_ref
            && !ctx.enums.contains_key(enum_name.as_str())
        {
            return Err(DoeError::UnknownEnum {
                enum_name: enum_name.clone(),
                context: format!("{}.{}", ctx.parent_fqn, raw.id),
            });
        }

        Ok(CompiledField {
            id: raw.id.clone(),
            doc: raw.doc.clone(),
            kind,
            repeat,
            if_expr,
            pos,
            enum_ref: raw.enum_ref.clone(),
        })
    }

    /// Resolve a type name string to a `FieldKind`.
    fn compile_type_ref(
        &self,
        type_name: &str,
        raw: &RawField,
        ctx: &FieldCtx<'_>,
    ) -> Result<FieldKind> {
        let context = format!("{}.{}", ctx.parent_fqn, raw.id);

        let fixed = numeric_type(type_name).or_else(|| {
            bit_type(type_name).map(|(bit_size, endian)| FieldKind::Bits { bit_size, endian })
        });
        if let Some(kind) = fixed {
            if raw.size.is_some() || raw.size_eos == Some(true) {
                return Err(DoeError::FieldError {
                    field: raw.id.clone(),
                    context: ctx.parent_fqn.to_owned(),
                    message: format!(
                        "'{type_name}' has a fixed size; 'size' and 'size-eos' are for bytes, str and user types"
                    ),
                });
            }
            return match kind {
                FieldKind::Bits { bit_size, endian } => bit_field(bit_size, endian, raw, ctx),
                other => Ok(other),
            };
        }

        match type_name {
            // ── Bytes ────────────────────────────────────────────────────
            "bytes" => {
                let size = compile_size(raw, ctx)?;
                Ok(FieldKind::Bytes { size })
            }

            // ── Strings ──────────────────────────────────────────────────
            "strz" => {
                let enc = compile_encoding(raw.encoding);
                Ok(FieldKind::Str {
                    size: StrSize::Terminator(raw.terminator.unwrap_or(0x00)),
                    encoding: enc,
                })
            }
            "str" => {
                let enc = compile_encoding(raw.encoding);
                let str_size = if let Some(term) = raw.terminator {
                    StrSize::Terminator(term)
                } else {
                    StrSize::Fixed(compile_size(raw, ctx)?)
                };
                Ok(FieldKind::Str {
                    size: str_size,
                    encoding: enc,
                })
            }

            // ── Bits ─────────────────────────────────────────────────────
            "bits" => {
                let bit_size = raw.bit_size.ok_or_else(|| DoeError::FieldError {
                    field: raw.id.clone(),
                    context: ctx.parent_fqn.to_owned(),
                    message: "'bits' type requires 'bit_size'".to_owned(),
                })?;
                bit_field(bit_size, EndianOverride::Inherit, raw, ctx)
            }

            // ── User-defined type ────────────────────────────────────────
            other => {
                // Try fully-qualified resolution first, then relative to parent
                let fqn = self.resolve_type_ref(other, ctx.parent_fqn);
                match fqn {
                    Some(name) => Ok(FieldKind::TypeRef {
                        type_name: name,
                        size: compile_optional_size(raw, ctx)?,
                    }),
                    None => Err(old_numeric_name(other).map_or_else(
                        || DoeError::UnknownType {
                            type_name: other.to_owned(),
                            context: context.clone(),
                        },
                        |renamed| DoeError::FieldError {
                            field: raw.id.clone(),
                            context: ctx.parent_fqn.to_owned(),
                            message: format!(
                                "type '{other}' was renamed '{renamed}': numeric types are named by their size in bytes (u1, u2, u4, u8, s1, s2, s4, s8, f4, f8), as in Kaitai Struct"
                            ),
                        },
                    )),
                }
            }
        }
    }

    /// Resolve a bare or partially-qualified type name to a fully-qualified one
    /// that exists in the registry.
    ///
    /// Search order:
    /// 1. Bare name as-is (for top-level imported types like `"zlib_block"`).
    /// 2. `<parent_fqn>::<name>` (inline sibling type).
    /// 3. Walk up the parent hierarchy: `<grandparent>::<name>`, etc.
    fn resolve_type_ref(&self, name: &str, parent_fqn: &str) -> Option<String> {
        // 1. Bare name
        if self.registry.contains(name) {
            return Some(name.to_owned());
        }

        // 2 & 3. Walk up parent hierarchy
        let mut scope = parent_fqn;
        loop {
            let candidate = format!("{scope}::{name}");
            if self.registry.contains(&candidate) {
                return Some(candidate);
            }
            // Strip one level
            match scope.rsplit_once("::") {
                Some((parent, _)) => scope = parent,
                None => break,
            }
        }

        None
    }

    // ── Path resolution ───────────────────────────────────────────────────────

    fn resolve_path(&self, name_or_path: &str, relative_to: Option<&Path>) -> Result<PathBuf> {
        // Relative path: starts with "./" or "../"
        if name_or_path.starts_with("./") || name_or_path.starts_with("../") {
            let base = relative_to.unwrap_or_else(|| Path::new("."));
            let candidate = base.join(name_or_path);
            if candidate.exists() {
                return Ok(candidate);
            }
            return Err(DoeError::SchemaNotFound {
                name: name_or_path.to_owned(),
                searched: vec![candidate],
            });
        }

        // Bare name: search include paths
        let stem = if crate::has_schema_extension(name_or_path) {
            name_or_path.to_owned()
        } else {
            format!("{name_or_path}.yaml")
        };

        let mut searched = Vec::new();
        for dir in &self.include_paths {
            let candidate = dir.join(&stem);
            searched.push(candidate.clone());
            if candidate.exists() {
                return Ok(candidate);
            }
        }

        Err(DoeError::SchemaNotFound {
            name: name_or_path.to_owned(),
            searched,
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper structs and functions
// ─────────────────────────────────────────────────────────────────────────────

/// Context passed through field compilation.
struct FieldCtx<'a> {
    parent_fqn: &'a str,
    seen_ids: &'a [String],
    enums: &'a HashMap<String, CompiledEnum>,
}

/// Create an empty placeholder `CompiledType` used during name collection.
fn placeholder(fqn: &str) -> CompiledType {
    CompiledType {
        name: fqn.to_owned(),
        doc: None,
        endian: Endian::Little,
        bit_endian: Endian::Big,
        fields: vec![],
        enums: HashMap::new(),
    }
}

fn raw_endian_to_ir(e: RawEndian) -> Endian {
    match e {
        RawEndian::Le => Endian::Little,
        RawEndian::Be => Endian::Big,
    }
}

fn compile_encoding(enc: Option<RawEncoding>) -> Encoding {
    match enc.unwrap_or(RawEncoding::Utf8) {
        RawEncoding::Utf8 => Encoding::Utf8,
        RawEncoding::Ascii => Encoding::Ascii,
        RawEncoding::Latin1 => Encoding::Latin1,
    }
}

fn compile_size(raw: &RawField, ctx: &FieldCtx<'_>) -> Result<SizeExpr> {
    if raw.size_eos == Some(true) {
        if raw.size.is_some() {
            return Err(DoeError::FieldError {
                field: raw.id.clone(),
                context: ctx.parent_fqn.to_owned(),
                message: "'size' and 'size-eos' cannot be used together".to_owned(),
            });
        }
        return Ok(SizeExpr::Eos);
    }
    match &raw.size {
        None => Err(DoeError::FieldError {
            field: raw.id.clone(),
            context: ctx.parent_fqn.to_owned(),
            message: format!(
                "'{}' type requires 'size'",
                raw.type_ref.as_deref().unwrap_or("?")
            ),
        }),
        Some(StringOrInt::Int(n)) => {
            usize::try_from(*n)
                .map(SizeExpr::Literal)
                .map_err(|_| DoeError::FieldError {
                    field: raw.id.clone(),
                    context: ctx.parent_fqn.to_owned(),
                    message: format!("size {n} is too large"),
                })
        }
        Some(StringOrInt::Str(s)) => {
            // Validate: all bare identifiers in the expression must refer to
            // fields that appear earlier in the same seq.
            validate_size_expr_refs(s, ctx)?;
            let ast = expr::parse(s)?;
            Ok(SizeExpr::Dynamic(ast))
        }
    }
}

/// Compile `size` or `size-eos` if the field has either.
fn compile_optional_size(raw: &RawField, ctx: &FieldCtx<'_>) -> Result<Option<SizeExpr>> {
    if raw.size.is_none() && raw.size_eos != Some(true) {
        return Ok(None);
    }
    compile_size(raw, ctx).map(Some)
}

/// Parse and compile a `repeat-expr` expression.
fn compile_repeat(raw: &RawField, ctx: &FieldCtx<'_>) -> Result<RepeatMode> {
    match raw.repeat {
        None => Ok(RepeatMode::Once),
        Some(RawRepeat::Eos) => Ok(RepeatMode::Eos),
        Some(RawRepeat::Until) => {
            let s = raw
                .repeat_until
                .as_deref()
                .ok_or_else(|| DoeError::FieldError {
                    field: raw.id.clone(),
                    context: ctx.parent_fqn.to_owned(),
                    message: "'repeat: until' requires 'repeat-until'".to_owned(),
                })?;
            let ast = expr::parse(s)?;
            Ok(RepeatMode::Until(ast))
        }
        Some(RawRepeat::Expr) => {
            let s = raw
                .repeat_expr
                .as_deref()
                .ok_or_else(|| DoeError::FieldError {
                    field: raw.id.clone(),
                    context: ctx.parent_fqn.to_owned(),
                    message: "'repeat: expr' requires 'repeat-expr'".to_owned(),
                })?;
            let ast = expr::parse(s)?;
            Ok(RepeatMode::Count(ast))
        }
    }
}

fn compile_contents(contents: &RawContents, field_id: &str, parent_fqn: &str) -> Result<FieldKind> {
    let bytes = match contents {
        RawContents::Bytes(b) => b.clone(),
        RawContents::Str(s) => s.as_bytes().to_vec(),
    };
    if bytes.is_empty() {
        return Err(DoeError::FieldError {
            field: field_id.to_owned(),
            context: parent_fqn.to_owned(),
            message: "'contents' must not be empty".to_owned(),
        });
    }
    Ok(FieldKind::Contents(bytes))
}

/// Compile `enums:` blocks into `CompiledEnum` maps.
fn compile_enums(
    raw_enums: &IndexMap<String, HashMap<String, String>>,
    context: &str,
) -> Result<HashMap<String, CompiledEnum>> {
    let mut result = HashMap::new();
    for (enum_name, raw_map) in raw_enums {
        let mut ce = CompiledEnum::default();
        for (key_str, variant_name) in raw_map {
            let key: u64 = parse_enum_key(key_str).ok_or_else(|| {
                DoeError::Schema(format!(
                    "enum '{enum_name}' in {context}: invalid key '{key_str}'"
                ))
            })?;
            ce.variants.insert(key, variant_name.clone());
        }
        result.insert(enum_name.clone(), ce);
    }
    Ok(result)
}

/// Parse an enum discriminant key — decimal or `0x`-prefixed hex.
fn parse_enum_key(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u64>().ok()
    }
}

/// The numeric type a Kaitai-style name stands for: `u1`, `u2`, `u4`, `u8`,
/// `s1`..`s8` (integers) or `f4`, `f8` (floats), with an optional `le` or `be`.
fn numeric_type(name: &str) -> Option<FieldKind> {
    let mut chars = name.chars();
    let family = chars.next()?;
    let bytes = chars.next()?.to_digit(10)?;
    let endian = match chars.as_str() {
        "" => EndianOverride::Inherit,
        "le" => EndianOverride::Little,
        "be" => EndianOverride::Big,
        _ => return None,
    };
    match (family, bytes) {
        ('u' | 's', 1 | 2 | 4 | 8) => {
            let width = match bytes {
                1 => IntWidth::W8,
                2 => IntWidth::W16,
                4 => IntWidth::W32,
                _ => IntWidth::W64,
            };
            Some(if family == 'u' {
                FieldKind::UInt { width, endian }
            } else {
                FieldKind::SInt { width, endian }
            })
        }
        ('f', 4) => Some(FieldKind::Float {
            width: FloatWidth::F32,
            endian,
        }),
        ('f', 8) => Some(FieldKind::Float {
            width: FloatWidth::F64,
            endian,
        }),
        _ => None,
    }
}

/// The size and bit order of a bit-field type name: `b1`..`b64`, with an
/// optional `le` or `be`.
fn bit_type(name: &str) -> Option<(u8, EndianOverride)> {
    let rest = name.strip_prefix('b')?;
    let digits_end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let (digits, suffix) = rest.split_at(digits_end);
    if digits.starts_with('0') {
        return None;
    }
    let endian = match suffix {
        "" => EndianOverride::Inherit,
        "le" => EndianOverride::Little,
        "be" => EndianOverride::Big,
        _ => return None,
    };
    let bits: u8 = digits.parse().ok()?;
    (1..=64).contains(&bits).then_some((bits, endian))
}

fn bit_field(
    bit_size: u8,
    endian: EndianOverride,
    raw: &RawField,
    ctx: &FieldCtx<'_>,
) -> Result<FieldKind> {
    if !(1..=64).contains(&bit_size) {
        return Err(DoeError::FieldError {
            field: raw.id.clone(),
            context: ctx.parent_fqn.to_owned(),
            message: format!("a bit field has 1 to 64 bits, not {bit_size}"),
        });
    }
    Ok(FieldKind::Bits { bit_size, endian })
}

/// The new name of a numeric type spelled the way doe used to spell it, such
/// as `u16le` (now `u2le`), `i32` (now `s4`) or `f64` (now `f8`).
fn old_numeric_name(name: &str) -> Option<String> {
    let family = name.chars().next()?;
    let rest = name.get(1..)?;
    let digits_end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let (bits, suffix) = rest.split_at(digits_end);
    if !matches!(suffix, "" | "le" | "be") {
        return None;
    }
    let bytes = match bits {
        "8" => 1,
        "16" => 2,
        "32" => 4,
        "64" => 8,
        _ => return None,
    };
    let family = match family {
        'u' => 'u',
        'i' => 's',
        'f' => 'f',
        _ => return None,
    };
    Some(format!("{family}{bytes}{suffix}"))
}

/// Check that a dynamic size/repeat-count expression only references field
/// names that have already appeared in the sequence (i.e. can have been
/// parsed before this field is encountered).
fn validate_size_expr_refs(expr_str: &str, ctx: &FieldCtx<'_>) -> Result<()> {
    // We walk identifiers in the expression string with a simple token scan
    // rather than re-implementing the AST walk; the expression parser will
    // catch actual syntax errors.
    let ast = expr::parse(expr_str)?;
    check_field_refs_in_expr(&ast, ctx, expr_str)
}

fn check_field_refs_in_expr(e: &expr::Expr, ctx: &FieldCtx<'_>, src: &str) -> Result<()> {
    use expr::Expr;
    match e {
        Expr::Int(_) | Expr::Float(_) | Expr::Bool(_) | Expr::Str(_) => Ok(()),
        Expr::Array(items) => items
            .iter()
            .try_for_each(|item| check_field_refs_in_expr(item, ctx, src)),
        Expr::Member(recv, _) => check_field_refs_in_expr(recv, ctx, src),
        Expr::Call(recv, _, args) => {
            check_field_refs_in_expr(recv, ctx, src)?;
            args.iter()
                .try_for_each(|arg| check_field_refs_in_expr(arg, ctx, src))
        }
        Expr::Index(target, index) => {
            check_field_refs_in_expr(target, ctx, src)?;
            check_field_refs_in_expr(index, ctx, src)
        }
        Expr::Ternary(cond, then, otherwise) => {
            check_field_refs_in_expr(cond, ctx, src)?;
            check_field_refs_in_expr(then, ctx, src)?;
            check_field_refs_in_expr(otherwise, ctx, src)
        }
        Expr::Field(parts) => {
            // Skip _parent references — those are resolved at parse time
            if parts.first().map(String::as_str) == Some("_parent") {
                return Ok(());
            }
            let Some(name) = parts.first() else {
                return Ok(());
            };
            if !ctx.seen_ids.iter().any(|id| id == name) {
                return Err(DoeError::FieldError {
                    field: name.clone(),
                    context: ctx.parent_fqn.to_owned(),
                    message: format!(
                        "size expression '{src}' references '{name}' which has not been defined yet in this seq"
                    ),
                });
            }
            Ok(())
        }
        Expr::Unary(_, inner) => check_field_refs_in_expr(inner, ctx, src),
        Expr::Binary(_, lhs, rhs) => {
            check_field_refs_in_expr(lhs, ctx, src)?;
            check_field_refs_in_expr(rhs, ctx, src)
        }
    }
}

/// Load a YAML schema from disk.
///
/// # Errors
///
/// Returns [`DoeError::Io`] if the file cannot be read, or [`DoeError::Yaml`]
/// if it is not a valid schema.
pub fn load_yaml(path: &Path) -> Result<RawSchema> {
    let text = std::fs::read_to_string(path).map_err(|e| DoeError::Io {
        path: path.to_owned(),
        source: e,
    })?;
    let mut schema: RawSchema = RawSchema::from_yaml_str(&text).map_err(|e| DoeError::Yaml {
        path: path.to_owned(),
        source: Box::new(e),
    })?;
    schema.source_path = Some(path.to_owned());
    Ok(schema)
}
