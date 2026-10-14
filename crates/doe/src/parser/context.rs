//! Parse context: tracks parsed field values so that later fields can
//! reference earlier ones in size / repeat-count / conditional expressions.
//!
//! The context is a stack of scopes, where each scope corresponds to one
//! struct being parsed.  When the engine descends into a sub-type it pushes
//! a new scope; when it returns it pops it.  This is what gives `_parent`
//! references their meaning.

use crate::schema::expr::{EvalContext, ExprValue};
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
    /// Bytes read so far by fields placed with `pos:`.
    reread_bytes: usize,
    /// The size of the input being parsed, which the `pos:` budget scales with.
    input_len: usize,
}

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

    /// Push a new scope (entering a sub-type).
    pub fn push_scope(&mut self) {
        self.eval_ctx.push_frame();
        self.value_frames.push(Vec::new());
    }

    /// Pop the current scope (returning from a sub-type).
    /// Returns the fields accumulated in the scope.
    pub fn pop_scope(&mut self) -> Vec<(String, Value)> {
        self.eval_ctx.pop_frame();
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

    /// Register `value` for expression evaluation.
    ///
    /// Integers, floats, strings, byte sequences and arrays of those are bound
    /// under `name`. The fields of a struct are bound under dotted names
    /// (`name.field`, `name.inner.field`, ...), so expressions can read
    /// `header.section_count`. Values that are not kept in memory (a
    /// [`Value::Blob`]), absent fields, and arrays with a struct among their
    /// items are not bound.
    fn bind_eval(&mut self, name: &str, value: &Value) {
        if let Some(v) = to_expr_value(value) {
            self.eval_ctx.bind(name, v);
        } else if let Value::Struct { fields, .. } = value {
            for (field, inner) in fields {
                self.bind_eval(&format!("{name}.{field}"), inner);
            }
        }
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

/// The expression value of `value`, if it has one.
fn to_expr_value(value: &Value) -> Option<ExprValue> {
    match value {
        Value::UInt(n) => Some(ExprValue::from(*n)),
        Value::SInt(n) => Some(ExprValue::from(*n)),
        Value::Enum { value, .. } => Some(ExprValue::from(*value)),
        Value::Float(x) => Some(ExprValue::Float(*x)),
        Value::Bool(b) => Some(ExprValue::Bool(*b)),
        Value::Str(s) => Some(ExprValue::Str(s.clone())),
        Value::Bytes(b) => Some(ExprValue::Bytes(b.clone())),
        Value::Array(items) => items
            .iter()
            .map(to_expr_value)
            .collect::<Option<Vec<_>>>()
            .map(ExprValue::Array),
        Value::Blob { .. } | Value::Struct { .. } | Value::Absent => None,
    }
}
