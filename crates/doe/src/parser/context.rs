//! Parse context: tracks parsed field values so that later fields can
//! reference earlier ones in size / repeat-count / conditional expressions.
//!
//! The context is a stack of scopes, where each scope corresponds to one
//! struct being parsed.  When the engine descends into a sub-type it pushes
//! a new scope; when it returns it pops it.  This is what gives `_parent`
//! references their meaning.

use crate::parser::cursor::Cursor;
use crate::parser::trace::TraceBuilder;
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
    /// What is known of each scope, by the number of its frame in `eval_ctx`.
    scopes: Vec<Scope>,
    /// Instances that could not be computed where their type ended, and are
    /// tried again once the whole input is read.
    deferred: Vec<Deferred>,
    /// The next number for the value that stands for an instance in `deferred`.
    next_deferred: u64,
    /// The `parent:` of the field being read, until the sub-type it applies to
    /// is entered.
    pending_parent: Option<usize>,
    /// Bytes read so far by fields placed with `pos:`.
    reread_bytes: usize,
    /// The size of the input being parsed, which the `pos:` budget scales with.
    input_len: usize,
    /// The size of the whole input that the parse started from, which tells
    /// the cursors over it from those over other bytes (a `process:` result).
    root_len: usize,
    /// Where the values come from, when the parse is traced.
    trace: Option<TraceBuilder>,
    /// Whether to keep what was read when the parse fails.
    keep_partial: bool,
    /// What the value that was being read had got as far as when the parse
    /// failed, until the one around it takes it in.
    partial: Option<Value>,
    /// How much work the parse may do, when it is limited.
    limits: Option<Meter>,
}

/// What a limited parse has used so far.
#[derive(Debug)]
struct Meter {
    fields_left: u64,
    deadline: Option<std::time::Instant>,
    checked: u32,
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

/// What is known of one scope: a struct being parsed, or that was.
#[derive(Debug, Default)]
struct Scope {
    /// The fields accumulated, as (`field_id`, Value).
    values: Vec<(String, Value)>,
    /// Which instances are computed or being computed, and their values until
    /// the end of the scope.
    instances: InstanceFrame,
}

/// An instance that is computed later; see [`ParseContext::defer`].
#[derive(Debug, Clone)]
pub struct Deferred {
    /// The number of the frame of the scope the instance belongs to.
    pub frame: usize,
    /// The fully-qualified name of its type.
    pub type_name: String,
    /// Its position among the instances of the type.
    pub instance: usize,
    /// The number of the value that stands for it in the result.
    pub id: u64,
    /// The stream of the scope, and where its type ended: the start in the
    /// whole input, the size and the position.
    pub stream: (usize, usize, usize),
}

#[derive(Debug, Default)]
struct InstanceFrame {
    states: std::collections::HashMap<String, InstanceState>,
    values: std::collections::HashMap<String, Value>,
    /// The byte order of the type of the scope, once it is known.
    endian: Option<Endian>,
}

/// The name of the struct that stands, in the values of a parse, for an
/// instance that is computed later.
pub const DEFERRED_TYPE: &str = "$deferred";

pub use crate::schema::expr::PARAM_PREFIX;

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
            scopes: vec![Scope::default()],
            deferred: Vec::new(),
            next_deferred: 0,
            pending_parent: None,
            reread_bytes: 0,
            input_len: 0,
            root_len: 0,
            trace: None,
            keep_partial: false,
            partial: None,
            limits: None,
        }
    }

    /// Limit the work of the parse to `max_fields` fields read and, when it is
    /// given, `max_time` (checked every so often, not at every field).
    pub(crate) fn limit(&mut self, max_fields: Option<u64>, max_time: Option<std::time::Duration>) {
        if max_fields.is_some() || max_time.is_some() {
            self.limits = Some(Meter {
                fields_left: max_fields.unwrap_or(u64::MAX),
                deadline: max_time.map(|time| std::time::Instant::now() + time),
                checked: 0,
            });
        }
    }

    /// Count one field read against the limits.
    ///
    /// # Errors
    ///
    /// [`DoeError::LimitExceeded`] when the fields or the time are used up.
    pub(crate) fn charge(&mut self) -> crate::error::Result<()> {
        let Some(meter) = self.limits.as_mut() else {
            return Ok(());
        };
        if meter.fields_left == 0 {
            return Err(crate::error::DoeError::LimitExceeded(
                "too many fields".to_owned(),
            ));
        }
        meter.fields_left -= 1;
        meter.checked += 1;
        if meter.checked >= 256 {
            meter.checked = 0;
            if meter
                .deadline
                .is_some_and(|d| std::time::Instant::now() >= d)
            {
                return Err(crate::error::DoeError::LimitExceeded(
                    "too much time".to_owned(),
                ));
            }
        }
        Ok(())
    }

    /// Keep what was read when the parse fails.
    pub(crate) fn enable_partial(&mut self) {
        self.keep_partial = true;
    }

    /// Whether what was read is kept when the parse fails.
    pub(crate) fn keeps_partial(&self) -> bool {
        self.keep_partial
    }

    /// Record what the value being read had got as far as.
    pub(crate) fn set_partial(&mut self, value: Value) {
        self.partial = Some(value);
    }

    /// Take what the value that failed had got as far as.
    pub(crate) fn take_partial(&mut self) -> Option<Value> {
        self.partial.take()
    }

    /// Record where every value comes from.
    pub(crate) fn enable_trace(&mut self) {
        self.trace = Some(TraceBuilder::new());
    }

    /// The record of where values come from, when there is one.
    pub(crate) fn trace(&mut self) -> Option<&mut TraceBuilder> {
        self.trace.as_mut()
    }

    /// Take the record of where values come from.
    pub(crate) fn take_trace(&mut self) -> Option<TraceBuilder> {
        self.trace.take()
    }

    /// Record the size of the whole input, which the `pos:` budget scales with
    /// (not that of a window a field is read in).
    pub fn set_input_len(&mut self, len: usize) {
        self.input_len = len;
    }

    /// Record the size of the whole input that a cursor was opened over.
    pub fn set_root_len(&mut self, len: usize) {
        self.root_len = len;
    }

    /// Whether `cursor` reads the input that the parse started from.
    pub fn is_input(&self, cursor: &Cursor<'_>) -> bool {
        cursor.root_len() == self.root_len
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
        self.eval_ctx.depth()
    }

    /// The scope being parsed.
    fn scope_mut(&mut self) -> Option<&mut Scope> {
        let current = self.eval_ctx.current_frame();
        self.scopes.get_mut(current)
    }

    /// Start frame `id` as an empty scope.
    fn start_scope(&mut self, id: usize) {
        if self.scopes.len() <= id {
            self.scopes.resize_with(id + 1, Scope::default);
        }
        if let Some(scope) = self.scopes.get_mut(id) {
            *scope = Scope::default();
        }
    }

    /// Record the byte order of the type of the current scope.
    pub fn set_endian(&mut self, endian: Endian) {
        if let Some(scope) = self.scope_mut() {
            scope.instances.endian = Some(endian);
        }
    }

    /// The byte order of the type of the current scope, or `default` if it has
    /// not been recorded.
    pub fn endian(&self, default: Endian) -> Endian {
        self.scopes
            .get(self.eval_ctx.current_frame())
            .and_then(|scope| scope.instances.endian)
            .unwrap_or(default)
    }

    /// The byte order of the type around the current scope, or `default`.
    pub fn parent_endian(&self, default: Endian) -> Endian {
        self.eval_ctx
            .enclosing_frame(1)
            .and_then(|below| self.scopes.get(below))
            .and_then(|scope| scope.instances.endian)
            .unwrap_or(default)
    }

    /// Push a new scope (entering a sub-type).
    pub fn push_scope(&mut self) {
        let id = self.eval_ctx.push_frame();
        self.start_scope(id);
    }

    /// Push a new scope for a sub-type of a field with `parent: _parent`: its
    /// `_parent` is that of the current scope, `up` times over.
    pub fn push_scope_under(&mut self, up: usize) {
        let id = self.eval_ctx.push_frame_under(up);
        self.start_scope(id);
    }

    /// The `parent:` set for the sub-type being entered, which is used once.
    pub fn take_pending_parent(&mut self) -> Option<usize> {
        self.pending_parent.take()
    }

    /// Say that the next sub-type that is entered has `parent: _parent` `up`
    /// times over.
    pub fn set_pending_parent(&mut self, up: Option<usize>) {
        self.pending_parent = up;
    }

    /// Pop the current scope (returning from a sub-type).
    /// Returns the fields accumulated in the scope.
    pub fn pop_scope(&mut self) -> Vec<(String, Value)> {
        let id = self.eval_ctx.current_frame();
        self.eval_ctx.pop_frame();
        self.scopes
            .get_mut(id)
            .map(|scope| std::mem::take(&mut scope.values))
            .unwrap_or_default()
    }

    /// Bind a parsed field value in the current scope.
    ///
    /// If the value has a numeric representation, it is also bound in the
    /// expression evaluator so size/repeat/if expressions can reference it.
    pub fn bind(&mut self, id: &str, value: Value) {
        self.bind_eval(id, &value);
        if let Some(scope) = self.scope_mut() {
            scope.values.push((id.to_owned(), value));
        }
    }

    /// Register `value` for expression evaluation, under `name`.
    ///
    /// Integers, floats, booleans, strings, byte sequences, arrays and
    /// sub-types (with their fields) are bound, so expressions can read
    /// `header.section_count` or `entries[0].offset`. A value that is not kept
    /// in memory (a [`Value::Blob`]) or too large to copy (see
    /// [`MAX_BOUND_VALUES`]) is bound as [`ExprValue::Omitted`], which knows only
    /// its size. A field that is not there is bound as a marker that is equal to nothing.
    fn bind_eval(&mut self, name: &str, value: &Value) {
        if matches!(value, Value::Absent) {
            self.eval_ctx
                .bind(name, crate::schema::expr::absent_marker());
            return;
        }
        let mut budget = MAX_BOUND_VALUES;
        if let Some(v) = to_expr_value(value, &mut budget) {
            self.eval_ctx.bind(name, v);
        }
    }

    /// Note that the instance `id` of the current type is about to be computed.
    pub fn begin_instance(&mut self, id: &str) -> InstanceStart {
        let Some(scope) = self.scope_mut() else {
            return InstanceStart::Compute;
        };
        let frame = &mut scope.instances;
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
        if let Some(scope) = self.scope_mut() {
            scope
                .instances
                .states
                .insert(id.to_owned(), InstanceState::Done);
            scope.instances.values.insert(id.to_owned(), value);
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
        self.scope_mut()?.instances.values.remove(id)
    }

    /// Forget that the instance `id` of the current type was started, because
    /// computing it failed, so that it can be tried again.
    pub fn abandon_instance(&mut self, id: &str) {
        if let Some(scope) = self.scope_mut() {
            scope.instances.states.remove(id);
        }
    }

    /// Whether the instance `id` of the current type has been computed.
    pub fn instance_done(&self, id: &str) -> bool {
        self.scopes
            .get(self.eval_ctx.current_frame())
            .is_some_and(|scope| scope.instances.states.get(id) == Some(&InstanceState::Done))
    }

    /// Put off the instance number `instance` of the type `type_name`, which
    /// belongs to the current scope, until the whole input is read. Keeps the
    /// scope and the ones around it. `stream` is where the type was read from:
    /// the start in the whole input, the size and the position. Returns the
    /// number of the value that stands for the instance in the result.
    pub fn defer(
        &mut self,
        type_name: &str,
        instance: usize,
        stream: (usize, usize, usize),
    ) -> u64 {
        self.eval_ctx.retain_scope();
        let id = self.next_deferred;
        self.next_deferred += 1;
        self.deferred.push(Deferred {
            frame: self.eval_ctx.current_frame(),
            type_name: type_name.to_owned(),
            instance,
            id,
            stream,
        });
        id
    }

    /// How many instances are waiting.
    pub fn deferred_len(&self) -> usize {
        self.deferred.len()
    }

    /// Forget the instances that were put off after the first `len`.
    pub fn truncate_deferred(&mut self, len: usize) {
        self.deferred.truncate(len);
    }

    /// Whether any instance is waiting.
    pub fn has_deferred(&self) -> bool {
        !self.deferred.is_empty()
    }

    /// Take the instances that are waiting.
    pub fn take_deferred(&mut self) -> Vec<Deferred> {
        std::mem::take(&mut self.deferred)
    }

    /// Put instances back that are still waiting, before any added since.
    pub fn put_back_deferred(&mut self, mut waiting: Vec<Deferred>) {
        waiting.append(&mut self.deferred);
        self.deferred = waiting;
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
        self.eval_ctx.set_io_at(
            cursor.window_start(),
            cursor.pos(),
            cursor.len(),
            cursor.is_eof(),
        );
    }

    /// Bind a value that exists only while one expression is evaluated, such
    /// as `_` in `repeat-until`, replacing whatever `name` was bound to.
    pub fn bind_scratch(&mut self, name: &str, value: &Value) {
        self.eval_ctx.unbind_prefix(name);
        self.bind_eval(name, value);
    }

    /// Look up the most recently bound value for `id` in the current scope.
    pub fn lookup(&self, id: &str) -> Option<&Value> {
        self.scopes
            .get(self.eval_ctx.current_frame())?
            .values
            .iter()
            .rev()
            .find(|(k, _)| k == id)
            .map(|(_, v)| v)
    }

    /// Returns the fields accumulated in the current scope so far,
    /// without popping the scope.
    pub fn current_fields(&self) -> &[(String, Value)] {
        self.scopes
            .get(self.eval_ctx.current_frame())
            .map_or(&[], |scope| scope.values.as_slice())
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
        Value::Enum { value, .. } => ExprValue::Int(*value),
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
        Value::Struct { type_name, fields } if type_name == DEFERRED_TYPE => {
            // An instance that is computed later; see `EvalContext::deferred_marker`.
            match fields.first() {
                Some((_, Value::UInt(id))) => EvalContext::deferred_marker(*id),
                _ => return None,
            }
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
