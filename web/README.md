# The web IDE

A static site that runs doe in the browser: the library, built for
WebAssembly (`crates/doe-wasm`), and a page that calls it. It is the front end
of the web IDE (see issue #2); this directory has the build and a first page,
and the panes come next.

## Build

Needs Node.js 20 or later, Rust with the target `wasm32-unknown-unknown`, and
`wasm-bindgen-cli` **at the version of the `wasm-bindgen` crate** (`=0.2.129`
in `crates/doe-wasm/Cargo.toml`; the glue it makes only works with that one):

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
cd web
npm ci
npm run build        # dist/: index.html, main.js, doe_wasm_bg.wasm
npm run serve        # http://127.0.0.1:4173/
```

`npm run build` builds the module with `cargo build -p doe-wasm --profile web`
(optimised for size; an unoptimised build overflows the 1 MiB stack of
WebAssembly on the deepest schemas the YAML reader allows), makes the glue with
`wasm-bindgen --target web`, and bundles `src/main.ts` with esbuild.
`npm run build:js` redoes only the page.

## Test

```sh
npm run typecheck                              # after a build: it reads the glue the build makes
npx playwright install --with-deps chromium    # once
npm test                                       # a headless Chromium calls the module
```

`tests/smoke.spec.ts` loads the page and calls the module with a schema and a
binary: the value, the ranges of its parts, the diagnostics with their positions,
and a parse that fails. The Rust side of the same calls is tested without a
browser in `crates/doe-wasm/tests/api.rs`; the shape of the JSON is in
`crates/doe-wasm/API.md`.

`--with-deps` installs the system libraries that Chromium needs and needs root.
Without it, the libraries have to be on `LD_LIBRARY_PATH`.

## Layout

| Path | What |
| --- | --- |
| `src/doe.ts` | Typed wrapper over the module: `loadDoe(url)` gives `check`, `parse`, `version` |
| `src/main.ts` | The page: an editor for a schema and what `check` says about it |
| `src/wasm/` | The module and its glue, made by the build (not committed) |
| `build.mjs`, `serve.mjs` | The build, and a static server for `dist/` |
| `tests/` | Playwright tests |
