//! Schema loading, compilation, and the compiled intermediate representation.
//!
//! This module tree is responsible for turning a YAML schema file into a
//! form the binary parser can consume efficiently.  The pipeline is:
//!
//! ```text
//! YAML on disk  ──►  raw::RawSchema  ──►  compiler::Compiler  ──►  ir::TypeRegistry
//! ```
//!
//! - [`raw`] — zero-logic serde structs that mirror the YAML exactly.
//! - [`compiler`] — validates, resolves cross-references, pre-parses
//!   expressions, and builds the [`ir::TypeRegistry`].
//! - [`ir`] — the compiled, fully-resolved representation consumed by the
//!   parser.
//! - [`expr`] — the expression language used in `size:`, `repeat-expr:`,
//!   and `if:` fields.

pub mod compiler;
pub mod expr;
pub mod ir;
pub mod raw;
