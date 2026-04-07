//! Binary parse engine.
//!
//! `Engine::parse_type` is the top-level entry point.  It walks a
//! `CompiledType`'s field list, dispatching each field to the appropriate
//! read routine, evaluating size/repeat/if expressions against the
//! accumulated parse context, and building a `Value::Struct`.

use crate::error::{DoeError, Result};
use crate::parser::context::ParseContext;
use crate::parser::cursor::Cursor;
use crate::schema::expr;
use crate::schema::ir::{
    CompiledField, CompiledType, Encoding, Endian, EndianOverride, FieldKind, FloatWidth, IntWidth,
    RepeatMode, SizeExpr, StrSize, TypeRegistry,
};
use crate::value::Value;

// ─────────────────────────────────────────────────────────────────────────────
// Engine
// ─────────────────────────────────────────────────────────────────────────────

pub struct Engine<'reg> {
    registry: &'reg TypeRegistry,
}

impl<'reg> Engine<'reg> {
    pub fn new(registry: &'reg TypeRegistry) -> Self {
        Engine { registry }
    }

    /// Parse `type_name` from the beginning of `cursor`, returning a
    /// `Value::Struct` with all fields populated.
    pub fn parse_type(&self, type_name: &str, cursor: &mut Cursor) -> Result<Value> {
        let ty = self.registry.get(type_name).ok_or_else(|| DoeError::UnknownType {
            type_name: type_name.to_owned(),
            context: "<engine>".to_owned(),
        })?;
        let mut ctx = ParseContext::new();
        self.parse_compiled_type(ty, cursor, &mut ctx)
    }

    // ── Internal recursion ────────────────────────────────────────────────────

    fn parse_compiled_type(
        &self,
        ty: &CompiledType,
        cursor: &mut Cursor,
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
        cursor: &mut Cursor,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        // Evaluate `if:` guard
        if let Some(if_expr) = &field.if_expr {
            let cond = expr::eval(if_expr, &ctx.eval_ctx, "<if>")
                .map_err(|e| annotate(e, &field.id, &parent_ty.name))?;
            if cond == 0 {
                return Ok(Value::Absent);
            }
        }

        // Dispatch on repeat mode
        match &field.repeat {
            RepeatMode::Once => {
                self.parse_field_once(field, parent_ty, cursor, ctx)
            }
            RepeatMode::Eos => {
                let mut items = Vec::new();
                while !cursor.is_eof() {
                    let v = self.parse_field_once(field, parent_ty, cursor, ctx)?;
                    items.push(v);
                }
                Ok(Value::Array(items))
            }
            RepeatMode::Count(count_expr) => {
                let n = expr::eval(count_expr, &ctx.eval_ctx, "<repeat-expr>")
                    .map_err(|e| annotate(e, &field.id, &parent_ty.name))?;
                if n < 0 {
                    return Err(DoeError::FieldError {
                        field: field.id.clone(),
                        context: parent_ty.name.clone(),
                        message: format!("repeat count evaluated to negative value: {}", n),
                    });
                }
                let mut items = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    let v = self.parse_field_once(field, parent_ty, cursor, ctx)?;
                    items.push(v);
                }
                Ok(Value::Array(items))
            }
            RepeatMode::Until(until_expr) => {
                let mut items = Vec::new();
                loop {
                    let v = self.parse_field_once(field, parent_ty, cursor, ctx)?;
                    // Bind the most recently parsed value to `_` so the
                    // until-expression can reference it, matching the convention
                    // used by KaitaiStruct.
                    let done = if let Some(n) = v.as_int() {
                        ctx.eval_ctx.bind("_", n);
                        expr::eval(until_expr, &ctx.eval_ctx, "<repeat-until>")
                            .map_err(|e| annotate(e, &field.id, &parent_ty.name))?
                            != 0
                    } else {
                        false
                    };
                    items.push(v);
                    if done || cursor.is_eof() { break; }
                }
                Ok(Value::Array(items))
            }
        }
    }

    /// Parse a single (non-repeated) instance of a field.
    fn parse_field_once(
        &self,
        field: &CompiledField,
        parent_ty: &CompiledType,
        cursor: &mut Cursor,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        let raw = self.read_kind(&field.kind, parent_ty, cursor, ctx)?;

        // Apply enum mapping if present
        if let Some(enum_name) = &field.enum_ref {
            if let Some(n) = raw.as_int() {
                let discriminant = n as u64;
                let variant = parent_ty.enums
                    .get(enum_name.as_str())
                    .and_then(|e| e.lookup(discriminant))
                    .map(str::to_owned);
                return Ok(Value::Enum { value: discriminant, name: variant });
            }
        }

        Ok(raw)
    }

    // ── FieldKind dispatch ────────────────────────────────────────────────────

    fn read_kind(
        &self,
        kind: &FieldKind,
        parent_ty: &CompiledType,
        cursor: &mut Cursor,
        ctx: &mut ParseContext,
    ) -> Result<Value> {
        match kind {
            FieldKind::UInt { width, endian } => {
                let e = resolve_endian(*endian, parent_ty.endian);
                self.read_uint(*width, e, cursor)
            }
            FieldKind::SInt { width, endian } => {
                let e = resolve_endian(*endian, parent_ty.endian);
                self.read_sint(*width, e, cursor)
            }
            FieldKind::Float { width } => {
                self.read_float(*width, parent_ty.endian, cursor)
            }
            FieldKind::Bytes { size } => {
                let n = eval_size(size, &ctx.eval_ctx)? as usize;
                let bytes = cursor.read_bytes(n)?.to_vec();
                Ok(Value::Bytes(bytes))
            }
            FieldKind::Str { size, encoding } => {
                self.read_str(size, *encoding, cursor, ctx)
            }
            FieldKind::Bits { bit_size } => {
                self.read_bits(*bit_size, cursor)
            }
            FieldKind::TypeRef { type_name } => {
                let sub_ty = self.registry.get(type_name).ok_or_else(|| DoeError::UnknownType {
                    type_name: type_name.clone(),
                    context: parent_ty.name.clone(),
                })?;
                ctx.push_scope();
                let result = self.parse_compiled_type(sub_ty, cursor, ctx);
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

    fn read_uint(&self, width: IntWidth, endian: Endian, cursor: &mut Cursor) -> Result<Value> {
        let v = match (width, endian) {
            (IntWidth::W8,  _)           => cursor.read_u8()?  as u64,
            (IntWidth::W16, Endian::Little) => cursor.read_u16_le()? as u64,
            (IntWidth::W16, Endian::Big)    => cursor.read_u16_be()? as u64,
            (IntWidth::W32, Endian::Little) => cursor.read_u32_le()? as u64,
            (IntWidth::W32, Endian::Big)    => cursor.read_u32_be()? as u64,
            (IntWidth::W64, Endian::Little) => cursor.read_u64_le()?,
            (IntWidth::W64, Endian::Big)    => cursor.read_u64_be()?,
        };
        Ok(Value::UInt(v))
    }

    fn read_sint(&self, width: IntWidth, endian: Endian, cursor: &mut Cursor) -> Result<Value> {
        let v = match (width, endian) {
            (IntWidth::W8,  _)              => cursor.read_i8()?  as i64,
            (IntWidth::W16, Endian::Little) => cursor.read_i16_le()? as i64,
            (IntWidth::W16, Endian::Big)    => cursor.read_i16_be()? as i64,
            (IntWidth::W32, Endian::Little) => cursor.read_i32_le()? as i64,
            (IntWidth::W32, Endian::Big)    => cursor.read_i32_be()? as i64,
            (IntWidth::W64, Endian::Little) => cursor.read_i64_le()?,
            (IntWidth::W64, Endian::Big)    => cursor.read_i64_be()?,
        };
        Ok(Value::SInt(v))
    }

    fn read_float(&self, width: FloatWidth, endian: Endian, cursor: &mut Cursor) -> Result<Value> {
        let v = match (width, endian) {
            (FloatWidth::F32, Endian::Little) => cursor.read_f32_le()? as f64,
            (FloatWidth::F32, Endian::Big)    => cursor.read_f32_be()? as f64,
            (FloatWidth::F64, Endian::Little) => cursor.read_f64_le()?,
            (FloatWidth::F64, Endian::Big)    => cursor.read_f64_be()?,
        };
        Ok(Value::Float(v))
    }

    fn read_str(
        &self,
        size: &StrSize,
        encoding: Encoding,
        cursor: &mut Cursor,
        ctx: &ParseContext,
    ) -> Result<Value> {
        let bytes: Vec<u8> = match size {
            StrSize::Terminator(term) => cursor.read_until(*term)?.to_vec(),
            StrSize::Fixed(sz) => {
                let n = eval_size(sz, &ctx.eval_ctx)? as usize;
                cursor.read_bytes(n)?.to_vec()
            }
        };
        let s = decode_string(&bytes, encoding)?;
        Ok(Value::Str(s))
    }

    fn read_bits(&self, bit_size: u8, cursor: &mut Cursor) -> Result<Value> {
        // Read the minimum number of bytes needed, then extract the bits.
        // For the initial implementation we read whole bytes and mask.
        // Sub-byte bit fields across byte boundaries are deferred.
        let byte_count = ((bit_size as usize) + 7) / 8;
        let bytes = cursor.read_bytes(byte_count)?;
        let mut v: u64 = 0;
        for &b in bytes {
            v = (v << 8) | b as u64;
        }
        // If not a multiple of 8, shift down to drop the padding bits
        let extra_bits = (byte_count * 8) as u8 - bit_size;
        v >>= extra_bits;
        // Mask to bit_size bits
        let mask = if bit_size >= 64 { u64::MAX } else { (1u64 << bit_size) - 1 };
        Ok(Value::UInt(v & mask))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn resolve_endian(field_override: EndianOverride, type_default: Endian) -> Endian {
    match field_override {
        EndianOverride::Inherit => type_default,
        EndianOverride::Little  => Endian::Little,
        EndianOverride::Big     => Endian::Big,
    }
}

fn eval_size(size: &SizeExpr, eval_ctx: &expr::EvalContext) -> Result<i64> {
    match size {
        SizeExpr::Literal(n) => Ok(*n as i64),
        SizeExpr::Dynamic(e) => {
            let v = expr::eval(e, eval_ctx, "<size>")?;
            if v < 0 {
                return Err(DoeError::Schema(format!(
                    "size expression evaluated to negative value: {}", v
                )));
            }
            Ok(v)
        }
    }
}

fn decode_string(bytes: &[u8], encoding: Encoding) -> Result<String> {
    match encoding {
        Encoding::Utf8 => {
            std::str::from_utf8(bytes)
                .map(str::to_owned)
                .map_err(|e| DoeError::Schema(format!("UTF-8 decode error: {}", e)))
        }
        Encoding::Ascii => {
            if bytes.iter().any(|&b| b > 0x7f) {
                return Err(DoeError::Schema("non-ASCII byte in ASCII string".into()));
            }
            Ok(String::from_utf8_lossy(bytes).into_owned())
        }
        Encoding::Latin1 => {
            // Latin-1 maps directly to the first 256 Unicode code points
            Ok(bytes.iter().map(|&b| b as char).collect())
        }
    }
}

fn annotate(e: DoeError, field: &str, type_name: &str) -> DoeError {
    DoeError::FieldError {
        field: field.to_owned(),
        context: type_name.to_owned(),
        message: e.to_string(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::compiler::compile_schema;
    use indoc::indoc;

    // ── Test helpers ─────────────────────────────────────────────────────────

    fn engine_from_yaml(yaml: &str) -> (Engine<'static>, &'static TypeRegistry) {
        // Leak the registry so the engine can borrow it with 'static lifetime
        // in tests.  This is only acceptable in test code.
        let schema: crate::schema::raw::RawSchema = serde_yaml::from_str(yaml).unwrap();
        let reg = Box::leak(Box::new(compile_schema(&schema).unwrap()));
        let engine = Engine::new(reg);
        (engine, reg)
    }

    fn parse(yaml: &str, type_name: &str, data: &[u8]) -> Value {
        let (engine, _) = engine_from_yaml(yaml);
        let mut cursor = Cursor::new(data);
        engine.parse_type(type_name, &mut cursor).unwrap()
    }

    fn parse_err(yaml: &str, type_name: &str, data: &[u8]) -> DoeError {
        let (engine, _) = engine_from_yaml(yaml);
        let mut cursor = Cursor::new(data);
        engine.parse_type(type_name, &mut cursor).unwrap_err()
    }

    fn get_field<'a>(value: &'a Value, name: &str) -> &'a Value {
        if let Value::Struct { fields, .. } = value {
            fields.iter().find(|(k, _)| k == name).map(|(_, v)| v)
                .unwrap_or_else(|| panic!("field '{}' not found", name))
        } else {
            panic!("expected Struct, got {:?}", value)
        }
    }

    // ── Unsigned integers ─────────────────────────────────────────────────────

    #[test]
    fn parse_u8() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: x
                type: u8
        "#}, "t", &[0x42]);
        assert_eq!(get_field(&v, "x"), &Value::UInt(0x42));
    }

    #[test]
    fn parse_u16_le() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: x
                type: u16le
        "#}, "t", &[0x34, 0x12]);
        assert_eq!(get_field(&v, "x"), &Value::UInt(0x1234));
    }

    #[test]
    fn parse_u16_be() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: x
                type: u16be
        "#}, "t", &[0x12, 0x34]);
        assert_eq!(get_field(&v, "x"), &Value::UInt(0x1234));
    }

    #[test]
    fn parse_u32_le() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: x
                type: u32le
        "#}, "t", &[0x78, 0x56, 0x34, 0x12]);
        assert_eq!(get_field(&v, "x"), &Value::UInt(0x12345678));
    }

    #[test]
    fn parse_u32_be() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: x
                type: u32be
        "#}, "t", &[0x12, 0x34, 0x56, 0x78]);
        assert_eq!(get_field(&v, "x"), &Value::UInt(0x12345678));
    }

    #[test]
    fn parse_u64_le() {
        let n: u64 = 0xdeadbeefcafe0001;
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: x
                type: u64le
        "#}, "t", &n.to_le_bytes());
        assert_eq!(get_field(&v, "x"), &Value::UInt(n));
    }

    #[test]
    fn parse_u8_inherits_meta_endian() {
        // u8 endian doesn't matter, but verify meta: be doesn't break anything
        let v = parse(indoc! {r#"
            id: t
            meta:
              endian: be
            seq:
              - id: x
                type: u8
        "#}, "t", &[0xff]);
        assert_eq!(get_field(&v, "x"), &Value::UInt(255));
    }

    #[test]
    fn parse_u16_inherits_be_meta() {
        let v = parse(indoc! {r#"
            id: t
            meta:
              endian: be
            seq:
              - id: x
                type: u16
        "#}, "t", &[0x01, 0x00]);
        assert_eq!(get_field(&v, "x"), &Value::UInt(0x0100));
    }

    // ── Signed integers ───────────────────────────────────────────────────────

    #[test]
    fn parse_i8_positive() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: x
                type: i8
        "#}, "t", &[0x7f]);
        assert_eq!(get_field(&v, "x"), &Value::SInt(127));
    }

    #[test]
    fn parse_i8_negative() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: x
                type: i8
        "#}, "t", &[0xff]);
        assert_eq!(get_field(&v, "x"), &Value::SInt(-1));
    }

    #[test]
    fn parse_i32_le_negative() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: x
                type: i32le
        "#}, "t", &(-100i32).to_le_bytes());
        assert_eq!(get_field(&v, "x"), &Value::SInt(-100));
    }

    #[test]
    fn parse_i64_be_min() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: x
                type: i64be
        "#}, "t", &i64::MIN.to_be_bytes());
        assert_eq!(get_field(&v, "x"), &Value::SInt(i64::MIN));
    }

    // ── Floats ────────────────────────────────────────────────────────────────

    #[test]
    fn parse_f32_le() {
        let f: f32 = 1.5;
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: x
                type: f32
        "#}, "t", &f.to_le_bytes());
        if let Value::Float(got) = get_field(&v, "x") {
            assert!((got - 1.5).abs() < 1e-6);
        } else { panic!("expected Float"); }
    }

    #[test]
    fn parse_f64_be() {
        let f: f64 = std::f64::consts::E;
        let v = parse(indoc! {r#"
            id: t
            meta:
              endian: be
            seq:
              - id: x
                type: f64
        "#}, "t", &f.to_be_bytes());
        if let Value::Float(got) = get_field(&v, "x") {
            assert!((got - std::f64::consts::E).abs() < 1e-15);
        } else { panic!("expected Float"); }
    }

    // ── Bytes ─────────────────────────────────────────────────────────────────

    #[test]
    fn parse_bytes_literal_size() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: buf
                type: bytes
                size: 4
        "#}, "t", &[0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(get_field(&v, "buf"), &Value::Bytes(vec![0xde, 0xad, 0xbe, 0xef]));
    }

    #[test]
    fn parse_bytes_dynamic_size() {
        let mut data = vec![0x03u8];     // length = 3
        data.extend_from_slice(&[0xaa, 0xbb, 0xcc]); // 3 bytes of data
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: length
                type: u8
              - id: buf
                type: bytes
                size: length
        "#}, "t", &data);
        assert_eq!(get_field(&v, "buf"), &Value::Bytes(vec![0xaa, 0xbb, 0xcc]));
    }

    #[test]
    fn parse_bytes_zero_length() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: len
                type: u8
              - id: buf
                type: bytes
                size: len
        "#}, "t", &[0x00]);
        assert_eq!(get_field(&v, "buf"), &Value::Bytes(vec![]));
    }

    // ── Strings ───────────────────────────────────────────────────────────────

    #[test]
    fn parse_str_fixed_size() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: name
                type: str
                size: 5
                encoding: ascii
        "#}, "t", b"hello");
        assert_eq!(get_field(&v, "name"), &Value::Str("hello".into()));
    }

    #[test]
    fn parse_strz_null_terminated() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: name
                type: strz
        "#}, "t", b"hello\x00world");
        assert_eq!(get_field(&v, "name"), &Value::Str("hello".into()));
    }

    #[test]
    fn parse_str_with_terminator() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: line
                type: str
                terminator: 10
        "#}, "t", b"line1\nrest");
        assert_eq!(get_field(&v, "line"), &Value::Str("line1".into()));
    }

    #[test]
    fn parse_str_latin1() {
        // byte 0xe9 is 'é' in latin-1
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: s
                type: str
                size: 3
                encoding: latin1
        "#}, "t", &[0x63, 0x61, 0xe9]); // "café" without the f
        assert_eq!(get_field(&v, "s"), &Value::Str("caé".into()));
    }

    #[test]
    fn parse_str_utf8() {
        let s = "héllo";
        let bytes = s.as_bytes();
        let yaml = format!(indoc! {r#"
            id: t
            seq:
              - id: s
                type: str
                size: {}
        "#}, bytes.len());
        let v = parse(&yaml, "t", bytes);
        assert_eq!(get_field(&v, "s"), &Value::Str("héllo".into()));
    }

    // ── Bits ─────────────────────────────────────────────────────────────────

    #[test]
    fn parse_bits_4() {
        // 0xab = 0b10101011; top 4 bits = 0b1010 = 10
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: nibble
                type: bits
                bit_size: 4
        "#}, "t", &[0xab]);
        assert_eq!(get_field(&v, "nibble"), &Value::UInt(10));
    }

    #[test]
    fn parse_bits_8() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: byte_val
                type: bits
                bit_size: 8
        "#}, "t", &[0xff]);
        assert_eq!(get_field(&v, "byte_val"), &Value::UInt(255));
    }

    #[test]
    fn parse_bits_1() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: flag
                type: bits
                bit_size: 1
        "#}, "t", &[0x80]); // top bit set
        assert_eq!(get_field(&v, "flag"), &Value::UInt(1));
    }

    // ── Contents ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_contents_match() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: magic
                contents: [0x89, 0x50, 0x4e, 0x47]
        "#}, "t", &[0x89, 0x50, 0x4e, 0x47]);
        assert_eq!(get_field(&v, "magic"), &Value::Bytes(vec![0x89, 0x50, 0x4e, 0x47]));
    }

    #[test]
    fn parse_contents_mismatch_is_error() {
        let err = parse_err(indoc! {r#"
            id: t
            seq:
              - id: magic
                contents: [0x89, 0x50, 0x4e, 0x47]
        "#}, "t", &[0x00, 0x00, 0x00, 0x00]);
        assert!(matches!(err, DoeError::Schema(_)));
    }

    // ── Enums ─────────────────────────────────────────────────────────────────

    #[test]
    fn parse_enum_known_variant() {
        let v = parse(indoc! {r#"
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
        "#}, "t", &[0x01]);
        assert_eq!(get_field(&v, "kind"), &Value::Enum { value: 1, name: Some("data".into()) });
    }

    #[test]
    fn parse_enum_unknown_variant() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: kind
                type: u8
                enum: kind_t
            enums:
              kind_t:
                "0": eof
        "#}, "t", &[0x99]);
        assert_eq!(get_field(&v, "kind"), &Value::Enum { value: 0x99, name: None });
    }

    // ── Conditional fields ────────────────────────────────────────────────────

    #[test]
    fn conditional_field_present() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: flags
                type: u8
              - id: extra
                type: u32le
                if: flags > 0
        "#}, "t", &[0x01, 0x78, 0x56, 0x34, 0x12]);
        assert_eq!(get_field(&v, "extra"), &Value::UInt(0x12345678));
    }

    #[test]
    fn conditional_field_absent() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: flags
                type: u8
              - id: extra
                type: u32le
                if: flags > 0
        "#}, "t", &[0x00]); // flags = 0 → extra is absent
        assert_eq!(get_field(&v, "extra"), &Value::Absent);
    }

    // ── Repeat modes ─────────────────────────────────────────────────────────

    #[test]
    fn repeat_eos() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: items
                type: u8
                repeat: eos
        "#}, "t", &[0x01, 0x02, 0x03]);
        if let Value::Array(items) = get_field(&v, "items") {
            assert_eq!(items.len(), 3);
            assert_eq!(items[0], Value::UInt(1));
            assert_eq!(items[2], Value::UInt(3));
        } else { panic!("expected Array"); }
    }

    #[test]
    fn repeat_eos_empty_input() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: items
                type: u8
                repeat: eos
        "#}, "t", &[]);
        if let Value::Array(items) = get_field(&v, "items") {
            assert!(items.is_empty());
        } else { panic!("expected Array"); }
    }

    #[test]
    fn repeat_expr_count() {
        let data = [0x03u8, 0x0a, 0x0b, 0x0c]; // count=3, items=[10,11,12]
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: count
                type: u8
              - id: items
                type: u8
                repeat: expr
                repeat-expr: count
        "#}, "t", &data);
        if let Value::Array(items) = get_field(&v, "items") {
            assert_eq!(items.len(), 3);
            assert_eq!(items[1], Value::UInt(0x0b));
        } else { panic!("expected Array"); }
    }

    #[test]
    fn repeat_expr_zero_count() {
        let data = [0x00u8]; // count=0
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: count
                type: u8
              - id: items
                type: u8
                repeat: expr
                repeat-expr: count
        "#}, "t", &data);
        if let Value::Array(items) = get_field(&v, "items") {
            assert!(items.is_empty());
        } else { panic!("expected Array"); }
    }

    #[test]
    fn repeat_until_null_terminator() {
        // Classic C-string style: read bytes until value == 0x00.
        // The terminator is included as the last item in the array.
        let data = [0x41u8, 0x42, 0x43, 0x00]; // "ABC\0"
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: chars
                type: u8
                repeat: until
                repeat-until: _ == 0
        "#}, "t", &data);
        if let Value::Array(items) = get_field(&v, "chars") {
            assert_eq!(items.len(), 4); // A, B, C, and the terminator
            assert_eq!(items[0], Value::UInt(0x41));
            assert_eq!(items[3], Value::UInt(0x00)); // terminator included
        } else { panic!("expected Array"); }
    }

    #[test]
    fn repeat_until_sentinel_value() {
        // Read u16le values until one equals 0xFFFF.
        let data: Vec<u8> = vec![
            0x01, 0x00,  // 1
            0x02, 0x00,  // 2
            0xFF, 0xFF,  // sentinel
        ];
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: values
                type: u16le
                repeat: until
                repeat-until: _ == 0xFFFF
        "#}, "t", &data);
        if let Value::Array(items) = get_field(&v, "values") {
            assert_eq!(items.len(), 3);
            assert_eq!(items[0], Value::UInt(1));
            assert_eq!(items[1], Value::UInt(2));
            assert_eq!(items[2], Value::UInt(0xFFFF)); // sentinel included
        } else { panic!("expected Array"); }
    }

    #[test]
    fn repeat_until_expression_with_bitwise() {
        // Stop when the high bit of a byte is set.
        let data = [0x01u8, 0x02, 0x83]; // 0x83 has high bit set
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: bytes
                type: u8
                repeat: until
                repeat-until: _ & 0x80
        "#}, "t", &data);
        if let Value::Array(items) = get_field(&v, "bytes") {
            assert_eq!(items.len(), 3);
            assert_eq!(items[2], Value::UInt(0x83));
        } else { panic!("expected Array"); }
    }

    #[test]
    fn repeat_until_stops_at_eof_if_condition_never_met() {
        // If the terminator condition is never true, iteration stops at EOF.
        let data = [0x01u8, 0x02, 0x03]; // no zero byte
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: items
                type: u8
                repeat: until
                repeat-until: _ == 0
        "#}, "t", &data);
        if let Value::Array(items) = get_field(&v, "items") {
            assert_eq!(items.len(), 3);
        } else { panic!("expected Array"); }
    }

    // ── Sub-types ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_subtype() {
        let data = [0x01u8, 0x00, 0x02, 0x00]; // two u16le values
        let v = parse(indoc! {r#"
            id: outer
            seq:
              - id: point
                type: point_t
            types:
              point_t:
                seq:
                  - id: x
                    type: u16le
                  - id: y
                    type: u16le
        "#}, "outer", &data);
        let point = get_field(&v, "point");
        assert_eq!(get_field(point, "x"), &Value::UInt(1));
        assert_eq!(get_field(point, "y"), &Value::UInt(2));
    }

    #[test]
    fn parse_repeated_subtype() {
        // 2 points × 4 bytes = 8 bytes, prefixed with count byte
        let v = parse(indoc! {r#"
            id: outer
            seq:
              - id: count
                type: u8
              - id: points
                type: point_t
                repeat: expr
                repeat-expr: count
            types:
              point_t:
                seq:
                  - id: x
                    type: u16le
                  - id: y
                    type: u16le
        "#}, "outer", &[2, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00]);
        if let Value::Array(points) = get_field(&v, "points") {
            assert_eq!(points.len(), 2);
            assert_eq!(get_field(&points[0], "x"), &Value::UInt(1));
            assert_eq!(get_field(&points[1], "y"), &Value::UInt(4));
        } else { panic!("expected Array"); }
    }

    // ── Multiple fields ───────────────────────────────────────────────────────

    #[test]
    fn parse_multiple_fields_sequential() {
        let v = parse(indoc! {r#"
            id: t
            seq:
              - id: a
                type: u8
              - id: b
                type: u16le
              - id: c
                type: u32le
        "#}, "t", &[0x01, 0x03, 0x00, 0x07, 0x00, 0x00, 0x00]);
        assert_eq!(get_field(&v, "a"), &Value::UInt(1));
        assert_eq!(get_field(&v, "b"), &Value::UInt(3));
        assert_eq!(get_field(&v, "c"), &Value::UInt(7));
    }

    // ── Realistic: TGA-like header ────────────────────────────────────────────

    #[test]
    fn parse_tga_like_header() {
        // Minimal TGA header layout
        let data = vec![
            0x00u8,       // id_length
            0x00,         // colormap_type
            0x02,         // image_type (truecolor)
            0x00, 0x00,   // colormap_first_entry
            0x00, 0x00,   // colormap_length
            0x00,         // colormap_entry_size
            0x00, 0x00,   // x_origin
            0x00, 0x00,   // y_origin
            0x80, 0x02,   // width = 640
            0xe0, 0x01,   // height = 480
            0x18,         // pixel_depth = 24
            0x00,         // image_descriptor
        ];
        let v = parse(indoc! {r#"
            id: tga_header
            meta:
              endian: le
            seq:
              - id: id_length
                type: u8
              - id: colormap_type
                type: u8
              - id: image_type
                type: u8
                enum: image_type_t
              - id: colormap_first_entry
                type: u16
              - id: colormap_length
                type: u16
              - id: colormap_entry_size
                type: u8
              - id: x_origin
                type: u16
              - id: y_origin
                type: u16
              - id: width
                type: u16
              - id: height
                type: u16
              - id: pixel_depth
                type: u8
              - id: image_descriptor
                type: u8
            enums:
              image_type_t:
                "0": no_image
                "1": colormap
                "2": truecolor
                "3": grayscale
        "#}, "tga_header", &data);

        assert_eq!(get_field(&v, "width"),  &Value::UInt(640));
        assert_eq!(get_field(&v, "height"), &Value::UInt(480));
        assert_eq!(get_field(&v, "pixel_depth"), &Value::UInt(24));
        assert_eq!(get_field(&v, "image_type"),
            &Value::Enum { value: 2, name: Some("truecolor".into()) });
    }

    // ── Realistic: length-prefixed string list ────────────────────────────────

    #[test]
    fn parse_length_prefixed_strings() {
        // Format: u8 count, then (u8 len, bytes) for each string
        let data: Vec<u8> = vec![
            0x02,               // count = 2
            0x05,               // str 0: len = 5
            b'h', b'e', b'l', b'l', b'o',
            0x03,               // str 1: len = 3
            b'f', b'o', b'o',
        ];
        let v = parse(indoc! {r#"
            id: string_list
            seq:
              - id: count
                type: u8
              - id: strings
                type: lp_string
                repeat: expr
                repeat-expr: count
            types:
              lp_string:
                seq:
                  - id: length
                    type: u8
                  - id: value
                    type: str
                    size: length
                    encoding: ascii
        "#}, "string_list", &data);

        if let Value::Array(strings) = get_field(&v, "strings") {
            assert_eq!(strings.len(), 2);
            assert_eq!(get_field(&strings[0], "value"), &Value::Str("hello".into()));
            assert_eq!(get_field(&strings[1], "value"), &Value::Str("foo".into()));
        } else {
            panic!("expected Array");
        }
    }

    // ── Error cases ───────────────────────────────────────────────────────────

    #[test]
    fn unknown_type_name_is_error() {
        let (engine, _reg) = engine_from_yaml(indoc! {r#"
            id: t
            seq: []
        "#});
        let mut cursor = Cursor::new(&[]);
        assert!(engine.parse_type("nonexistent", &mut cursor).is_err());
    }

    #[test]
    fn buffer_underrun_is_error() {
        let err = parse_err(indoc! {r#"
            id: t
            seq:
              - id: x
                type: u32le
        "#}, "t", &[0x01, 0x02]); // only 2 bytes, need 4
        assert!(matches!(err, DoeError::Schema(_)));
    }

    // ── decode_string ─────────────────────────────────────────────────────────

    #[test]
    fn decode_ascii_valid() {
        assert_eq!(
            decode_string(b"hello", Encoding::Ascii).unwrap(),
            "hello"
        );
    }

    #[test]
    fn decode_ascii_high_byte_is_error() {
        assert!(decode_string(&[0x80], Encoding::Ascii).is_err());
    }

    #[test]
    fn decode_utf8_valid() {
        let s = "héllo";
        assert_eq!(decode_string(s.as_bytes(), Encoding::Utf8).unwrap(), s);
    }

    #[test]
    fn decode_utf8_invalid_is_error() {
        assert!(decode_string(&[0xff, 0xfe], Encoding::Utf8).is_err());
    }

    #[test]
    fn decode_latin1_high_bytes() {
        // 0xe9 = 'é' in latin-1 = U+00E9
        let result = decode_string(&[0xe9], Encoding::Latin1).unwrap();
        assert_eq!(result, "é");
    }

    // ── resolve_endian ────────────────────────────────────────────────────────

    #[test]
    fn resolve_endian_inherit_uses_type_default() {
        assert_eq!(resolve_endian(EndianOverride::Inherit, Endian::Big),    Endian::Big);
        assert_eq!(resolve_endian(EndianOverride::Inherit, Endian::Little), Endian::Little);
    }

    #[test]
    fn resolve_endian_override_ignores_type_default() {
        assert_eq!(resolve_endian(EndianOverride::Little, Endian::Big),    Endian::Little);
        assert_eq!(resolve_endian(EndianOverride::Big,    Endian::Little), Endian::Big);
    }
}
