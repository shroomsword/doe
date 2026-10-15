//! Binary parse engine.
//!
//! `Engine::parse_type` is the top-level entry point.  It walks a
//! `CompiledType`'s field list, dispatching each field to the appropriate
//! read routine, evaluating size/repeat/if expressions against the
//! accumulated parse context, and building a `Value::Struct`.

use std::cmp::Ordering;

use crate::error::{DoeError, Result};
use crate::parser::context::{InstanceStart, ParseContext};
use crate::parser::cursor::{BitOrder, Cursor, MAX_TERMINATED_LEN};
use crate::schema::expr;
use crate::schema::expr::ExprValue;
use crate::schema::ir::{
    CompiledField, CompiledInstance, CompiledType, CompiledValid, Encoding, Endian, EndianOverride,
    FieldKind, FloatWidth, InstanceKind, IntWidth, RepeatMode, SizeExpr, StrSize, TypeRegistry,
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

/// Walks a compiled [`TypeRegistry`] over a byte buffer to produce a [`Value`] tree.
#[derive(Debug)]
pub struct Engine<'reg> {
    registry: &'reg TypeRegistry,
    /// Whether the instances of a type are part of the result.
    instances: bool,
}

impl<'reg> Engine<'reg> {
    /// Create an engine that resolves type names against `registry`.
    pub fn new(registry: &'reg TypeRegistry) -> Self {
        Engine {
            registry,
            instances: true,
        }
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
        let ty = self
            .registry
            .get(type_name)
            .ok_or_else(|| DoeError::UnknownType {
                type_name: type_name.to_owned(),
                context: "<engine>".to_owned(),
            })?;
        let mut ctx = ParseContext::new();
        ctx.set_input_len(cursor.len());
        self.parse_compiled_type(ty, cursor, &mut ctx)
    }

    // ── Internal recursion ────────────────────────────────────────────────────

    fn parse_compiled_type(
        &self,
        ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        let mut fields: Vec<(String, Value)> = Vec::new();

        for field in &ty.fields {
            let value = self.parse_field(field, ty, cursor, ctx)?;

            // Bind into the context so subsequent fields can reference this one
            ctx.bind(&field.id, value.clone());
            fields.push((field.id.clone(), value));
        }

        // Instances come after the sequence, in schema order. Some may have
        // been computed already, because a field used them.
        for instance in &ty.instances {
            self.ensure_instance(ty, instance, cursor, ctx)?;
        }
        for instance in &ty.instances {
            if let Some(value) = ctx.take_instance(&instance.id)
                && self.instances
            {
                fields.push((instance.id.clone(), value));
            }
        }

        Ok(Value::Struct {
            type_name: ty.name.clone(),
            fields,
        })
    }

    fn parse_field(
        &self,
        field: &CompiledField,
        parent_ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        // Compute the instances the field's expressions use.
        for name in &field.uses_instances {
            self.ensure_named_instance(parent_ty, name, cursor, ctx)?;
        }

        // Evaluate `if:` guard
        if let Some(if_expr) = &field.if_expr {
            ctx.set_io(cursor);
            let cond = expr::eval_bool(if_expr, &ctx.eval_ctx, "<if>")
                .map_err(|e| annotate(&e, &field.id, &parent_ty.name))?;
            if !cond {
                return Ok(Value::Absent);
            }
        }

        let Some(pos_expr) = &field.pos else {
            return self.parse_repeats(field, parent_ty, cursor, ctx);
        };

        // `pos:` reads the whole field (every repeat) somewhere else, then
        // puts the cursor back where the sequence left off.
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
        let value = self.parse_repeats(field, parent_ty, cursor, ctx)?;
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
            RepeatMode::Eos => {
                let mut items = Vec::new();
                while !cursor.is_eof() {
                    let v = self.parse_repeated_element(field, parent_ty, cursor, ctx)?;
                    items.push(v);
                }
                Ok(Value::Array(items))
            }
            RepeatMode::Count(count_expr) => {
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
                for _ in 0..count {
                    let v = self.parse_repeated_element(field, parent_ty, cursor, ctx)?;
                    items.push(v);
                }
                Ok(Value::Array(items))
            }
            RepeatMode::Until(until_expr) => {
                let mut items = Vec::new();
                loop {
                    let v = self.parse_repeated_element(field, parent_ty, cursor, ctx)?;
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
                Ok(Value::Array(items))
            }
        }
    }

    /// Parse one element of a repeated field, requiring it to consume input.
    ///
    /// An element that consumes nothing (an empty type, a zero-size `bytes`)
    /// would repeat forever under `eos` or `until` and, with a large count,
    /// effectively forever under `repeat-expr`, so it is an error.
    fn parse_repeated_element(
        &self,
        field: &CompiledField,
        parent_ty: &CompiledType,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        let before = cursor.bit_position();
        let value = self.parse_field_once(field, parent_ty, cursor, ctx)?;
        if cursor.bit_position() == before {
            return Err(DoeError::FieldError {
                field: field.id.clone(),
                context: parent_ty.name.clone(),
                message: "repeated element consumed no bytes".to_owned(),
            });
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
        let raw = self.read_kind(&field.kind, parent_ty, cursor, ctx)?;
        if let Some(valid) = &field.valid {
            Self::check_valid(valid, field, parent_ty, &raw, cursor, ctx)?;
        }
        Ok(apply_enum(raw, field.enum_ref.as_deref(), parent_ty))
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
            let member = value
                .as_int()
                .map(i64::cast_unsigned)
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
            FieldKind::UInt { width, endian } => {
                let e = resolve_endian(*endian, parent_ty.endian);
                Self::read_uint(*width, e, cursor)
            }
            FieldKind::SInt { width, endian } => {
                let e = resolve_endian(*endian, parent_ty.endian);
                Self::read_sint(*width, e, cursor)
            }
            FieldKind::Float { width, endian } => {
                let e = resolve_endian(*endian, parent_ty.endian);
                Self::read_float(*width, e, cursor)
            }
            FieldKind::Bytes { size } => {
                let n = eval_size(size, ctx, cursor)?;
                Self::read_blob(n, cursor)
            }
            FieldKind::Str { size, encoding } => Self::read_str(size, *encoding, cursor, ctx),
            FieldKind::Bits { bit_size, endian } => {
                let order = match resolve_endian(*endian, parent_ty.bit_endian) {
                    Endian::Big => BitOrder::MostSignificantFirst,
                    Endian::Little => BitOrder::LeastSignificantFirst,
                };
                let bits = cursor.read_bits(*bit_size, order)?;
                Ok(if *bit_size == 1 {
                    Value::Bool(bits != 0)
                } else {
                    Value::UInt(bits)
                })
            }
            FieldKind::TypeRef { type_name, size } => {
                if ctx.depth() >= MAX_TYPE_DEPTH {
                    return Err(DoeError::FieldError {
                        field: type_name.clone(),
                        context: parent_ty.name.clone(),
                        message: format!("types are nested more than {MAX_TYPE_DEPTH} levels deep"),
                    });
                }
                let sub_ty = self
                    .registry
                    .get(type_name)
                    .ok_or_else(|| DoeError::UnknownType {
                        type_name: type_name.clone(),
                        context: parent_ty.name.clone(),
                    })?;
                // The size is an expression of the fields around this one, so
                // it is evaluated before the new scope exists.
                let window = match size {
                    Some(size) => Some(eval_size(size, ctx, cursor)?),
                    None => None,
                };
                ctx.push_scope();
                let result = match window {
                    None => self.parse_compiled_type(sub_ty, cursor, ctx),
                    Some(n) => self.parse_in_window(sub_ty, n, cursor, ctx),
                };
                let _inner_fields = ctx.pop_scope();
                result
            }
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
        let computed = self.compute_instance(ty, instance, cursor, ctx);
        cursor.reset(mark);
        let (value, offset) = computed?;
        ctx.finish_instance(
            &instance.id,
            Value::Instance {
                value: Box::new(value),
                offset,
            },
        );
        Ok(())
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
                let start = cursor.absolute_pos();
                let value = self.parse_field(field, ty, cursor, ctx)?;
                if value.is_absent() {
                    return Ok((value, None));
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

    fn read_str(
        size: &StrSize,
        encoding: Encoding,
        cursor: &mut Cursor<'_>,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        let bytes: Vec<u8> = match size {
            StrSize::Terminator(term) => cursor.read_until(*term)?.into_owned(),
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
        let s = decode_string(&bytes, encoding)?;
        Ok(Value::Str(s))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

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

fn decode_string(bytes: &[u8], encoding: Encoding) -> Result<String> {
    match encoding {
        Encoding::Utf8 => std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|e| DoeError::Schema(format!("UTF-8 decode error: {e}"))),
        Encoding::Ascii => {
            if bytes.iter().any(|&b| b > 0x7f) {
                return Err(DoeError::Schema("non-ASCII byte in ASCII string".into()));
            }
            Ok(String::from_utf8_lossy(bytes).into_owned())
        }
        Encoding::Latin1 => {
            // Latin-1 maps directly to the first 256 Unicode code points
            Ok(bytes.iter().map(|&b| char::from(b)).collect())
        }
    }
}

/// Name an integer value after the member of `enum_name` it matches.
fn apply_enum(value: Value, enum_name: Option<&str>, ty: &CompiledType) -> Value {
    let (Some(enum_name), Some(n)) = (enum_name, value.as_int()) else {
        return value;
    };
    let discriminant = n.cast_unsigned();
    let name = ty
        .enums
        .get(enum_name)
        .and_then(|e| e.lookup(discriminant))
        .map(str::to_owned);
    Value::Enum {
        value: discriminant,
        name,
    }
}

/// The parsed value an expression result stands for, for an instance.
fn to_value(computed: &ExprValue) -> std::result::Result<Value, String> {
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
