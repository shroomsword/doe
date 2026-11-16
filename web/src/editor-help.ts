// Help while a schema is written: completion of keys, types and names, a tooltip on what is under the pointer, and go to
// definition. The keys and the built-in types come from the compiler (`language`); the names come from the text itself,
// read by `schema-text.ts`, so all of it works while the schema has a mistake.

import { type Completion, type CompletionContext, type CompletionResult, autocompletion } from '@codemirror/autocomplete';
import { type Extension } from '@codemirror/state';
import { EditorView, type Tooltip, hoverTooltip, keymap } from '@codemirror/view';
import type { InspectResponse, LanguageResponse } from './doe.js';
import {
  type Node,
  type Outline,
  contextAt,
  definedTypes,
  docOf,
  enclosingType,
  enumsInScope,
  fieldsInScope,
  importsOf,
  pathOf,
  placeOf,
  readOutline,
} from './schema-text.js';

/** What the help needs to know of the page. */
export interface HelpSources {
  /** The language, once it has been asked for. */
  language(): LanguageResponse | null;
  /** What `inspect` last said of the schemas (or null). */
  found(): InspectResponse | null;
  /** The texts of the files of the page, by name (the open schema is not necessarily among them). */
  files(): ReadonlyMap<string, string>;
  /** The names that can be imported. */
  importable(): string[];
  /** Show where a definition is: in the open schema (path null), or in another file of the page. */
  show(path: string | null, from: number, to: number): void;
  /** Say that a definition was not found. */
  say(message: string): void;
}

/** The names that an expression has besides the fields. */
const SPECIAL = ['_parent', '_root', '_io', '_index', '_'];

/** The keys whose value is an expression. */
const EXPRESSIONS = new Set(['size', 'repeat-expr', 'repeat-until', 'if', 'pos', 'value']);

/** The keys whose value is free text, where nothing is offered. */
const TEXT = new Set(['doc', 'contents', 'id', 'doc-ref', 'title', 'license', 'ks-version', 'terminator']);

/** The last part of an imported name: `a/b/c` is `c`. */
function importAlias(name: string): string {
  return (name.split('/').pop() ?? name).replace(/\.ksy$/, '');
}

/** The text of a file of the page that an import names. */
function fileOf(name: string, files: ReadonlyMap<string, string>): [string, string] | null {
  for (const [path, text] of files) {
    const bare = path.replace(/\.ksy$/, '');
    if (path === name || bare === name || bare.endsWith(`/${name}`) || importAlias(path) === importAlias(name)) return [path, text];
  }
  return null;
}

/** An imported type: how it is written in a `type:`, and where it is defined. */
interface Imported {
  name: string;
  /** The file of the page that has it, when the page has it. */
  path: string | null;
  from: number;
  to: number;
  doc: string | null;
}

/** The types of the schemas that the text imports, as they are written after `type:`. */
function importedTypes(outline: Outline, text: string, sources: HelpSources): Imported[] {
  const out: Imported[] = [];
  const seen = new Set<string>();
  const files = sources.files();
  for (const imported of importsOf(outline)) {
    const name = (imported.value ?? '').replace(/^["']|["']$/g, '');
    if (!name) continue;
    const file = fileOf(name, files);
    if (file) {
      const other = readOutline(file[1]);
      const id =
        other.roots.find((n) => n.key === 'id')?.value ??
        other.roots.find((n) => n.key === 'meta')?.children.find((n) => n.key === 'id')?.value ??
        importAlias(name);
      if (!seen.has(id)) {
        seen.add(id);
        out.push({ name: id, path: file[0], from: 0, to: 0, doc: null });
      }
      for (const type of definedTypes(other)) {
        const written = `${id}::${type.key}`;
        if (seen.has(written)) continue;
        seen.add(written);
        out.push({ name: written, path: file[0], from: type.from, to: type.to, doc: docOf(type, file[1]) });
      }
    }
  }
  for (const info of sources.found()?.schemas ?? []) {
    for (const written of info.types) {
      if (seen.has(written) || !info.imports.length) continue;
      // only what the text itself imports: the types of other schemas that the page happens to have are not in scope
      const id = written.split('::')[0] ?? '';
      if (!importsOf(outline).some((n) => importAlias(n.value ?? '') === importAlias(info.path) || importAlias(n.value ?? '') === id)) continue;
      seen.add(written);
      out.push({ name: written, path: null, from: 0, to: 0, doc: null });
    }
  }
  void text;
  return out;
}

function complete(context: CompletionContext, sources: HelpSources): CompletionResult | null {
  const language = sources.language();
  if (!language) return null;
  const text = context.state.doc.toString();
  const outline = readOutline(text);
  const at = contextAt(text, context.pos, outline);
  // nothing in a comment
  const lineBefore = context.state.doc.lineAt(context.pos).text.slice(0, context.pos - context.state.doc.lineAt(context.pos).from);
  if (/(^|\s)#/.test(lineBefore)) return null;
  const explicit = context.explicit;
  const wordless = at.prefix === '';
  // typing a new line does not open a list: it opens when a word is begun, or when it is asked for (Ctrl-Space)
  if (wordless && !explicit) return null;
  const options: Completion[] = [];
  const basic = (label: string, detail: string, type: string, apply?: string): void => {
    options.push({ label, detail, type, apply: apply ?? label });
  };
  if (at.mode === 'key') {
    // a list of imports: the names that can be imported
    if (at.parent?.kind === 'item' && at.parent.parent?.key === 'imports') {
      for (const name of sources.importable()) basic(name, 'schema', 'namespace');
    } else if (at.place) {
      const keys = language.places[at.place] ?? [];
      const have = new Set(at.present);
      for (const key of keys) {
        if (key.hidden || have.has(key.key)) continue;
        options.push({ label: key.key, detail: key.doc, type: 'property', apply: `${key.key}: ` });
      }
    }
  } else {
    const key = at.key;
    if (TEXT.has(key)) return null;
    const node = at.node;
    if (key === 'type') {
      for (const type of language.types) basic(type.name, type.doc, 'type');
      for (const type of definedTypes(outline)) {
        basic(type.key, docOf(type, text) ?? 'type', 'class');
      }
      for (const imported of importedTypes(outline, text, sources)) basic(imported.name, imported.doc ?? 'imported type', 'class');
    } else if (key === 'enum') {
      for (const entry of enumsInScope(node, outline)) basic(entry.key, 'enum', 'enum');
    } else if (language.values[key]) {
      for (const value of language.values[key] ?? []) basic(value.name, value.doc, 'constant');
    } else if (EXPRESSIONS.has(key) || key === 'valid') {
      const dot = /([A-Za-z_][\w]*)\.\w*$/.exec(lineBefore);
      if (!dot) {
        for (const id of fieldsInScope(node, outline)) basic(id, 'field', 'variable');
        for (const name of SPECIAL) basic(name, 'special', 'keyword');
      }
    } else {
      return null;
    }
  }
  if (options.length === 0) return null;
  return { from: at.from, options, validFor: /^[\w:.-]*$/ };
}

/** What to say of a built-in key, type or value at a word. */
function describeWord(word: string, language: LanguageResponse, place: string | null, key: string | null): string | null {
  if (place) {
    const found = language.places[place]?.find((k) => k.key === word);
    if (found) return `${found.key}: ${found.doc}`;
  }
  if (key && language.values[key]) {
    const value = language.values[key]?.find((v) => v.name === word);
    if (value) return `${value.name}: ${value.doc}`;
  }
  return null;
}

/** A line about a definition in the text: what it is, and the first line of its `doc`. */
function summary(node: Node, text: string, what: string): string {
  const doc = docOf(node, text);
  const seq = node.children.find((c) => c.key === 'seq');
  const detail = seq ? ` (${seq.children.length} fields)` : '';
  return `${what} ${node.key}${detail}${doc ? `: ${doc}` : ''}`;
}

/** The word of an expression or a name at an offset (`a`, `a::b`, `-x`), with where it is. */
function wordAt(text: string, pos: number): { word: string; from: number; to: number } | null {
  let from = pos;
  let to = pos;
  const isWord = (c: string | undefined): boolean => c !== undefined && /[\w:-]/.test(c);
  while (from > 0 && isWord(text[from - 1])) from -= 1;
  while (to < text.length && isWord(text[to])) to += 1;
  if (from === to) return null;
  return { word: text.slice(from, to), from, to };
}

interface Found {
  /** What the word is. */
  kind: 'key' | 'builtin' | 'type' | 'enum' | 'field' | 'import' | 'value';
  text: string;
  /** Where its definition is (null: there is none to go to). */
  target: { path: string | null; from: number; to: number } | null;
  word: { word: string; from: number; to: number };
}

/** Find what is under a position: used by the tooltip and by go to definition. */
function lookup(text: string, pos: number, sources: HelpSources): Found | null {
  const language = sources.language();
  const word = wordAt(text, pos);
  if (!language || !word) return null;
  const outline = readOutline(text);
  const line = text.slice(text.lastIndexOf('\n', pos - 1) + 1, text.indexOf('\n', pos) === -1 ? text.length : text.indexOf('\n', pos));
  const lineNumber = text.slice(0, pos).split('\n').length;
  const node = outline.nodes.find((n) => n.line === lineNumber && n.kind === 'key') ?? null;
  if (/^\s*#/.test(line)) return null;
  const onKey = node !== null && pos >= node.from && pos <= node.to;
  if (onKey && node) {
    const info = describeWord(node.key, language, placeOf(pathOf(node.parent)), null);
    if (info) return { kind: 'key', text: info, target: null, word };
    return null;
  }
  if (node && node.value !== null && pos >= node.valueFrom && pos <= node.valueTo) {
    const key = node.key;
    const name = word.word;
    if (key === 'type') {
      const builtin = language.types.find((t) => t.name === name);
      if (builtin) return { kind: 'builtin', text: `${builtin.name}: ${builtin.doc}`, target: null, word };
      const local = definedTypes(outline).find((t) => t.key === name);
      if (local) return { kind: 'type', text: summary(local, text, 'type'), target: { path: null, from: local.from, to: local.to }, word };
      const imported = importedTypes(outline, text, sources).find((t) => t.name === name);
      if (imported) {
        const target = imported.path ? { path: imported.path, from: imported.from, to: imported.to } : null;
        return { kind: 'type', text: `imported type ${imported.name}${imported.doc ? `: ${imported.doc}` : ''}`, target, word };
      }
      return null;
    }
    if (key === 'enum') {
      const entry = enumsInScope(node, outline).find((e) => e.key === name);
      if (entry) return { kind: 'enum', text: `enum ${entry.key}`, target: { path: null, from: entry.from, to: entry.to }, word };
      return null;
    }
    const value = language.values[key]?.find((v) => v.name === name);
    if (value) return { kind: 'value', text: `${value.name}: ${value.doc}`, target: null, word };
    if (EXPRESSIONS.has(key) || key === 'valid') {
      const seq = fieldsInScope(node, outline);
      if (seq.includes(name)) {
        const target = findField(outline, node, name);
        return { kind: 'field', text: `field ${name}`, target: target ? { path: null, from: target.from, to: target.to } : null, word };
      }
    }
  }
  // an item of `imports:`
  const item = outline.nodes.find((n) => n.kind === 'item' && n.line === lineNumber && n.value !== null);
  if (item && item.parent?.key === 'imports') {
    const name = (item.value ?? '').replace(/^["']|["']$/g, '');
    const file = fileOf(name, sources.files());
    if (file) {
      const types = definedTypes(readOutline(file[1])).map((t) => t.key);
      return { kind: 'import', text: `${file[0]}${types.length ? ` · types: ${types.join(', ')}` : ''}`, target: { path: file[0], from: 0, to: 0 }, word };
    }
    return { kind: 'import', text: `${name} (not among the files of the page)`, target: null, word };
  }
  return null;
}

/** The node of the field called `name` in the type that has `from`. */
function findField(outline: Outline, from: Node, name: string): Node | null {
  const type = enclosingType(from);
  const scope = type ? type.children : outline.roots;
  for (const holder of scope) {
    if (holder.key === 'seq') {
      for (const item of holder.children) {
        const id = item.children.find((c) => c.key === 'id');
        if (id?.value === name) return id;
      }
    } else if (holder.key === 'instances') {
      const instance = holder.children.find((c) => c.key === name);
      if (instance) return instance;
    } else if (holder.key === 'params') {
      for (const item of holder.children) {
        const id = item.children.find((c) => c.key === 'id');
        if (id?.value === name) return id;
      }
    }
  }
  return null;
}

/** Go to the definition of what is at the cursor. */
function goToDefinition(view: EditorView, sources: HelpSources): boolean {
  const pos = view.state.selection.main.head;
  return goTo(view, pos, sources);
}

function goTo(view: EditorView, pos: number, sources: HelpSources): boolean {
  const found = lookup(view.state.doc.toString(), pos, sources);
  if (!found) {
    sources.say('no definition here');
    return true;
  }
  if (!found.target) {
    sources.say(found.kind === 'builtin' ? `${found.word.word} is built in: it has no definition in a schema` : `no definition for ${found.word.word}`);
    return true;
  }
  sources.show(found.target.path, found.target.from, found.target.to);
  return true;
}

/** The extensions of the help for the editor of the schema. */
export function schemaHelp(sources: HelpSources): Extension {
  return [
    autocompletion({
      override: [(context) => complete(context, sources)],
      activateOnTyping: true,
      icons: false,
    }),
    hoverTooltip((view, pos): Tooltip | null => {
      const found = lookup(view.state.doc.toString(), pos, sources);
      if (!found) return null;
      return {
        pos: found.word.from,
        end: found.word.to,
        above: true,
        create: () => {
          const dom = document.createElement('div');
          dom.className = 'doe-help';
          dom.textContent = found.text;
          return { dom };
        },
      };
    }),
    keymap.of([
      { key: 'F12', run: (view) => goToDefinition(view, sources) },
      {
        key: 'F1',
        run: (view) => {
          const found = lookup(view.state.doc.toString(), view.state.selection.main.head, sources);
          sources.say(found ? found.text : 'nothing to say here');
          return true;
        },
      },
    ]),
    EditorView.domEventHandlers({
      mousedown(event, view) {
        if (!(event.ctrlKey || event.metaKey) || event.button !== 0) return false;
        const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
        if (pos === null) return false;
        event.preventDefault();
        return goTo(view, pos, sources);
      },
    }),
  ];
}
