// Which kind of value owns each byte of the input, for the colours of the Bytes pane. The Parsed pane tells the kinds of
// value apart by colour (numbers, strings, byte runs, enums, booleans); the Bytes pane draws each byte in the colour of the
// leaf field it belongs to, by the same function (`valueKind`), so the two panes cannot drift apart.
//
// The index is a sorted list of disjoint segments made once per parse, after the tree is shown, and searched for the bytes
// in view: memory is proportional to the number of leaf fields, and a row costs a binary search.

import type { TraceNode } from './doe.js';
import { type ValueKind, valueKind } from './model.js';

/** The kinds, in the order the index stores them (0 is a field that has no kind of its own). */
export const KINDS: readonly ValueKind[] = ['number', 'string', 'bytes', 'enum', 'boolean'];

/** The kind of the value of a node, as the tree colours it. */
export function kindOfNode(node: TraceNode): ValueKind | null {
  if (node.integer) return node.integer.name === null ? 'number' : 'enum';
  return valueKind(node.summary);
}

export class ByteKinds {
  constructor(
    private readonly starts: Uint32Array,
    private readonly ends: Uint32Array,
    private readonly kinds: Uint8Array,
  ) {}

  /** How many segments. */
  get size(): number {
    return this.starts.length;
  }

  /** The kind of the leaf field that owns the byte, or null when it is a field with no kind or no field holds the byte. */
  kindAt(byte: number): ValueKind | null {
    let low = 0;
    let high = this.starts.length - 1;
    let found = -1;
    while (low <= high) {
      const middle = (low + high) >>> 1;
      if ((this.starts[middle] ?? 0) <= byte) {
        found = middle;
        low = middle + 1;
      } else high = middle - 1;
    }
    if (found < 0 || byte >= (this.ends[found] ?? 0)) return null;
    const kind = this.kinds[found] ?? 0;
    return kind === 0 ? null : (KINDS[kind - 1] ?? null);
  }
}

/** Added to the priority of what is read from an instance: a field of the sequence holds a byte before an instance does. */
const INSTANCE = 2 ** 30;

/**
 * Index the leaf fields of a trace. A leaf is a node with no children that covers bytes; a struct or an array has its
 * children's bytes, not its own. Where two leaves cover one byte (a field placed with `pos:`, an instance, bit fields that
 * share a byte) the first in the sequence wins, then the instances, as "the next field that holds this byte" does.
 */
export function buildByteKinds(root: TraceNode): ByteKinds {
  const starts: number[] = [];
  const ends: number[] = [];
  const priority: number[] = [];
  const kind: number[] = [];
  // an explicit stack, in the order of the tree: a trace can be deep
  const stack: { node: TraceNode; instance: boolean }[] = [{ node: root, instance: false }];
  let order = 0;
  while (stack.length > 0) {
    const item = stack.pop();
    if (!item) break;
    const { node } = item;
    const instance = item.instance || node.instance;
    // what a transform produced has offsets that are not the input's
    if (node.processed) continue;
    if (node.children.length > 0) {
      for (let i = node.children.length - 1; i >= 0; i -= 1) {
        const child = node.children[i];
        if (child) stack.push({ node: child, instance });
      }
      continue;
    }
    if (node.start === null || node.end === null || node.end <= node.start) continue;
    starts.push(Math.floor(node.start / 8));
    ends.push(Math.ceil(node.end / 8));
    priority.push((instance ? INSTANCE : 0) + order);
    const k = kindOfNode(node);
    kind.push(k === null ? 0 : KINDS.indexOf(k) + 1);
    order += 1;
  }
  return sweep(starts, ends, priority, kind);
}

/** Cut the intervals into disjoint segments, each with the kind of the interval of the best priority that covers it. */
function sweep(starts: number[], ends: number[], priority: number[], kind: number[]): ByteKinds {
  const n = starts.length;
  const order = new Uint32Array(n);
  for (let i = 0; i < n; i += 1) order[i] = i;
  // a sequence is mostly in the order of its bytes already
  let sorted = true;
  for (let i = 1; i < n; i += 1) {
    if ((starts[order[i] ?? 0] ?? 0) < (starts[order[i - 1] ?? 0] ?? 0)) {
      sorted = false;
      break;
    }
  }
  if (!sorted) order.sort((a, b) => (starts[a] ?? 0) - (starts[b] ?? 0));

  // a min-heap of the intervals that cover the position, by priority
  const heap: number[] = [];
  const better = (a: number, b: number): boolean => (priority[a] ?? 0) < (priority[b] ?? 0);
  const push = (index: number): void => {
    heap.push(index);
    let at = heap.length - 1;
    while (at > 0) {
      const parent = (at - 1) >> 1;
      if (!better(heap[at] ?? 0, heap[parent] ?? 0)) break;
      [heap[at], heap[parent]] = [heap[parent] ?? 0, heap[at] ?? 0];
      at = parent;
    }
  };
  const pop = (): void => {
    const last = heap.pop();
    if (heap.length === 0 || last === undefined) return;
    heap[0] = last;
    let at = 0;
    for (;;) {
      const left = at * 2 + 1;
      const right = left + 1;
      let best = at;
      if (left < heap.length && better(heap[left] ?? 0, heap[best] ?? 0)) best = left;
      if (right < heap.length && better(heap[right] ?? 0, heap[best] ?? 0)) best = right;
      if (best === at) break;
      [heap[at], heap[best]] = [heap[best] ?? 0, heap[at] ?? 0];
      at = best;
    }
  };

  const outStarts: number[] = [];
  const outEnds: number[] = [];
  const outKinds: number[] = [];
  let position = 0;
  let next = 0;
  while (next < n || heap.length > 0) {
    if (heap.length === 0) position = starts[order[next] ?? 0] ?? 0;
    while (next < n && (starts[order[next] ?? 0] ?? 0) <= position) {
      push(order[next] ?? 0);
      next += 1;
    }
    while (heap.length > 0 && (ends[heap[0] ?? 0] ?? 0) <= position) pop();
    const top = heap[0];
    if (top === undefined) continue;
    const limit = next < n ? (starts[order[next] ?? 0] ?? 0) : Number.POSITIVE_INFINITY;
    const end = Math.min(ends[top] ?? 0, limit);
    const last = outKinds.length - 1;
    if (last >= 0 && outEnds[last] === position && outKinds[last] === (kind[top] ?? 0)) outEnds[last] = end;
    else {
      outStarts.push(position);
      outEnds.push(end);
      outKinds.push(kind[top] ?? 0);
    }
    position = end;
  }
  return new ByteKinds(Uint32Array.from(outStarts), Uint32Array.from(outEnds), Uint8Array.from(outKinds));
}
