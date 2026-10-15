//! Integration tests for the doe crate.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

mod api;
mod compiler;
mod config;
mod conformance;
mod context;
mod cursor;
mod elf;
mod engine;
mod expr;
mod ir;
mod raw;
mod render_json;
mod render_text;
mod value;
