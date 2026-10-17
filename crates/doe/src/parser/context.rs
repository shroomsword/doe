//! Parse context: tracks parsed field values so that later fields can
//! reference earlier ones in size / repeat-count / conditional expressions.
//!
//! The context is a stack of scopes, where each scope corresponds to one
//! struct being parsed.  When the engine descends into a sub-type it pushes
//! a new scope; when it returns it pops it.  This is what gives `_parent`
//! references their meaning.

use crate::parser::cursor::Cursor;
use crate::schema::expr::{EvalContext, ExprValue};
use crate::schema::ir::{CompiledParam, Endian};
use crate::value::Value;

// ─────────────────────────────────────────────────────────────────────────────
// ParseContext
// ─────────────────────────────────────────────────────────────────────────────

/// Runtime parse context.
///
/// Maintains two parallel stacks:
/// - `eval_ctx`: bindings for the expression evaluator.
/// - `value_stack`: the actual `Value` objects (needed to look up struct
///   fields by name if we ever support more complex path expressions).
#[derive(Debug)]
pub struct ParseContext {
    /// Numeric bindings visible to size, repeat, and `if` expressions.
    pub eval_ctx: EvalContext,
    /// Stack of frames; each frame is a vec of (`field_id`, Value).
    value_frames: Vec<Vec<(String, Value)>>,
    /// Per scope, which instances are computed or being computed, and their
    /// values until the end of the scope.
    instance_frames: Vec<InstanceFrame>,
    /// Bytes read so far by fields placed with `pos:`.
    reread_bytes: usize,
    /// The size of the input being parsed, which the `pos:` budget scales with.
    input_len: usize,
}

/// Where an instance is in being computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstanceState {
    InProgress,
    Done,
}

/// What [`ParseContext::begin_instance`] found out about an instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceStart {
    /// It has not been computed: compute it now.
    Compute,
    /// It already has been.
    Done,
    /// It is being computed, so it depends on itself.
    Cycle,
}

#[derive(Debug, Default)]
struct InstanceFrame {
    states: std::collections::HashMap<String, InstanceState>,
    values: std::collections::HashMap<String, Value>,
    /// The byte order of the type of the scope, once it is known.
    endian: Option<Endian>,
}

/// The most values one binding copies into the expression context; a larger
/// array or sub-type is bound as [`ExprValue::Omitted`].
pub const MAX_BOUND_VALUES: usize = 1 << 20;

/// A `pos:` field may read this many times the size of the input in total,
/// plus [`REREAD_FLOOR`]: enough for any table of records, but not for
/// re-reading a large region once per element of a long array.
pub const REREAD_FACTOR: usize = 16;

/// The least number of bytes the `pos:` budget allows, for tiny inputs.
pub const REREAD_FLOOR: usize = 1 << 20;

impl ParseContext {
    /// Create a context with a single, empty root scope.
    pub fn new() -> Self {
        ParseContext {
            eval_ctx: EvalContext::new(),
            value_frames: vec![Vec::new()],
            instance_frames: vec![InstanceFrame::default()],
            reread_bytes: 0,
            input_len: 0,
        }
    }

    /// Record the size of the whole input, which the `pos:` budget scales with
    /// (not that of a window a field is read in).
    pub fn set_input_len(&mut self, len: usize) {
        self.input_len = len;
    }

    /// Account for `n` bytes read by a `pos:` field.
    ///
    /// Because `pos:` can send many elements to the same region, the total is
    /// bounded; see [`REREAD_FACTOR`].
    ///
    /// # Errors
    ///
    /// Returns a message if the budget is spent.
    pub fn charge_reread(&mut self, n: usize) -> Result<(), String> {
        self.reread_bytes = self.reread_bytes.saturating_add(n);
        let budget = self
            .input_len
            .saturating_mul(REREAD_FACTOR)
            .saturating_add(REREAD_FLOOR);
        if self.reread_bytes > budget {
            return Err(format!(
                "fields placed with pos read more than {budget} bytes in total"
            ));
        }
        Ok(())
    }

    /// The number of scopes entered, including the root scope.
    pub fn depth(&self) -> usize {
        self.value_frames.len()
    }

    /// Record the byte order of the type of the current scope.
    pub fn set_endian(&mut self, endian: Endian) {
        if let Some(frame) = self.instance_frames.last_mut() {
            frame.endian = Some(endian);
        }
    }

    /// The byte order of the type of the current scope, or `default` if it has
    /// not been recorded.
    pub fn endian(&self, default: Endian) -> Endian {
        self.instance_frames
            .last()
            .and_then(|frame| frame.endian)
            .unwrap_or(default)
    }

    /// The byte order of the type around the current scope, or `default`.
    pub fn parent_endian(&self, default: Endian) -> Endian {
        self.instance_frames
            .len()
            .checked_sub(2)
            .and_then(|below| self.instance_frames.get(below))
            .and_then(|frame| frame.endian)
            .unwrap_or(default)
    }

    /// Push a new scope (entering a sub-type).
    pub fn push_scope(&mut self) {
        self.eval_ctx.push_frame();
        self.value_frames.push(Vec::new());
        self.instance_frames.push(InstanceFrame::default());
    }

    /// Pop the current scope (returning from a sub-type).
    /// Returns the fields accumulated in the scope.
    pub fn pop_scope(&mut self) -> Vec<(String, Value)> {
        self.eval_ctx.pop_frame();
        self.instance_frames.pop();
        self.value_frames.pop().unwrap_or_default()
    }

    /// Bind a parsed field value in the current scope.
    ///
    /// If the value has a numeric representation, it is also bound in the
    /// expression evaluator so size/repeat/if expressions can reference it.
    pub fn bind(&mut self, id: &str, value: Value) {
        self.bind_eval(id, &value);
        if self.value_frames.is_empty() {
            self.value_frames.push(Vec::new());
        }
        if let Some(frame) = self.value_frames.last_mut() {
            frame.push((id.to_owned(), value));
        }
    }

    /// Register `value` for expression evaluation, under `name`.
    ///
    /// Integers, floats, booleans, strings, byte sequences, arrays and
    /// sub-types (with their fields) are bound, so expressions can read
    /// `header.section_count` or `entries[0].offset`. A value that is not kept
    /// in memory (a [`Value::Blob`]) or too large to copy (see
    /// [`MAX_BOUND_VALUES`]) is bound as [`ExprValue::Omitted`], which knows only
    /// its size. Absent fields are not bound.
    fn bind_eval(&mut self, name: &str, value: &Value) {
        let mut budget = MAX_BOUND_VALUES;
        if let Some(v) = to_expr_value(value, &mut budget) {
            self.eval_ctx.bind(name, v);
        }
    }

    /// Note that the instance `id` of the current type is about to be computed.
    pub fn begin_instance(&mut self, id: &str) -> InstanceStart {
        let Some(frame) = self.instance_frames.last_mut() else {
            return InstanceStart::Compute;
        };
        match frame.states.get(id) {
            Some(InstanceState::Done) => InstanceStart::Done,
            Some(InstanceState::InProgress) => InstanceStart::Cycle,
            None => {
                frame
                    .states
                    .insert(id.to_owned(), InstanceState::InProgress);
                InstanceStart::Compute
            }
        }
    }

    /// Record the value of the instance `id` of the current type, and make it
    /// visible to expressions (without the `Instance` wrapper).
    pub fn finish_instance(&mut self, id: &str, value: Value) {
        let inner = match &value {
            Value::Instance { value, .. } => value.as_ref(),
            other => other,
        };
        let mut budget = MAX_BOUND_VALUES;
        if let Some(v) = to_expr_value(inner, &mut budget) {
            self.eval_ctx.bind(id, v);
        }
        if let Some(frame) = self.instance_frames.last_mut() {
            frame.states.insert(id.to_owned(), InstanceState::Done);
            frame.values.insert(id.to_owned(), value);
        }
    }

    /// Bind the values passed to a type, as its parameters, in the current
    /// scope. Parameters without a value (a mismatch the compiler rules out)
    /// stay unbound.
    pub fn bind_params(&mut self, params: &[CompiledParam], values: Vec<ExprValue>) {
        for (param, value) in params.iter().zip(values) {
            self.eval_ctx.bind(&param.id, value);
        }
    }

    /// Take the value of the instance `id` of the current type, if it has
    /// been computed.
    pub fn take_instance(&mut self, id: &str) -> Option<Value> {
        self.instance_frames.last_mut()?.values.remove(id)
    }

    /// The value of `value` in an expression, if it has one.
    pub fn expr_value(value: &Value) -> Option<ExprValue> {
        let mut budget = MAX_BOUND_VALUES;
        to_expr_value(value, &mut budget)
    }

    /// Tell expressions where they are: the position, size and end of the
    /// stream (the whole input or a window) that `cursor` reads, which `_io`
    /// stands for.
    pub fn set_io(&mut self, cursor: &Cursor<'_>) {
        self.eval_ctx
            .set_io(cursor.pos(), cursor.len(), cursor.is_eof());
    }

    /// Bind a value that exists only while one expression is evaluated, such
    /// as `_` in `repeat-until`, replacing whatever `name` was bound to.
    pub fn bind_scratch(&mut self, name: &str, value: &Value) {
        self.eval_ctx.unbind_prefix(name);
        self.bind_eval(name, value);
    }

    /// Look up the most recently bound value for `id` in the current scope.
    pub fn lookup(&self, id: &str) -> Option<&Value> {
        self.value_frames
            .last()?
            .iter()
            .rev()
            .find(|(k, _)| k == id)
            .map(|(_, v)| v)
    }

    /// Returns the fields accumulated in the current scope so far,
    /// without popping the scope.
    pub fn current_fields(&self) -> &[(String, Value)] {
        self.value_frames.last().map_or(&[], Vec::as_slice)
    }
}

impl Default for ParseContext {
    fn default() -> Self {
        Self::new()
    }
}

/// The expression value of `value`, if it has one; `budget` is how many more
/// values may be copied.
fn to_expr_value(value: &Value, budget: &mut usize) -> Option<ExprValue> {
    let too_large = |len: usize| ExprValue::Omitted { len };
    Some(match value {
        Value::UInt(n) => ExprValue::from(*n),
        Value::SInt(n) => ExprValue::from(*n),
        Value::Enum { value, .. } => ExprValue::from(*value),
        Value::Float(x) => ExprValue::Float(*x),
        Value::Bool(b) => ExprValue::Bool(*b),
        Value::Str(s) => ExprValue::Str(s.clone()),
        Value::Bytes(b) => ExprValue::Bytes(b.clone()),
        Value::Blob { len, .. } => too_large(usize::try_from(*len).unwrap_or(usize::MAX)),
        Value::Array(items) => {
            if items.len() > *budget {
                return Some(too_large(items.len()));
            }
            *budget -= items.len();
            ExprValue::Array(
                items
                    .iter()
                    .map(|item| to_expr_value(item, budget).unwrap_or(too_large(0)))
                    .collect(),
            )
        }
        Value::Struct { fields, .. } => {
            if fields.len() > *budget {
                return Some(too_large(fields.len()));
            }
            *budget -= fields.len();
            ExprValue::Struct(
                fields
                    .iter()
                    .filter_map(|(name, field)| {
                        to_expr_value(field, budget).map(|v| (name.clone(), v))
                    })
                    .collect(),
            )
        }
        Value::Instance { value, .. } => return to_expr_value(value, budget),
        Value::Absent => return None,
    })
}
