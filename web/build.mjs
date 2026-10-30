// Builds the web IDE into dist/: the WebAssembly module (crates/doe-wasm), its
// JavaScript glue, and the page.
//
//   npm run build            everything
//   npm run build:js         only the page (the module in src/wasm is reused)
//
// Needs the Rust target wasm32-unknown-unknown and wasm-bindgen-cli at the
// version of the wasm-bindgen crate (see README.md).
import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, posix } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { build } from 'esbuild';
import { buildHelp } from './help.mjs';

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
// The help: the web IDE and the schema sections of the README, made here so that
// the page and the README of the same commit cannot get out of step (the build
// fails when a section is missing, see help.mjs).
writeFileSync(join(dist, 'help.html'), buildHelp(readFileSync(join(root, 'README.md'), 'utf8')).html);

// The gallery picker's list: first the types of doe's own library (types/),
// then the formats of the Kaitai Struct gallery (the submodule), with the files
// that each one imports, and the commit the gallery is at.
// The library's definitions are copied to dist/library/ (they are small and
// ours): the page fetches them from its own origin. The gallery's are not
// copied: the page fetches them at that commit when one is picked (some of them
// are not CC0).
const gallery = join(root, 'conformance', 'kaitai_struct_formats');
const manifest = { repo: 'kaitai-io/kaitai_struct_formats', commit: '', formats: [] };
const libraryText = new Map();
{
  const types = join(root, 'types');
  mkdirSync(join(dist, 'library'), { recursive: true });
  for (const name of readdirSync(types).filter((n) => n.endsWith('.yaml')).sort()) {
    const source = readFileSync(join(types, name), 'utf8');
    const id = /^id:\s*([A-Za-z0-9_]+)/m.exec(source)?.[1];
    if (!id) continue;
    // the first line of `doc:`, whether it is a block (`doc: |`) or a line
    const title = /^doc:\s*[|>]?[-+]?\s*(\S[^\n]*)/m.exec(source)?.[1]?.trim() ?? null;
    const path = `library/${name}`;
    libraryText.set(path, source);
    writeFileSync(join(dist, path), source);
    // every type has a sample file, made by types/make_samples.py, that the tests parse
    manifest.formats.push({ path, id, title, imports: [], bundled: true, verified: 'crafted' });
  }
  console.log(`gallery.json: ${libraryText.size} types of the doe library`);
}
// a file of the library, or of the gallery
const text = (path) => libraryText.get(path) ?? readFileSync(join(gallery, path), 'utf8');
if (existsSync(join(gallery, 'README.md'))) {
  manifest.commit = execFileSync('git', ['-C', gallery, 'rev-parse', 'HEAD']).toString().trim();
  const files = new Set();
  const walk = (dir) => {
    for (const entry of readdirSync(join(gallery, dir), { withFileTypes: true })) {
      const path = posix.join(dir, entry.name);
      if (entry.isDirectory()) {
        if (!entry.name.startsWith('.') && !entry.name.startsWith('_')) walk(path);
      } else if (entry.name.endsWith('.ksy')) files.add(path);
    }
  };
  walk('.');
  // `imports:` is a list of names: relative to the file, or to the gallery with a leading slash.
  const importsOf = (path) => {
    const lines = text(path).split(/\r?\n/);
    const start = lines.findIndex((line) => /^\s*imports:\s*$/.test(line));
    const names = [];
    if (start >= 0) {
      const indent = /^\s*/.exec(lines[start])[0].length;
      for (const line of lines.slice(start + 1)) {
        const item = /^(\s*)-\s+['"]?([^'"#\s]+)['"]?/.exec(line);
        if (item) names.push(item[2]);
        else if (line.trim() !== '' && !line.trim().startsWith('#') && /^\s*/.exec(line)[0].length <= indent) break;
      }
    }
    return names.map((name) => (name.startsWith('/') ? name.slice(1) : posix.normalize(posix.join(posix.dirname(path), name))) + '.ksy');
  };
  const closure = (path, seen = new Set()) => {
    for (const next of importsOf(path)) {
      if (!seen.has(next) && files.has(next)) {
        seen.add(next);
        closure(next, seen);
      }
    }
    return seen;
  };
  for (const path of [...files].sort()) {
    const source = text(path);
    const id = /^\s*id:\s*([A-Za-z0-9_]+)/m.exec(source.slice(source.indexOf('meta:')))?.[1];
    const title = /^\s*title:\s*(.+?)\s*$/m.exec(source)?.[1]?.replace(/^['"]|['"]$/g, '');
    if (!id || !source.includes('meta:')) continue;
    manifest.formats.push({ path, id, title: title ?? null, imports: [...closure(path)] });
  }
  // Which formats have been tried on a file (conformance/gallery_samples.tsv),
  // so that the picker can say so.
  const table = join(root, 'conformance', 'gallery_samples.tsv');
  if (existsSync(table)) {
    const verified = new Map();
    for (const line of readFileSync(table, 'utf8').split(/\r?\n/).slice(1)) {
      const [name, status] = line.split('\t');
      if (name && (status === 'real' || status === 'crafted')) verified.set(`${name}.ksy`, status);
    }
    for (const format of manifest.formats) {
      const status = verified.get(format.path);
      if (status) format.verified = status;
    }
  }
} else {
  console.warn('gallery.json: the submodule conformance/kaitai_struct_formats is not checked out; the picker will be empty');
}
// What each format's files start with, and what it is called, to find the
// schema of a file without fetching every definition: asked of the module
// that was just built, so the page and the library agree.
const wasm = await import(pathToFileURL(join(web, 'src', 'wasm', 'doe_wasm.js')).href);
wasm.initSync({ module: readFileSync(join(web, 'src', 'wasm', 'doe_wasm_bg.wasm')) });
let withSignature = 0;
for (const format of manifest.formats) {
  const files = Object.fromEntries(format.imports.map((path) => [path, text(path)]));
  const answer = JSON.parse(wasm.signature(JSON.stringify({ schema: text(format.path), files, path: format.path, include_paths: [''] })));
  format.names = answer.ok ? [...new Set(answer.names)] : [format.id.toLowerCase()];
  if (answer.ok && answer.magic.length > 0) {
    format.magic = answer.magic;
    withSignature += 1;
  }
}
console.log(`gallery.json: ${withSignature} of ${manifest.formats.length} formats have a signature`);
writeFileSync(join(dist, 'gallery.json'), JSON.stringify(manifest));
console.log(`gallery.json: ${manifest.formats.length} formats at ${manifest.commit.slice(0, 12) || '(none)'}`);
