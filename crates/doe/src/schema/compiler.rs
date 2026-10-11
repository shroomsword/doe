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
#[allow(dead_code)]
pub fn compile_schema(schema: &RawSchema) -> Result<TypeRegistry> {
    let mut compiler = Compiler::new(vec![]);
    compiler.process_schema(schema)?;
    Ok(compiler.registry)
}

// ─────────────────────────────────────────────────────────────────────────────
// Compiler state
// ─────────────────────────────────────────────────────────────────────────────

/// Stateful compiler that accumulates types across multiple schema files.
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
    pub fn load(&mut self, name_or_path: &str, relative_to: Option<&Path>) -> Result<()> {
        let path = self.resolve_path(name_or_path, relative_to)?;
        let schema = load_yaml(&path)?;
        self.process_schema(&schema)
    }

    /// Compile an already-parsed schema.
    pub fn process_schema(&mut self, schema: &RawSchema) -> Result<()> {
        // If we have already processed this exact id, check whether it came
        // from the same source.  An id being loaded again via a shared import
        // is fine (idempotent); the same id from a *different* file is an error.
        if self.processed.contains(&schema.id) {
            let incoming = schema
                .source_path
                .as_deref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<in-memory>".to_owned());
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
        self.compile_type_decl(
            &schema.id,
            &schema.id,
            &schema.seq,
            &schema.types,
            &schema.enums,
            endian,
        )?;

        self.loading_stack.pop();
        self.processed.insert(schema.id.clone());
        // Record source for future duplicate detection.
        let source = schema
            .source_path
            .as_deref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<in-memory>".to_owned());
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
            let fqn = format!("{}::{}", prefix, name);
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
    fn compile_type_decl(
        &mut self,
        fqn: &str,     // fully-qualified name for this type
        type_id: &str, // bare name (last segment), used in error messages
        seq: &[RawField],
        types: &IndexMap<String, RawTypeDecl>,
        enums: &IndexMap<String, HashMap<String, String>>,
        endian: Endian,
    ) -> Result<()> {
        // Compile enums for this type
        let compiled_enums = compile_enums(enums, fqn)?;

        // Compile each field
        let mut fields = Vec::with_capacity(seq.len());
        let mut seen_ids: Vec<String> = Vec::new(); // for forward-ref validation

        for raw_field in seq {
            let ctx = FieldCtx {
                parent_fqn: fqn,
                type_id,
                seen_ids: &seen_ids,
                endian,
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
            fields,
            enums: compiled_enums,
        };
        self.registry.insert(fqn.to_owned(), ct);

        // Recurse into nested types
        for (name, decl) in types {
            let child_fqn = format!("{}::{}", fqn, name);
            // Nested types inherit endian from parent unless they override it
            // (RawTypeDecl has no meta; endian is purely inherited)
            self.compile_type_decl(
                &child_fqn,
                name,
                &decl.seq,
                &decl.types,
                &decl.enums,
                endian,
            )?;
        }

        Ok(())
    }

    fn compile_field(&self, raw: &RawField, ctx: &FieldCtx) -> Result<CompiledField> {
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
        let if_expr = raw.if_expr.as_deref().map(|s| expr::parse(s)).transpose()?;

        // Validate enum reference if present
        if let Some(enum_name) = &raw.enum_ref {
            if !ctx.enums.contains_key(enum_name.as_str()) {
                return Err(DoeError::UnknownEnum {
                    enum_name: enum_name.clone(),
                    context: format!("{}.{}", ctx.parent_fqn, raw.id),
                });
            }
        }

        Ok(CompiledField {
            id: raw.id.clone(),
            doc: raw.doc.clone(),
            kind,
            repeat,
            if_expr,
            enum_ref: raw.enum_ref.clone(),
        })
    }

    /// Resolve a type name string to a `FieldKind`.
    fn compile_type_ref(
        &self,
        type_name: &str,
        raw: &RawField,
        ctx: &FieldCtx,
    ) -> Result<FieldKind> {
        let context = format!("{}.{}", ctx.parent_fqn, raw.id);

        match type_name {
            // ── Unsigned integers ────────────────────────────────────────
            "u8" => Ok(FieldKind::UInt {
                width: IntWidth::W8,
                endian: EndianOverride::Inherit,
            }),
            "u16" => Ok(FieldKind::UInt {
                width: IntWidth::W16,
                endian: EndianOverride::Inherit,
            }),
            "u32" => Ok(FieldKind::UInt {
                width: IntWidth::W32,
                endian: EndianOverride::Inherit,
            }),
            "u64" => Ok(FieldKind::UInt {
                width: IntWidth::W64,
                endian: EndianOverride::Inherit,
            }),
            "u16le" => Ok(FieldKind::UInt {
                width: IntWidth::W16,
                endian: EndianOverride::Little,
            }),
            "u32le" => Ok(FieldKind::UInt {
                width: IntWidth::W32,
                endian: EndianOverride::Little,
            }),
            "u64le" => Ok(FieldKind::UInt {
                width: IntWidth::W64,
                endian: EndianOverride::Little,
            }),
            "u16be" => Ok(FieldKind::UInt {
                width: IntWidth::W16,
                endian: EndianOverride::Big,
            }),
            "u32be" => Ok(FieldKind::UInt {
                width: IntWidth::W32,
                endian: EndianOverride::Big,
            }),
            "u64be" => Ok(FieldKind::UInt {
                width: IntWidth::W64,
                endian: EndianOverride::Big,
            }),

            // ── Signed integers ──────────────────────────────────────────
            "i8" => Ok(FieldKind::SInt {
                width: IntWidth::W8,
                endian: EndianOverride::Inherit,
            }),
            "i16" => Ok(FieldKind::SInt {
                width: IntWidth::W16,
                endian: EndianOverride::Inherit,
            }),
            "i32" => Ok(FieldKind::SInt {
                width: IntWidth::W32,
                endian: EndianOverride::Inherit,
            }),
            "i64" => Ok(FieldKind::SInt {
                width: IntWidth::W64,
                endian: EndianOverride::Inherit,
            }),
            "i16le" => Ok(FieldKind::SInt {
                width: IntWidth::W16,
                endian: EndianOverride::Little,
            }),
            "i32le" => Ok(FieldKind::SInt {
                width: IntWidth::W32,
                endian: EndianOverride::Little,
            }),
            "i64le" => Ok(FieldKind::SInt {
                width: IntWidth::W64,
                endian: EndianOverride::Little,
            }),
            "i16be" => Ok(FieldKind::SInt {
                width: IntWidth::W16,
                endian: EndianOverride::Big,
            }),
            "i32be" => Ok(FieldKind::SInt {
                width: IntWidth::W32,
                endian: EndianOverride::Big,
            }),
            "i64be" => Ok(FieldKind::SInt {
                width: IntWidth::W64,
                endian: EndianOverride::Big,
            }),

            // ── Floats ───────────────────────────────────────────────────
            "f32" => Ok(FieldKind::Float {
                width: FloatWidth::F32,
            }),
            "f64" => Ok(FieldKind::Float {
                width: FloatWidth::F64,
            }),

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
                Ok(FieldKind::Bits { bit_size })
            }

            // ── User-defined type ────────────────────────────────────────
            other => {
                // Try fully-qualified resolution first, then relative to parent
                let fqn = self.resolve_type_ref(other, ctx.parent_fqn);
                match fqn {
                    Some(name) => Ok(FieldKind::TypeRef { type_name: name }),
                    None => Err(DoeError::UnknownType {
                        type_name: other.to_owned(),
                        context,
                    }),
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
            let candidate = format!("{}::{}", scope, name);
            if self.registry.contains(&candidate) {
                return Some(candidate);
            }
            // Strip one level
            match scope.rfind("::") {
                Some(idx) => scope = &scope[..idx],
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
        let stem = if name_or_path.ends_with(".yaml") || name_or_path.ends_with(".yml") {
            name_or_path.to_owned()
        } else {
            format!("{}.yaml", name_or_path)
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
#[allow(dead_code)]
struct FieldCtx<'a> {
    parent_fqn: &'a str,
    type_id: &'a str,
    seen_ids: &'a [String],
    endian: Endian,
    enums: &'a HashMap<String, CompiledEnum>,
}

/// Create an empty placeholder `CompiledType` used during name collection.
fn placeholder(fqn: &str) -> CompiledType {
    CompiledType {
        name: fqn.to_owned(),
        doc: None,
        endian: Endian::Little,
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

fn compile_size(raw: &RawField, ctx: &FieldCtx) -> Result<SizeExpr> {
    match &raw.size {
        None => Err(DoeError::FieldError {
            field: raw.id.clone(),
            context: ctx.parent_fqn.to_owned(),
            message: format!(
                "'{}' type requires 'size'",
                raw.type_ref.as_deref().unwrap_or("?")
            ),
        }),
        Some(StringOrInt::Int(n)) => Ok(SizeExpr::Literal(*n as usize)),
        Some(StringOrInt::Str(s)) => {
            // Validate: all bare identifiers in the expression must refer to
            // fields that appear earlier in the same seq.
            validate_size_expr_refs(s, ctx)?;
            let ast = expr::parse(s)?;
            Ok(SizeExpr::Dynamic(ast))
        }
    }
}

/// Parse and compile a `repeat-expr` expression.
fn compile_repeat(raw: &RawField, ctx: &FieldCtx) -> Result<RepeatMode> {
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

/// Check that a dynamic size/repeat-count expression only references field
/// names that have already appeared in the sequence (i.e. can have been
/// parsed before this field is encountered).
fn validate_size_expr_refs(expr_str: &str, ctx: &FieldCtx) -> Result<()> {
    // We walk identifiers in the expression string with a simple token scan
    // rather than re-implementing the AST walk; the expression parser will
    // catch actual syntax errors.
    let ast = expr::parse(expr_str)?;
    check_field_refs_in_expr(&ast, ctx, expr_str)
}

fn check_field_refs_in_expr(e: &expr::Expr, ctx: &FieldCtx, src: &str) -> Result<()> {
    use expr::Expr;
    match e {
        Expr::Int(_) | Expr::Bool(_) => Ok(()),
        Expr::Field(parts) => {
            // Skip _parent references — those are resolved at parse time
            if parts.first().map(String::as_str) == Some("_parent") {
                return Ok(());
            }
            let name = &parts[0];
            if !ctx.seen_ids.iter().any(|id| id == name) {
                return Err(DoeError::FieldError {
                    field: name.clone(),
                    context: ctx.parent_fqn.to_owned(),
                    message: format!(
                        "size expression '{}' references '{}' which has not been defined yet in this seq",
                        src, name
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
pub fn load_yaml(path: &Path) -> Result<RawSchema> {
    let text = std::fs::read_to_string(path).map_err(|e| DoeError::Io {
        path: path.to_owned(),
        source: e,
    })?;
    let mut schema: RawSchema = serde_yaml::from_str(&text).map_err(|e| DoeError::Yaml {
        path: path.to_owned(),
        source: e,
    })?;
    schema.source_path = Some(path.to_owned());
    Ok(schema)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ir::{EndianOverride, FieldKind, IntWidth, RepeatMode, StrSize};
    use indoc::indoc;

    fn compile(yaml: &str) -> TypeRegistry {
        let schema: RawSchema = serde_yaml::from_str(yaml).expect("YAML parse");
        compile_schema(&schema).expect("compile")
    }

    fn compile_err(yaml: &str) -> DoeError {
        let schema: RawSchema = serde_yaml::from_str(yaml).expect("YAML parse");
        compile_schema(&schema).expect_err("expected compile error")
    }

    // ── Primitive integer types ───────────────────────────────────────────────

    #[test]
    fn compile_u8_field() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: x
                type: u8
        "#});
        let ty = reg.get("t").unwrap();
        assert!(matches!(
            ty.fields[0].kind,
            FieldKind::UInt {
                width: IntWidth::W8,
                endian: EndianOverride::Inherit
            }
        ));
    }

    #[test]
    fn compile_u32be_field() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: x
                type: u32be
        "#});
        let ty = reg.get("t").unwrap();
        assert!(matches!(
            ty.fields[0].kind,
            FieldKind::UInt {
                width: IntWidth::W32,
                endian: EndianOverride::Big
            }
        ));
    }

    #[test]
    fn compile_u64le_field() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: x
                type: u64le
        "#});
        let ty = reg.get("t").unwrap();
        assert!(matches!(
            ty.fields[0].kind,
            FieldKind::UInt {
                width: IntWidth::W64,
                endian: EndianOverride::Little
            }
        ));
    }

    #[test]
    fn compile_i32_field() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: x
                type: i32
        "#});
        let ty = reg.get("t").unwrap();
        assert!(matches!(
            ty.fields[0].kind,
            FieldKind::SInt {
                width: IntWidth::W32,
                endian: EndianOverride::Inherit
            }
        ));
    }

    #[test]
    fn compile_i16be_field() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: x
                type: i16be
        "#});
        let ty = reg.get("t").unwrap();
        assert!(matches!(
            ty.fields[0].kind,
            FieldKind::SInt {
                width: IntWidth::W16,
                endian: EndianOverride::Big
            }
        ));
    }

    #[test]
    fn compile_all_unsigned_widths() {
        for (type_str, expected_width) in &[
            ("u8", IntWidth::W8),
            ("u16", IntWidth::W16),
            ("u32", IntWidth::W32),
            ("u64", IntWidth::W64),
        ] {
            let yaml = format!("id: t\nseq:\n  - id: x\n    type: {}", type_str);
            let reg = compile(&yaml);
            let ty = reg.get("t").unwrap();
            if let FieldKind::UInt { width, .. } = ty.fields[0].kind {
                assert_eq!(&width, expected_width, "width mismatch for {}", type_str);
            } else {
                panic!("expected UInt for {}", type_str);
            }
        }
    }

    // ── Float types ───────────────────────────────────────────────────────────

    #[test]
    fn compile_f32_field() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: x
                type: f32
        "#});
        let ty = reg.get("t").unwrap();
        assert!(matches!(
            ty.fields[0].kind,
            FieldKind::Float {
                width: FloatWidth::F32
            }
        ));
    }

    #[test]
    fn compile_f64_field() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: x
                type: f64
        "#});
        let ty = reg.get("t").unwrap();
        assert!(matches!(
            ty.fields[0].kind,
            FieldKind::Float {
                width: FloatWidth::F64
            }
        ));
    }

    // ── Bytes and strings ─────────────────────────────────────────────────────

    #[test]
    fn compile_bytes_literal_size() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: buf
                type: bytes
                size: 16
        "#});
        let ty = reg.get("t").unwrap();
        assert!(matches!(
            ty.fields[0].kind,
            FieldKind::Bytes {
                size: SizeExpr::Literal(16)
            }
        ));
    }

    #[test]
    fn compile_bytes_dynamic_size() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: len
                type: u32
              - id: buf
                type: bytes
                size: len
        "#});
        let ty = reg.get("t").unwrap();
        assert!(matches!(
            ty.fields[1].kind,
            FieldKind::Bytes {
                size: SizeExpr::Dynamic(_)
            }
        ));
    }

    #[test]
    fn compile_str_with_size() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: name
                type: str
                size: 8
                encoding: ascii
        "#});
        let ty = reg.get("t").unwrap();
        if let FieldKind::Str {
            size: StrSize::Fixed(SizeExpr::Literal(8)),
            encoding,
        } = &ty.fields[0].kind
        {
            assert_eq!(*encoding, Encoding::Ascii);
        } else {
            panic!("unexpected field kind: {:?}", ty.fields[0].kind);
        }
    }

    #[test]
    fn compile_strz() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: name
                type: strz
        "#});
        let ty = reg.get("t").unwrap();
        assert!(matches!(
            ty.fields[0].kind,
            FieldKind::Str {
                size: StrSize::Terminator(0),
                ..
            }
        ));
    }

    #[test]
    fn compile_str_with_terminator() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: name
                type: str
                terminator: 0x0a
        "#});
        let ty = reg.get("t").unwrap();
        assert!(matches!(
            ty.fields[0].kind,
            FieldKind::Str {
                size: StrSize::Terminator(0x0a),
                ..
            }
        ));
    }

    // ── Bits ─────────────────────────────────────────────────────────────────

    #[test]
    fn compile_bits_field() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: flags
                type: bits
                bit_size: 4
        "#});
        let ty = reg.get("t").unwrap();
        assert!(matches!(ty.fields[0].kind, FieldKind::Bits { bit_size: 4 }));
    }

    #[test]
    fn bits_without_bit_size_is_error() {
        let err = compile_err(indoc! {r#"
            id: t
            seq:
              - id: flags
                type: bits
        "#});
        assert!(matches!(err, DoeError::FieldError { .. }));
    }

    // ── Contents ─────────────────────────────────────────────────────────────

    #[test]
    fn compile_contents_bytes() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: magic
                contents: [0x89, 0x50, 0x4e, 0x47]
        "#});
        let ty = reg.get("t").unwrap();
        assert!(
            matches!(&ty.fields[0].kind, FieldKind::Contents(b) if b == &[0x89u8, 0x50, 0x4e, 0x47])
        );
    }

    #[test]
    fn compile_contents_string() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: magic
                contents: "RIFF"
        "#});
        let ty = reg.get("t").unwrap();
        assert!(matches!(&ty.fields[0].kind, FieldKind::Contents(b) if b == b"RIFF"));
    }

    // ── Repeat modes ─────────────────────────────────────────────────────────

    #[test]
    fn compile_repeat_eos() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: items
                type: u8
                repeat: eos
        "#});
        let ty = reg.get("t").unwrap();
        assert!(matches!(ty.fields[0].repeat, RepeatMode::Eos));
    }

    #[test]
    fn compile_repeat_expr() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: count
                type: u16
              - id: items
                type: u8
                repeat: expr
                repeat-expr: count
        "#});
        let ty = reg.get("t").unwrap();
        assert!(matches!(ty.fields[1].repeat, RepeatMode::Count(_)));
    }

    #[test]
    fn repeat_expr_without_repeat_expr_is_error() {
        let err = compile_err(indoc! {r#"
            id: t
            seq:
              - id: items
                type: u8
                repeat: expr
        "#});
        assert!(matches!(err, DoeError::FieldError { .. }));
    }

    #[test]
    fn compile_repeat_until() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: items
                type: u8
                repeat: until
                repeat-until: _ == 0
        "#});
        let ty = reg.get("t").unwrap();
        assert!(matches!(ty.fields[0].repeat, RepeatMode::Until(_)));
    }

    #[test]
    fn repeat_until_without_repeat_until_is_error() {
        let err = compile_err(indoc! {r#"
            id: t
            seq:
              - id: items
                type: u8
                repeat: until
        "#});
        assert!(matches!(err, DoeError::FieldError { .. }));
    }

    // ── Conditional fields ────────────────────────────────────────────────────

    #[test]
    fn compile_if_expr() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: flags
                type: u8
              - id: extra
                type: u32
                if: flags > 0
        "#});
        let ty = reg.get("t").unwrap();
        assert!(ty.fields[1].if_expr.is_some());
    }

    // ── Enums ─────────────────────────────────────────────────────────────────

    #[test]
    fn compile_enum_decimal_keys() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: kind
                type: u8
                enum: kind_t
            enums:
              kind_t:
                "0": eof
                "1": data
                "2": header
        "#});
        let ty = reg.get("t").unwrap();
        let e = ty.enums.get("kind_t").unwrap();
        assert_eq!(e.lookup(0), Some("eof"));
        assert_eq!(e.lookup(1), Some("data"));
        assert_eq!(e.lookup(99), None);
    }

    #[test]
    fn compile_enum_hex_keys() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: type_field
                type: u8
                enum: types_t
            enums:
              types_t:
                "0x01": text
                "0xff": binary
        "#});
        let ty = reg.get("t").unwrap();
        let e = ty.enums.get("types_t").unwrap();
        assert_eq!(e.lookup(0x01), Some("text"));
        assert_eq!(e.lookup(0xff), Some("binary"));
    }

    #[test]
    fn unknown_enum_ref_is_error() {
        let err = compile_err(indoc! {r#"
            id: t
            seq:
              - id: x
                type: u8
                enum: nonexistent
        "#});
        assert!(matches!(err, DoeError::UnknownEnum { .. }));
    }

    // ── Sub-types ─────────────────────────────────────────────────────────────

    #[test]
    fn compile_inline_subtype() {
        let reg = compile(indoc! {r#"
            id: outer
            seq:
              - id: hdr
                type: header
            types:
              header:
                seq:
                  - id: magic
                    type: u32
                  - id: version
                    type: u16
        "#});
        assert!(reg.contains("outer"));
        assert!(reg.contains("outer::header"));
        let hdr = reg.get("outer::header").unwrap();
        assert_eq!(hdr.fields.len(), 2);
        assert_eq!(hdr.fields[0].id, "magic");
    }

    #[test]
    fn subtype_ref_resolved_correctly() {
        let reg = compile(indoc! {r#"
            id: outer
            seq:
              - id: hdr
                type: header
            types:
              header:
                seq:
                  - id: val
                    type: u8
        "#});
        let outer = reg.get("outer").unwrap();
        if let FieldKind::TypeRef { type_name } = &outer.fields[0].kind {
            assert_eq!(type_name, "outer::header");
        } else {
            panic!("expected TypeRef");
        }
    }

    #[test]
    fn compile_nested_subtypes() {
        let reg = compile(indoc! {r#"
            id: root
            seq:
              - id: a
                type: level1
            types:
              level1:
                seq:
                  - id: b
                    type: level2
                types:
                  level2:
                    seq:
                      - id: val
                        type: u32
        "#});
        assert!(reg.contains("root"));
        assert!(reg.contains("root::level1"));
        assert!(reg.contains("root::level1::level2"));
    }

    // ── Forward-ref validation ────────────────────────────────────────────────

    #[test]
    fn size_expr_forward_ref_is_error() {
        // `buf` is defined before `len`, so `size: len` is a forward ref.
        let err = compile_err(indoc! {r#"
            id: t
            seq:
              - id: buf
                type: bytes
                size: len
              - id: len
                type: u32
        "#});
        assert!(
            matches!(err, DoeError::FieldError { .. }),
            "expected FieldError, got {:?}",
            err
        );
    }

    #[test]
    fn size_expr_back_ref_is_ok() {
        let reg = compile(indoc! {r#"
            id: t
            seq:
              - id: len
                type: u32
              - id: buf
                type: bytes
                size: len
        "#});
        assert!(reg.contains("t"));
    }

    // ── Meta endian propagation ───────────────────────────────────────────────

    #[test]
    fn big_endian_propagated_to_type() {
        let reg = compile(indoc! {r#"
            id: be_file
            meta:
              endian: be
            seq:
              - id: val
                type: u32
        "#});
        let ty = reg.get("be_file").unwrap();
        assert_eq!(ty.endian, Endian::Big);
    }

    #[test]
    fn little_endian_is_default() {
        let reg = compile(indoc! {r#"
            id: le_file
            seq:
              - id: val
                type: u32
        "#});
        let ty = reg.get("le_file").unwrap();
        assert_eq!(ty.endian, Endian::Little);
    }

    // ── Error cases ───────────────────────────────────────────────────────────

    #[test]
    fn unknown_type_ref_is_error() {
        let err = compile_err(indoc! {r#"
            id: t
            seq:
              - id: x
                type: does_not_exist
        "#});
        assert!(matches!(err, DoeError::UnknownType { .. }));
    }

    #[test]
    fn field_without_type_or_contents_is_error() {
        let err = compile_err(indoc! {r#"
            id: t
            seq:
              - id: x
        "#});
        assert!(matches!(err, DoeError::FieldError { .. }));
    }

    #[test]
    fn bytes_without_size_is_error() {
        let err = compile_err(indoc! {r#"
            id: t
            seq:
              - id: buf
                type: bytes
        "#});
        assert!(matches!(err, DoeError::FieldError { .. }));
    }

    // ── Realistic schemas ─────────────────────────────────────────────────────

    #[test]
    fn png_like_compiles() {
        let reg = compile(indoc! {r#"
            id: png
            meta:
              endian: be
            seq:
              - id: signature
                contents: [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]
              - id: chunks
                type: chunk
                repeat: eos
            types:
              chunk:
                seq:
                  - id: length
                    type: u32be
                  - id: type
                    type: str
                    size: 4
                    encoding: ascii
                  - id: body
                    type: bytes
                    size: length
                  - id: crc
                    type: u32be
        "#});
        assert!(reg.contains("png"));
        assert!(reg.contains("png::chunk"));

        let chunk = reg.get("png::chunk").unwrap();
        assert_eq!(chunk.fields.len(), 4);

        // body.size should be dynamic (references `length`)
        assert!(matches!(
            chunk.fields[2].kind,
            FieldKind::Bytes {
                size: SizeExpr::Dynamic(_)
            }
        ));
    }

    #[test]
    fn elf_like_compiles() {
        let reg = compile(indoc! {r#"
            id: elf
            seq:
              - id: header
                type: elf_header
            types:
              elf_header:
                seq:
                  - id: magic
                    contents: [0x7f, 0x45, 0x4c, 0x46]
                  - id: bitness
                    type: u8
                    enum: bits_t
                  - id: endian_byte
                    type: u8
                    enum: endian_t
                  - id: pad
                    type: bytes
                    size: 9
                  - id: e_type
                    type: u16le
                    enum: obj_type_t
                  - id: machine
                    type: u16le
                enums:
                  bits_t:
                    "1": b32
                    "2": b64
                  endian_t:
                    "1": le
                    "2": be
                  obj_type_t:
                    "0": no_file
                    "2": exec
        "#});
        assert!(reg.contains("elf"));
        assert!(reg.contains("elf::elf_header"));
        let hdr = reg.get("elf::elf_header").unwrap();
        assert_eq!(hdr.enums.len(), 3);
        assert_eq!(hdr.enums["bits_t"].lookup(2), Some("b64"));
        assert_eq!(hdr.enums["obj_type_t"].lookup(2), Some("exec"));
    }

    // ── parse_enum_key ────────────────────────────────────────────────────────

    #[test]
    fn parse_enum_key_decimal() {
        assert_eq!(parse_enum_key("0"), Some(0));
        assert_eq!(parse_enum_key("255"), Some(255));
        assert_eq!(parse_enum_key("1"), Some(1));
    }

    #[test]
    fn parse_enum_key_hex() {
        assert_eq!(parse_enum_key("0x00"), Some(0));
        assert_eq!(parse_enum_key("0xff"), Some(255));
        assert_eq!(parse_enum_key("0xFF"), Some(255));
        assert_eq!(parse_enum_key("0x10"), Some(16));
    }

    #[test]
    fn parse_enum_key_invalid() {
        assert_eq!(parse_enum_key(""), None);
        assert_eq!(parse_enum_key("abc"), None);
        assert_eq!(parse_enum_key("0xgg"), None);
    }

    // ── Duplicate type detection ──────────────────────────────────────────────

    #[test]
    fn duplicate_top_level_id_from_different_files_is_error() {
        // Two separately loaded in-memory schemas with the same id but
        // different source_path values should be rejected.
        use std::path::PathBuf;
        let mut schema_a: RawSchema = serde_yaml::from_str(indoc! {r#"
            id: my_type
            seq:
              - id: x
                type: u8
        "#})
        .unwrap();
        schema_a.source_path = Some(PathBuf::from("/types/a.yaml"));

        let mut schema_b: RawSchema = serde_yaml::from_str(indoc! {r#"
            id: my_type
            seq:
              - id: y
                type: u16
        "#})
        .unwrap();
        schema_b.source_path = Some(PathBuf::from("/types/b.yaml"));

        let mut compiler = Compiler::new(vec![]);
        compiler.process_schema(&schema_a).unwrap();
        let err = compiler.process_schema(&schema_b).unwrap_err();
        assert!(
            matches!(err, DoeError::DuplicateType { .. }),
            "expected DuplicateType, got {:?}",
            err
        );
        if let DoeError::DuplicateType {
            name,
            first,
            second,
        } = err
        {
            assert_eq!(name, "my_type");
            assert!(first.contains("a.yaml"), "first={}", first);
            assert!(second.contains("b.yaml"), "second={}", second);
        }
    }

    #[test]
    fn same_schema_loaded_twice_via_import_is_idempotent() {
        // The same schema id from the same source is not a duplicate —
        // this happens when a schema is reachable via multiple import paths.
        use std::path::PathBuf;
        let mut schema: RawSchema = serde_yaml::from_str(indoc! {r#"
            id: shared
            seq:
              - id: val
                type: u8
        "#})
        .unwrap();
        schema.source_path = Some(PathBuf::from("/types/shared.yaml"));

        let mut compiler = Compiler::new(vec![]);
        compiler.process_schema(&schema).unwrap();
        // Processing again with the same source path should succeed silently.
        compiler.process_schema(&schema).unwrap();
        assert!(compiler.registry.contains("shared"));
    }
}
