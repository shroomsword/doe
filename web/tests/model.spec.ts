// The logic that needs no browser.

import { expect, test } from '@playwright/test';
import type { TraceNode } from '../src/doe.js';
import {
  MAX_INPUT_BYTES,
  byteRange,
  checkSize,
  describeRange,
  editorRange,
  findByte,
  nodeAt,
  pathName,
  unitsOfBytes,
} from '../src/model.js';

function node(name: string, start: number | null, end: number | null, children: TraceNode[] = [], extra: Partial<TraceNode> = {}): TraceNode {
  return { name, type: null, start, end, instance: false, failed: false, processed: false, summary: null, integer: null, children, ...extra };
}

// root: bytes 0-7; a: byte 0; items: bytes 1-6 with [0]: bytes 1-3 (kind: 1, len: 2-3), [1]: bytes 4-6;
// tail: an instance read from byte 7; bits: 4 bits of byte 6.
const tree = node('root', 0, 64, [
  node('a', 0, 8),
  node('items', 8, 56, [
    node('[0]', 8, 32, [node('kind', 8, 16), node('len', 16, 32)]),
    node('[1]', 32, 56, [node('kind', 32, 40), node('len', 40, 48), node('flags', 48, 52)]),
  ]),
  node('tail', 56, 64, [], { instance: true }),
]);

test('nodeAt follows a path and gives null past the end', () => {
  expect(nodeAt(tree, [])?.name).toBe('root');
  expect(nodeAt(tree, [1, 1, 2])?.name).toBe('flags');
  expect(nodeAt(tree, [1, 5])).toBeNull();
});

test('byteRange rounds outwards and is null for nothing', () => {
  expect(byteRange(node('x', 8, 16))).toEqual({ first: 1, end: 2 });
  expect(byteRange(node('x', 12, 20))).toEqual({ first: 1, end: 3 });
  expect(byteRange(node('x', 16, 16))).toBeNull();
  expect(byteRange(node('x', null, null))).toBeNull();
});

test('findByte gives the innermost node, not an instance, and null outside', () => {
  expect(findByte(tree, 0)).toEqual([0]);
  expect(findByte(tree, 1)).toEqual([1, 0, 0]);
  expect(findByte(tree, 3)).toEqual([1, 0, 1]);
  expect(findByte(tree, 6)).toEqual([1, 1, 2]);
  // Byte 7 is only the instance's, so the nearest node that is not one holds it.
  expect(findByte(tree, 7)).toEqual([]);
  expect(findByte(tree, 8)).toBeNull();
});

test('findByte does not look into what a transform produced', () => {
  const processed = node('root', 0, 16, [node('packed', 0, 16, [node('inside', 0, 8, [], { processed: true })])]);
  expect(findByte(processed, 0)).toEqual([0]);
});

test('describeRange says bytes for whole bytes and bits otherwise', () => {
  expect(describeRange(node('x', 8, 16))).toBe('byte 1');
  expect(describeRange(node('x', 8, 32))).toBe('bytes 1–3');
  expect(describeRange(node('x', 12, 20))).toBe('bits 12–19 (bytes 1–2)');
  expect(describeRange(node('x', 4, 4))).toBeNull();
});

test('pathName joins the names along a path', () => {
  expect(pathName(tree, [1, 1, 0])).toBe('root/items/[1]/kind');
});

test('checkSize refuses more than 256 MiB and says why', () => {
  expect(checkSize(MAX_INPUT_BYTES)).toBeNull();
  const message = checkSize(MAX_INPUT_BYTES + 1);
  expect(message).toContain('256 MiB');
  expect(message).toContain('WebAssembly');
});

test('byte lengths are turned into editor units, also for non-ASCII text', () => {
  expect(unitsOfBytes('abcdef', 3)).toBe(3);
  expect(unitsOfBytes('é1', 2)).toBe(1);
  expect(unitsOfBytes('€x', 3)).toBe(1);
  expect(unitsOfBytes('😀x', 4)).toBe(2);
  expect(unitsOfBytes('ab', 10)).toBe(2);
});

test('editorRange covers the value of a position and always something', () => {
  // "    size: n +" with the value at column 11, 3 bytes long
  expect(editorRange('    size: n +', 100, { column: 11, len: 3 })).toEqual({ from: 110, to: 113 });
  // no length: one character
  expect(editorRange('  - id: a', 0, { column: 9, len: null })).toEqual({ from: 8, to: 9 });
  // after a wide character the column counts characters
  expect(editorRange('# €€ size: x', 0, { column: 11, len: 1 })).toEqual({ from: 10, to: 11 });
  // past the end of the line
  expect(editorRange('ab', 0, { column: 9, len: 3 })).toEqual({ from: 2, to: 2 });
});
