// Runs a WebAssembly test binary built for wasm32-wasip1, with Node's WASI.
//
//   CARGO_TARGET_WASM32_WASIP1_RUNNER="node scripts/wasm-runner.mjs" \
//     cargo test -p doe --test limits --target wasm32-wasip1 --no-default-features
//
// Cargo calls it with the path of the binary and the arguments of the test.
import { readFile } from 'node:fs/promises';
import { WASI } from 'node:wasi';

const [file, ...args] = process.argv.slice(2);
const wasi = new WASI({
  version: 'preview1',
  args: [file, ...args],
  env: process.env,
  returnOnExit: true,
  // The tests need no files; the working directory is there for those that do.
  preopens: { '.': '.' },
});
const module = await WebAssembly.compile(await readFile(file));
const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
process.exitCode = wasi.start(instance);
