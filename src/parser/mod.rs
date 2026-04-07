//! Binary parse engine and supporting types.
//!
//! The parser walks a compiled [`crate::schema::ir::TypeRegistry`] against a
//! byte buffer and produces a [`crate::Value`] tree.  The three sub-modules
//! divide responsibilities cleanly:
//!
//! - [`cursor`] — a zero-copy, position-tracking view over an immutable
//!   `&[u8]` buffer with typed read methods for every integer and float width.
//! - [`context`] — the runtime parse context: a stack of scopes that binds
//!   field names to their parsed values so that later fields can reference
//!   earlier ones in size and repeat-count expressions.
//! - [`engine`] — the top-level dispatcher that walks the schema IR field by
//!   field, calling into `cursor` for raw bytes, `context` for expression
//!   evaluation, and recursing into sub-types.

pub mod context;
pub mod cursor;
pub mod engine;
