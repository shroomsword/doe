//! Output renderers for the [`crate::Value`] tree.
//!
//! Two renderers are provided:
//!
//! - [`text`] — indented human-readable text, where each level of nesting is
//!   represented by an additional indent unit (default: two spaces).
//! - [`json`] — JSON output via `serde_json`, with both compact and
//!   pretty-printed variants.
//!
//! Both renderers consume a [`crate::Value`] reference and return a `String`;
//! neither performs any I/O.

pub mod json;
pub mod text;
