// Copy a field as its path, value, JSON, bytes, place or schema line. The text of each format is made here, from the trace
// and the parsed value; the page decides where it goes (the clipboard, the status line). The rules are the ones of the
// terminal viewer (`doe::copy`), and both test the same examples.

import type { TraceNode } from './doe.js';
import { type Path, nodeAt } from './model.js';

/** The most that is copied, in bytes of the field or of the text. */
export const COPY_LIMIT = 1024 * 1024;

export interface CopyFormat {
  letter: string;
  name: string;
}

/** What can be copied, in the order of the menu. */
export const COPY_FORMATS: readonly CopyFormat[] = [
  { letter: 'p', name: 'Path' },
  { letter: 'v', name: 'Value' },
  { letter: 'j', name: 'JSON' },
  { letter: 'x', name: 'Hex' },
  { letter: 'X', name: 'Hex, no spaces' },
  { letter: 'c', name: 'C array' },
  { letter: 'o', name: 'Offset and length' },
  { letter: 's', name: 'Schema field' },
];

/** Whether `name` can be written after a dot: an id of the schema. */
function isId(name: string): boolean {
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(name);
}

function isElement(name: string): boolean {
  return /^\[\d+\]$/.test(name);
}

/** The path of the node at `path` written as an expression: `$` for the root, `entries[2].len` for a field. */
export function pathText(root: TraceNode, path: Path): string {
  let out = '';
  let node = root;
  for (const index of path) {
    const child = node.children[index];
    if (!child) break;
    if (isElement(child.name)) out += child.name;
    else if (isId(child.name)) out += `${out === '' ? '' : '.'}${child.name}`;
    else out += `["${child.name.replace(/["\\]/g, '\\$&')}"]`;
    node = child;
  }
  return out === '' ? '$' : out;
}

type Step = { name: string } | { index: number };

function steps(text: string): Step[] | null {
  const source = text.trim();
  if (source === '') return null;
  if (source === '$') return [];
  const out: Step[] = [];
  let i = 0;
  let first = true;
  const id = (): string => {
    const start = i;
    while (i < source.length && /[A-Za-z0-9_]/.test(source[i] ?? '')) i += 1;
    return source.slice(start, i);
  };
  while (i < source.length) {
    const c = source[i] ?? '';
    if (c === '.' && !first) {
      i += 1;
      const name = id();
      if (name === '') return null;
      out.push({ name });
    } else if (c === '[') {
      i += 1;
      if (source[i] === '"') {
        i += 1;
        let name = '';
        for (;;) {
          const d = source[i];
          if (d === undefined) return null;
          if (d === '"') break;
          if (d === '\\') i += 1;
          name += source[i] ?? '';
          i += 1;
        }
        i += 1;
        out.push({ name });
      } else {
        const start = i;
        while (i < source.length && /[0-9]/.test(source[i] ?? '')) i += 1;
        if (start === i) return null;
        out.push({ index: Number(source.slice(start, i)) });
      }
      if (source[i] !== ']') return null;
      i += 1;
    } else if (first && /[A-Za-z_]/.test(c)) {
      out.push({ name: id() });
    } else return null;
    first = false;
  }
  return out;
}

/** The path of indexes of the node that `text` names (the inverse of `pathText`), or null. */
export function resolvePath(root: TraceNode, text: string): Path | null {
  const list = steps(text);
  if (list === null) return null;
  const out: number[] = [];
  let node = root;
  for (const step of list) {
    const name = 'name' in step ? step.name : `[${step.index}]`;
    const at = node.children.findIndex((child) => child.name === name);
    const child = node.children[at];
    if (!child) return null;
    out.push(at);
    node = child;
  }
  return out;
}

export function hexSpaced(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join(' ');
}

export function hexContinuous(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

/** The name of a C variable made from a path: `entries[2].len` is `entries_2_len`, `$` is `data`. */
export function cName(path: string): string {
  if (path === '$') return 'data';
  const name = path
    .replace(/[^A-Za-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .toLowerCase();
  if (name === '') return 'data';
  return /^[0-9]/.test(name) ? `_${name}` : name;
}

/** The bytes as a C array named after the path: `unsigned char entries_2_len[] = { 0x01, 0x02 };`. */
export function cArray(path: string, bytes: Uint8Array): string {
  const body = Array.from(bytes, (b) => `0x${b.toString(16).padStart(2, '0')}`).join(', ');
  return `unsigned char ${cName(path)}[] = { ${body === '' ? '' : `${body} `}};`;
}

/** Where the node is: `0x1f4+2`, `0x1f4 bits 3..7` for a bit field, with a note for bytes that a `process:` made. Null for a computed node. */
export function placeText(node: TraceNode): string | null {
  if (node.start === null || node.end === null) return null;
  let text: string;
  if (node.start % 8 === 0 && node.end % 8 === 0) text = `0x${(node.start / 8).toString(16)}+${(node.end - node.start) / 8}`;
  else {
    const first = node.start % 8;
    text = `0x${Math.floor(node.start / 8).toString(16)} bits ${first}..${first + node.end - node.start}`;
  }
  return node.processed ? `${text} in the decoded bytes` : text;
}

function bare(type: string): string {
  return type.split('::').pop() ?? type;
}

/** The value of the node in the parsed JSON, walking by the names of the trace; undefined when it is not there. */
export function jsonAt(value: unknown, root: TraceNode, path: Path): unknown {
  let current: unknown = value;
  let node = root;
  for (const index of path) {
    const child = node.children[index];
    if (!child || current === null || typeof current !== 'object') return undefined;
    if (Array.isArray(current)) current = current[Number(child.name.slice(1, -1))];
    else current = (current as Record<string, unknown>)[child.name];
    node = child;
  }
  return current;
}

/** The type of the field as the schema writes it: `u2`, `s4`, `f8`, `b3`, `str`, `bytes`, or the name of its type. */
export function schemaKind(node: TraceNode, json: unknown): string {
  if (node.type) return bare(node.type);
  const bits = node.start !== null && node.end !== null ? node.end - node.start : 0;
  const sized = (letter: string): string => (bits > 0 && bits % 8 === 0 ? `${letter}${bits / 8}` : `b${bits}`);
  if (typeof json === 'boolean') return 'b1';
  if (typeof json === 'string') return 'str';
  if (typeof json === 'number') return Number.isInteger(json) ? sized(json < 0 ? 's' : 'u') : sized('f');
  if (json !== null && typeof json === 'object') {
    const kind = (json as { $type?: unknown }).$type;
    if (kind === 'bytes') return 'bytes';
    if (kind === 'enum') return sized('u');
  }
  return node.integer ? sized('u') : 'type';
}

export interface CopyContext {
  root: TraceNode;
  path: Path;
  /** The whole parsed value as JSON, if the parse made it. */
  value: unknown;
  /** The bytes of the input, if they are in the page. */
  bytes: Uint8Array | null;
  /** The value as the tree shows it (the radix of the field applied). */
  shown: string | null;
}

/** The text that copying the selected node as `letter` makes, or why it cannot be made. */
export function copyText(letter: string, context: CopyContext): { text: string } | { error: string } {
  const node = nodeAt(context.root, context.path);
  if (!node) return { error: 'nothing is selected' };
  const path = pathText(context.root, context.path);
  const json = jsonAt(context.value, context.root, context.path);
  switch (letter) {
    case 'p':
      return { text: path };
    case 'v':
      if (typeof json === 'string' && !node.problem) return { text: json };
      return context.shown !== null ? { text: context.shown } : { error: 'This has no single value: copy it as JSON' };
    case 'j':
      return json === undefined ? { error: 'The value was not kept, so there is no JSON of it' } : { text: JSON.stringify(json, null, 2) };
    case 'x':
    case 'X':
    case 'c': {
      if (node.processed) return { error: 'Those bytes are decoded by a process: and are not in the file; Offset and length says where' };
      if (node.start === null || node.end === null) return { error: 'This is computed and has no bytes' };
      if (node.start % 8 !== 0 || node.end % 8 !== 0) return { error: 'This is not a whole number of bytes; Offset and length says which bits' };
      const length = (node.end - node.start) / 8;
      if (length > COPY_LIMIT) return { error: `${length} bytes is more than the ${COPY_LIMIT} that can be copied here: save the value, or use doe --json` };
      if (!context.bytes) return { error: 'The bytes of this file are not in the page' };
      const data = context.bytes.subarray(node.start / 8, node.end / 8);
      return { text: letter === 'x' ? hexSpaced(data) : letter === 'X' ? hexContinuous(data) : cArray(path, data) };
    }
    case 'o': {
      const place = placeText(node);
      return place === null ? { error: 'This is computed and is not in the file' } : { text: place };
    }
    case 's': {
      const parent = context.path.length > 0 ? nodeAt(context.root, context.path.slice(0, -1)) : null;
      const kind = schemaKind(node, json);
      return { text: parent?.type ? `${node.name}: ${kind} in ${bare(parent.type)}` : `${node.name}: ${kind}` };
    }
    default:
      return { error: `${letter} is not one of the choices` };
  }
}
