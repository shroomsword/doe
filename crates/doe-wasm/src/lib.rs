//! doe for WebAssembly: the library behind the web IDE.
//!
//! These functions are exported to JavaScript by `wasm-bindgen`. The requests
//! and the answers are JSON strings (see `API.md`), so that the page, a worker
//! and the tests all call them the same way, with no objects that live in
//! WebAssembly memory to free.
//!
//! The Rust side of the calls is in [`api`], which has no dependency on
//! `wasm-bindgen` and is tested natively.

pub use doe::api;

use serde::Serialize;
use wasm_bindgen::prelude::wasm_bindgen;

fn to_json<T: Serialize>(response: &T) -> String {
    // The answers are made of strings, numbers and lists: they always
    // serialise; the fallback is a valid answer that says what happened.
    serde_json::to_string(response).unwrap_or_else(|e| {
        format!(
            "{{\"ok\":false,\"diagnostics\":[{{\"severity\":\"error\",\"kind\":\"request\",\"message\":{:?},\"position\":null}}]}}",
            e.to_string()
        )
    })
}

/// Compile the schema of `request` (JSON) and say what is wrong with it.
/// Returns JSON: `{ok, id, diagnostics}`.
#[wasm_bindgen]
pub fn check(request: &str) -> String {
    to_json(&api::check(request))
}

/// List the schemas of `request` (JSON): its own and the files it imports, the
/// imports that are missing, and the types that can be parsed with. Returns
/// JSON: `{ok, schemas, missing, diagnostics}`.
#[wasm_bindgen]
pub fn inspect(request: &str) -> String {
    to_json(&api::inspect(request))
}

/// What a schema's files start with and what the format is called, for
/// finding the schema of a file. Returns JSON: `{ok, magic, names, diagnostics}`.
#[wasm_bindgen]
pub fn signature(request: &str) -> String {
    to_json(&api::signature(request))
}

/// The order in which to try schemas on a file that starts with `head`. The
/// request is JSON, `{sources: [{name, magic, names}], name, all}`. Returns
/// JSON: `{ok, order: [{index, why}], excluded, diagnostics}`.
#[wasm_bindgen]
pub fn detect_order(request: &str, head: &[u8]) -> String {
    to_json(&api::detect_order(request, head))
}

/// Compile the schema of `request` (JSON) and parse `bytes` with it. Returns
/// JSON: `{ok, id, diagnostics, value, trace, error}`.
#[wasm_bindgen]
pub fn parse(request: &str, bytes: &[u8]) -> String {
    to_json(&api::parse(request, bytes))
}

/// Compile the schema of `request` (JSON), parse `bytes` with it and render the
/// whole value as the command line does (`export` in [`api`]; that is a reserved
/// word in JavaScript): `format` is `text`, `json` or
/// `pretty`. Returns JSON: `{ok, diagnostics, output, error}`.
#[wasm_bindgen]
pub fn render(request: &str, bytes: &[u8], format: &str) -> String {
    to_json(&api::export(request, bytes, format))
}

/// The version of doe in this module.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
