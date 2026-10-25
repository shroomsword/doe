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
mod encoding;
mod engine;
mod expr;
mod gallery_update;
mod ir;
mod locate;
mod raw;
mod render_json;
mod render_text;
mod trace;
mod typearg;
mod value;
