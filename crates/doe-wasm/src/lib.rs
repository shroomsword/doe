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

/// Decode the payload of a file (`process`: `gzip`, `xz`, `zstd`, ... as the `payload:` of its schema says) from byte
/// `from` of `bytes`, to at most 256 MiB. This is how the page shows a `.tar.gz` as the tar it holds.
///
/// # Errors
///
/// The text of the reason: an unknown process, a start past the end, a cut or corrupt stream, a bad checksum, or
/// more data than the limit.
#[wasm_bindgen]
pub fn decode(process: &str, bytes: &[u8], from: u32) -> Result<Vec<u8>, String> {
    doe::parser::process::decode_payload(
        process,
        bytes,
        u64::from(from),
        doe::parser::process::MAX_PROCESSED_LEN,
    )
    .map_err(|e| e.to_string())
}

/// A checksum that is computed as bytes come: `new Hasher("md5")`, `update(chunk)` for each piece, `finish()` for the
/// lower case hex digits. The worker keeps the input, and feeds this from it a few MiB at a time, so a file of any
/// size is hashed without a copy of it and without holding the worker.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Hasher {
    inner: Option<doe::filesystem::Hasher>,
}

#[wasm_bindgen]
impl Hasher {
    /// A checksum of the algorithm `algorithm` (`md5`); an error for a name that is not one.
    ///
    /// # Errors
    ///
    /// The name is not an algorithm.
    #[wasm_bindgen(constructor)]
    pub fn new(algorithm: &str) -> Result<Hasher, String> {
        let algorithm = doe::filesystem::Algorithm::from_name(algorithm)
            .ok_or_else(|| format!("{algorithm} is not a checksum algorithm"))?;
        Ok(Hasher {
            inner: Some(doe::filesystem::Hasher::new(algorithm)),
        })
    }

    /// The next bytes.
    pub fn update(&mut self, bytes: &[u8]) {
        if let Some(inner) = self.inner.as_mut() {
            inner.update(bytes);
        }
    }

    /// The digits of the checksum of everything given to `update`; empty if it was finished already.
    pub fn finish(&mut self) -> String {
        self.inner
            .take()
            .map(doe::filesystem::Hasher::finish)
            .unwrap_or_default()
    }
}

/// The version of doe in this module.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
