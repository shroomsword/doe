// Generates conformance/expected/<format>/<sample>.json with the reference
// Kaitai Struct compiler: each formats/<format>.ksy is compiled to JavaScript,
// every samples/<format>/*.bin is parsed with the result, and the parsed
// object is written out in the canonical form that the Rust test compares
// with doe's output (see conformance/README.md).
//
//   npm ci && npm run generate

'use strict';

const fs = require('fs');
const path = require('path');
const YAML = require('yaml');
const compiler = require('kaitai-struct-compiler');

const root = __dirname;
const formatsDir = path.join(root, 'formats');
const samplesDir = path.join(root, 'samples');
const expectedDir = path.join(root, 'expected');
const generatedDir = path.join(root, '.generated');

// Keys are compared without case and underscores, because the JavaScript
// runtime renames `file_size` to `fileSize`.
const normalizeKey = (key) => key.replace(/_/g, '').toLowerCase();

// The parameters of each type, by the name of its class: the runtime exposes
// them as properties, but they are not part of what a type reads.
function collectParams(ksy, out = new Map()) {
  const visit = (id, type) => {
    out.set(className(id), new Set((type.params || []).map((p) => normalizeKey(p.id))));
    for (const [childId, child] of Object.entries(type.types || {})) visit(childId, child);
  };
  visit(ksy.meta.id, ksy);
  return out;
}

function dump(value, params) {
  if (value === undefined || value === null) return null;
  if (typeof value === 'bigint') {
    return Number.isSafeInteger(Number(value)) ? Number(value) : value.toString();
  }
  if (typeof value === 'number' || typeof value === 'boolean' || typeof value === 'string') {
    return value;
  }
  if (value instanceof Uint8Array) return Buffer.from(value).toString('hex');
  if (Array.isArray(value)) return value.map((item) => dump(item, params));
  if (typeof value === 'object') {
    const names = new Set(Object.keys(value));
    // Instances are getters on the prototype.
    const proto = Object.getPrototypeOf(value);
    for (const name of Object.getOwnPropertyNames(proto)) {
      const descriptor = Object.getOwnPropertyDescriptor(proto, name);
      if (descriptor && descriptor.get) names.add(name);
    }
    const skip = params.get(value.constructor.name) || new Set();
    const out = {};
    for (const name of [...names].sort()) {
      if (name.startsWith('_') || name === 'constructor' || skip.has(normalizeKey(name))) continue;
      out[normalizeKey(name)] = dump(value[name], params);
    }
    return out;
  }
  throw new Error(`cannot dump a ${typeof value}`);
}

const galleryPrefix = 'gallery';

// The names of the formats under `dir`: the path without `.ksy`, with `/`.
function listFormats(dir, prefix) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    if (entry.isDirectory()) return listFormats(path.join(dir, entry.name), `${prefix}${entry.name}/`);
    return entry.name.endsWith('.ksy') ? [`${prefix}${entry.name.slice(0, -4)}`] : [];
  });
}

function className(id) {
  return id
    .split('_')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join('');
}

async function main() {
  fs.rmSync(generatedDir, { recursive: true, force: true });
  fs.mkdirSync(generatedDir, { recursive: true });
  const only = process.argv[2];
  const formats = listFormats(formatsDir, '')
    .filter((name) => !only || name === only)
    .sort();
  if (formats.length === 0) throw new Error('no formats found');

  for (const name of formats) {
    const dir = path.join(samplesDir, name);
    const hasSamples = fs.existsSync(dir);
    // A gallery definition without samples is only there for others to import.
    if (!hasSamples && name.startsWith(`${galleryPrefix}/`) && !only) continue;
    const ksy = YAML.parse(fs.readFileSync(path.join(formatsDir, `${name}.ksy`), 'utf8'));
    // The gallery definitions import each other the way the gallery does: a
    // name with a leading slash from the root of the gallery, any other next
    // to the definition.
    const inGallery = name.startsWith(`${galleryPrefix}/`);
    const importRoot = inGallery ? path.join(formatsDir, galleryPrefix) : formatsDir;
    const here = path.dirname(path.join(formatsDir, `${name}.ksy`));
    const importer = {
      importYaml: (importName) => {
        const candidates = [
          path.join(importRoot, `${importName}.ksy`),
          path.join(here, `${importName}.ksy`),
        ];
        const found = candidates.find((candidate) => fs.existsSync(candidate));
        if (!found) throw new Error(`cannot find the import ${importName} of ${name}`);
        return Promise.resolve(YAML.parse(fs.readFileSync(found, 'utf8')));
      },
    };
    const files = await compiler.compile('javascript', ksy, importer, false);
    for (const [file, code] of Object.entries(files)) {
      fs.writeFileSync(path.join(generatedDir, file), code);
    }
    const Parser = require(path.join(generatedDir, `${className(ksy.meta.id)}.js`))[
      className(ksy.meta.id)
    ];
    const KaitaiStream = require('kaitai-struct/KaitaiStream');

    const params = collectParams(ksy);
    const samples = hasSamples
      ? fs.readdirSync(dir).filter((f) => f.endsWith('.bin')).sort()
      : [];
    if (samples.length === 0) throw new Error(`${name} has no samples`);
    fs.mkdirSync(path.join(expectedDir, name), { recursive: true });
    for (const sample of samples) {
      const parsed = new Parser(new KaitaiStream(fs.readFileSync(path.join(dir, sample))));
      const json = JSON.stringify(dump(parsed, params), null, 2) + '\n';
      fs.writeFileSync(path.join(expectedDir, name, sample.replace(/\.bin$/, '.json')), json);
      console.log(`${name}/${sample}`);
    }
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
