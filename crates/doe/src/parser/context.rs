//! Parse context: tracks parsed field values so that later fields can
//! reference earlier ones in size / repeat-count / conditional expressions.
//!
//! The context is a stack of scopes, where each scope corresponds to one
//! struct being parsed.  When the engine descends into a sub-type it pushes
//! a new scope; when it returns it pops it.  This is what gives `_parent`
//! references their meaning.

use crate::schema::expr::EvalContext;
use crate::value::Value;

// ─────────────────────────────────────────────────────────────────────────────
// ParseContext
// ─────────────────────────────────────────────────────────────────────────────

/// Runtime parse context.
///
/// Maintains two parallel stacks:
/// - `eval_ctx`: numeric bindings for the expression evaluator.
/// - `value_stack`: the actual `Value` objects (needed to look up struct
///   fields by name if we ever support more complex path expressions).
#[derive(Debug)]
pub struct ParseContext {
    /// Numeric bindings visible to size, repeat, and `if` expressions.
    pub eval_ctx: EvalContext,
    /// Stack of frames; each frame is a vec of (`field_id`, Value).
    value_frames: Vec<Vec<(String, Value)>>,
}

impl ParseContext {
    /// Create a context with a single, empty root scope.
    pub fn new() -> Self {
        ParseContext {
            eval_ctx: EvalContext::new(),
            value_frames: vec![Vec::new()],
        }
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
        self.bind_numeric(id, &value);
        if self.value_frames.is_empty() {
            self.value_frames.push(Vec::new());
        }
        if let Some(frame) = self.value_frames.last_mut() {
            frame.push((id.to_owned(), value));
        }
    }

    /// Register the numeric parts of `value` for expression evaluation.
    ///
    /// An integer is bound under `name`. The integer fields of a struct are
    /// bound under dotted names (`name.field`, `name.inner.field`, ...), so
    /// expressions can read `header.section_count`. Arrays, strings and bytes
    /// have no numeric value and are not bound.
    fn bind_numeric(&mut self, name: &str, value: &Value) {
        if let Some(n) = value.as_int() {
            self.eval_ctx.bind(name, n);
        } else if let Value::Struct { fields, .. } = value {
            for (field, inner) in fields {
                self.bind_numeric(&format!("{name}.{field}"), inner);
            }
        }
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
