// Builds the web IDE into dist/: the WebAssembly module (crates/doe-wasm), its
// JavaScript glue, and the page.
//
//   npm run build            everything
//   npm run build:js         only the page (the module in src/wasm is reused)
//
// Needs the Rust target wasm32-unknown-unknown and wasm-bindgen-cli at the
// version of the wasm-bindgen crate (see README.md).
import { execFileSync } from 'node:child_process';
import { cpSync, mkdirSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';

const web = dirname(fileURLToPath(import.meta.url));
const root = dirname(web);
const dist = join(web, 'dist');
const run = (command, args) => execFileSync(command, args, { cwd: root, stdio: 'inherit' });

if (!process.argv.includes('--js-only')) {
  // Optimised: an unoptimised build overflows the stack of WebAssembly (1 MiB)
  // on the deepest schemas the YAML reader allows.
  run('cargo', ['build', '-p', 'doe-wasm', '--target', 'wasm32-unknown-unknown', '--profile', 'web', '--locked']);
  rmSync(join(web, 'src', 'wasm'), { recursive: true, force: true });
  run('wasm-bindgen', [
    '--target', 'web',
    '--out-dir', join(web, 'src', 'wasm'),
    '--out-name', 'doe_wasm',
    join(root, 'target', 'wasm32-unknown-unknown', 'web', 'doe_wasm.wasm'),
  ]);
}

rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });
await build({
  entryPoints: [join(web, 'src', 'main.ts'), join(web, 'src', 'worker.ts')],
  outdir: dist,
  bundle: true,
  format: 'esm',
  target: 'es2022',
  minify: true,
  sourcemap: true,
  logLevel: 'info',
});
cpSync(join(web, 'src', 'wasm', 'doe_wasm_bg.wasm'), join(dist, 'doe_wasm_bg.wasm'));
cpSync(join(web, 'index.html'), join(dist, 'index.html'));
cpSync(join(web, 'src', 'style.css'), join(dist, 'style.css'));
