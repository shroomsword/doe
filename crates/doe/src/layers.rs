//! Files inside files: a `.tar.gz` is a gzip file that holds a tar.
//!
//! A schema says that a file of it holds another file with the `payload:` key (`process:` and, if the data does not
//! start at the beginning, `from:`). [`unwrap`] finds the schema of a file as [`crate::detect`] does, and while the
//! schema it found has a payload, decodes the payload (in memory, up to a limit) and finds the schema of that, to a
//! depth. The result is a stack of layers, outermost first, and the bytes of the innermost one when there is more than
//! one. Nothing is written to disk.
//!
//! The offsets of the inner layers are in the decoded bytes, not in the file.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::detect::{Detection, Library, Options};
use crate::error::{DoeError, Result};
use crate::parser::process::{self, MAX_PROCESSED_LEN};

/// How many layers are followed at most (a gzip of a tar of ...).
pub const MAX_DEPTH: usize = 4;

/// One layer: the type that fits it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Layer {
    /// The type that fits the layer.
    pub type_name: String,
}

/// The layers of a file.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Unwrapped {
    /// What fits each layer, outermost first. Empty when no schema fits the file.
    pub layers: Vec<Layer>,
    /// The decoded bytes of each layer after the first (the first is the file itself): `data[0]` is the second layer.
    pub data: Vec<Vec<u8>>,
    /// The detection of each layer, to say what was tried.
    pub detections: Vec<Detection>,
    /// Why the unwrapping stopped before the data ran out of layers, if it did.
    pub notes: Vec<String>,
}

impl Unwrapped {
    /// The layers as `gzip_file > tar`.
    #[must_use]
    pub fn describe(&self) -> String {
        self.layers
            .iter()
            .map(|l| l.type_name.as_str())
            .collect::<Vec<_>>()
            .join(" > ")
    }
}

/// The name the payload of a file would have as a file: the name without its last extension, and `.tar` for the
/// extensions that stand for a compressed tar (`x.tgz` holds `x.tar`).
#[must_use]
pub fn payload_name(file_name: &str) -> String {
    let Some((stem, ext)) = file_name.rsplit_once('.') else {
        return file_name.to_owned();
    };
    if stem.is_empty() {
        return file_name.to_owned();
    }
    match ext.to_ascii_lowercase().as_str() {
        "tgz" | "taz" | "tbz" | "tbz2" | "txz" | "tzst" => format!("{stem}.tar"),
        _ => stem.to_owned(),
    }
}

/// Decode `data` as the schema's `payload:` says.
fn decode(process_name: &str, data: &[u8], limit: usize) -> Result<Vec<u8>> {
    match process_name {
        "gzip" => process::gunzip(data, limit),
        "deflate" => process::inflate_raw(data, limit),
        "zlib" => process::inflate(data, limit),
        "brotli" => process::brotli_decode(data, limit),
        other => Err(DoeError::Schema(format!(
            "the payload says process '{other}': zlib, deflate, gzip and brotli are supported"
        ))),
    }
}

/// Find the layers of `file` among the types of the include paths, at most `max_layers` of them (1: only the outermost
/// one is looked for, as [`crate::detect::detect_file`] does).
///
/// # Errors
///
/// Fails when the file cannot be read, or two types of the include paths have the same id; a layer that cannot be
/// decoded stops the unwrapping with a note, it is not an error.
pub fn unwrap(
    file: &Path,
    include_paths: &[PathBuf],
    options: &Options,
    max_layers: usize,
    max_bytes: usize,
) -> Result<Unwrapped> {
    unwrap_as(file, include_paths, options, max_layers, max_bytes, None)
}

/// Like [`unwrap`], with the type of the outermost layer given instead of looked for (`first`: the id of a type of the
/// include paths): a file that is opened as `gzip_file` is still shown as the tar it holds.
///
/// # Errors
///
/// As [`unwrap`]; and when `first` is not a type of the include paths.
pub fn unwrap_as(
    file: &Path,
    include_paths: &[PathBuf],
    options: &Options,
    max_layers: usize,
    max_bytes: usize,
    first: Option<&str>,
) -> Result<Unwrapped> {
    let mut out = Unwrapped::default();
    if let Some(given) = first {
        // a type that holds nothing is the only layer: the type paths do not have to be compiled to know it
        let mut compiler = crate::schema::compiler::Compiler::new(include_paths.to_vec());
        compiler.load(given, None)?;
        if !compiler.registry.payloads.contains_key(given) {
            out.layers.push(Layer {
                type_name: given.to_owned(),
            });
            return Ok(out);
        }
    }
    let library = Library::load(include_paths)?;
    let mut name = if let Some(given) = first {
        if !library.sources().iter().any(|s| s.name == given) {
            return Err(DoeError::Schema(format!(
                "'{given}' is not a type of the type paths"
            )));
        }
        given.to_owned()
    } else {
        let found = library.detect(file, options)?;
        let Some(chosen) = found.chosen().map(|a| a.name.clone()) else {
            out.detections.push(found);
            return Ok(out);
        };
        out.detections.push(found);
        chosen
    };
    out.layers.push(Layer {
        type_name: name.clone(),
    });
    let file_name = file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    let mut inner_name = file_name;
    let max_layers = max_layers.clamp(1, MAX_DEPTH);
    while out.layers.len() < max_layers {
        let Some(payload) = library.payload_of(&name) else {
            break;
        };
        // the data of this layer: the file itself the first time
        let owned;
        let data: &[u8] = if let Some(bytes) = out.data.last() {
            bytes
        } else {
            let size = std::fs::metadata(file)
                .map_err(|source| DoeError::Io {
                    path: file.to_owned(),
                    source,
                })?
                .len();
            if size > u64::try_from(max_bytes).unwrap_or(u64::MAX) {
                out.notes.push(format!(
                    "{name}: the file is {size} bytes, more than the limit of {max_bytes} for unwrapping"
                ));
                break;
            }
            owned = std::fs::read(file).map_err(|source| DoeError::Io {
                path: file.to_owned(),
                source,
            })?;
            &owned
        };
        let start = usize::try_from(payload.from).unwrap_or(usize::MAX);
        let Some(coded) = data.get(start..) else {
            out.notes.push(format!(
                "{name}: the payload starts at byte {start}, past the end"
            ));
            break;
        };
        let decoded = match decode(&payload.process, coded, max_bytes.min(MAX_PROCESSED_LEN)) {
            Ok(decoded) => decoded,
            Err(error) => {
                out.notes.push(format!(
                    "could not unwrap {name} ({}): {error}",
                    payload.process
                ));
                break;
            }
        };
        inner_name = payload_name(&inner_name);
        let detection = library.detect_bytes(&decoded, Some(&inner_name), options)?;
        let Some(next) = detection.chosen().map(|a| a.name.clone()) else {
            out.notes.push(format!(
                "the payload of {name} ({} bytes) fits no schema",
                decoded.len()
            ));
            out.detections.push(detection);
            break;
        };
        out.detections.push(detection);
        out.layers.push(Layer {
            type_name: next.clone(),
        });
        name = next;
        out.data.push(decoded);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::payload_name;

    #[test]
    fn the_payload_of_a_file_is_named_without_its_last_extension() {
        assert_eq!(payload_name("a.tar.gz"), "a.tar");
        assert_eq!(payload_name("a.tgz"), "a.tar");
        assert_eq!(payload_name("a.gz"), "a");
        assert_eq!(payload_name("a"), "a");
        assert_eq!(payload_name(".gz"), ".gz");
    }
}
