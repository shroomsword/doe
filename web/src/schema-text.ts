// The text of a schema, read as far as an editor needs to help: where the types, fields, enums and imports are, and where
// the cursor is in the structure. It is read by lines and indentation, not by a YAML parser, so it works on a schema that
// has a mistake, and on a line that is not finished (a key being typed, a value that has no colon yet).

/** A line of the text that has a key, or starts an item of a list. */
export interface Node {
  kind: 'key' | 'item';
  /** The key (empty for an item). */
  key: string;
  /** The column that the key (or the dash of an item) is at. */
  indent: number;
  /** The offsets of the key in the text (of the dash for an item). */
  from: number;
  to: number;
  /** The text after the colon (or after the dash), cut of a comment; null when there is none. */
  value: string | null;
  valueFrom: number;
  valueTo: number;
  /** The line, from 1. */
  line: number;
  parent: Node | null;
  children: Node[];
}

export interface Outline {
  roots: Node[];
  /** Every node, in the order of the text. */
  nodes: Node[];
}

/** The part of a line before a comment that begins with `#` after a space, and outside quotes. */
function withoutComment(text: string): string {
  let quote = '';
  for (let i = 0; i < text.length; i += 1) {
    const c = text[i] ?? '';
    if (quote) {
      if (c === quote) quote = '';
    } else if (c === '"' || c === "'") {
      quote = c;
    } else if (c === '#' && (i === 0 || /\s/.test(text[i - 1] ?? ''))) {
      return text.slice(0, i);
    }
  }
  return text;
}

const KEY = /^([^\s#:'"[\]{},&*!|>%@`-][^:]*?|-[^\s:][^:]*?)\s*:(?=\s|$)/;

/** Read the structure of the text. */
export function readOutline(text: string): Outline {
  const roots: Node[] = [];
  const nodes: Node[] = [];
  const stack: Node[] = [];
  let offset = 0;
  let skipBelow = -1;
  const lines = text.split('\n');
  const add = (node: Node): void => {
    // an item may sit at the column of the key that holds it (`seq:` and `- id: a` under it)
    while (stack.length > 0) {
      const top = stack[stack.length - 1] as Node;
      const sameColumn = top.indent === node.indent;
      if (top.indent > node.indent || (sameColumn && !(node.kind === 'item' && top.kind === 'key'))) stack.pop();
      else break;
    }
    node.parent = stack[stack.length - 1] ?? null;
    (node.parent ? node.parent.children : roots).push(node);
    nodes.push(node);
    stack.push(node);
  };
  lines.forEach((raw, index) => {
    const from = offset;
    offset += raw.length + 1;
    const line = withoutComment(raw);
    const trimmed = line.trim();
    if (trimmed === '') return;
    const indent = line.length - line.trimStart().length;
    // the lines of a block scalar (`doc: |`) are text, not keys
    if (skipBelow >= 0) {
      if (indent > skipBelow) return;
      skipBelow = -1;
    }
    let column = indent;
    let rest = line.slice(indent);
    // `- ` once or more: each starts an item
    while (/^-(\s|$)/.test(rest)) {
      const dashEnd = rest.match(/^-\s*/)?.[0].length ?? 1;
      const item: Node = {
        kind: 'item',
        key: '',
        indent: column,
        from: from + column,
        to: from + column + 1,
        value: null,
        valueFrom: from + column + dashEnd,
        valueTo: from + column + dashEnd,
        line: index + 1,
        parent: null,
        children: [],
      };
      add(item);
      column += dashEnd;
      rest = rest.slice(dashEnd);
    }
    const found = KEY.exec(rest);
    const last = nodes[nodes.length - 1];
    if (!found) {
      // a scalar item (`- foo`): the value of the item that this line started
      if (rest.trim() !== '' && last?.kind === 'item' && last.line === index + 1) {
        last.value = rest.trim();
        last.valueFrom = from + column;
        last.valueTo = from + column + rest.trimEnd().length;
      }
      return;
    }
    const key = (found[1] ?? '').trim();
    const afterColon = rest.slice(found[0].length);
    const value = afterColon.trim();
    const valueFrom = from + column + found[0].length + (afterColon.length - afterColon.trimStart().length);
    add({
      kind: 'key',
      key,
      indent: column,
      from: from + column,
      to: from + column + key.length,
      value: value === '' ? null : value,
      valueFrom,
      valueTo: valueFrom + value.length,
      line: index + 1,
      parent: null,
      children: [],
    });
    if (/^[|>]/.test(value)) skipBelow = column;
  });
  return { roots, nodes };
}

/** The keys of the children of a node (or of the roots). */
export function childKeys(of: Node | null, outline: Outline): string[] {
  return (of ? of.children : outline.roots).filter((n) => n.kind === 'key').map((n) => n.key);
}

/** The ancestors of a node, outermost first, as the segments that the language says: the key, or `-` for an item. */
export function pathOf(node: Node | null): string[] {
  const path: string[] = [];
  for (let at = node; at; at = at.parent) path.unshift(at.kind === 'item' ? '-' : at.key);
  return path;
}

/** Which place of a schema the keys of a mapping at `path` belong to: the names that the language call uses. */
export function placeOf(path: readonly string[]): string | null {
  let state: string = 'schema';
  for (const segment of path) {
    switch (state) {
      case 'schema':
      case 'type':
        state = ({ meta: 'meta', seq: 'seq', instances: 'instances', types: 'types', enums: 'enums', params: 'params', payload: 'payload', filesystem: 'filesystem' } as const)[segment as 'meta'] ?? 'none';
        break;
      case 'seq':
        state = segment === '-' ? 'field' : 'none';
        break;
      case 'params':
        state = segment === '-' ? 'param' : 'none';
        break;
      case 'instances':
        state = 'instance';
        break;
      case 'types':
        state = 'type';
        break;
      case 'enums':
        state = 'enum';
        break;
      case 'field':
      case 'instance':
        state = segment === 'type' ? 'switch' : segment === 'valid' ? 'valid' : 'none';
        break;
      case 'meta':
        state = segment === 'endian' ? 'switch' : 'none';
        break;
      case 'switch':
        state = segment === 'cases' ? 'cases' : 'none';
        break;
      default:
        state = 'none';
    }
  }
  return state === 'schema' || state === 'type' || state === 'meta' || state === 'field' || state === 'instance' || state === 'switch' || state === 'param' || state === 'payload' || state === 'filesystem' || state === 'valid' ? state : null;
}

/** What the cursor is at, in the structure of the text. */
export interface Context {
  /** The place that a key written here belongs to, or null when no key goes here. */
  place: string | null;
  /** A key is being written (before any colon), or the value of `key` is. */
  mode: 'key' | 'value';
  /** The key whose value is being written. */
  key: string;
  /** The ancestors of the line, as `-` for an item, outermost first; the key itself is not in them. */
  path: string[];
  /** What is written of the word, and where it starts. */
  prefix: string;
  from: number;
  /** The keys that the mapping has already (so they are not offered again). */
  present: string[];
  /** The node that holds the line: the mapping's owner (a key or an item), null at the top. */
  parent: Node | null;
  /** The node of the line itself, when it has a key. */
  node: Node | null;
}

/** Find where the cursor (at `pos`) is. */
export function contextAt(text: string, pos: number, outline: Outline = readOutline(text)): Context {
  const lineFrom = text.lastIndexOf('\n', pos - 1) + 1;
  const before = text.slice(lineFrom, pos);
  const colon = /^(\s*(?:-\s+)*)([^\s#:'"][^:]*?)\s*:\s*(\S*)$/.exec(withoutComment(before));
  // the line, as the outline has it, if it has a key
  const lineNumber = text.slice(0, pos).split('\n').length;
  const node = outline.nodes.find((n) => n.kind === 'key' && n.line === lineNumber) ?? null;
  if (colon && node) {
    const parent = node.parent;
    return {
      place: null,
      mode: 'value',
      key: node.key,
      path: pathOf(parent),
      prefix: colon[3] ?? '',
      from: pos - (colon[3] ?? '').length,
      present: [],
      parent: node,
      node,
    };
  }
  const word = /^(\s*(?:-\s+)*)([^\s#:'"]*)$/.exec(withoutComment(before));
  const prefix = word?.[2] ?? '';
  const indent = (word?.[1] ?? '').length;
  const parent = ownerAt(outline, lineNumber, indent);
  const present = (parent ? parent.children : outline.roots)
    .filter((n) => n.kind === 'key' && n.line !== lineNumber)
    .map((n) => n.key);
  // a scalar item has no keys
  const place = parent?.kind === 'item' && parent.value !== null && parent.line !== lineNumber ? null : placeOf(pathOf(parent));
  return { place, mode: 'key', key: '', path: pathOf(parent), prefix, from: pos - prefix.length, present, parent, node };
}

/** The node that owns a new line at `column` in `line`: walk the nodes before the line as the reader does. */
function ownerAt(outline: Outline, line: number, column: number): Node | null {
  const stack: Node[] = [];
  for (const node of outline.nodes) {
    if (node.line >= line) break;
    while (stack.length > 0) {
      const top = stack[stack.length - 1] as Node;
      if (top.indent > node.indent || (top.indent === node.indent && !(node.kind === 'item' && top.kind === 'key'))) stack.pop();
      else break;
    }
    stack.push(node);
  }
  // a key written at `column` is owned by the closest node that starts before it; a key at the column of an earlier key
  // is that key's sibling, which has the same owner
  while (stack.length > 0) {
    const top = stack[stack.length - 1] as Node;
    if (top.indent >= column) stack.pop();
    else break;
  }
  return stack[stack.length - 1] ?? null;
}

/** The entries of the mapping under the first key `name` among the nodes under `of` (all the nodes when null). */
function under(of: Node | null, outline: Outline, name: string): Node[] {
  const scope = of ? of.children : outline.roots;
  return scope.filter((n) => n.kind === 'key' && n.key === name).flatMap((n) => n.children);
}

/** The type that a node is in: the nearest `types:` entry above it, or null for the main type. */
export function enclosingType(node: Node | null): Node | null {
  for (let at = node; at; at = at.parent) {
    if (at.kind === 'key' && at.parent?.kind === 'key' && at.parent.key === 'types') return at;
  }
  return null;
}

/** Every type that the text defines: the entries of every `types:`, with the node that has its definition. */
export function definedTypes(outline: Outline): Node[] {
  return outline.nodes.filter((n) => n.kind === 'key' && n.parent?.kind === 'key' && n.parent.key === 'types');
}

/** The enums of a type, and of the types around it, nearest first. */
export function enumsInScope(node: Node | null, outline: Outline): Node[] {
  const out: Node[] = [];
  for (let type = enclosingType(node); ; type = enclosingType(type?.parent ?? null)) {
    out.push(...under(type, outline, 'enums'));
    if (!type) break;
  }
  return out;
}

/** The fields that an expression at `node` can name: the ids of the fields before it in the same type (all of them when
 *  the expression is not in a field of `seq`), the instances and the parameters of the type. */
export function fieldsInScope(node: Node | null, outline: Outline): string[] {
  const type = enclosingType(node);
  const seq = under(type, outline, 'seq');
  // the item that holds the node, if it is in `seq`
  let item: Node | null = null;
  for (let at = node; at; at = at.parent) {
    if (at.kind === 'item' && at.parent?.key === 'seq') item = at;
  }
  const ids: string[] = [];
  for (const entry of seq) {
    if (item && entry === item) break;
    const id = entry.children.find((c) => c.key === 'id')?.value;
    if (id) ids.push(id);
  }
  for (const instance of under(type, outline, 'instances')) if (instance.kind === 'key') ids.push(instance.key);
  for (const param of under(type, outline, 'params')) {
    const id = param.children.find((c) => c.key === 'id')?.value;
    if (id) ids.push(id);
  }
  return ids;
}

/** The text of the `doc:` of a definition, when it has one on its line (a block text is cut at its first line). */
export function docOf(node: Node, text: string): string | null {
  const doc = node.children.find((c) => c.key === 'doc');
  if (!doc) return null;
  if (doc.value && !/^[|>]/.test(doc.value)) return doc.value.replace(/^["']|["']$/g, '');
  // a block scalar: the first non-empty line under it
  const after = text.slice(doc.valueTo);
  const first = after.split('\n').slice(1).find((l) => l.trim() !== '');
  return first ? first.trim() : null;
}

/** The names that the text imports (the top-level `imports:` and `meta.imports:`). */
export function importsOf(outline: Outline): Node[] {
  const top = under(null, outline, 'imports');
  const meta = outline.roots.filter((n) => n.key === 'meta').flatMap((m) => under(m, outline, 'imports'));
  return [...top, ...meta].filter((n) => n.kind === 'item' && n.value !== null);
}
