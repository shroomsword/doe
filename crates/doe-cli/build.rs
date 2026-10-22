//! Embeds the web IDE (`web/dist`, made by `npm run build` in `web/`) in the
//! binary when the `ide` feature is on. The directory can be named with
//! `DOE_IDE_DIST`. When it is not there, a page that says so is embedded
//! instead, so that the crate still builds without the web toolchain.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    reason = "A build script reports failure by panicking, and talks to cargo on stdout."
)]

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

const PLACEHOLDER: &str = "<!doctype html><meta charset=utf-8><title>doe</title>\
<p>This build of doe has no web IDE in it: it was built without the files of <code>web/dist</code>. \
Build them (<code>npm run build</code> in <code>web/</code>) and build doe again.</p>";

fn main() {
    println!("cargo:rerun-if-env-changed=DOE_IDE_DIST");
    println!("cargo:rerun-if-changed=build.rs");
    if env::var_os("CARGO_FEATURE_IDE").is_none() {
        return;
    }
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let dist =
        env::var_os("DOE_IDE_DIST").map_or_else(|| manifest.join("../../web/dist"), PathBuf::from);
    println!("cargo:rerun-if-changed={}", dist.display());

    let mut files = Vec::new();
    if dist.join("index.html").is_file() {
        collect(&dist, &dist, &mut files);
    }
    files.sort();

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let mut code = String::from("pub(crate) static ASSETS: &[(&str, &[u8])] = &[\n");
    if files.is_empty() {
        let placeholder = out.join("placeholder.html");
        fs::write(&placeholder, PLACEHOLDER).unwrap();
        writeln!(
            code,
            "    (\"index.html\", include_bytes!({:?})),",
            placeholder.display().to_string()
        )
        .unwrap();
    }
    for (name, path) in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        writeln!(
            code,
            "    ({name:?}, include_bytes!({:?})),",
            fs::canonicalize(path).unwrap().display().to_string()
        )
        .unwrap();
    }
    code.push_str("];\n");
    fs::write(out.join("ide_assets.rs"), code).unwrap();
}

/// Every file under `dir`, named by its path from `root` with `/`.
fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(root, &path, files);
        } else if path.extension().is_none_or(|ext| ext != "map") {
            let name = path
                .strip_prefix(root)
                .unwrap()
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            files.push((name, path));
        }
    }
}
