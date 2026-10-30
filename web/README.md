# The web IDE

The hosted web IDE is at <https://shroomsword.github.io/doe/>.

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
npx playwright install --with-deps chromium firefox   # once
npm test                                       # the tests, in headless Chromium and Firefox
```

`tests/smoke.spec.ts` loads the page and calls the module with a schema and a
binary: the value, the ranges of its parts, the diagnostics with their positions,
and a parse that fails. The Rust side of the same calls is tested without a
browser in `crates/doe-wasm/tests/api.rs`; the shape of the JSON is in
`crates/doe-wasm/API.md`.

`--with-deps` installs the system libraries that the browsers need and needs root.
Without it, the libraries have to be on `LD_LIBRARY_PATH`, and Playwright's check
of the host skipped with `PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=1`.

## Using it

The editor and the file are empty at first (open a file, open or type a schema, or pick a type from the gallery). Four panes stay in step:

- **Schema**: an editor (CodeMirror 6, YAML) for a doe schema or a Kaitai Struct
  `.ksy`. What is wrong with it is underlined where it is, and the schema is
  checked again a moment after the last change. *Open schema* loads a file into
  it; *Add imports* adds files that it imports (they are found next to the schema,
  and in the include paths as Kaitai Struct does); a dropped `.ksy` or `.yaml` is
  an import.
- **Parsed**: the value as a tree, each field with its type, its value, its enum
  name and the bytes it was read from; instances are in italics. When a parse
  fails, the tree is what was read until then, and the node where it stopped is
  marked and open.
- **Bytes**: the input, 16 to a row. Only the rows in view are made, so a file of
  hundreds of megabytes scrolls (for a very long file one pixel of the scroll bar
  is many rows).
- **Problems**: what is wrong with the schema (with its line and column; a click
  goes there) and why a parse failed (a click selects the node).

A click on a field of the tree marks its bytes, and a click on a byte selects the
innermost field that holds it and opens the way to it; the arrow keys move in the
tree. *Open file* (or a dropped file that is not a schema) is the input. A file of
more than 256 MiB is refused, as WebAssembly has 4 GiB of memory at most; the
local version (`doe --ide FILE`) has no such limit: it reads the file itself. Its tests (`tests/local.spec.ts`) run against `target/debug/doe`, so build the page first and then `cargo build -p doe-cli`.

Everything runs in the page; nothing is sent anywhere. The checks run in a Web
Worker, so the editor stays responsive. Each edit starts a new run and drops the
one before; a run that takes longer than 5 s, or that **Cancel parse** ends, is
stopped by replacing the worker, and the problems pane says so. A worker that
runs out of memory is reported the same way. For the tests, `?limit=<ms>` sets
the time limit.

## Layout

| Path | What |
| --- | --- |
| `src/doe.ts` | Typed wrapper over the module: `loadDoe(url)` gives `check`, `parse`, `version` |
| `src/share.ts` | The schema in a link (`#s=`, deflate and base64url) and in `localStorage` |
| `src/local.ts` | The calls of `doe --ide`: its types and the input file that stays on the machine |
| `src/worker.ts`, `src/client.ts` | The worker that holds the module, and the page's calls to it with time limit and cancel |
| `src/main.ts`, `src/app.ts` | The page, and the app that keeps the panes in step |
| `src/editor.ts`, `src/tree.ts`, `src/hex.ts`, `src/problems.ts` | The four panes |
| `src/model.ts` | The logic that needs no DOM: nodes and bytes, the size limit, positions |
| `src/style.css` | The styles |
| `src/wasm/` | The module and its glue, made by the build (not committed) |
| `build.mjs`, `serve.mjs` | The build, and a static server for `dist/` |
| `tests/` | Playwright tests: `model.spec.ts` (no browser), `smoke.spec.ts` (the module), `ide.spec.ts` (the page); `fixture.ts` is the small schema and file that the page tests load, since the page starts empty |

## Publishing

Every push to `main` builds `web/dist` in CI and publishes it to GitHub Pages
(`.github/workflows/pages.yml`; the Pages source of the repository must be
"GitHub Actions"). The release workflow builds it too and puts it in each `doe`
binary (`crates/doe-cli/build.rs` embeds `web/dist`). The steps that build it are
in `.github/actions/build-web`.
