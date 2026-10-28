//! Binary parse engine.
//!
//! `Engine::parse_type` is the top-level entry point.  It walks a
//! `CompiledType`'s field list, dispatching each field to the appropriate
//! read routine, evaluating size/repeat/if expressions against the
//! accumulated parse context, and building a `Value::Struct`.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::error::{DoeError, Result};
use crate::parser::context::{DEFERRED_TYPE, Deferred, InstanceStart, PARAM_PREFIX, ParseContext};
use crate::parser::cursor::{BitOrder, Cursor, MAX_TERMINATED_LEN};
use crate::parser::encoding;
use crate::parser::process::{self, MAX_PROCESSED_LEN};
use crate::parser::trace::TraceNode;
use crate::schema::expr;
use crate::schema::expr::{ExprValue, deferred_id};
use crate::schema::ir::{
    CompiledField, CompiledInstance, CompiledIo, CompiledType, CompiledValid, Cut, Encoding,
    Endian, EndianOverride, EndianRule, EndianSwitch, FieldKind, FloatWidth, InstanceKind,
    IntWidth, Process, RepeatMode, SizeExpr, StrSize, TypeRegistry,
};
use crate::value::Value;

// ─────────────────────────────────────────────────────────────────────────────
// Engine
// ─────────────────────────────────────────────────────────────────────────────

/// The deepest nesting of types that [`Engine`] parses.
///
/// A type may refer to itself, directly or through other types; without a
/// limit such a schema would recurse until the stack overflowed.
pub const MAX_TYPE_DEPTH: usize = 128;

/// The longest `bytes` field whose contents are kept in the result.
///
/// A longer one is read as a [`Value::Blob`]: its position, length and first
/// [`BLOB_PREVIEW_LEN`] bytes, so that parsing does not hold, and the output
/// does not print, a sequence as large as the input.
pub const MAX_INLINE_BYTES: usize = 64 * 1024;

/// How many leading bytes of a long `bytes` field a [`Value::Blob`] keeps.
pub const BLOB_PREVIEW_LEN: usize = 256;

/// The most elements reserved up front for a repeated field. The count comes
/// from the input and can be far larger than the input.
const MAX_RESERVED_ELEMENTS: usize = 1024;

/// How many elements of a repeated field may take no bytes, beyond the number
/// of bytes read by the repeat.
const MAX_EMPTY_ELEMENTS: usize = 4096;

/// What [`Engine::parse_type_partial`] got as far as.
#[derive(Debug)]
#[non_exhaustive]
pub struct ParseOutcome {
    /// The value, or, when the parse failed, as much of it as was read. `None`
    /// when nothing was read before the failure, or when it was not kept
    /// (a failure that is not in a type or a repeated field).
    pub value: Option<Value>,
    /// Where the values came from. When the parse failed, the value that was
    /// being read is [`failed`](TraceNode::failed) and the ones around it end
    /// where it did. It is only the root when a parse is not traced.
    pub trace: TraceNode,
    /// Why the parse failed; `None` when it did not.
    pub error: Option<DoeError>,
}

/// How much work a parse may do before it stops with an error. A parse that
/// tries a schema on a file that it may not describe uses this, so that a
/// schema that loops or reads without end cannot hold up the others.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// The most fields (and elements of a repeated field) that are read.
    pub max_fields: Option<u64>,
    /// The longest the parse may take. Not available in WebAssembly, where
    /// there is no clock: use `max_fields` there.
    pub max_time: Option<std::time::Duration>,
}

impl Limits {
    /// Limit only the number of fields.
    #[must_use]
    pub fn fields(max_fields: u64) -> Self {
        Limits {
            max_fields: Some(max_fields),
            max_time: None,
        }
    }
}

/// Walks a compiled [`TypeRegistry`] over a byte buffer to produce a [`Value`] tree.
#[derive(Debug)]
pub struct Engine<'reg> {
    registry: &'reg TypeRegistry,
    /// Whether the instances of a type are part of the result.
    instances: bool,
    /// How much work a parse may do.
    limits: Limits,
}

impl<'reg> Engine<'reg> {
    /// Create an engine that resolves type names against `registry`.
    pub fn new(registry: &'reg TypeRegistry) -> Self {
        Engine {
            registry,
            instances: true,
            limits: Limits::default(),
        }
    }

    /// Limit the work of every parse by this engine (none, by default).
    #[must_use]
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Choose whether the instances of a type are part of the result (the
    /// default). Those that the fields of `seq` use are computed either way.
    #[must_use]
    pub fn with_instances(mut self, instances: bool) -> Self {
        self.instances = instances;
        self
    }

    /// Parse `type_name` from the beginning of `cursor`, returning a
    /// `Value::Struct` with all fields populated.
    ///
    /// # Errors
    ///
    /// Returns an error if the type is unknown, the buffer is too short, or
    /// the data does not match the schema.
    pub fn parse_type(&self, type_name: &str, cursor: &mut Cursor<'_>) -> Result<Value> {
        let outcome = self.parse_with(type_name, cursor, false, false)?;
        match (outcome.error, outcome.value) {
            (Some(e), _) => Err(e),
            (None, Some(value)) => Ok(value),
            (None, None) => Err(DoeError::Schema("the parse left no value".to_owned())),
        }
    }

    /// Parse `type_name` as [`parse_type`](Self::parse_type) does, and also
    /// return where in the input every value was read from.
    ///
    /// The root of the [`TraceNode`] tree is the type itself. Offsets are in
    /// bits from the start of the input the cursor was made over, so the tree
    /// is that of the whole input even when the cursor is a window of it.
    ///
    /// # Errors
    ///
    /// The same as [`parse_type`](Self::parse_type).
    pub fn parse_type_traced(
        &self,
        type_name: &str,
        cursor: &mut Cursor<'_>,
    ) -> Result<(Value, TraceNode)> {
        let outcome = self.parse_with(type_name, cursor, true, false)?;
        if let Some(e) = outcome.error {
            return Err(e);
        }
        outcome
            .value
            .ok_or_else(|| DoeError::Schema("the parse left no value".to_owned()))
            .map(|value| (value, outcome.trace))
    }

    /// Parse `type_name`, and when the parse fails keep what was read before
    /// it did: the [`ParseOutcome`] has the value and the trace of every field
    /// read until then, and the error.
    ///
    /// A value that was being read when the parse failed is there as far as it
    /// got: a type with the fields before the failing one, a repeated field
    /// with the elements before it. In the trace, the value that was being read
    /// is marked [`failed`](TraceNode::failed). A field whose value fails a
    /// `valid:` rule keeps the value it read.
    ///
    /// # Errors
    ///
    /// Only when nothing could be tried: the type is unknown, or takes
    /// parameters and cannot be parsed on its own. A failed parse is a
    /// [`ParseOutcome`] with an `error`.
    pub fn parse_type_partial(
        &self,
        type_name: &str,
        cursor: &mut Cursor<'_>,
    ) -> Result<ParseOutcome> {
        self.parse_with(type_name, cursor, true, true)
    }

    fn parse_with(
        &self,
        type_name: &str,
        cursor: &mut Cursor<'_>,
        traced: bool,
        partial: bool,
    ) -> Result<ParseOutcome> {
        let ty = self
            .registry
            .get(type_name)
            .ok_or_else(|| DoeError::UnknownType {
                type_name: type_name.to_owned(),
                context: "<engine>".to_owned(),
            })?;
        if !ty.params.is_empty() {
            return Err(DoeError::FieldError {
                field: type_name.to_owned(),
                context: "<engine>".to_owned(),
                message: format!(
                    "the type takes {} parameter{} and cannot be parsed on its own",
                    ty.params.len(),
                    if ty.params.len() == 1 { "" } else { "s" }
                ),
            });
        }
        let mut ctx = ParseContext::new();
        ctx.limit(self.limits.max_fields, self.limits.max_time);
        ctx.set_input_len(cursor.len());
        ctx.set_root_len(cursor.root_len());
        // `sizeof<type>` and `_sizeof`, which the compiler turned into names
        // of the root scope, are the sizes of the types that always have one.
        for (name, size) in &self.registry.static_sizes {
            ctx.eval_ctx.bind(
                &format!("sizeof<{name}>"),
                ExprValue::Int(i128::from(*size)),
            );
        }
        if traced {
            ctx.enable_trace();
            let start = cursor.bit_position();
            if let Some(trace) = ctx.trace() {
                trace.begin(type_name, start);
            }
        }
        if partial {
            ctx.enable_partial();
        }
        let result = self
            .parse_compiled_type(ty, cursor, &mut ctx)
            .and_then(|value| {
                if ctx.has_deferred() {
                    self.settle_deferred(cursor, &mut ctx, value)
                } else {
                    Ok(value)
                }
            });
        let end = cursor.bit_position();
        let (value, error) = match result {
            Ok(value) => (Some(value), None),
            Err(e) => {
                // What the values that failed had read, with the instances that
                // were waiting for later left out.
                let mut value = ctx.take_partial();
                if let Some(value) = value.as_mut() {
                    substitute_deferred(value, &HashMap::new());
                }
                if let Some(trace) = ctx.trace() {
                    trace.close_failed();
                }
                (value, Some(e))
            }
        };
        let tree = ctx.take_trace().and_then(|mut builder| {
            if error.is_none() {
                builder.finish(end);
            }
            builder.into_tree()
        });
        let trace = tree.unwrap_or_else(|| TraceNode::untraced(type_name));
        Ok(ParseOutcome {
            value,
            trace,
            error,
        })
    }

    // ── Instances that wait ───────────────────────────────────────────────────

    /// Compute the instances that could not be computed where their type ended
    /// (they use what is read later), now that the input is read, and put
    /// them in their places in `value`.
    ///
    /// They are tried in rounds: one may need another that comes after it. If
    /// a round computes none, the first error of the round is the result.
    fn settle_deferred(
        &self,
        cursor: &Cursor<'_>,
        ctx: &mut ParseContext,
        mut value: Value,
    ) -> Result<Value> {
        let mut results: HashMap<u64, Value> = HashMap::new();
        while ctx.has_deferred() {
            let waiting = ctx.take_deferred();
            let total = waiting.len();
            let mut still = Vec::new();
            let mut first_error = None;
            for item in waiting {
                let mark = ctx.deferred_len();
                match self.compute_deferred(&item, cursor, ctx) {
                    Ok(computed) => {
                        // What was bound with the struct it belongs to can now
                        // be told what it is.
                        if let Some(settled) = ParseContext::expr_value(&computed) {
                            ctx.eval_ctx.resolve_deferred(item.id, settled);
                        }
                        results.insert(item.id, computed);
                    }
                    Err(e) => {
                        // What the failed attempt put off belongs to nothing.
                        ctx.take_partial();
                        ctx.truncate_deferred(mark);
                        first_error.get_or_insert(e);
                        still.push(item);
                    }
                }
            }
            if still.len() == total {
                // None could be computed, so none will be.
                if ctx.keeps_partial() {
                    substitute_deferred(&mut value, &results);
                    ctx.set_partial(value);
                }
                return Err(first_error.unwrap_or_else(|| {
                    DoeError::Schema("an instance could not be computed".to_owned())
                }));
            }
            ctx.put_back_deferred(still);
        }
        substitute_deferred(&mut value, &results);
        Ok(value)
    }

    /// Compute one instance that waited, in the scope it belongs to.
    fn compute_deferred(
        &self,
        item: &Deferred,
        cursor: &Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        let ty = self
            .registry
            .get(&item.type_name)
            .ok_or_else(|| DoeError::UnknownType {
                type_name: item.type_name.clone(),
                context: "<engine>".to_owned(),
            })?;
        let instance = ty.instances.get(item.instance).ok_or_else(|| {
            DoeError::Schema(format!("{} has no instance {}", ty.name, item.instance))
        })?;
        let saved = ctx.eval_ctx.enter_frame(item.frame);
        if let Some(trace) = ctx.trace() {
            trace.resume(item.id);
        }
        let (start, size, end) = item.stream;
        let outcome = cursor.stream(start, size).and_then(|mut own| {
            own.seek(end)?;
            self.ensure_instance(ty, instance, &mut own, ctx)?;
            Ok(ctx.take_instance(&instance.id))
        });
        ctx.eval_ctx.restore_frames(saved);
        outcome?.ok_or_else(|| DoeError::FieldError {
            field: instance.id.clone(),
            context: ty.name.clone(),
            message: "the instance was not computed".to_owned(),
        })
    }

    // ── Internal recursion ────────────────────────────────────────────────────

    fn parse_compiled_type(
        &self,
        ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        let mut fields: Vec<(String, Value)> = Vec::new();
        let endian = Self::type_endian(ty, cursor, ctx)?;
        ctx.set_endian(endian);
        if let Some(trace) = ctx.trace() {
            trace.enter_type(&ty.name);
        }
        if self.registry.uses_io {
            // `_parent._io` and `_root._io` are looked up in the scope.
            ctx.eval_ctx.bind("_io", stream_expr(cursor));
        }

        if ctx.depth() == 1 {
            // A constant of the root (`value: 3`) is known before anything is read, so that the types read
            // first can use it as `_root.name`.
            self.compute_constants(ty, cursor, ctx)?;
        }

        for field in &ty.fields {
            let start = cursor.absolute_pos();
            let start_bits = Self::trace_start(&field.kind, cursor);
            let depth = ctx.trace().map(|trace| {
                let depth = trace.depth();
                trace.begin(&field.id, start_bits);
                depth
            });
            let value = match self.parse_field(field, ty, cursor, ctx) {
                Ok(value) => value,
                Err(e) => return Err(Self::keep_struct(ty, fields, Some(&field.id), ctx, e)),
            };
            if let Some(depth) = depth
                && let Some(trace) = ctx.trace()
            {
                if value.is_absent() {
                    trace.discard(depth);
                } else {
                    trace.finish(cursor.bit_position());
                }
            }

            // Bind into the context so subsequent fields can reference this one
            ctx.bind(&field.id, value.clone());
            if self.registry.uses_field_sizes {
                // `field._sizeof`: the bytes the field took.
                let size = cursor.absolute_pos().saturating_sub(start);
                ctx.eval_ctx.bind(
                    &format!("{}._sizeof", field.id),
                    ExprValue::Int(i128::try_from(size).unwrap_or(i128::MAX)),
                );
            }
            fields.push((field.id.clone(), value));
        }

        if !ty.instances.is_empty()
            && let Err(e) = self.finish_instances(ty, cursor, ctx, &mut fields)
        {
            return Err(Self::keep_struct(ty, fields, None, ctx, e));
        }
        if !ty.params.is_empty() {
            // The parameters, which expressions can read as fields of the value.
            for param in &ty.params {
                if let Some(value) = ctx.eval_ctx.get(&param.id).and_then(|v| to_value(v).ok()) {
                    fields.push((format!("{PARAM_PREFIX}{}", param.id), value));
                }
            }
        }
        if self.registry.uses_io {
            // The stream the value was parsed from, for `some_field._io`.
            fields.push((STREAM_FIELD.to_owned(), stream_value(cursor)));
        }
        if let Some(trace) = ctx.trace() {
            trace.leave_type();
        }

        Ok(Value::Struct {
            type_name: ty.name.clone(),
            fields,
        })
    }

    /// A type failed to parse: when partial results are kept, record the type
    /// as far as it got, with what the failing field had read of itself, and
    /// pass the error on.
    fn keep_struct(
        ty: &CompiledType,
        mut fields: Vec<(String, Value)>,
        failing: Option<&str>,
        ctx: &mut ParseContext,
        error: DoeError,
    ) -> DoeError {
        if ctx.keeps_partial() {
            let inner = ctx.take_partial();
            if let (Some(name), Some(inner)) = (failing, inner) {
                fields.push((name.to_owned(), inner));
            }
            ctx.set_partial(Value::Struct {
                type_name: ty.name.clone(),
                fields,
            });
        }
        error
    }

    /// A repeated field failed: record the elements read, and the one that
    /// failed as far as it got.
    fn keep_array(mut items: Vec<Value>, ctx: &mut ParseContext, error: DoeError) -> DoeError {
        if ctx.keeps_partial() {
            if let Some(inner) = ctx.take_partial() {
                items.push(inner);
            }
            ctx.set_partial(Value::Array(items));
        }
        error
    }

    /// The byte order of `ty` for this parse: its own, or the one a value
    /// chooses, or the one of the type around it.
    fn type_endian(
        ty: &CompiledType,
        cursor: &Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Endian> {
        match &ty.endian_rule {
            EndianRule::Fixed => Ok(ty.endian),
            EndianRule::Inherit => Ok(ctx.parent_endian(ty.endian)),
            EndianRule::Switch(switch) => Self::choose_endian(switch, ty, cursor, ctx),
        }
    }

    fn choose_endian(
        switch: &EndianSwitch,
        ty: &CompiledType,
        cursor: &Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Endian> {
        let fail = |message: String| DoeError::FieldError {
            field: "meta.endian".to_owned(),
            context: ty.name.clone(),
            message,
        };
        ctx.set_io(cursor);
        let selector = expr::eval(&switch.on, &ctx.eval_ctx, "<endian>")
            .map_err(|e| annotate(&e, "meta.endian", &ty.name))?;
        for (key, endian) in &switch.cases {
            let key = expr::eval(key, &ctx.eval_ctx, "<endian>")
                .map_err(|e| annotate(&e, "meta.endian", &ty.name))?;
            if expr::values_equal(&selector, &key, "<endian>")? {
                return Ok(*endian);
            }
        }
        switch
            .default
            .ok_or_else(|| fail(format!("{selector} matches no case of the byte order")))
    }

    /// Compute the instances of `ty` whose value is a constant of the schema.
    fn compute_constants(
        &self,
        ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<()> {
        for instance in &ty.instances {
            if let InstanceKind::Value {
                expr: value_expr,
                if_expr: None,
                ..
            } = &instance.kind
                && expr::is_constant(value_expr)
            {
                self.ensure_instance(ty, instance, cursor, ctx)?;
            }
        }
        Ok(())
    }

    /// Compute the instances of `ty` and add them to `fields`, in schema
    /// order, after the fields of `seq`. Some may have been computed already,
    /// because a field used them.
    ///
    /// The functions on the path of a nested type are kept small, with the
    /// bulk of the work in functions like this one that the recursion does not
    /// pass through, so that deeply nested types do not use much stack.
    fn finish_instances(
        &self,
        ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
        fields: &mut Vec<(String, Value)>,
    ) -> Result<()> {
        let mut waiting: HashMap<usize, u64> = HashMap::new();
        for (index, instance) in ty.instances.iter().enumerate() {
            let mark = ctx.deferred_len();
            if let Err(e) = self.ensure_instance(ty, instance, cursor, ctx) {
                // One that uses what is read later is computed once everything
                // is, when the input it is in is the one being parsed.
                if !ctx.is_input(cursor) {
                    return Err(e);
                }
                ctx.take_partial();
                ctx.truncate_deferred(mark);
                let stream = (cursor.window_start(), cursor.len(), cursor.pos());
                let id = ctx.defer(&ty.name, index, stream);
                waiting.insert(index, id);
                if let Some(trace) = ctx.trace() {
                    // Its node stays, empty, until it is computed.
                    trace.keep_for(id, &instance.id);
                }
            }
        }
        for (index, instance) in ty.instances.iter().enumerate() {
            if let Some(&id) = waiting.get(&index) {
                if self.instances {
                    fields.push((instance.id.clone(), deferred_placeholder(id)));
                }
            } else if let Some(value) = ctx.take_instance(&instance.id)
                && self.instances
            {
                fields.push((instance.id.clone(), value));
            }
        }
        Ok(())
    }

    fn parse_field(
        &self,
        field: &CompiledField,
        parent_ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        ctx.charge()?;
        if !field.uses_instances.is_empty() {
            self.force_instances(field, parent_ty, cursor, ctx)?;
        }
        if field.if_expr.is_some() && !Self::guard_holds(field, parent_ty, cursor, ctx)? {
            return Ok(Value::Absent);
        }
        if field.pos.is_some() {
            return self.parse_at_pos(field, parent_ty, cursor, ctx);
        }
        self.parse_repeats(field, parent_ty, cursor, ctx)
    }

    /// Compute the instances the field's expressions use.
    fn force_instances(
        &self,
        field: &CompiledField,
        parent_ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<()> {
        for name in &field.uses_instances {
            self.ensure_named_instance(parent_ty, name, cursor, ctx)?;
        }
        Ok(())
    }

    /// Whether the field's `if:` guard is true.
    fn guard_holds(
        field: &CompiledField,
        parent_ty: &CompiledType,
        cursor: &Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<bool> {
        let Some(if_expr) = &field.if_expr else {
            return Ok(true);
        };
        ctx.set_io(cursor);
        expr::eval_bool(if_expr, &ctx.eval_ctx, "<if>")
            .map_err(|e| annotate(&e, &field.id, &parent_ty.name))
    }

    /// `pos:` reads the whole field (every repeat) somewhere else, then puts
    /// the cursor back where the sequence left off.
    fn parse_at_pos(
        &self,
        field: &CompiledField,
        parent_ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        let Some(pos_expr) = &field.pos else {
            return self.parse_repeats(field, parent_ty, cursor, ctx);
        };
        ctx.set_io(cursor);
        let target = expr::eval_int(pos_expr, &ctx.eval_ctx, "<pos>")
            .map_err(|e| annotate(&e, &field.id, &parent_ty.name))?;
        let target = usize::try_from(target).map_err(|_| DoeError::FieldError {
            field: field.id.clone(),
            context: parent_ty.name.clone(),
            message: format!("pos evaluated to an invalid offset: {target}"),
        })?;
        let resume = cursor.mark();
        cursor
            .seek(target)
            .map_err(|e| annotate(&e, &field.id, &parent_ty.name))?;
        let placed_at = cursor.bit_position();
        let value = self.parse_repeats(field, parent_ty, cursor, ctx)?;
        let placed_end = cursor.bit_position();
        if let Some(trace) = ctx.trace() {
            trace.place(placed_at, placed_end);
        }
        ctx.charge_reread(cursor.pos().saturating_sub(target))
            .map_err(|message| DoeError::FieldError {
                field: field.id.clone(),
                context: parent_ty.name.clone(),
                message,
            })?;
        cursor.reset(resume);
        Ok(value)
    }

    /// Dispatch on the field's repeat mode.
    fn parse_repeats(
        &self,
        field: &CompiledField,
        parent_ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        match &field.repeat {
            RepeatMode::Once => self.parse_field_once(field, parent_ty, cursor, ctx),
            RepeatMode::Eos => self.repeat_eos(field, parent_ty, cursor, ctx),
            RepeatMode::Count(count_expr) => {
                self.repeat_count(count_expr, field, parent_ty, cursor, ctx)
            }
            RepeatMode::Until(until_expr) => {
                self.repeat_until(until_expr, field, parent_ty, cursor, ctx)
            }
        }
    }

    fn repeat_eos(
        &self,
        field: &CompiledField,
        parent_ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        let mut items = Vec::new();
        while !cursor.is_eof() {
            Self::expose_elements(field, &items, ctx);
            match self.parse_repeated_element(items.len(), field, parent_ty, cursor, ctx) {
                Ok(v) => items.push(v),
                Err(e) => return Err(Self::keep_array(items, ctx, e)),
            }
        }
        Self::hide_elements(field, ctx);
        Ok(Value::Array(items))
    }

    fn repeat_count(
        &self,
        count_expr: &expr::Expr,
        field: &CompiledField,
        parent_ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        ctx.set_io(cursor);
        let n = expr::eval_int(count_expr, &ctx.eval_ctx, "<repeat-expr>")
            .map_err(|e| annotate(&e, &field.id, &parent_ty.name))?;
        if n < 0 {
            return Err(DoeError::FieldError {
                field: field.id.clone(),
                context: parent_ty.name.clone(),
                message: format!("repeat count evaluated to negative value: {n}"),
            });
        }
        let count = usize::try_from(n).map_err(|_| DoeError::FieldError {
            field: field.id.clone(),
            context: parent_ty.name.clone(),
            message: format!("repeat count too large: {n}"),
        })?;
        // Each element consumes at least one byte, so never reserve more
        // than the remaining input can hold, and never a size that a
        // large input and a large count could make unreasonable.
        let mut items =
            Vec::with_capacity(count.min(cursor.remaining()).min(MAX_RESERVED_ELEMENTS));
        // An element may take no bytes (the entry after a `long` in a Java
        // constant pool, or a type of instances only that a repeat uses to
        // compute a table), but not more often than there are bytes read in
        // this repeat and a few thousand more, so that an enormous count of
        // empty elements is an error instead of an endless loop.
        let start = cursor.bit_position();
        let mut empty = 0usize;
        for index in 0..count {
            Self::expose_elements(field, &items, ctx);
            let before = cursor.bit_position();
            let v = match self.parse_element(index, field, parent_ty, cursor, ctx) {
                Ok(v) => v,
                Err(e) => return Err(Self::keep_array(items, ctx, e)),
            };
            if cursor.bit_position() == before {
                empty += 1;
                let read_bytes = cursor.bit_position().saturating_sub(start).div_ceil(8);
                if usize::try_from(read_bytes).is_ok_and(|read| empty > read + MAX_EMPTY_ELEMENTS) {
                    items.push(v);
                    return Err(Self::keep_array(
                        items,
                        ctx,
                        Self::no_bytes_error(field, parent_ty),
                    ));
                }
            }
            items.push(v);
        }
        Self::hide_elements(field, ctx);
        Ok(Value::Array(items))
    }

    fn repeat_until(
        &self,
        until_expr: &expr::Expr,
        field: &CompiledField,
        parent_ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        let mut items = Vec::new();
        loop {
            Self::expose_elements(field, &items, ctx);
            let v = match self.parse_repeated_element(items.len(), field, parent_ty, cursor, ctx) {
                Ok(v) => v,
                Err(e) => return Err(Self::keep_array(items, ctx, e)),
            };
            // Bind the most recently parsed value to `_` so the
            // until-expression can reference it, matching the convention
            // used by KaitaiStruct.
            ctx.bind_scratch("_", &v);
            ctx.set_io(cursor);
            let done = expr::eval_bool(until_expr, &ctx.eval_ctx, "<repeat-until>")
                .map_err(|e| annotate(&e, &field.id, &parent_ty.name))?;
            items.push(v);
            if done || cursor.is_eof() {
                break;
            }
        }
        Self::hide_elements(field, ctx);
        Ok(Value::Array(items))
    }

    /// Let the expressions of a repeated field that uses the elements read so
    /// far (`groups[_index - 1]`) see them under the name of the field.
    fn expose_elements(field: &CompiledField, items: &[Value], ctx: &mut ParseContext) {
        if field.reads_itself {
            ctx.bind_scratch(&field.id, &Value::Array(items.to_vec()));
        }
    }

    /// The end of a repeated field: `_index` and the elements go out of sight
    /// (the field is bound with all its elements once it is read).
    fn hide_elements(field: &CompiledField, ctx: &mut ParseContext) {
        ctx.eval_ctx.unbind_prefix("_index");
        if field.reads_itself {
            ctx.eval_ctx.unbind_prefix(&field.id);
        }
    }

    /// Parse element number `index` of a repeated field, requiring it to
    /// consume input.
    ///
    /// An element that consumes nothing (an empty type, a zero-size `bytes`)
    /// would repeat forever under `eos` or `until`, so it is an error. Under
    /// `repeat-expr` see [`Self::repeat_count`].
    fn parse_repeated_element(
        &self,
        index: usize,
        field: &CompiledField,
        parent_ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        ctx.charge()?;
        let before = cursor.bit_position();
        let value = self.parse_element(index, field, parent_ty, cursor, ctx)?;
        if cursor.bit_position() == before {
            return Err(Self::no_bytes_error(field, parent_ty));
        }
        Ok(value)
    }

    fn no_bytes_error(field: &CompiledField, parent_ty: &CompiledType) -> DoeError {
        DoeError::FieldError {
            field: field.id.clone(),
            context: parent_ty.name.clone(),
            message: "repeated element consumed no bytes".to_owned(),
        }
    }

    /// Where a field of `kind` starts, in bits: a field that is not a bit
    /// field starts on a byte boundary, and one made of other fields starts
    /// where its first does (which the trace sees when it is finished).
    fn trace_start(kind: &FieldKind, cursor: &Cursor<'_>) -> u64 {
        let here = cursor.bit_position();
        match kind {
            FieldKind::Bits { .. } | FieldKind::TypeRef { .. } | FieldKind::Switch { .. } => here,
            _ => here.next_multiple_of(8),
        }
    }

    /// Parse element number `index` of a repeated field, whatever it consumes.
    fn parse_element(
        &self,
        index: usize,
        field: &CompiledField,
        parent_ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        // `_index` is the position of the element, for the expressions of the
        // field and of the arguments and `until` that go with it.
        ctx.eval_ctx.bind(
            "_index",
            ExprValue::Int(i128::try_from(index).unwrap_or(i128::MAX)),
        );
        let start = Self::trace_start(&field.kind, cursor);
        if let Some(trace) = ctx.trace() {
            trace.begin(&format!("[{index}]"), start);
        }
        let value = self.parse_field_once(field, parent_ty, cursor, ctx)?;
        let end = cursor.bit_position();
        if let Some(trace) = ctx.trace() {
            trace.finish(end);
        }
        Ok(value)
    }

    /// Parse a single (non-repeated) instance of a field.
    fn parse_field_once(
        &self,
        field: &CompiledField,
        parent_ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        ctx.set_pending_parent(field.parent_up);
        let raw = match &field.process {
            Some(process) => self.read_processed(field, process, parent_ty, cursor, ctx)?,
            None => self
                .read_kind(&field.kind, parent_ty, cursor, ctx)
                .map_err(|e| match (&field.kind, e) {
                    // A string that is not valid in its encoding, or too long.
                    (FieldKind::Str { .. }, e @ DoeError::Schema(_)) => {
                        annotate(&e, &field.id, &parent_ty.name)
                    }
                    (_, e) => e,
                })?,
        };
        if let Some(valid) = &field.valid
            && !valid.expr_instances.is_empty()
            && let Some(read) = ParseContext::expr_value(&raw)
        {
            // the rule uses instances of the type, which may use the value just read
            ctx.eval_ctx.bind(&field.id, read);
            for name in &valid.expr_instances {
                self.ensure_named_instance(parent_ty, name, cursor, ctx)?;
            }
        }
        if let Some(valid) = &field.valid
            && let Err(e) = Self::check_valid(valid, field, parent_ty, &raw, cursor, ctx)
        {
            // The value was read; it is the rule that it fails.
            if ctx.keeps_partial() {
                ctx.set_partial(raw);
            }
            return Err(e);
        }
        Ok(apply_enum(raw, field.enum_ref.as_deref(), parent_ty))
    }

    /// Read a field whose bytes are transformed (`process:`): take its window,
    /// apply the transform, and read the field from the result.
    fn read_processed(
        &self,
        field: &CompiledField,
        process: &Process,
        parent_ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        let fail = |message: String| DoeError::FieldError {
            field: field.id.clone(),
            context: parent_ty.name.clone(),
            message,
        };
        let (FieldKind::Bytes { size }
        | FieldKind::Str {
            size: StrSize::Fixed(size),
            ..
        }
        | FieldKind::TypeRef {
            size: Some(size), ..
        }) = &field.kind
        else {
            return Err(fail("process: the field has no size".to_owned()));
        };
        let n = eval_size(size, ctx, cursor)?;
        if n > MAX_PROCESSED_LEN {
            return Err(fail(format!(
                "process: {n} bytes are more than the limit of {MAX_PROCESSED_LEN} bytes"
            )));
        }
        let offset = cursor.absolute_pos();
        let mut data = cursor.read_bytes(n)?.into_owned();
        ctx.set_io(cursor);
        let apply = |ctx: &ParseContext, e: &expr::Expr| {
            expr::eval(e, &ctx.eval_ctx, "<process>")
                .map_err(|err| annotate(&err, &field.id, &parent_ty.name))
        };
        let int_arg = |ctx: &ParseContext, e: &expr::Expr| {
            expr::eval_int(e, &ctx.eval_ctx, "<process>")
                .map_err(|err| annotate(&err, &field.id, &parent_ty.name))
        };
        match process {
            Process::Xor(key) => {
                let key = match apply(ctx, key)? {
                    ExprValue::Bytes(bytes) => bytes,
                    ExprValue::Int(k) => vec![u8::try_from(k & 0xff).unwrap_or_default()],
                    other => {
                        return Err(fail(format!(
                            "process: an xor key is an integer or bytes, not a {}",
                            other.type_name()
                        )));
                    }
                };
                process::xor(&mut data, &key).map_err(|e| fail(format!("process: {e}")))?;
            }
            Process::Rol(by) => process::rotate_left(&mut data, int_arg(ctx, by)?),
            Process::Ror(by) => process::rotate_left(&mut data, int_arg(ctx, by)?.wrapping_neg()),
            Process::Zlib => {
                data = process::inflate(&data, MAX_PROCESSED_LEN)
                    .map_err(|e| fail(format!("process: {e}")))?;
            }
        }
        let mut inner = Cursor::new(&data);
        if let Some(trace) = ctx.trace() {
            trace.enter_processed();
        }
        let read = self.read_processed_value(field, parent_ty, &data, offset, &mut inner, ctx);
        if let Some(trace) = ctx.trace() {
            trace.leave_processed();
        }
        read
    }

    /// The value of a `process:` field, read from the bytes the transform made.
    fn read_processed_value(
        &self,
        field: &CompiledField,
        parent_ty: &CompiledType,
        data: &[u8],
        offset: usize,
        inner: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        let fail = |message: String| DoeError::FieldError {
            field: field.id.clone(),
            context: parent_ty.name.clone(),
            message,
        };
        match &field.kind {
            FieldKind::Bytes { .. } => {
                if data.len() <= MAX_INLINE_BYTES {
                    return Ok(Value::Bytes(data.to_vec()));
                }
                let preview = data.get(..BLOB_PREVIEW_LEN).unwrap_or_default().to_vec();
                Ok(Value::Blob {
                    offset: u64::try_from(offset).unwrap_or(u64::MAX),
                    len: u64::try_from(data.len()).unwrap_or(u64::MAX),
                    preview,
                })
            }
            FieldKind::Str { encoding, .. } => {
                Self::read_str(&StrSize::Fixed(SizeExpr::Eos), *encoding, inner, ctx)
            }
            FieldKind::TypeRef {
                type_name, args, ..
            } => {
                let whole = FieldKind::TypeRef {
                    type_name: type_name.clone(),
                    size: None,
                    args: args.clone(),
                };
                self.read_kind(&whole, parent_ty, inner, ctx)
            }
            _ => Err(fail("process: the field has no size".to_owned())),
        }
    }

    /// Fail unless `value`, just read for `field`, satisfies every rule.
    fn check_valid(
        valid: &CompiledValid,
        field: &CompiledField,
        parent_ty: &CompiledType,
        value: &Value,
        cursor: &Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<()> {
        let fail = |message: String| DoeError::FieldError {
            field: field.id.clone(),
            context: parent_ty.name.clone(),
            message: format!("validation failed: {message}"),
        };
        let evaluate = |e: &expr::Expr, ctx: &ParseContext| {
            expr::eval(e, &ctx.eval_ctx, "<valid>")
                .map_err(|err| annotate(&err, &field.id, &parent_ty.name))
        };
        let Some(read) = ParseContext::expr_value(value) else {
            return Err(fail(format!("a {} cannot be checked", value.type_name())));
        };
        ctx.set_io(cursor);
        // The rules may name the field itself, which has the value just read.
        ctx.eval_ctx.bind(&field.id, read.clone());
        if let Some(rule) = &valid.eq {
            let want = evaluate(rule, ctx)?;
            if !expr::values_equal(&read, &want, "<valid>")? {
                return Err(fail(format!("{read} is not equal to {want}")));
            }
        }
        if let Some(rule) = &valid.min {
            let min = evaluate(rule, ctx)?;
            if matches!(
                expr::compare_values(&read, &min, "<valid>")?,
                Some(Ordering::Less) | None
            ) {
                return Err(fail(format!("{read} is less than the minimum {min}")));
            }
        }
        if let Some(rule) = &valid.max {
            let max = evaluate(rule, ctx)?;
            if matches!(
                expr::compare_values(&read, &max, "<valid>")?,
                Some(Ordering::Greater) | None
            ) {
                return Err(fail(format!("{read} is more than the maximum {max}")));
            }
        }
        if !valid.any_of.is_empty() {
            let mut options = Vec::with_capacity(valid.any_of.len());
            for rule in &valid.any_of {
                options.push(evaluate(rule, ctx)?);
            }
            let mut found = false;
            for option in &options {
                if expr::values_equal(&read, option, "<valid>")? {
                    found = true;
                    break;
                }
            }
            if !found {
                let list: Vec<String> = options.iter().map(ToString::to_string).collect();
                return Err(fail(format!("{read} is not one of {}", list.join(", "))));
            }
        }
        if let Some(rule) = &valid.expr {
            // `_` is the value, only for this expression.
            ctx.eval_ctx.bind("_", read.clone());
            let outcome = expr::eval_bool(rule, &ctx.eval_ctx, "<valid>");
            ctx.eval_ctx.unbind_prefix("_");
            if !outcome.map_err(|err| annotate(&err, &field.id, &parent_ty.name))? {
                return Err(fail(format!("{read} does not satisfy the expression")));
            }
        }
        if valid.in_enum
            && let Some(enum_name) = &field.enum_ref
        {
            let member = enum_discriminant(value)
                .and_then(enum_key)
                .and_then(|n| parent_ty.enums.get(enum_name.as_str())?.lookup(n));
            if member.is_none() {
                return Err(fail(format!(
                    "{read} is not a member of the enum '{enum_name}'"
                )));
            }
        }
        Ok(())
    }

    // ── FieldKind dispatch ────────────────────────────────────────────────────

    fn read_kind(
        &self,
        kind: &FieldKind,
        parent_ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        match kind {
            FieldKind::UInt { width, endian } => Self::read_uint(
                *width,
                resolve_endian(*endian, ctx.endian(parent_ty.endian)),
                cursor,
            ),
            FieldKind::SInt { width, endian } => Self::read_sint(
                *width,
                resolve_endian(*endian, ctx.endian(parent_ty.endian)),
                cursor,
            ),
            FieldKind::Float { width, endian } => Self::read_float(
                *width,
                resolve_endian(*endian, ctx.endian(parent_ty.endian)),
                cursor,
            ),
            FieldKind::Bytes { size } => Self::read_sized_bytes(size, cursor, ctx),
            FieldKind::BytesCut { cut } => Ok(Value::Bytes(Self::read_cut(cut, cursor, ctx)?)),
            FieldKind::Str { size, encoding } => Self::read_str(size, *encoding, cursor, ctx),
            FieldKind::Bits { bit_size, endian } => {
                Self::read_bit_field(*bit_size, *endian, parent_ty, cursor)
            }
            FieldKind::TypeRef {
                type_name,
                size,
                args,
            } => self.read_type_ref(type_name, (size.as_ref(), args), parent_ty, cursor, ctx),
            FieldKind::Switch {
                on,
                size,
                cases,
                default,
            } => self.read_switch(
                on,
                size.as_ref(),
                cases,
                default.as_deref(),
                parent_ty,
                cursor,
                ctx,
            ),
            FieldKind::Contents(expected) => {
                let actual = cursor.read_bytes(expected.len())?;
                if actual != expected.as_slice() {
                    return Err(DoeError::Schema(format!(
                        "contents mismatch at offset {}: expected {:?}, got {:?}",
                        cursor.pos() - expected.len(),
                        expected,
                        actual,
                    )));
                }
                Ok(Value::Bytes(expected.clone()))
            }
        }
    }

    fn read_uint(width: IntWidth, endian: Endian, cursor: &mut Cursor<'_>) -> Result<Value> {
        let v = match (width, endian) {
            (IntWidth::W8, _) => u64::from(cursor.read_u8()?),
            (IntWidth::W16, Endian::Little) => u64::from(cursor.read_u16_le()?),
            (IntWidth::W16, Endian::Big) => u64::from(cursor.read_u16_be()?),
            (IntWidth::W32, Endian::Little) => u64::from(cursor.read_u32_le()?),
            (IntWidth::W32, Endian::Big) => u64::from(cursor.read_u32_be()?),
            (IntWidth::W64, Endian::Little) => cursor.read_u64_le()?,
            (IntWidth::W64, Endian::Big) => cursor.read_u64_be()?,
        };
        Ok(Value::UInt(v))
    }

    fn read_sint(width: IntWidth, endian: Endian, cursor: &mut Cursor<'_>) -> Result<Value> {
        let v = match (width, endian) {
            (IntWidth::W8, _) => i64::from(cursor.read_i8()?),
            (IntWidth::W16, Endian::Little) => i64::from(cursor.read_i16_le()?),
            (IntWidth::W16, Endian::Big) => i64::from(cursor.read_i16_be()?),
            (IntWidth::W32, Endian::Little) => i64::from(cursor.read_i32_le()?),
            (IntWidth::W32, Endian::Big) => i64::from(cursor.read_i32_be()?),
            (IntWidth::W64, Endian::Little) => cursor.read_i64_le()?,
            (IntWidth::W64, Endian::Big) => cursor.read_i64_be()?,
        };
        Ok(Value::SInt(v))
    }

    fn read_float(width: FloatWidth, endian: Endian, cursor: &mut Cursor<'_>) -> Result<Value> {
        let v = match (width, endian) {
            (FloatWidth::F32, Endian::Little) => f64::from(cursor.read_f32_le()?),
            (FloatWidth::F32, Endian::Big) => f64::from(cursor.read_f32_be()?),
            (FloatWidth::F64, Endian::Little) => cursor.read_f64_le()?,
            (FloatWidth::F64, Endian::Big) => cursor.read_f64_be()?,
        };
        Ok(Value::Float(v))
    }

    /// Compute the instance of `ty` called `name`, unless it already is.
    fn ensure_named_instance(
        &self,
        ty: &CompiledType,
        name: &str,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<()> {
        match ty.instances.iter().find(|instance| instance.id == name) {
            Some(instance) => self.ensure_instance(ty, instance, cursor, ctx),
            None => Ok(()),
        }
    }

    /// Compute `instance` of `ty` once, leaving the position of `cursor` as it
    /// was and recording the value in the context.
    fn ensure_instance(
        &self,
        ty: &CompiledType,
        instance: &CompiledInstance,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<()> {
        match ctx.begin_instance(&instance.id) {
            InstanceStart::Done => return Ok(()),
            InstanceStart::Cycle => {
                return Err(DoeError::FieldError {
                    field: instance.id.clone(),
                    context: ty.name.clone(),
                    message: "the instance depends on itself, directly or through other instances"
                        .to_owned(),
                });
            }
            InstanceStart::Compute => {}
        }
        let mark = cursor.mark();
        let start_bits = cursor.bit_position();
        let depth = ctx.trace().map(|trace| {
            let depth = trace.depth();
            trace.begin_instance(&instance.id, start_bits);
            depth
        });
        let computed = self.compute_instance(ty, instance, cursor, ctx);
        cursor.reset(mark);
        let (value, offset) = match computed {
            Ok(done) => done,
            Err(e) => {
                if let Some(depth) = depth
                    && let Some(trace) = ctx.trace()
                {
                    trace.abandon(depth);
                }
                // What the instance read belongs to no field.
                ctx.take_partial();
                ctx.abandon_instance(&instance.id);
                return Err(e);
            }
        };
        if let Some(depth) = depth
            && let Some(trace) = ctx.trace()
        {
            if value.is_absent() {
                trace.discard(depth);
            } else if offset.is_some() {
                // Already told where, by `compute_instance`.
                trace.finish_placed();
            } else {
                trace.finish_without_span();
            }
        }
        ctx.finish_instance(
            &instance.id,
            Value::Instance {
                value: Box::new(value),
                offset,
            },
        );
        Ok(())
    }

    /// The stream that `io:` of an instance names, positioned at its start.
    fn open_stream<'buf>(
        &self,
        io: &CompiledIo,
        id: &str,
        ty: &CompiledType,
        cursor: &mut Cursor<'buf>,
        ctx: &mut ParseContext,
    ) -> Result<Cursor<'buf>> {
        for name in &io.uses_instances {
            self.ensure_named_instance(ty, name, cursor, ctx)?;
        }
        ctx.set_io(cursor);
        let stream =
            expr::eval(&io.expr, &ctx.eval_ctx, "<io>").map_err(|e| annotate(&e, id, &ty.name))?;
        let range = match &stream {
            ExprValue::Struct(fields) => {
                let number = |name: &str| {
                    fields.iter().find(|(field, _)| field == name).and_then(
                        |(_, value)| match value {
                            ExprValue::Int(n) => usize::try_from(*n).ok(),
                            _ => None,
                        },
                    )
                };
                number("start").zip(number("size"))
            }
            _ => None,
        };
        let Some((start, size)) = range else {
            return Err(DoeError::FieldError {
                field: id.to_owned(),
                context: ty.name.clone(),
                message: format!("'io' is not a stream: {stream}"),
            });
        };
        cursor
            .stream(start, size)
            .map_err(|e| annotate(&e, id, &ty.name))
    }

    /// The value of an instance and, if it was read, where from.
    fn compute_instance(
        &self,
        ty: &CompiledType,
        instance: &CompiledInstance,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<(Value, Option<u64>)> {
        let id = &instance.id;
        match &instance.kind {
            InstanceKind::Read(field) => {
                // `io:` reads from another stream than the one of the type.
                let mut other;
                let cursor = match &instance.io {
                    Some(io) => {
                        other = self.open_stream(io, id, ty, cursor, ctx)?;
                        &mut other
                    }
                    None => cursor,
                };
                let start = cursor.absolute_pos();
                let start_bits = cursor.bit_position();
                let value = self.parse_field(field, ty, cursor, ctx)?;
                if value.is_absent() {
                    return Ok((value, None));
                }
                let end_bits = cursor.bit_position();
                if let Some(trace) = ctx.trace() {
                    // Unless `pos:` placed it, it was read from here.
                    trace.place_unless_placed(start_bits, end_bits);
                }
                // `pos:` moved the cursor and put it back; where it went is
                // the expression's value again.
                let at = match &field.pos {
                    Some(pos) => {
                        ctx.set_io(cursor);
                        let target = expr::eval_int(pos, &ctx.eval_ctx, "<pos>")
                            .map_err(|e| annotate(&e, id, &ty.name))?;
                        let base = cursor.absolute_pos().saturating_sub(cursor.pos());
                        usize::try_from(target).map_or(start, |t| base.saturating_add(t))
                    }
                    None => start,
                };
                Ok((value, Some(u64::try_from(at).unwrap_or(u64::MAX))))
            }
            InstanceKind::Value {
                expr: value_expr,
                if_expr,
                enum_ref,
                uses_instances,
                ..
            } => {
                for name in uses_instances {
                    self.ensure_named_instance(ty, name, cursor, ctx)?;
                }
                ctx.set_io(cursor);
                if let Some(cond) = if_expr
                    && !expr::eval_bool(cond, &ctx.eval_ctx, "<if>")
                        .map_err(|e| annotate(&e, id, &ty.name))?
                {
                    return Ok((Value::Absent, None));
                }
                let computed = expr::eval(value_expr, &ctx.eval_ctx, "<value>")
                    .map_err(|e| annotate(&e, id, &ty.name))?;
                let value = to_value(&computed).map_err(|message| DoeError::FieldError {
                    field: id.clone(),
                    context: ty.name.clone(),
                    message,
                })?;
                Ok((apply_enum(value, enum_ref.as_deref(), ty), None))
            }
        }
    }

    /// The values of the arguments a field passes to a type with parameters.
    fn eval_args(
        args: &[expr::Expr],
        type_name: &str,
        parent_ty: &CompiledType,
        cursor: &Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Vec<ExprValue>> {
        ctx.set_io(cursor);
        args.iter()
            .map(|arg| {
                expr::eval(arg, &ctx.eval_ctx, "<argument>")
                    .map_err(|e| annotate(&e, &format!("argument of {type_name}"), &parent_ty.name))
            })
            .collect()
    }

    fn read_sized_bytes(
        size: &SizeExpr,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        let n = eval_size(size, ctx, cursor)?;
        Self::read_blob(n, cursor)
    }

    fn read_bit_field(
        bit_size: u8,
        endian: EndianOverride,
        parent_ty: &CompiledType,
        cursor: &mut Cursor<'_>,
    ) -> Result<Value> {
        let order = match resolve_endian(endian, parent_ty.bit_endian) {
            Endian::Big => BitOrder::MostSignificantFirst,
            Endian::Little => BitOrder::LeastSignificantFirst,
        };
        let bits = cursor.read_bits(bit_size, order)?;
        Ok(if bit_size == 1 {
            Value::Bool(bits != 0)
        } else {
            Value::UInt(bits)
        })
    }

    /// Read a field of a user type, in a window if it has a size.
    fn read_type_ref(
        &self,
        type_name: &str,
        (size, args): (Option<&SizeExpr>, &[expr::Expr]),
        parent_ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        if ctx.depth() >= MAX_TYPE_DEPTH {
            return Err(DoeError::FieldError {
                field: type_name.to_owned(),
                context: parent_ty.name.clone(),
                message: format!("types are nested more than {MAX_TYPE_DEPTH} levels deep"),
            });
        }
        let sub_ty = self
            .registry
            .get(type_name)
            .ok_or_else(|| DoeError::UnknownType {
                type_name: type_name.to_owned(),
                context: parent_ty.name.clone(),
            })?;
        // The size and the arguments are expressions of the fields around
        // this one, so they are evaluated before the new scope exists.
        let window = match size {
            Some(size) => Some(eval_size(size, ctx, cursor)?),
            None => None,
        };
        let values = if args.is_empty() {
            Vec::new()
        } else {
            Self::eval_args(args, &sub_ty.name, parent_ty, cursor, ctx)?
        };
        match ctx.take_pending_parent() {
            Some(up) => ctx.push_scope_under(up),
            None => ctx.push_scope(),
        }
        ctx.bind_params(&sub_ty.params, values);
        let result = match window {
            None => self.parse_compiled_type(sub_ty, cursor, ctx),
            Some(n) => self.parse_in_window(sub_ty, n, cursor, ctx),
        };
        let _inner_fields = ctx.pop_scope();
        result
    }

    /// Read a field whose type is chosen by a value: the first case whose key
    /// equals the selector, else the default.
    #[allow(
        clippy::too_many_arguments,
        reason = "the parts of one switch plus the parse state; a struct would only rename them"
    )]
    fn read_switch(
        &self,
        on: &expr::Expr,
        size: Option<&SizeExpr>,
        cases: &[(expr::Expr, FieldKind)],
        default: Option<&FieldKind>,
        parent_ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        ctx.set_io(cursor);
        let selector = expr::eval(on, &ctx.eval_ctx, "<switch-on>")?;
        let mut chosen = default;
        for (key, kind) in cases {
            let key = expr::eval(key, &ctx.eval_ctx, "<case>")?;
            if expr::values_equal(&selector, &key, "<case>")? {
                chosen = Some(kind);
                break;
            }
        }
        let Some(size) = size else {
            // Without a size, a selector that matches nothing leaves the field out.
            return match chosen {
                Some(kind) => self.read_kind(kind, parent_ty, cursor, ctx),
                None => Ok(Value::Absent),
            };
        };
        let n = eval_size(size, ctx, cursor)?;
        let mut window = cursor.sub(cursor.pos(), n)?;
        let value = match chosen {
            Some(kind) => self.read_kind(kind, parent_ty, &mut window, ctx)?,
            None => Self::read_blob(n, &mut window)?,
        };
        cursor.skip(n)?;
        Ok(value)
    }

    /// Parse `ty` in a window of the next `n` bytes, then continue after all
    /// of them, however many the type used. The type cannot read past the
    /// window, and `pos:` inside it is relative to its start.
    fn parse_in_window(
        &self,
        ty: &CompiledType,
        n: usize,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        let mut window = cursor.sub(cursor.pos(), n)?;
        let value = self.parse_compiled_type(ty, &mut window, ctx)?;
        cursor.skip(n)?;
        Ok(value)
    }

    /// Read `n` bytes. A sequence longer than [`MAX_INLINE_BYTES`] is not read
    /// into memory: the result keeps where it is, its length and its first
    /// [`BLOB_PREVIEW_LEN`] bytes.
    fn read_blob(n: usize, cursor: &mut Cursor<'_>) -> Result<Value> {
        if n <= MAX_INLINE_BYTES {
            return Ok(Value::Bytes(cursor.read_bytes(n)?.into_owned()));
        }
        let offset = cursor.absolute_pos();
        let preview = cursor.peek_bytes(BLOB_PREVIEW_LEN)?.into_owned();
        cursor.skip(n)?;
        Ok(Value::Blob {
            offset: u64::try_from(offset).unwrap_or(u64::MAX),
            len: u64::try_from(n).unwrap_or(u64::MAX),
            preview,
        })
    }

    /// Read a value that ends at a terminator, or is cut from a window of
    /// `size` bytes (and always consumes all of it).
    fn read_cut(cut: &Cut, cursor: &mut Cursor<'_>, ctx: &mut ParseContext) -> Result<Vec<u8>> {
        let Some(size) = &cut.size else {
            let term = cut.terminator.unwrap_or(0x00);
            return Ok(cursor
                .read_terminated(term, cut.include, cut.consume, cut.eos_error)?
                .into_owned());
        };
        let n = eval_size(size, ctx, cursor)?;
        if n > MAX_TERMINATED_LEN {
            return Err(DoeError::Schema(format!(
                "value of {n} bytes is longer than the limit of {MAX_TERMINATED_LEN} bytes"
            )));
        }
        let mut bytes = cursor.read_bytes(n)?.into_owned();
        if let Some(pad) = cut.pad_right {
            while bytes.last() == Some(&pad) {
                bytes.pop();
            }
        }
        let at = cut
            .terminator
            .and_then(|term| bytes.iter().position(|&b| b == term));
        if let Some(at) = at {
            bytes.truncate(at + usize::from(cut.include));
        }
        Ok(bytes)
    }

    fn read_str(
        size: &StrSize,
        encoding: Encoding,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        let bytes: Vec<u8> = match size {
            StrSize::Terminator(term) => cursor
                .read_terminated(*term, false, true, true)?
                .into_owned(),
            StrSize::Cut(cut) => Self::read_cut(cut, cursor, ctx)?,
            StrSize::Fixed(sz) => {
                let n = eval_size(sz, ctx, cursor)?;
                if n > MAX_TERMINATED_LEN {
                    return Err(DoeError::Schema(format!(
                        "string of {n} bytes is longer than the limit of {MAX_TERMINATED_LEN} bytes"
                    )));
                }
                cursor.read_bytes(n)?.into_owned()
            }
        };
        let s = encoding::decode(&bytes, encoding)?;
        Ok(Value::Str(s))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// The value that stands for the instance number `id` that is computed later.
fn deferred_placeholder(id: u64) -> Value {
    Value::Struct {
        type_name: DEFERRED_TYPE.to_owned(),
        fields: vec![("id".to_owned(), Value::UInt(id))],
    }
}

/// The number of the instance that `value` stands for, if it stands for one.
fn placeholder_id(value: &Value) -> Option<u64> {
    match value {
        Value::Struct { type_name, fields } if type_name == DEFERRED_TYPE => match fields.first() {
            Some((_, Value::UInt(id))) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

/// Put the instances that were computed later in the places that stand for
/// them in `value`. One can stand in several places (a value that was copied
/// from a struct that had it), and can contain places that stand for others.
fn substitute_deferred(value: &mut Value, results: &HashMap<u64, Value>) {
    fill_deferred(value, results, 0);
}

fn fill_deferred(value: &mut Value, results: &HashMap<u64, Value>, depth: usize) {
    if let Some(id) = placeholder_id(value) {
        // Instances can contain each other, through the values that copy them.
        *value = match results.get(&id) {
            Some(found) if depth < MAX_TYPE_DEPTH => found.clone(),
            _ => Value::Absent,
        };
        if matches!(value, Value::Absent) {
            return;
        }
        return fill_deferred(value, results, depth + 1);
    }
    match value {
        Value::Struct { fields, .. } => {
            for (_, field) in fields.iter_mut() {
                fill_deferred(field, results, depth);
            }
        }
        Value::Array(items) => {
            for item in items {
                fill_deferred(item, results, depth);
            }
        }
        Value::Instance { value, .. } => fill_deferred(value, results, depth),
        _ => {}
    }
}

/// The name of the field that records the stream a value was parsed from.
pub const STREAM_FIELD: &str = "_io";

/// The stream `cursor` reads, as an expression value: where its window starts
/// in the whole input, and its size.
fn stream_expr(cursor: &Cursor<'_>) -> ExprValue {
    let number = |n: usize| ExprValue::Int(i128::try_from(n).unwrap_or(i128::MAX));
    ExprValue::Struct(vec![
        ("start".to_owned(), number(cursor.window_start())),
        ("size".to_owned(), number(cursor.len())),
    ])
}

/// The same as a parsed value.
fn stream_value(cursor: &Cursor<'_>) -> Value {
    let number = |n: usize| Value::UInt(u64::try_from(n).unwrap_or(u64::MAX));
    Value::Struct {
        type_name: STREAM_FIELD.to_owned(),
        fields: vec![
            ("start".to_owned(), number(cursor.window_start())),
            ("size".to_owned(), number(cursor.len())),
        ],
    }
}

fn resolve_endian(field_override: EndianOverride, type_default: Endian) -> Endian {
    match field_override {
        EndianOverride::Inherit => type_default,
        EndianOverride::Little => Endian::Little,
        EndianOverride::Big => Endian::Big,
    }
}

fn eval_size(size: &SizeExpr, ctx: &mut ParseContext, cursor: &Cursor<'_>) -> Result<usize> {
    match size {
        SizeExpr::Literal(n) => Ok(*n),
        SizeExpr::Eos => Ok(cursor.remaining()),
        SizeExpr::Dynamic(e) => {
            ctx.set_io(cursor);
            let v = expr::eval_int(e, &ctx.eval_ctx, "<size>")?;
            if v < 0 {
                return Err(DoeError::Schema(format!(
                    "size expression evaluated to negative value: {v}"
                )));
            }
            usize::try_from(v)
                .map_err(|_| DoeError::Schema(format!("size expression value too large: {v}")))
        }
    }
}

/// The number an enum is applied to. A single bit is a boolean unless it has an
/// enum, which takes it as the integer 0 or 1.
fn enum_discriminant(value: &Value) -> Option<i128> {
    match value {
        Value::Bool(bit) => Some(i128::from(*bit)),
        Value::UInt(n) => Some(i128::from(*n)),
        Value::SInt(n) => Some(i128::from(*n)),
        Value::Enum { value, .. } => Some(*value),
        Value::Instance { value, .. } => enum_discriminant(value),
        _ => None,
    }
}

/// The number an enum member is looked up by: a negative discriminant is its
/// two's complement as 64 bits.
fn enum_key(discriminant: i128) -> Option<u64> {
    u64::try_from(discriminant)
        .ok()
        .or_else(|| i64::try_from(discriminant).ok().map(i64::cast_unsigned))
}

/// Name an integer value after the member of `enum_name` it matches.
fn apply_enum(value: Value, enum_name: Option<&str>, ty: &CompiledType) -> Value {
    let (Some(enum_name), Some(discriminant)) = (enum_name, enum_discriminant(&value)) else {
        return value;
    };
    let name = ty
        .enums
        .get(enum_name)
        .and_then(|e| e.lookup(enum_key(discriminant)?))
        .map(str::to_owned);
    Value::Enum {
        value: discriminant,
        name,
    }
}

/// The parsed value an expression result stands for, for an instance.
fn to_value(computed: &ExprValue) -> std::result::Result<Value, String> {
    // An instance that is computed later stands for itself until it is.
    if let Some(id) = deferred_id(computed) {
        return Ok(deferred_placeholder(id));
    }
    Ok(match computed {
        ExprValue::Int(n) => {
            if let Ok(n) = u64::try_from(*n) {
                Value::UInt(n)
            } else if let Ok(n) = i64::try_from(*n) {
                Value::SInt(n)
            } else {
                return Err(format!("the value {n} does not fit in 64 bits"));
            }
        }
        ExprValue::Float(x) => Value::Float(*x),
        ExprValue::Bool(b) => Value::Bool(*b),
        ExprValue::Str(s) => Value::Str(s.clone()),
        ExprValue::Bytes(b) => Value::Bytes(b.clone()),
        ExprValue::Array(items) => Value::Array(
            items
                .iter()
                .map(to_value)
                .collect::<std::result::Result<_, _>>()?,
        ),
        ExprValue::Struct(fields) => Value::Struct {
            type_name: "<value>".to_owned(),
            fields: fields
                .iter()
                .map(|(name, v)| to_value(v).map(|v| (name.clone(), v)))
                .collect::<std::result::Result<_, _>>()?,
        },
        ExprValue::Omitted { .. } => {
            return Err("the value is too large to use in an expression".to_owned());
        }
    })
}

fn annotate(e: &DoeError, field: &str, type_name: &str) -> DoeError {
    DoeError::FieldError {
        field: field.to_owned(),
        context: type_name.to_owned(),
        message: e.to_string(),
    }
}
