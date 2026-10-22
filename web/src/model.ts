// The logic of the page that needs no DOM: finding nodes of the trace, the
// bytes of a node, the node of a byte, the limit on the input, and turning the
// positions doe gives into positions of the editor.

import type { Position, TraceNode } from './doe.js';

/** The largest input; a WebAssembly module has 4 GiB of memory at most, and the page holds the file and a copy. */
export const MAX_INPUT_BYTES = 256 * 1024 * 1024;

/** The children to follow from the root to a node: `[1, 0]` is the first child of the second child. */
export type Path = readonly number[];

/** The message for an input that is too large, or null when it is fine. */
export function checkSize(size: number): string | null {
  if (size <= MAX_INPUT_BYTES) return null;
  const mib = (n: number): string => (n / (1024 * 1024)).toFixed(0);
  return `The file is ${mib(size)} MiB; the most that can be parsed in the browser is ${mib(MAX_INPUT_BYTES)} MiB (WebAssembly memory). The local version, doe --ide, has no such limit.`;
}

export function nodeAt(root: TraceNode, path: Path): TraceNode | null {
  let node: TraceNode = root;
  for (const index of path) {
    const child = node.children[index];
    if (!child) return null;
    node = child;
  }
  return node;
}

/** The nodes from the root to the node at `path`, both included. */
export function nodesAlong(root: TraceNode, path: Path): TraceNode[] {
  const nodes = [root];
  let node: TraceNode = root;
  for (const index of path) {
    const child = node.children[index];
    if (!child) break;
    nodes.push(child);
    node = child;
  }
  return nodes;
}

/** The bytes a node covers: from `first` up to, not including, `end`. Null when it covers none. */
export function byteRange(node: TraceNode): { first: number; end: number } | null {
  if (node.start === null || node.end === null || node.end <= node.start) return null;
  return { first: Math.floor(node.start / 8), end: Math.ceil(node.end / 8) };
}

/** The innermost node that holds the byte, as a path from the root; null if no node does. */
export function findByte(root: TraceNode, byte: number): Path | null {
  const holds = (node: TraceNode): boolean => {
    const range = byteRange(node);
    return range !== null && range.first <= byte && byte < range.end;
  };
  if (!holds(root)) return null;
  const path: number[] = [];
  let node = root;
  for (;;) {
    // Instances are somewhere else in the input, and what a transform produced
    // has offsets that are not the input's: neither is looked in.
    const index = node.children.findIndex((child) => !child.processed && !child.instance && holds(child));
    const next = node.children[index];
    if (index < 0 || !next) return path;
    path.push(index);
    node = next;
  }
}

/** "bytes 4–7" for whole bytes, "bits 12–20 (bytes 1–2)" for others; null if it covers nothing. */
export function describeRange(node: TraceNode): string | null {
  const range = byteRange(node);
  if (!range || node.start === null || node.end === null) return null;
  const last = range.end - 1;
  const bytes = range.first === last ? `byte ${range.first}` : `bytes ${range.first}–${last}`;
  return node.start % 8 === 0 && node.end % 8 === 0 ? bytes : `bits ${node.start}–${node.end - 1} (${bytes})`;
}

/** The names from the root to a node, as `chunks/items/[1]/body`. */
export function pathName(root: TraceNode, path: Path): string {
  return nodesAlong(root, path)
    .map((node) => node.name)
    .join('/');
}

/** The number of UTF-16 units at the start of `text` that are `bytes` bytes of UTF-8. */
export function unitsOfBytes(text: string, bytes: number): number {
  let units = 0;
  let used = 0;
  for (const char of text) {
    if (used >= bytes) break;
    const code = char.codePointAt(0) ?? 0;
    used += code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
    units += char.length;
  }
  return units;
}

/** The index in `line` of the character in column `column` (from 1, counting characters, not UTF-16 units). */
export function indexOfColumn(line: string, column: number): number {
  let index = 0;
  let seen = 1;
  for (const char of line) {
    if (seen >= column) break;
    index += char.length;
    seen += 1;
  }
  return index;
}

/** The range of the editor that a position of doe means, given the text of the line it is on and where that starts. */
export function editorRange(line: string, lineFrom: number, position: Pick<Position, 'column' | 'len'>): { from: number; to: number } {
  const start = indexOfColumn(line, position.column);
  const length = position.len ? unitsOfBytes(line.slice(start), position.len) : 0;
  // A diagnostic needs something to underline: the rest of the word, at least one character.
  const end = Math.max(start + length, Math.min(start + 1, line.length));
  return { from: lineFrom + start, to: lineFrom + Math.min(end, line.length) };
}
