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
    CompiledEnum, CompiledField, CompiledInstance, CompiledIo, CompiledParam, CompiledType,
    CompiledValid, Cut, Encoding, Endian, EndianOverride, EndianRule, EndianSwitch, FieldKind,
    FloatWidth, InstanceKind, IntWidth, Process, RepeatMode, SizeExpr, StrSize, TypeRegistry,
};
use crate::schema::raw::{
    ExprText, RawContentItem, RawContents, RawEncoding, RawEndian, RawEndianSpec, RawEndianSwitch,
    RawEnum, RawField, RawInstance, RawMeta, RawParam, RawRepeat, RawSchema, RawSwitch, RawType,
    RawTypeDecl, RawValid, RawValidRules, StringOrInt,
};
#[cfg(feature = "fs")]
use crate::schema::source::FsSource;
use crate::schema::source::SchemaSource;

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
    /// Where the schema files are looked for and read.
    source: Box<dyn SchemaSource>,
}

impl Compiler {
    /// Create a compiler that resolves bare schema names against
    /// `include_paths`, in priority order, in the files of the filesystem.
    ///
    /// Without the `fs` feature there is no filesystem: the compiler has an
    /// empty [`MemorySource`](crate::schema::source::MemorySource), so only
    /// schemas given to [`process_schema`](Self::process_schema) are compiled,
    /// and imports are not found. Use [`with_source`](Self::with_source) with the
    /// files.
    pub fn new(include_paths: Vec<PathBuf>) -> Self {
        #[cfg(feature = "fs")]
        let source: Box<dyn SchemaSource> = Box::new(FsSource);
        #[cfg(not(feature = "fs"))]
        let source: Box<dyn SchemaSource> = Box::new(crate::schema::source::MemorySource::new());
        Compiler::with_source(include_paths, source)
    }

    /// Create a compiler that looks for schema files in `source` instead of
    /// the filesystem, for example a [`MemorySource`](crate::schema::source::MemorySource).
    /// The rules of lookup are the same.
    pub fn with_source(include_paths: Vec<PathBuf>, source: Box<dyn SchemaSource>) -> Self {
        Compiler {
            include_paths,
            registry: TypeRegistry::new(),
            loading_stack: Vec::new(),
            processed: HashSet::new(),
            source_paths: HashMap::new(),
            source,
        }
    }

    /// Read and parse the schema file at `path` from this compiler's source,
    /// as `load_yaml` does from the filesystem. Pass the result to
    /// [`process_schema`](Self::process_schema).
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or is not a valid schema.
    pub fn read_schema(&self, path: &Path) -> Result<RawSchema> {
        load_yaml_from(self.source.as_ref(), path)
    }

    /// Where an import of a schema in `importing_dir` is found, by the rules
    /// that compiling uses: a name that starts with `./` or `../` is relative to
    /// the importing file, one that starts with a slash is looked for in the
    /// include paths only, and any other is looked for next to the importing
    /// file first and then in the include paths.
    ///
    /// # Errors
    ///
    /// Returns [`DoeError::SchemaNotFound`] with the places that were looked in.
    pub fn find_import(&self, import: &str, importing_dir: Option<&Path>) -> Result<PathBuf> {
        self.resolve_import(import, importing_dir)
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
        let schema = self.read_schema(&path)?;
        self.process_schema(&schema)
    }

    /// Compile an already-parsed schema.
    ///
    /// # Errors
    ///
    /// Returns an error if the schema is invalid, conflicts with an already
    /// loaded id, or has an import cycle.
    pub fn process_schema(&mut self, schema: &RawSchema) -> Result<()> {
        if schema.id.is_empty() {
            return Err(DoeError::Schema(
                "the schema has no id (`id`, or `meta.id` in a Kaitai Struct file)".to_owned(),
            ));
        }
        if let Some(meta_id) = &schema.meta.id
            && *meta_id != schema.id
        {
            return Err(DoeError::Schema(format!(
                "the schema's id '{}' and its meta.id '{meta_id}' differ",
                schema.id
            )));
        }
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

        // Two schemas may import each other. The one that is being compiled
        // has registered its types (below), which the other one is compiled
        // against; when it comes up again it is left to the first visit.
        if self.loading_stack.contains(&schema.id) {
            return Ok(());
        }

        self.loading_stack.push(schema.id.clone());

        // Collect all type names defined in this schema (pass 1), before the
        // imports, so that a schema that imports this one back can refer to
        // them.
        self.collect_type_names(schema);

        // Process imports first (depth-first)
        for import in schema.imports.iter().chain(&schema.meta.imports) {
            let rel = schema.source_path.as_deref().and_then(|p| p.parent());
            self.load_import(import, rel)?;
        }

        // Compile all types in this schema (pass 2)
        self.compile_type_decl(
            &schema.id,
            TypeParts {
                params: &schema.params,
                seq: &schema.seq,
                types: &schema.types,
                enums: &schema.enums,
                instances: &schema.instances,
                meta: Some(&schema.meta),
            },
            &HashMap::new(),
            TypeDefaults {
                endian: Endian::Little,
                bit_endian: Endian::Big,
                encoding: None,
                runtime_endian: true,
            },
        )?;

        self.link_parent_instances();
        self.measure_types();

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

    /// Make a field compute the instances of its type that the type it
    /// reads uses as `_parent.name` before it reads that type, so that the
    /// instance is there, computed against the stream of its own type, when
    /// the nested type asks for it.
    fn link_parent_instances(&mut self) {
        let wanted = parent_instance_uses(&self.registry);
        let mut additions: Additions = Vec::new();
        for (name, ty) in &self.registry.types {
            let instance_names: HashSet<&str> =
                ty.instances.iter().map(|i| i.id.as_str()).collect();
            if instance_names.is_empty() {
                continue;
            }
            let mut per_field = Vec::new();
            for (index, field) in all_fields(ty).enumerate() {
                let mut found: Vec<String> = Vec::new();
                for target in kind_targets(&field.kind) {
                    for (up, used) in wanted.get(target).into_iter().flatten() {
                        if *up == 1
                            && instance_names.contains(used.as_str())
                            && !field.uses_instances.contains(used)
                            && !found.contains(used)
                        {
                            found.push(used.clone());
                        }
                    }
                }
                if !found.is_empty() {
                    per_field.push((index, found));
                }
            }
            if !per_field.is_empty() {
                additions.push((name.clone(), per_field));
            }
        }
        for (name, per_field) in additions {
            let Some(ty) = self.registry.types.get_mut(&name) else {
                continue;
            };
            for (index, found) in per_field {
                if let Some(field) = all_fields_mut(ty).nth(index) {
                    field.uses_instances.extend(found);
                }
            }
        }
    }

    /// Work out which types always have the same size, and whether any
    /// expression asks for the size of a field.
    fn measure_types(&mut self) {
        let mut sizes: HashMap<String, Option<u64>> = HashMap::new();
        for name in self.registry.types.keys() {
            static_size(&self.registry, name, &mut sizes, 0);
        }
        self.registry.static_sizes = sizes
            .into_iter()
            .filter_map(|(name, size)| Some((name, size?)))
            .collect();
        let mut field_sizes = HashMap::new();
        for (name, ty) in &self.registry.types {
            let mut known = HashMap::new();
            let fixed: Vec<(String, u64)> = ty
                .fields
                .iter()
                .filter_map(|field| {
                    let size = field_static_size(&self.registry, field, &mut known, 0)?;
                    Some((field.id.clone(), size))
                })
                .collect();
            if !fixed.is_empty() {
                field_sizes.insert(name.clone(), fixed);
            }
        }
        self.registry.static_field_sizes = field_sizes;
        self.registry.uses_io = self
            .registry
            .types
            .values()
            .any(|ty| ty.instances.iter().any(|instance| instance.io.is_some()));
        self.registry.uses_field_sizes = self.registry.types.values().any(|ty| {
            field_paths(type_exprs(ty))
                .iter()
                .any(|parts| parts.len() > 1 && parts.last().is_some_and(|p| p == "_sizeof"))
        });
    }

    // ── Name collection (pass 1) ─────────────────────────────────────────────

    /// Walk the schema tree and register every type name we'll produce, so
    /// that forward references within the same schema work during compilation.
    fn collect_type_names(&mut self, schema: &RawSchema) {
        // The root type
        // The enums a type sees are known now, so that an enum member
        // qualified with a type can name a type that is compiled later. A
        // malformed enum is reported when its type is compiled.
        let enums = compile_enums(&schema.enums, &schema.id).unwrap_or_default();
        self.registry
            .types
            .entry(schema.id.clone())
            .or_insert_with(|| placeholder(&schema.id, &schema.params, &enums));

        let prefix = schema.id.clone();
        self.collect_nested_names(&prefix, &schema.types, &enums);
    }

    fn collect_nested_names(
        &mut self,
        prefix: &str,
        types: &IndexMap<String, RawTypeDecl>,
        outer_enums: &HashMap<String, CompiledEnum>,
    ) {
        for (name, decl) in types {
            let fqn = format!("{prefix}::{name}");
            let mut enums = outer_enums.clone();
            enums.extend(compile_enums(&decl.enums, &fqn).unwrap_or_default());
            self.registry
                .types
                .entry(fqn.clone())
                .or_insert_with(|| placeholder(&fqn, &decl.params, &enums));
            self.collect_nested_names(&fqn, &decl.types, &enums);
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
        parts: TypeParts<'_>,
        outer_enums: &HashMap<String, CompiledEnum>,
        defaults: TypeDefaults,
    ) -> Result<()> {
        let TypeParts {
            params,
            seq,
            types,
            enums,
            instances,
            meta,
        } = parts;
        let bit_endian = meta
            .and_then(|m| m.bit_endian)
            .map_or(defaults.bit_endian, raw_endian_to_ir);
        let encoding = meta.and_then(|m| m.encoding).or(defaults.encoding);
        // Enums visible in this type: the enclosing ones, then its own.
        let mut compiled_enums = outer_enums.clone();
        compiled_enums.extend(compile_enums(enums, fqn)?);

        // `enum: type::name` names an enum of another type: it is added under
        // the name as written, so it is found like the others.
        let qualified = seq
            .iter()
            .map(|f| (f.id.as_str(), &f.enum_ref))
            .chain(instances.iter().map(|(id, i)| (id.as_str(), &i.enum_ref)));
        for (id, enum_ref) in qualified {
            let Some(written) = enum_ref.as_ref().filter(|e| e.contains("::")) else {
                continue;
            };
            let found = self
                .qualified_enum(written, fqn)
                .ok_or_else(|| DoeError::FieldError {
                    field: id.to_owned(),
                    context: fqn.to_owned(),
                    message: format!("'{written}' is not an enum of a type"),
                })?;
            compiled_enums.insert(written.clone(), found);
        }

        let instance_names: HashSet<String> = instances.keys().cloned().collect();
        if let Some(clash) = seq.iter().find(|f| instance_names.contains(&f.id)) {
            return Err(DoeError::FieldError {
                field: clash.id.clone(),
                context: fqn.to_owned(),
                message: "is both a field of 'seq' and an instance".to_owned(),
            });
        }
        let param_names: HashSet<String> = params.iter().map(|p| p.id.clone()).collect();
        let duplicate_param = params
            .iter()
            .enumerate()
            .find(|(i, p)| params.iter().take(*i).any(|earlier| earlier.id == p.id));
        let clash = duplicate_param
            .map(|(_, p)| (p.id.as_str(), "is declared twice in 'params'"))
            .or_else(|| {
                seq.iter()
                    .map(|f| f.id.as_str())
                    .chain(instance_names.iter().map(String::as_str))
                    .find(|id| param_names.contains(*id))
                    .map(|id| (id, "is both a parameter and a field or an instance"))
            });
        if let Some((id, message)) = clash {
            return Err(DoeError::FieldError {
                field: id.to_owned(),
                context: fqn.to_owned(),
                message: message.to_owned(),
            });
        }

        // The byte order: this type's own, else the one of the type around it.
        let (endian, endian_rule) = match meta.and_then(|m| m.endian.as_ref()) {
            Some(RawEndianSpec::Fixed(e)) => (raw_endian_to_ir(*e), EndianRule::Fixed),
            Some(RawEndianSpec::Switch(switch)) => {
                let ctx = FieldCtx {
                    parent_fqn: fqn,
                    seen_ids: &[],
                    enums: &compiled_enums,
                    instances: &HashSet::new(),
                    params: &param_names,
                    fixed_sizes: &HashMap::new(),
                    encoding,
                    registry: &self.registry,
                };
                let rule = compile_endian_switch(switch, &ctx)?;
                (defaults.endian, EndianRule::Switch(rule))
            }
            None if defaults.runtime_endian => (defaults.endian, EndianRule::Inherit),
            None => (defaults.endian, EndianRule::Fixed),
        };
        let runtime_endian = !matches!(endian_rule, EndianRule::Fixed);
        let defaults = TypeDefaults {
            endian,
            bit_endian,
            encoding,
            runtime_endian,
        };

        // Compile each field
        let mut fields = Vec::with_capacity(seq.len());
        let mut seen_ids: Vec<String> = Vec::new(); // for forward-ref validation
        let fixed_sizes = fixed_field_sizes(seq);

        for raw_field in seq {
            let ctx = FieldCtx {
                parent_fqn: fqn,
                seen_ids: &seen_ids,
                enums: &compiled_enums,
                instances: &instance_names,
                params: &param_names,
                fixed_sizes: &fixed_sizes,
                encoding,
                registry: &self.registry,
            };
            let compiled = self.compile_field(raw_field, &ctx)?;
            seen_ids.push(raw_field.id.clone());
            fields.push(compiled);
        }

        // Instances may use any field of the sequence and any other instance.
        let mut compiled_instances = Vec::with_capacity(instances.len());
        for (id, raw_instance) in instances {
            let ctx = FieldCtx {
                parent_fqn: fqn,
                seen_ids: &seen_ids,
                enums: &compiled_enums,
                instances: &instance_names,
                params: &param_names,
                fixed_sizes: &fixed_sizes,
                encoding,
                registry: &self.registry,
            };
            compiled_instances.push(self.compile_instance(id, raw_instance, &ctx)?);
        }

        let ct = CompiledType {
            name: fqn.to_owned(),
            doc: None, // doc not propagated here; add if needed
            endian,
            endian_rule,
            bit_endian,
            params: compile_params(params),
            fields,
            instances: compiled_instances,
            enums: compiled_enums.clone(),
        };
        self.registry.insert(fqn.to_owned(), ct);

        // Recurse into nested types
        for (name, decl) in types {
            let child_fqn = format!("{fqn}::{name}");
            // Nested types inherit the byte order, bit order and encoding of
            // the type around them, unless they give their own.
            self.compile_type_decl(
                &child_fqn,
                TypeParts {
                    params: &decl.params,
                    seq: &decl.seq,
                    types: &decl.types,
                    enums: &decl.enums,
                    instances: &decl.instances,
                    meta: decl.meta.as_ref(),
                },
                &compiled_enums,
                defaults,
            )?;
        }

        Ok(())
    }

    /// Compile one entry of `instances`: a read like a `seq` field, or a `value`.
    fn compile_instance(
        &self,
        id: &str,
        raw: &RawInstance,
        ctx: &FieldCtx<'_>,
    ) -> Result<CompiledInstance> {
        let bad = |message: String| DoeError::FieldError {
            field: id.to_owned(),
            context: ctx.parent_fqn.to_owned(),
            message,
        };
        let Some(value) = &raw.value else {
            let field = self
                .compile_field(&raw.to_field(id), ctx)
                .map_err(|e| match e {
                    DoeError::FieldError { message, .. } if message.contains("neither") => {
                        bad("an instance needs 'value', or 'type' or 'contents' to read".to_owned())
                    }
                    other => other,
                })?;
            let io = raw
                .io
                .as_deref()
                .map(|text| {
                    let expr = compile_expr(text, ctx).map_err(|e| bad(format!("'io': {e}")))?;
                    validate_size_expr_refs(text, ctx)?;
                    let uses_instances = instance_refs(&[&expr], ctx.instances);
                    Ok::<_, DoeError>(CompiledIo {
                        expr,
                        uses_instances,
                    })
                })
                .transpose()?;
            return Ok(CompiledInstance {
                id: id.to_owned(),
                kind: InstanceKind::Read(Box::new(field)),
                io,
            });
        };
        if raw.reads() {
            return Err(bad(
                "'value' cannot be combined with attributes that read data (type, contents, size, pos, repeat, ...)"
                    .to_owned(),
            ));
        }
        let expr = compile_expr(&value.0, ctx).map_err(|e| bad(format!("'value': {e}")))?;
        validate_size_expr_refs(&value.0, ctx)?;
        let if_expr = raw
            .if_expr
            .as_deref()
            .map(|text| compile_expr(text, ctx))
            .transpose()?;
        if let Some(enum_name) = &raw.enum_ref
            && !ctx.enums.contains_key(enum_name.as_str())
        {
            return Err(DoeError::UnknownEnum {
                enum_name: enum_name.clone(),
                context: format!("{}.{id}", ctx.parent_fqn),
            });
        }
        let mut exprs = vec![&expr];
        exprs.extend(if_expr.as_ref());
        let uses_instances = instance_refs(&exprs, ctx.instances);
        Ok(CompiledInstance {
            io: None,
            id: id.to_owned(),
            kind: InstanceKind::Value {
                doc: raw.doc.clone(),
                expr,
                if_expr,
                enum_ref: raw.enum_ref.clone(),
                uses_instances,
            },
        })
    }

    fn compile_field(&self, raw: &RawField, ctx: &FieldCtx<'_>) -> Result<CompiledField> {
        if raw.repeat.is_none() {
            return self.compile_field_in(raw, ctx);
        }
        // The expressions of a repeated field may use the elements read so far.
        let mut seen = ctx.seen_ids.to_vec();
        seen.push(raw.id.clone());
        let inner = FieldCtx {
            seen_ids: &seen,
            ..*ctx
        };
        let mut field = self.compile_field_in(raw, &inner)?;
        field.reads_itself = field_paths(field_exprs(&field))
            .iter()
            .any(|parts| parts.first() == Some(&raw.id));
        Ok(field)
    }

    fn compile_field_in(&self, raw: &RawField, ctx: &FieldCtx<'_>) -> Result<CompiledField> {
        let kind = if let Some(contents) = &raw.contents {
            compile_contents(contents, &raw.id, ctx.parent_fqn)?
        } else {
            match &raw.type_ref {
                Some(RawType::Name(type_name)) => self.compile_type_ref(type_name, raw, ctx)?,
                Some(RawType::Switch(switch)) => self.compile_switch(switch, raw, ctx)?,
                // As in Kaitai Struct, a field with a size and no type is bytes.
                None if raw.size.is_some()
                    || raw.size_eos == Some(true)
                    || raw.terminator.is_some() =>
                {
                    self.compile_type_ref("bytes", raw, ctx)?
                }
                None => {
                    return Err(DoeError::FieldError {
                        field: raw.id.clone(),
                        context: ctx.parent_fqn.to_owned(),
                        message: "field has neither 'type' nor 'contents'".to_owned(),
                    });
                }
            }
        };

        let repeat = compile_repeat(raw, ctx)?;
        let if_expr = raw
            .if_expr
            .as_deref()
            .map(|text| compile_expr(text, ctx))
            .transpose()?;
        let pos = raw
            .pos
            .as_ref()
            .map(|p| compile_expr(&p.as_str(), ctx))
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

        let process = compile_process(raw, &kind, ctx)?;
        let mut field = CompiledField {
            id: raw.id.clone(),
            doc: raw.doc.clone(),
            kind,
            repeat,
            if_expr,
            pos,
            enum_ref: raw.enum_ref.clone(),
            uses_instances: Vec::new(),
            reads_itself: false,
            parent_up: compile_parent(raw, ctx)?,
            valid: compile_valid(raw, ctx)?,
            process,
        };
        field.uses_instances = instance_refs(&field_exprs(&field), ctx.instances);
        Ok(field)
    }

    /// Compile a `type` that is chosen by a value.
    fn compile_switch(
        &self,
        switch: &RawSwitch,
        raw: &RawField,
        ctx: &FieldCtx<'_>,
    ) -> Result<FieldKind> {
        let bad = |message: String| DoeError::FieldError {
            field: raw.id.clone(),
            context: ctx.parent_fqn.to_owned(),
            message,
        };
        if switch.cases.is_empty() {
            return Err(bad("'switch-on' needs at least one case".to_owned()));
        }
        let on =
            compile_expr(&switch.switch_on, ctx).map_err(|e| bad(format!("'switch-on': {e}")))?;
        validate_size_expr_refs(&switch.switch_on, ctx)?;
        // A size is a window around whichever type is chosen, so the cases do
        // not take it themselves, but `bytes` and `str` fill the window.
        let size = compile_optional_size(raw, ctx)?;
        let mut cases = Vec::new();
        let mut default = None;
        for (key, type_name) in &switch.cases {
            let mut case_raw = raw.clone();
            case_raw.size = None;
            case_raw.size_eos =
                Some(size.is_some() && matches!(type_name.as_str(), "bytes" | "str"));
            case_raw.type_ref = Some(RawType::Name(type_name.clone()));
            let kind = self.compile_type_ref(type_name, &case_raw, ctx)?;
            if key == "_" {
                default = Some(Box::new(kind));
            } else {
                let key_expr =
                    compile_expr(key, ctx).map_err(|e| bad(format!("case '{key}': {e}")))?;
                cases.push((key_expr, kind));
            }
        }
        Ok(FieldKind::Switch {
            on,
            size,
            cases,
            default,
        })
    }

    /// Compile the arguments of a field's type and check them against the
    /// parameters of the type `type_fqn`.
    fn compile_type_args(
        &self,
        type_fqn: &str,
        arg_texts: &[&str],
        raw: &RawField,
        ctx: &FieldCtx<'_>,
    ) -> Result<Vec<expr::Expr>> {
        let bad = |message: String| DoeError::FieldError {
            field: raw.id.clone(),
            context: ctx.parent_fqn.to_owned(),
            message,
        };
        let wanted = self.registry.get(type_fqn).map_or(0, |ty| ty.params.len());
        if wanted != arg_texts.len() {
            let plural = |n: usize| if n == 1 { "parameter" } else { "parameters" };
            return Err(bad(format!(
                "type '{type_fqn}' takes {wanted} {}, but {} {} given",
                plural(wanted),
                arg_texts.len(),
                if arg_texts.len() == 1 { "was" } else { "were" }
            )));
        }
        arg_texts
            .iter()
            .map(|text| {
                let arg =
                    compile_expr(text, ctx).map_err(|e| bad(format!("argument '{text}': {e}")))?;
                validate_size_expr_refs(text, ctx)?;
                Ok(arg)
            })
            .collect()
    }

    /// Resolve a type name string to a `FieldKind`.
    fn compile_type_ref(
        &self,
        type_name: &str,
        raw: &RawField,
        ctx: &FieldCtx<'_>,
    ) -> Result<FieldKind> {
        let context = format!("{}.{}", ctx.parent_fqn, raw.id);

        // `name(arg, ...)` passes arguments to a type that has parameters.
        let (type_name, arg_texts) =
            split_type_call(type_name).map_err(|message| DoeError::FieldError {
                field: raw.id.clone(),
                context: ctx.parent_fqn.to_owned(),
                message,
            })?;

        if !arg_texts.is_empty()
            && (matches!(type_name, "bytes" | "str" | "strz" | "bits")
                || numeric_type(type_name).is_some()
                || bit_type(type_name).is_some())
        {
            return Err(DoeError::FieldError {
                field: raw.id.clone(),
                context: ctx.parent_fqn.to_owned(),
                message: format!(
                    "type '{type_name}' takes 0 parameters, but {} {} given",
                    arg_texts.len(),
                    if arg_texts.len() == 1 { "was" } else { "were" }
                ),
            });
        }

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
            "bytes" => match compile_cut(raw, ctx, None, true)? {
                Some(cut) => Ok(FieldKind::BytesCut { cut }),
                None => Ok(FieldKind::Bytes {
                    size: compile_size(raw, ctx)?,
                }),
            },

            // ── Strings ──────────────────────────────────────────────────
            "strz" => {
                let enc = compile_encoding(raw.encoding.or(ctx.encoding));
                let size = match compile_cut(raw, ctx, Some(0x00), false)? {
                    Some(cut) => StrSize::Cut(cut),
                    None => StrSize::Terminator(raw.terminator.unwrap_or(0x00)),
                };
                Ok(FieldKind::Str {
                    size,
                    encoding: enc,
                })
            }
            "str" => {
                let enc = compile_encoding(raw.encoding.or(ctx.encoding));
                let str_size = if let Some(cut) = compile_cut(raw, ctx, None, false)? {
                    StrSize::Cut(cut)
                } else if let Some(term) = raw.terminator {
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
                    Some(name) => {
                        let args = self.compile_type_args(&name, &arg_texts, raw, ctx)?;
                        Ok(FieldKind::TypeRef {
                            type_name: name,
                            size: compile_optional_size(raw, ctx)?,
                            args,
                        })
                    }
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

    /// The enum `type::name` stands for, seen from the type `from`.
    fn qualified_enum(&self, written: &str, from: &str) -> Option<CompiledEnum> {
        let (type_name, enum_name) = written.rsplit_once("::")?;
        let fqn = resolve_in(&self.registry, type_name, from)?;
        self.registry.get(&fqn)?.enums.get(enum_name).cloned()
    }

    /// Resolve a bare or partially-qualified type name to a fully-qualified one
    /// that exists in the registry.
    ///
    /// Search order:
    /// 1. Bare name as-is (for top-level imported types like `"zlib_block"`).
    /// 2. `<parent_fqn>::<name>` (inline sibling type).
    /// 3. Walk up the parent hierarchy: `<grandparent>::<name>`, etc.
    fn resolve_type_ref(&self, name: &str, parent_fqn: &str) -> Option<String> {
        resolve_in(&self.registry, name, parent_fqn)
    }

    // ── Path resolution ───────────────────────────────────────────────────────

    fn resolve_path(&self, name_or_path: &str, relative_to: Option<&Path>) -> Result<PathBuf> {
        // Relative path: starts with "./" or "../"
        if name_or_path.starts_with("./") || name_or_path.starts_with("../") {
            let base = relative_to.unwrap_or_else(|| Path::new("."));
            let candidate = base.join(name_or_path);
            if self.source.exists(&candidate) {
                return Ok(candidate);
            }
            return Err(DoeError::SchemaNotFound {
                name: name_or_path.to_owned(),
                searched: vec![candidate],
            });
        }

        // Bare name (or absolute path): search the include paths.
        let dirs: Vec<&Path> = self.include_paths.iter().map(PathBuf::as_path).collect();
        find_schema(self.source.as_ref(), name_or_path, name_or_path, &dirs)
    }

    /// Where an import is. A name that starts with `./` or `../` is relative
    /// to the importing file. As in Kaitai Struct, one that starts with a slash
    /// is looked for in the include paths only, and any other is looked for
    /// next to the importing file first and then in the include paths.
    fn resolve_import(&self, import: &str, importing_dir: Option<&Path>) -> Result<PathBuf> {
        if let Some(rest) = import.strip_prefix('/') {
            let dirs: Vec<&Path> = self.include_paths.iter().map(PathBuf::as_path).collect();
            return find_schema(self.source.as_ref(), import, rest, &dirs);
        }
        if import.starts_with("./") || import.starts_with("../") {
            return self.resolve_path(import, importing_dir);
        }
        let dirs: Vec<&Path> = importing_dir
            .into_iter()
            .chain(self.include_paths.iter().map(PathBuf::as_path))
            .collect();
        find_schema(self.source.as_ref(), import, import, &dirs)
    }

    fn load_import(&mut self, import: &str, importing_dir: Option<&Path>) -> Result<()> {
        let path = self.resolve_import(import, importing_dir)?;
        log::debug!("loading imported schema {import} from {}", path.display());
        let schema = self.read_schema(&path)?;
        self.process_schema(&schema)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper structs and functions
// ─────────────────────────────────────────────────────────────────────────────

/// Find the schema file `name` in `dirs`, trying the extensions of schemas
/// when it has none. `shown` is the name in the error.
fn find_schema(
    source: &dyn SchemaSource,
    shown: &str,
    name: &str,
    dirs: &[&Path],
) -> Result<PathBuf> {
    let names: Vec<String> = if crate::has_schema_extension(name) {
        vec![name.to_owned()]
    } else {
        crate::SCHEMA_EXTENSIONS
            .iter()
            .map(|ext| format!("{name}.{ext}"))
            .collect()
    };
    let mut searched = Vec::new();
    for dir in dirs {
        for file in &names {
            let candidate = dir.join(file);
            searched.push(candidate.clone());
            if source.exists(&candidate) {
                return Ok(candidate);
            }
        }
    }
    Err(DoeError::SchemaNotFound {
        name: shown.to_owned(),
        searched,
    })
}

/// Context passed through field compilation.
struct FieldCtx<'a> {
    parent_fqn: &'a str,
    seen_ids: &'a [String],
    enums: &'a HashMap<String, CompiledEnum>,
    /// The names of the type's instances, which any expression may use.
    instances: &'a HashSet<String>,
    /// The names of the type's parameters, which any expression may use.
    params: &'a HashSet<String>,
    /// The size in bytes of the fields of the sequence that always take the
    /// same, so `later_field._sizeof` can be used before the field is read.
    fixed_sizes: &'a HashMap<String, i128>,
    /// The encoding of strings that do not give one.
    encoding: Option<RawEncoding>,
    /// The types compiled so far, and the names of the others in the schema
    /// with the enums they see, for enum members qualified with a type.
    registry: &'a TypeRegistry,
}

/// What a type inherits from the schema or the type around it.
#[derive(Clone, Copy)]
struct TypeDefaults {
    endian: Endian,
    bit_endian: Endian,
    encoding: Option<RawEncoding>,
    /// A value chooses the byte order of the type around.
    runtime_endian: bool,
}

/// The parts of a type declaration that are compiled together: the same for
/// the top level of a schema and for a nested type.
#[derive(Clone, Copy)]
struct TypeParts<'a> {
    params: &'a [RawParam],
    seq: &'a [RawField],
    types: &'a IndexMap<String, RawTypeDecl>,
    enums: &'a IndexMap<String, RawEnum>,
    instances: &'a IndexMap<String, RawInstance>,
    /// The `meta` of the type: its own byte order, bit order and encoding.
    meta: Option<&'a RawMeta>,
}

/// Split `name(arg, ...)` into the name and the text of each argument, at the
/// commas that are not inside parentheses, brackets or quotes. A name without
/// parentheses has no arguments.
fn split_type_call(text: &str) -> std::result::Result<(&str, Vec<&str>), String> {
    let Some((name, rest)) = text.split_once('(') else {
        return Ok((text, Vec::new()));
    };
    let Some(inner) = rest.trim_end().strip_suffix(')') else {
        return Err(format!("'{text}': the arguments are not closed with ')'"));
    };
    let mut args = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (at, c) in inner.char_indices() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' && open == '"' {
                escaped = true;
            } else if c == open {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '(' | '[' => depth += 1,
            ')' | ']' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| format!("'{text}': unbalanced parentheses"))?;
            }
            ',' if depth == 0 => {
                args.push(inner.get(start..at).unwrap_or_default().trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    if depth != 0 || quote.is_some() {
        return Err(format!("'{text}': unbalanced parentheses or quotes"));
    }
    let last = inner.get(start..).unwrap_or_default().trim();
    if !last.is_empty() || !args.is_empty() {
        args.push(last);
    }
    if args.iter().any(|arg| arg.is_empty()) {
        return Err(format!("'{text}': an argument is empty"));
    }
    Ok((name.trim(), args))
}

/// The parameters of a type, as compiled.
fn compile_params(params: &[RawParam]) -> Vec<CompiledParam> {
    params
        .iter()
        .map(|param| CompiledParam {
            id: param.id.clone(),
            doc: param.doc.clone(),
        })
        .collect()
}

/// For each type, the instances to compute before each of its fields (by index).
type Additions = Vec<(String, Vec<(usize, Vec<String>)>)>;

/// The fields of a type that read data: those of `seq`, then the instances
/// that read.
fn all_fields(ty: &CompiledType) -> impl Iterator<Item = &CompiledField> {
    ty.fields
        .iter()
        .chain(ty.instances.iter().filter_map(|i| match &i.kind {
            InstanceKind::Read(field) => Some(field.as_ref()),
            InstanceKind::Value { .. } => None,
        }))
}

fn all_fields_mut(ty: &mut CompiledType) -> impl Iterator<Item = &mut CompiledField> {
    ty.fields
        .iter_mut()
        .chain(ty.instances.iter_mut().filter_map(|i| match &mut i.kind {
            InstanceKind::Read(field) => Some(field.as_mut()),
            InstanceKind::Value { .. } => None,
        }))
}

/// The names of the types a field reads.
fn kind_targets(kind: &FieldKind) -> Vec<&str> {
    match kind {
        FieldKind::TypeRef { type_name, .. } => vec![type_name.as_str()],
        FieldKind::Switch { cases, default, .. } => cases
            .iter()
            .map(|(_, case)| case)
            .chain(default.as_deref())
            .flat_map(kind_targets)
            .collect(),
        _ => Vec::new(),
    }
}

/// Every expression of a type: those of its fields and instances, and of its
/// byte order.
fn type_exprs(ty: &CompiledType) -> Vec<&expr::Expr> {
    let mut exprs: Vec<&expr::Expr> = all_fields(ty).flat_map(field_exprs).collect();
    for instance in &ty.instances {
        if let InstanceKind::Value { expr, if_expr, .. } = &instance.kind {
            exprs.push(expr);
            exprs.extend(if_expr.as_ref());
        }
        exprs.extend(instance.io.as_ref().map(|io| &io.expr));
    }
    if let EndianRule::Switch(switch) = &ty.endian_rule {
        exprs.push(&switch.on);
        exprs.extend(switch.cases.iter().map(|(key, _)| key));
    }
    exprs
}

/// The paths of the fields the expressions refer to (`_parent.x.y`).
fn field_paths(exprs: Vec<&expr::Expr>) -> Vec<&Vec<String>> {
    use expr::Expr;
    let mut paths = Vec::new();
    let mut stack = exprs;
    while let Some(node) = stack.pop() {
        match node {
            Expr::Field(parts) => paths.push(parts),
            Expr::Array(items) => stack.extend(items.iter()),
            Expr::Member(inner, _) | Expr::Unary(_, inner) => stack.push(inner),
            Expr::Call(recv, _, args) => {
                stack.extend(args.iter());
                stack.push(recv);
            }
            Expr::Index(a, b) | Expr::Binary(_, a, b) => {
                stack.push(a);
                stack.push(b);
            }
            Expr::Ternary(a, b, c) => {
                stack.push(a);
                stack.push(b);
                stack.push(c);
            }
            Expr::Int(_) | Expr::Float(_) | Expr::Bool(_) | Expr::Str(_) | Expr::Enum(_) => {}
        }
    }
    paths
}

/// The size in bytes of `name` if it is always the same: made of fields of a
/// fixed size that are neither optional (unless empty), repeated (unless a
/// number of times written as a literal), placed with `pos`, nor bit fields.
/// Cached in `known`.
fn static_size(
    registry: &TypeRegistry,
    name: &str,
    known: &mut HashMap<String, Option<u64>>,
    depth: usize,
) -> Option<u64> {
    if let Some(size) = known.get(name) {
        return *size;
    }
    // A type that contains itself, or is nested too deep, has no fixed size.
    if depth > 64 {
        return None;
    }
    known.insert(name.to_owned(), None);
    let ty = registry.get(name)?;
    let mut total: u64 = 0;
    for field in &ty.fields {
        total = total.checked_add(field_static_size(registry, field, known, depth)?)?;
    }
    known.insert(name.to_owned(), Some(total));
    Some(total)
}

/// The size in bytes of one field of a type, if it is always the same (see [`static_size`]).
fn field_static_size(
    registry: &TypeRegistry,
    field: &CompiledField,
    known: &mut HashMap<String, Option<u64>>,
    depth: usize,
) -> Option<u64> {
    // `repeat: expr` with a number written in the schema reads the field that many times
    let times = match &field.repeat {
        RepeatMode::Once => 1,
        RepeatMode::Count(crate::schema::expr::Expr::Int(n)) => u64::try_from(*n).ok()?,
        _ => return None,
    };
    if field.pos.is_some() {
        return None;
    }
    let size = match &field.kind {
        FieldKind::UInt { width, .. } | FieldKind::SInt { width, .. } => match width {
            IntWidth::W8 => 1,
            IntWidth::W16 => 2,
            IntWidth::W32 => 4,
            IntWidth::W64 => 8,
        },
        FieldKind::Float { width, .. } => match width {
            FloatWidth::F32 => 4,
            FloatWidth::F64 => 8,
        },
        FieldKind::Contents(bytes) => u64::try_from(bytes.len()).ok()?,
        FieldKind::Bytes {
            size: SizeExpr::Literal(n),
        }
        | FieldKind::Str {
            size: StrSize::Fixed(SizeExpr::Literal(n)),
            ..
        }
        | FieldKind::TypeRef {
            size: Some(SizeExpr::Literal(n)),
            ..
        }
        // a string or bytes cut at a terminator out of a window of a given size takes the window
        | FieldKind::Str {
            size:
                StrSize::Cut(crate::schema::ir::Cut {
                    size: Some(SizeExpr::Literal(n)),
                    ..
                }),
            ..
        }
        | FieldKind::BytesCut {
            cut:
                crate::schema::ir::Cut {
                    size: Some(SizeExpr::Literal(n)),
                    ..
                },
        } => u64::try_from(*n).ok()?,
        FieldKind::TypeRef {
            type_name,
            size: None,
            ..
        } => static_size(registry, type_name, known, depth + 1)?,
        _ => return None,
    };
    let size = size.checked_mul(times)?;
    // An optional field has no fixed size, unless it takes no bytes anyway
    // (`- size: 0` with an `if`, which some definitions use to make
    // something be computed first).
    if field.if_expr.is_some() && size != 0 {
        return None;
    }
    Some(size)
}

/// For each type, which instances of the types around it it uses, directly or
/// through the types it reads: pairs of how many `_parent`s up, and the name.
fn parent_instance_uses(
    registry: &TypeRegistry,
) -> HashMap<&str, std::collections::BTreeSet<(usize, String)>> {
    let mut uses: HashMap<&str, std::collections::BTreeSet<(usize, String)>> = HashMap::new();
    for (name, ty) in &registry.types {
        let entry = uses.entry(name.as_str()).or_default();
        for parts in field_paths(type_exprs(ty)) {
            let up = parts.iter().take_while(|p| *p == "_parent").count();
            if up > 0
                && let Some(used) = parts.get(up)
            {
                entry.insert((up, used.clone()));
            }
        }
    }
    // A type that reads a type using `_parent._parent.x` uses `_parent.x`.
    loop {
        let mut changed = false;
        for (name, ty) in &registry.types {
            let inherited: Vec<(usize, String)> = all_fields(ty)
                .flat_map(|field| kind_targets(&field.kind))
                .filter_map(|target| uses.get(target))
                .flatten()
                .filter(|(up, _)| *up > 1)
                .map(|(up, used)| (up - 1, used.clone()))
                .collect();
            if let Some(entry) = uses.get_mut(name.as_str()) {
                for item in inherited {
                    changed |= entry.insert(item);
                }
            }
        }
        if !changed {
            break;
        }
    }
    uses
}

/// Resolve a bare or partially-qualified type name to a fully-qualified one
/// that exists in `registry`, looking from the type `parent_fqn`.
fn resolve_in(registry: &TypeRegistry, name: &str, parent_fqn: &str) -> Option<String> {
    // 1. Bare name as-is (for top-level imported types like `"zlib_block"`).
    if registry.contains(name) {
        return Some(name.to_owned());
    }
    // 2 & 3. `<parent_fqn>::<name>`, then up the parent hierarchy.
    let mut scope = parent_fqn;
    loop {
        let candidate = format!("{scope}::{name}");
        if registry.contains(&candidate) {
            return Some(candidate);
        }
        match scope.rsplit_once("::") {
            Some((parent, _)) => scope = parent,
            None => return None,
        }
    }
}

/// Create an empty placeholder `CompiledType` used during name collection.
fn placeholder(
    fqn: &str,
    params: &[RawParam],
    enums: &HashMap<String, CompiledEnum>,
) -> CompiledType {
    CompiledType {
        name: fqn.to_owned(),
        doc: None,
        endian: Endian::Little,
        endian_rule: EndianRule::Fixed,
        bit_endian: Endian::Big,
        params: compile_params(params),
        fields: vec![],
        instances: vec![],
        enums: enums.clone(),
    }
}

fn raw_endian_to_ir(e: RawEndian) -> Endian {
    match e {
        RawEndian::Le => Endian::Little,
        RawEndian::Be => Endian::Big,
    }
}

pub(crate) fn compile_encoding(enc: Option<RawEncoding>) -> Encoding {
    match enc.unwrap_or(RawEncoding::Utf8) {
        RawEncoding::Utf8 => Encoding::Utf8,
        RawEncoding::Ascii => Encoding::Ascii,
        RawEncoding::Latin1 => Encoding::Latin1,
        RawEncoding::Utf16Le => Encoding::Utf16Le,
        RawEncoding::Utf16Be => Encoding::Utf16Be,
        RawEncoding::Utf32Le => Encoding::Utf32Le,
        RawEncoding::Utf32Be => Encoding::Utf32Be,
        RawEncoding::Cp437 => Encoding::Cp437,
        RawEncoding::Legacy(name) => Encoding::Legacy(name),
    }
}

/// The `Cut` a `bytes` or `str` field asks for with `include`, `consume`,
/// `eos-error`, `pad-right`, or a `size` together with a `terminator`; `None`
/// when it is a plain size or a plain terminator. `default_terminator` is the
/// terminator `strz` has.
fn compile_cut(
    raw: &RawField,
    ctx: &FieldCtx<'_>,
    default_terminator: Option<u8>,
    bytes: bool,
) -> Result<Option<Cut>> {
    let terminator = raw.terminator.or(default_terminator);
    let sized = raw.size.is_some() || raw.size_eos == Some(true);
    let options = raw.include.is_some()
        || raw.consume.is_some()
        || raw.eos_error.is_some()
        || raw.pad_right.is_some();
    if !(options || terminator.is_some() && (sized || bytes)) {
        return Ok(None);
    }
    let bad = |message: &str| DoeError::FieldError {
        field: raw.id.clone(),
        context: ctx.parent_fqn.to_owned(),
        message: message.to_owned(),
    };
    if terminator.is_none()
        && (raw.include.is_some() || raw.consume.is_some() || raw.eos_error.is_some())
    {
        return Err(bad(
            "'include', 'consume' and 'eos-error' need a 'terminator'",
        ));
    }
    if sized {
        if raw.consume.is_some() || raw.eos_error.is_some() {
            return Err(bad(
                "'consume' and 'eos-error' are for a value that is not read from a 'size'",
            ));
        }
    } else {
        if raw.pad_right.is_some() {
            return Err(bad("'pad-right' needs a 'size'"));
        }
        if terminator.is_none() {
            return Err(bad("a value needs a 'size', 'size-eos' or a 'terminator'"));
        }
    }
    Ok(Some(Cut {
        size: if sized {
            Some(compile_size(raw, ctx)?)
        } else {
            None
        },
        terminator,
        include: raw.include.unwrap_or(false),
        consume: raw.consume.unwrap_or(true),
        eos_error: raw.eos_error.unwrap_or(true),
        pad_right: raw.pad_right,
    }))
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
                raw.type_ref
                    .as_ref()
                    .and_then(RawType::as_name)
                    .unwrap_or("?")
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
            Ok(SizeExpr::Dynamic(compile_expr(s, ctx)?))
        }
    }
}

/// Compile `parent:`: how many `_parent`s up the `_parent` of the type the
/// field reads is.
fn compile_parent(raw: &RawField, ctx: &FieldCtx<'_>) -> Result<Option<usize>> {
    let Some(text) = &raw.parent else {
        return Ok(None);
    };
    let path = text.trim();
    let up = path.split('.').count();
    if path.split('.').all(|part| part.trim() == "_parent") {
        return Ok(Some(up));
    }
    Err(DoeError::FieldError {
        field: raw.id.clone(),
        context: ctx.parent_fqn.to_owned(),
        message: format!("'parent: {path}' is not supported: use _parent, or _parent._parent"),
    })
}

/// Compile `process:`, if the field has it.
fn compile_process(
    raw: &RawField,
    kind: &FieldKind,
    ctx: &FieldCtx<'_>,
) -> Result<Option<Process>> {
    let Some(text) = &raw.process else {
        return Ok(None);
    };
    let bad = |message: String| DoeError::FieldError {
        field: raw.id.clone(),
        context: ctx.parent_fqn.to_owned(),
        message: format!("process '{text}': {message}"),
    };
    let sized = matches!(
        kind,
        FieldKind::Bytes { .. }
            | FieldKind::Str {
                size: StrSize::Fixed(_),
                ..
            }
            | FieldKind::TypeRef { size: Some(_), .. }
    );
    if !sized {
        return Err(bad(
            "it applies to 'bytes', a 'str' or a user type that has a 'size' or 'size-eos'"
                .to_owned(),
        ));
    }
    let (name, args) = split_type_call(text.trim()).map_err(bad)?;
    let count = |wanted: usize| -> Result<()> {
        if args.len() == wanted {
            return Ok(());
        }
        Err(bad(format!(
            "'{name}' takes {wanted} argument{}, but {} {} given",
            if wanted == 1 { "" } else { "s" },
            args.len(),
            if args.len() == 1 { "was" } else { "were" }
        )))
    };
    let argument = || -> Result<expr::Expr> {
        count(1)?;
        let text = args.first().copied().unwrap_or_default();
        validate_size_expr_refs(text, ctx)?;
        compile_expr(text, ctx).map_err(|e| bad(format!("argument '{text}': {e}")))
    };
    let process = match name.trim() {
        "zlib" => {
            count(0)?;
            Process::Zlib
        }
        "xor" => Process::Xor(argument()?),
        "rol" => Process::Rol(argument()?),
        "ror" => Process::Ror(argument()?),
        other => {
            return Err(bad(format!(
                "unknown process '{other}' (xor, rol, ror and zlib are supported)"
            )));
        }
    };
    Ok(Some(process))
}

/// Compile `valid:`, if the field has it.
fn compile_valid(raw: &RawField, ctx: &FieldCtx<'_>) -> Result<Option<CompiledValid>> {
    let Some(valid) = &raw.valid else {
        return Ok(None);
    };
    let bad = |message: String| DoeError::FieldError {
        field: raw.id.clone(),
        context: ctx.parent_fqn.to_owned(),
        message: format!("'valid': {message}"),
    };
    let rules = match valid {
        RawValid::Eq(value) => RawValidRules {
            eq: Some(value.clone()),
            ..RawValidRules::default()
        },
        RawValid::Rules(rules) => rules.clone(),
    };
    if rules.is_empty() {
        return Err(bad(
            "needs a value or at least one of eq, min, max, any-of, expr, in-enum".to_owned(),
        ));
    }
    // Rules other than `expr` are about fields read earlier, or the field
    // itself; `expr` is about `_`.
    let mut seen = ctx.seen_ids.to_vec();
    seen.push(raw.id.clone());
    let with_self = FieldCtx {
        seen_ids: &seen,
        ..*ctx
    };
    let rule = |text: &ExprText| -> Result<expr::Expr> {
        let ast = compile_expr(&text.0, &with_self).map_err(|e| bad(e.to_string()))?;
        validate_size_expr_refs(&text.0, &with_self)?;
        Ok(ast)
    };
    let any_of = match &rules.any_of {
        Some(list) if list.is_empty() => return Err(bad("'any-of' is empty".to_owned())),
        Some(list) => list.iter().map(&rule).collect::<Result<Vec<_>>>()?,
        None => Vec::new(),
    };
    let in_enum = rules.in_enum == Some(true);
    if in_enum && raw.enum_ref.is_none() {
        return Err(bad("'in-enum' needs 'enum' on the field".to_owned()));
    }
    let expr = rules
        .expr
        .as_ref()
        .map(|text| compile_expr(&text.0, ctx).map_err(|e| bad(e.to_string())))
        .transpose()?;
    let expr_instances = expr
        .as_ref()
        .map(|e| instance_refs(&[e], ctx.instances))
        .unwrap_or_default();
    Ok(Some(CompiledValid {
        eq: rules.eq.as_ref().map(rule).transpose()?,
        min: rules.min.as_ref().map(rule).transpose()?,
        max: rules.max.as_ref().map(rule).transpose()?,
        any_of,
        expr,
        in_enum,
        expr_instances,
    }))
}

/// Every expression a field evaluates.
fn field_exprs(field: &CompiledField) -> Vec<&expr::Expr> {
    fn size<'a>(size: &'a SizeExpr, out: &mut Vec<&'a expr::Expr>) {
        if let SizeExpr::Dynamic(e) = size {
            out.push(e);
        }
    }
    fn kind<'a>(k: &'a FieldKind, out: &mut Vec<&'a expr::Expr>) {
        match k {
            FieldKind::Bytes { size: s }
            | FieldKind::Str {
                size: StrSize::Fixed(s),
                ..
            } => size(s, out),
            FieldKind::BytesCut { cut: c }
            | FieldKind::Str {
                size: StrSize::Cut(c),
                ..
            } => {
                if let Some(s) = &c.size {
                    size(s, out);
                }
            }
            FieldKind::TypeRef {
                size: window, args, ..
            } => {
                if let Some(window) = window {
                    size(window, out);
                }
                out.extend(args.iter());
            }
            FieldKind::Switch {
                on,
                size: window,
                cases,
                default,
            } => {
                out.push(on);
                if let Some(window) = window {
                    size(window, out);
                }
                for (key, case) in cases {
                    out.push(key);
                    kind(case, out);
                }
                if let Some(case) = default {
                    kind(case, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    out.extend(field.if_expr.as_ref());
    out.extend(field.pos.as_ref());
    if let Some(valid) = &field.valid {
        out.extend(valid.eq.as_ref());
        out.extend(valid.min.as_ref());
        out.extend(valid.max.as_ref());
        out.extend(valid.any_of.iter());
    }
    match &field.repeat {
        RepeatMode::Count(e) | RepeatMode::Until(e) => out.push(e),
        RepeatMode::Once | RepeatMode::Eos => {}
    }
    if let Some(Process::Xor(e) | Process::Rol(e) | Process::Ror(e)) = &field.process {
        out.push(e);
    }
    kind(&field.kind, &mut out);
    out
}

/// The names in `names` that the expressions refer to (as the first part of a
/// field path), in order of first use.
fn instance_refs(exprs: &[&expr::Expr], names: &HashSet<String>) -> Vec<String> {
    use expr::Expr;
    let mut found: Vec<String> = Vec::new();
    if names.is_empty() {
        return found;
    }
    let mut stack: Vec<&Expr> = exprs.iter().rev().copied().collect();
    while let Some(node) = stack.pop() {
        match node {
            Expr::Field(parts) => {
                if let Some(first) = parts.first()
                    && names.contains(first)
                    && !found.contains(first)
                {
                    found.push(first.clone());
                }
            }
            Expr::Array(items) => stack.extend(items.iter().rev()),
            Expr::Member(inner, _) | Expr::Unary(_, inner) => stack.push(inner),
            Expr::Call(recv, _, args) => {
                stack.extend(args.iter().rev());
                stack.push(recv);
            }
            Expr::Index(a, b) | Expr::Binary(_, a, b) => {
                stack.push(b);
                stack.push(a);
            }
            Expr::Ternary(a, b, c) => {
                stack.push(c);
                stack.push(b);
                stack.push(a);
            }
            Expr::Int(_) | Expr::Float(_) | Expr::Bool(_) | Expr::Str(_) | Expr::Enum(_) => {}
        }
    }
    found
}

/// Parse an expression and replace its enum members (`kind_t::data`) with
/// their values, using the enums visible in the field's type.
fn compile_expr(text: &str, ctx: &FieldCtx<'_>) -> Result<expr::Expr> {
    use expr::Expr;
    let mut ast = expr::parse(text)?;
    // A stack of subexpressions to visit, rather than recursion.
    let mut stack = vec![&mut ast];
    while let Some(node) = stack.pop() {
        match node {
            Expr::Enum(path) => {
                let value = enum_member_value(path, ctx)?;
                *node = Expr::Int(value);
            }
            Expr::Array(items) => stack.extend(items.iter_mut()),
            Expr::Member(inner, _) | Expr::Unary(_, inner) => stack.push(inner),
            Expr::Call(recv, _, args) => {
                stack.push(recv);
                stack.extend(args.iter_mut());
            }
            Expr::Index(a, b) | Expr::Binary(_, a, b) => {
                stack.push(a);
                stack.push(b);
            }
            Expr::Ternary(a, b, c) => {
                stack.push(a);
                stack.push(b);
                stack.push(c);
            }
            Expr::Field(parts) => {
                if let Some(replacement) = sizeof_marker(parts, ctx)? {
                    *node = replacement;
                }
            }
            Expr::Int(_) | Expr::Float(_) | Expr::Bool(_) | Expr::Str(_) => {}
        }
    }
    Ok(ast)
}

/// The fields of a sequence that always take the same number of bytes: fixed
/// `contents` and numbers, read once.
fn fixed_field_sizes(seq: &[RawField]) -> HashMap<String, i128> {
    let mut sizes = HashMap::new();
    for field in seq {
        if field.repeat.is_some() || field.if_expr.is_some() {
            continue;
        }
        let size = if let Some(contents) = &field.contents {
            match compile_contents(contents, &field.id, "") {
                Ok(FieldKind::Contents(bytes)) => i128::try_from(bytes.len()).ok(),
                _ => None,
            }
        } else if let (Some(RawType::Name(name)), None) = (&field.type_ref, &field.size) {
            match numeric_type(name) {
                Some(FieldKind::UInt { width, .. } | FieldKind::SInt { width, .. }) => {
                    Some(match width {
                        IntWidth::W8 => 1,
                        IntWidth::W16 => 2,
                        IntWidth::W32 => 4,
                        IntWidth::W64 => 8,
                    })
                }
                Some(FieldKind::Float { width, .. }) => Some(match width {
                    FloatWidth::F32 => 4,
                    FloatWidth::F64 => 8,
                }),
                _ => None,
            }
        } else {
            None
        };
        if let Some(size) = size {
            sizes.insert(field.id.clone(), size);
        }
    }
    sizes
}

/// What `_sizeof`, `sizeof<type>` and `bitsizeof<type>` stand for: the size of
/// a primitive, or a reference to the size of a type, which is known once all
/// types are compiled (see [`TypeRegistry::static_sizes`]).
fn sizeof_marker(parts: &[String], ctx: &FieldCtx<'_>) -> Result<Option<expr::Expr>> {
    use expr::{BinaryOp, Expr};
    if let [field, suffix] = parts
        && suffix == "_sizeof"
        && let Some(size) = ctx.fixed_sizes.get(field)
    {
        return Ok(Some(Expr::Int(*size)));
    }
    let [only] = parts else {
        return Ok(None);
    };
    let size_of = |fqn: &str| Expr::Field(vec!["_root".to_owned(), format!("sizeof<{fqn}>")]);
    if only == "_sizeof" {
        // The size of the type the expression is in.
        return Ok(Some(size_of(ctx.parent_fqn)));
    }
    let (bits, rest) = match (
        only.strip_prefix("bitsizeof<"),
        only.strip_prefix("sizeof<"),
    ) {
        (Some(rest), _) => (true, rest),
        (None, Some(rest)) => (false, rest),
        _ => return Ok(None),
    };
    let Some(name) = rest.strip_suffix('>') else {
        return Ok(None);
    };
    let primitive_bits = if let Some(kind) = numeric_type(name) {
        match kind {
            FieldKind::UInt { width, .. } | FieldKind::SInt { width, .. } => Some(match width {
                IntWidth::W8 => 8,
                IntWidth::W16 => 16,
                IntWidth::W32 => 32,
                IntWidth::W64 => 64,
            }),
            FieldKind::Float { width, .. } => Some(match width {
                FloatWidth::F32 => 32,
                FloatWidth::F64 => 64,
            }),
            _ => None,
        }
    } else {
        bit_type(name).map(|(bits, _)| i128::from(bits))
    };
    if let Some(primitive_bits) = primitive_bits {
        let n = if bits {
            primitive_bits
        } else {
            (primitive_bits + 7) / 8
        };
        return Ok(Some(Expr::Int(n)));
    }
    let fqn =
        resolve_in(ctx.registry, name, ctx.parent_fqn).ok_or_else(|| DoeError::UnknownType {
            type_name: name.to_owned(),
            context: ctx.parent_fqn.to_owned(),
        })?;
    let bytes = size_of(&fqn);
    Ok(Some(if bits {
        Expr::Binary(BinaryOp::Mul, Box::new(bytes), Box::new(Expr::Int(8)))
    } else {
        bytes
    }))
}

/// The value of the enum member `path` names (`["kind_t", "data"]`).
fn enum_member_value(path: &[String], ctx: &FieldCtx<'_>) -> Result<i128> {
    let context = ctx.parent_fqn.to_owned();
    let (enum_name, member, enums) = match path {
        [enum_name, member] => (enum_name, member, ctx.enums),
        [type_path @ .., enum_name, member] if !type_path.is_empty() => {
            // `type::enum::member`: the enums of the type the path names.
            let type_name = type_path.join("::");
            let ty = resolve_in(ctx.registry, &type_name, ctx.parent_fqn)
                .and_then(|fqn| ctx.registry.get(&fqn))
                .ok_or_else(|| DoeError::UnknownType {
                    type_name: type_name.clone(),
                    context: context.clone(),
                })?;
            (enum_name, member, &ty.enums)
        }
        _ => {
            return Err(DoeError::Schema(format!(
                "{context}: '{}' is not an enum member (use enum::member or type::enum::member)",
                path.join("::")
            )));
        }
    };
    let compiled = enums
        .get(enum_name.as_str())
        .ok_or_else(|| DoeError::UnknownEnum {
            enum_name: enum_name.clone(),
            context: context.clone(),
        })?;
    compiled.number_of(member).ok_or_else(|| {
        DoeError::Schema(format!(
            "{context}: enum '{enum_name}' has no member '{member}'"
        ))
    })
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
            Ok(RepeatMode::Until(compile_expr(s, ctx)?))
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
            Ok(RepeatMode::Count(compile_expr(s, ctx)?))
        }
    }
}

fn compile_contents(contents: &RawContents, field_id: &str, parent_fqn: &str) -> Result<FieldKind> {
    let bytes = match contents {
        RawContents::Bytes(b) => b.clone(),
        RawContents::Str(s) => s.as_bytes().to_vec(),
        RawContents::Items(items) => items
            .iter()
            .flat_map(|item| match item {
                RawContentItem::Byte(b) => vec![*b],
                RawContentItem::Str(s) => s.as_bytes().to_vec(),
            })
            .collect(),
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
/// Compile `meta.endian: {switch-on, cases}`.
fn compile_endian_switch(switch: &RawEndianSwitch, ctx: &FieldCtx<'_>) -> Result<EndianSwitch> {
    let bad = |message: String| DoeError::FieldError {
        field: "meta.endian".to_owned(),
        context: ctx.parent_fqn.to_owned(),
        message,
    };
    if switch.cases.is_empty() {
        return Err(bad("'switch-on' needs at least one case".to_owned()));
    }
    let on = compile_expr(&switch.switch_on, ctx).map_err(|e| bad(format!("'switch-on': {e}")))?;
    validate_size_expr_refs(&switch.switch_on, ctx)?;
    let mut cases = Vec::new();
    let mut default = None;
    for (key, endian) in &switch.cases {
        let endian = raw_endian_to_ir(*endian);
        if key == "_" {
            default = Some(endian);
        } else {
            let key_expr = compile_expr(key, ctx).map_err(|e| bad(format!("case '{key}': {e}")))?;
            cases.push((key_expr, endian));
        }
    }
    Ok(EndianSwitch { on, cases, default })
}

fn compile_enums(
    raw_enums: &IndexMap<String, RawEnum>,
    context: &str,
) -> Result<HashMap<String, CompiledEnum>> {
    let mut result = HashMap::new();
    for (enum_name, raw_map) in raw_enums {
        let mut ce = CompiledEnum::default();
        for (key_str, variant) in raw_map {
            let (key, negative) = parse_enum_key(key_str).ok_or_else(|| {
                DoeError::Schema(format!(
                    "enum '{enum_name}' in {context}: invalid key '{key_str}'"
                ))
            })?;
            ce.variants.insert(key, variant.id().to_owned());
            if negative {
                ce.negative.insert(key);
            }
        }
        result.insert(enum_name.clone(), ce);
    }
    Ok(result)
}

/// Parse an enum discriminant key — decimal or `0x`-prefixed hex.
///
/// A negative key (`-1`) gives the two's complement of its 64 bits, and `true`
/// says it was negative.
fn parse_enum_key(s: &str) -> Option<(u64, bool)> {
    // `_` may separate digits, as in an expression.
    let s: String = s.trim().chars().filter(|c| *c != '_').collect();
    let s = s.as_str();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok().map(|n| (n, false))
    } else if s.starts_with('-') {
        s.parse::<i64>().ok().map(|n| (n.cast_unsigned(), true))
    } else {
        s.parse::<u64>().ok().map(|n| (n, false))
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
        Expr::Int(_) | Expr::Float(_) | Expr::Bool(_) | Expr::Str(_) | Expr::Enum(_) => Ok(()),
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
            // Skip _parent, _root and _io references — those are resolved at
            // parse time
            if matches!(
                parts.first().map(String::as_str),
                Some("_parent" | "_root" | "_io" | "_index")
            ) {
                return Ok(());
            }
            let Some(name) = parts.first() else {
                return Ok(());
            };
            // `sizeof<type>`, `bitsizeof<type>` and `_sizeof` are not fields.
            if name == "_sizeof" || name.starts_with("sizeof<") || name.starts_with("bitsizeof<") {
                return Ok(());
            }
            if matches!(parts.as_slice(), [field, suffix] if suffix == "_sizeof" && ctx.fixed_sizes.contains_key(field))
            {
                return Ok(());
            }
            if !ctx.seen_ids.iter().any(|id| id == name)
                && !ctx.instances.contains(name)
                && !ctx.params.contains(name)
            {
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
#[cfg(feature = "fs")]
pub fn load_yaml(path: &Path) -> Result<RawSchema> {
    load_yaml_from(&FsSource, path)
}

/// Read and parse the schema file at `path` from `source`, and record the path
/// as the file the schema came from.
///
/// # Errors
///
/// Returns an error if the file cannot be read or is not a valid schema.
pub fn load_yaml_from(source: &dyn SchemaSource, path: &Path) -> Result<RawSchema> {
    let text = source.read(path)?;
    let mut schema: RawSchema = RawSchema::from_yaml_str(&text).map_err(|e| DoeError::Yaml {
        path: path.to_owned(),
        source: Box::new(e),
    })?;
    schema.source_path = Some(path.to_owned());
    Ok(schema)
}
