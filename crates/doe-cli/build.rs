//! Embeds the web IDE (`web/dist`, made by `npm run build` in `web/`) in the
//! binary when the `ide` feature is on. The directory can be named with
//! `DOE_IDE_DIST`. When it is not there, a page that says so is embedded
//! instead, so that the crate still builds without the web toolchain (a build
//! that must have the page, such as a release, sets `DOE_IDE_REQUIRE_BUNDLE`
//! and fails instead).

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
    println!("cargo:rerun-if-env-changed=DOE_IDE_REQUIRE_BUNDLE");
    println!("cargo:rerun-if-changed=build.rs");
    bundle_types();
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
    assert!(
        !files.is_empty() || env::var_os("DOE_IDE_REQUIRE_BUNDLE").is_none(),
        "DOE_IDE_REQUIRE_BUNDLE is set, but {} has no index.html: build the web IDE first (npm run build in web/)",
        dist.display()
    );

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
    writeln!(
        code,
        "/// Whether the page of the web IDE is in this binary.\npub(crate) const BUNDLED: bool = {};",
        !files.is_empty()
    )
    .unwrap();
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

/// Embeds the type library (`types/*.yaml`) and the Kaitai Struct gallery
/// (the `.ksy` files of `conformance/kaitai_struct_formats`) for
/// `doe --install-types`, when the `bundled-types` feature is on.
///
/// A published crate cannot reach the files outside its directory, so
/// `bundled/` (made by `scripts/bundle-types`, and listed in the package) is
/// used first, with the library in `bundled/doe` and the gallery in
/// `bundled/kaitai`. In a checkout they are read where they are. When neither is
/// there nothing is embedded, unless `DOE_TYPES_REQUIRE_BUNDLE` is set (a
/// release sets it), which makes that an error.
fn bundle_types() {
    println!("cargo:rerun-if-env-changed=DOE_TYPES_REQUIRE_BUNDLE");
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    if env::var_os("CARGO_FEATURE_BUNDLED_TYPES").is_some() {
        let packaged = manifest.join("bundled");
        println!("cargo:rerun-if-changed={}", packaged.display());
        let (library, gallery) = if packaged.join("doe").is_dir() {
            (packaged.join("doe"), packaged.join("kaitai"))
        } else {
            (
                manifest.join("../../types"),
                manifest.join("../../conformance/kaitai_struct_formats"),
            )
        };
        for (prefix, dir, extension) in [("doe", library, "yaml"), ("kaitai", gallery, "ksy")] {
            println!("cargo:rerun-if-changed={}", dir.display());
            if dir.is_dir() {
                collect_types(&dir, &dir, prefix, extension, &mut files);
            }
        }
    }
    files.sort();
    assert!(
        !files.is_empty() || env::var_os("DOE_TYPES_REQUIRE_BUNDLE").is_none(),
        "DOE_TYPES_REQUIRE_BUNDLE is set, but there are no types to embed: check out the submodules (git submodule update --init) or run scripts/bundle-types"
    );
    let mut code = String::from("pub(crate) static TYPES: &[(&str, &[u8])] = &[\n");
    for (name, path) in &files {
        writeln!(
            code,
            "    ({name:?}, include_bytes!({:?})),",
            fs::canonicalize(path).unwrap().display().to_string()
        )
        .unwrap();
    }
    code.push_str("];\n");
    fs::write(out.join("types_bundle.rs"), code).unwrap();
}

/// The files of a type library below `dir` (`*.yaml` directly in it, the
/// gallery's `*.ksy` at any depth, and the gallery's README), named
/// `prefix/path`.
fn collect_types(
    root: &Path,
    dir: &Path,
    prefix: &str,
    extension: &str,
    files: &mut Vec<(String, PathBuf)>,
) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.is_dir() {
            // the gallery's own tooling is not types
            if extension == "ksy" && !name.starts_with('.') && name != "_build" {
                collect_types(root, &path, prefix, extension, files);
            }
        } else if path.extension().is_some_and(|e| e == extension)
            || (extension == "ksy" && dir == root && name == "README.md")
        {
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            println!("cargo:rerun-if-changed={}", path.display());
            files.push((format!("{prefix}/{relative}"), path));
        }
    }
}
