//! Binary parse engine.
//!
//! `Engine::parse_type` is the top-level entry point.  It walks a
//! `CompiledType`'s field list, dispatching each field to the appropriate
//! read routine, evaluating size/repeat/if expressions against the
//! accumulated parse context, and building a `Value::Struct`.

use crate::error::{DoeError, Result};
use crate::parser::context::ParseContext;
use crate::parser::cursor::{BitOrder, Cursor, MAX_TERMINATED_LEN};
use crate::schema::expr;
use crate::schema::ir::{
    CompiledField, CompiledType, Encoding, Endian, EndianOverride, FieldKind, FloatWidth, IntWidth,
    RepeatMode, SizeExpr, StrSize, TypeRegistry,
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
}

impl<'reg> Engine<'reg> {
    /// Create an engine that resolves type names against `registry`.
    pub fn new(registry: &'reg TypeRegistry) -> Self {
        Engine { registry }
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
        // Evaluate `if:` guard
        if let Some(if_expr) = &field.if_expr {
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

        // Apply enum mapping if present
        if let Some(enum_name) = &field.enum_ref
            && let Some(n) = raw.as_int()
        {
            let discriminant = n.cast_unsigned();
            let variant = parent_ty
                .enums
                .get(enum_name.as_str())
                .and_then(|e| e.lookup(discriminant))
                .map(str::to_owned);
            return Ok(Value::Enum {
                value: discriminant,
                name: variant,
            });
        }

        Ok(raw)
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
                let n = eval_size(size, &ctx.eval_ctx, cursor)?;
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
                let window = size
                    .as_ref()
                    .map(|size| eval_size(size, &ctx.eval_ctx, cursor))
                    .transpose()?;
                ctx.push_scope();
                let result = match window {
                    None => self.parse_compiled_type(sub_ty, cursor, ctx),
                    Some(n) => self.parse_in_window(sub_ty, n, cursor, ctx),
                };
                let _inner_fields = ctx.pop_scope();
                result
            }
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
        ctx: &ParseContext,
    ) -> Result<Value> {
        let bytes: Vec<u8> = match size {
            StrSize::Terminator(term) => cursor.read_until(*term)?.into_owned(),
            StrSize::Fixed(sz) => {
                let n = eval_size(sz, &ctx.eval_ctx, cursor)?;
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

fn eval_size(size: &SizeExpr, eval_ctx: &expr::EvalContext, cursor: &Cursor<'_>) -> Result<usize> {
    match size {
        SizeExpr::Literal(n) => Ok(*n),
        SizeExpr::Eos => Ok(cursor.remaining()),
        SizeExpr::Dynamic(e) => {
            let v = expr::eval_int(e, eval_ctx, "<size>")?;
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

fn annotate(e: &DoeError, field: &str, type_name: &str) -> DoeError {
    DoeError::FieldError {
        field: field.to_owned(),
        context: type_name.to_owned(),
        message: e.to_string(),
    }
}
