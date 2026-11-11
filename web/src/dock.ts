// Docking: how the three movable panes (Schema, Parsed, Bytes) are arranged above the Status pane, which is fixed along the
// bottom. The arrangement is a tree of splits: a row (children side by side) or a column (children one under the other)
// whose children are panes or smaller splits, each with a weight. Dragging a pane to one of eight positions makes a new tree
// (`dockTo`); the page draws the tree on one grid (`dockGrid`). No DOM here.

/** The panes that can be moved. Status is fixed at the bottom and is not in the tree. */
export type Movable = 'schema' | 'parsed' | 'bytes';
export const MOVABLE: readonly Movable[] = ['schema', 'parsed', 'bytes'];

export interface Split {
  /** `row`: the children are side by side, from the left; `col`: one under the other, from the top. */
  dir: 'row' | 'col';
  children: Node[];
  /** The share of each child (relative to the others). */
  weights: number[];
}

export type Node = Movable | Split;

export const isMovable = (name: unknown): name is Movable => MOVABLE.includes(name as Movable);
export const isSplit = (node: Node): node is Split => typeof node !== 'string';

/** The eight positions of a drop: the four corners and the four sides of the area. */
export type Zone = 'tl' | 't' | 'tr' | 'l' | 'r' | 'bl' | 'b' | 'br';
export const ZONES: readonly Zone[] = ['tl', 't', 'tr', 'l', 'r', 'bl', 'b', 'br'];
export const ZONE_NAMES: Record<Zone, string> = {
  tl: 'top left',
  t: 'top',
  tr: 'top right',
  l: 'left',
  r: 'right',
  bl: 'bottom left',
  b: 'bottom',
  br: 'bottom right',
};
export const isCorner = (zone: Zone): boolean => zone.length === 2;

/** The share of a pane docked to a side, and the weight that Status has beside the rest. */
const SIDE = 1 / 3;
/** How much of the height Status has beside the panes (1) when the person set it with a separator; null is its own size, low. */
export const DEFAULT_STATUS: number | null = null;

/** The three side by side, in this order: what the wide window shows. */
export function defaultTree(order: readonly Movable[] = MOVABLE): Node {
  return { dir: 'row', children: [...order], weights: [1, 1, 1] };
}

/** The panes of a tree in reading order: left to right, top to bottom. */
export function leaves(node: Node): Movable[] {
  return isSplit(node) ? node.children.flatMap(leaves) : [node];
}

/** A split with one child is that child, and a split inside one that goes the same way is merged into it. */
export function tidy(node: Node): Node {
  if (!isSplit(node)) return node;
  const children: Node[] = [];
  const weights: number[] = [];
  node.children.forEach((child, i) => {
    const tidied = tidy(child);
    const weight = node.weights[i] ?? 1;
    if (isSplit(tidied) && tidied.dir === node.dir) {
      const total = tidied.weights.reduce((a, b) => a + b, 0) || 1;
      tidied.children.forEach((inner, j) => {
        children.push(inner);
        weights.push((weight * (tidied.weights[j] ?? 1)) / total);
      });
    } else {
      children.push(tidied);
      weights.push(weight);
    }
  });
  if (children.length === 1) return children[0] as Node;
  return { dir: node.dir, children, weights };
}

/** The tree without a pane (null when it was the only one). */
export function without(node: Node, pane: Movable): Node | null {
  if (!isSplit(node)) return node === pane ? null : node;
  const children: Node[] = [];
  const weights: number[] = [];
  node.children.forEach((child, i) => {
    const rest = without(child, pane);
    if (rest !== null) {
      children.push(rest);
      weights.push(node.weights[i] ?? 1);
    }
  });
  if (children.length === 0) return null;
  return tidy({ dir: node.dir, children, weights });
}

const row = (children: Node[], weights: number[]): Node => ({ dir: 'row', children, weights });
const col = (children: Node[], weights: number[]): Node => ({ dir: 'col', children, weights });

/**
 * The tree with `pane` docked at `zone`. A side makes the pane a strip of a third along that side of the whole area, and the
 * others keep their arrangement in what is left. A corner makes it that quarter, and the others fill the rest in reading order,
 * one column at a time (the left column from the top, then the right column), so that with two others the column that does not
 * have the pane is one tall cell. Null when there is nothing to dock against (the pane is alone) or a corner needs more panes
 * than there are.
 */
export function dockTo(tree: Node, pane: Movable, zone: Zone): Node | null {
  const rest = without(tree, pane);
  if (rest === null) return null;
  const others = leaves(rest);
  switch (zone) {
    case 'l':
      return tidy(row([pane, rest], [SIDE, 1 - SIDE]));
    case 'r':
      return tidy(row([rest, pane], [1 - SIDE, SIDE]));
    case 't':
      return tidy(col([pane, rest], [SIDE, 1 - SIDE]));
    case 'b':
      return tidy(col([rest, pane], [1 - SIDE, SIDE]));
    default:
      break;
  }
  if (others.length < 2) return null;
  const [first, second] = others as [Movable, Movable];
  switch (zone) {
    case 'tr':
      return row([first, col([pane, second], [1, 1])], [1, 1]);
    case 'br':
      return row([first, col([second, pane], [1, 1])], [1, 1]);
    case 'tl':
      return row([col([pane, first], [1, 1]), second], [1, 1]);
    case 'bl':
      return row([col([first, pane], [1, 1]), second], [1, 1]);
    default:
      return null;
  }
}

/** The tree with two panes in each other's places. */
export function swapIn(node: Node, a: Movable, b: Movable): Node {
  if (!isSplit(node)) return node === a ? b : node === b ? a : node;
  return { dir: node.dir, children: node.children.map((child) => swapIn(child, a, b)), weights: [...node.weights] };
}

/** The position that a pane has in the tree, for saying where a pane went: its zone, or null when it is in the middle. */
export function zoneOf(tree: Node, pane: Movable): Zone | null {
  const box = rects(tree).get(pane);
  if (!box) return null;
  const e = 0.01;
  const left = box.x0 < e;
  const right = box.x1 > 1 - e;
  const top = box.y0 < e;
  const bottom = box.y1 > 1 - e;
  if (left && right && top && bottom) return null;
  if (top && left && !(bottom || right)) return 'tl';
  if (top && right && !(bottom || left)) return 'tr';
  if (bottom && left && !(top || right)) return 'bl';
  if (bottom && right && !(top || left)) return 'br';
  if (left && top && bottom) return 'l';
  if (right && top && bottom) return 'r';
  if (top && left && right) return 't';
  if (bottom && left && right) return 'b';
  return null;
}

// ── what is shown ────────────────────────────────────────────────────────────

/** A pane of the tree that is shown, or a split of those that are: the tree without the closed panes. */
export type View = { pane: Movable } | { split: ViewSplit };

export interface ViewSplit {
  dir: 'row' | 'col';
  children: View[];
  /** The weight of each child. */
  weights: number[];
  /** Where the split is in the whole tree (the index of the child at each step from the root). */
  path: number[];
  /** The index in that split of each child that is shown, to change its weight. */
  from: number[];
}

/** The tree without the panes that are closed; null when none is shown. A split with one child shown is that child. */
export function view(node: Node, shown: ReadonlySet<Movable>, path: number[] = []): View | null {
  if (!isSplit(node)) return shown.has(node) ? { pane: node } : null;
  const children: View[] = [];
  const weights: number[] = [];
  const from: number[] = [];
  node.children.forEach((child, i) => {
    const inner = view(child, shown, [...path, i]);
    if (inner) {
      children.push(inner);
      weights.push(node.weights[i] ?? 1);
      from.push(i);
    }
  });
  if (children.length === 0) return null;
  if (children.length === 1) return children[0] as View;
  return { split: { dir: node.dir, children, weights, path, from } };
}

export interface Box {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

/** Where each pane is in the unit square (x to the right, y down): the share of each split goes by its weights. */
export function rects(node: Node, shown?: ReadonlySet<Movable>): Map<Movable, Box> {
  const out = new Map<Movable, Box>();
  const tree = shown ? view(node, shown) : toView(node);
  if (tree) place(tree, { x0: 0, y0: 0, x1: 1, y1: 1 }, out);
  return out;
}

/** The whole tree as a view (every pane shown). */
function toView(node: Node, path: number[] = []): View {
  if (!isSplit(node)) return { pane: node };
  return { split: { dir: node.dir, children: node.children.map((c, i) => toView(c, [...path, i])), weights: [...node.weights], path, from: node.children.map((_, i) => i) } };
}

function place(node: View, box: Box, out: Map<Movable, Box>): void {
  if ('pane' in node) {
    out.set(node.pane, box);
    return;
  }
  const { dir, children, weights } = node.split;
  const total = weights.reduce((a, b) => a + b, 0) || 1;
  let at = 0;
  children.forEach((child, i) => {
    const share = (weights[i] ?? 1) / total;
    const next =
      dir === 'row'
        ? { x0: box.x0 + (box.x1 - box.x0) * at, x1: box.x0 + (box.x1 - box.x0) * (at + share), y0: box.y0, y1: box.y1 }
        : { y0: box.y0 + (box.y1 - box.y0) * at, y1: box.y0 + (box.y1 - box.y0) * (at + share), x0: box.x0, x1: box.x1 };
    place(child, next, out);
    at += share;
  });
}

/** The splits of a view, outermost first. */
export function splitsOf(node: View | null): ViewSplit[] {
  if (!node || 'pane' in node) return [];
  return [node.split, ...node.split.children.flatMap(splitsOf)];
}

/** The panes under a view node. */
export function viewPanes(node: View): Movable[] {
  return 'pane' in node ? [node.pane] : node.split.children.flatMap(viewPanes);
}

/** The tree with two neighbours' weights changed: the children `first` and `second` of the split at `path`. */
export function withWeights(node: Node, path: readonly number[], first: number, second: number, w1: number, w2: number): Node {
  if (!isSplit(node)) return node;
  const [step, ...rest] = path;
  if (step === undefined) {
    const weights = [...node.weights];
    weights[first] = w1;
    weights[second] = w2;
    return { ...node, weights };
  }
  return { ...node, children: node.children.map((child, i) => (i === step ? withWeights(child, rest, first, second, w1, w2) : child)) };
}

/** The weight of the child `index` of the split at `path`. */
export function weightAt(node: Node, path: readonly number[], index: number): number {
  let at: Node = node;
  for (const step of path) {
    if (!isSplit(at)) return 1;
    at = at.children[step] ?? at;
  }
  return isSplit(at) ? (at.weights[index] ?? 1) : 1;
}

// ── the grid ─────────────────────────────────────────────────────────────────

export interface DockGrid {
  /** `grid-template-columns` and `grid-template-rows`, with Status as the last row when it is shown. */
  columns: string;
  rows: string;
  /** The lines of each pane that is shown (`1 / 3`), and of Status. */
  place: Partial<Record<Movable | 'status', { column: string; row: string }>>;
}

/**
 * The tracks for a tree that is shown: every pane has its own edges, so the columns of the grid are the stretches between all
 * the places where a pane ends and the rows likewise, and a pane spans the stretches between its edges. Status, when it is
 * shown, is one row under all of them with `status` as its weight beside the weight 1 that the panes have together.
 */
export function dockGrid(tree: Node, shown: ReadonlySet<Movable>, statusShown: boolean, status: number | null): DockGrid | null {
  const boxes = rects(tree, shown);
  if (boxes.size === 0 && !statusShown) return null;
  const key = (x: number): number => Math.round(x * 1e6);
  const xs = new Set<number>([0, key(1)]);
  const ys = new Set<number>([0, key(1)]);
  for (const box of boxes.values()) {
    for (const x of [box.x0, box.x1]) xs.add(key(x));
    for (const y of [box.y0, box.y1]) ys.add(key(y));
  }
  const xLines = [...xs].sort((a, b) => a - b);
  const yLines = [...ys].sort((a, b) => a - b);
  const xIndex = new Map(xLines.map((x, i) => [x, i + 1]));
  const yIndex = new Map(yLines.map((y, i) => [y, i + 1]));
  const place: DockGrid['place'] = {};
  for (const [pane, box] of boxes) place[pane] = { column: `${xIndex.get(key(box.x0))} / ${xIndex.get(key(box.x1))}`, row: `${yIndex.get(key(box.y0))} / ${yIndex.get(key(box.y1))}` };
  const tracks = (lines: number[], scale: number): string[] => lines.slice(1).map((line, i) => `minmax(0, ${(Math.max(line - (lines[i] as number), 1) * scale).toFixed(0)}fr)`);
  const rows = boxes.size > 0 ? tracks(yLines, 1) : [];
  const columns = boxes.size > 0 ? tracks(xLines, 1) : ['minmax(0, 1fr)'];
  if (statusShown) {
    // the panes are 1e6 of height together; Status is `status` times that, or as high as it is (up to a limit) when nothing was set
    rows.push(status === null ? 'auto' : `minmax(0, ${Math.round(status * 1e6)}fr)`);
    place.status = { column: `1 / ${columns.length + 1}`, row: `${rows.length} / ${rows.length + 1}` };
  }
  return { columns: columns.join(' '), rows: rows.join(' '), place };
}

// ── checking what was stored ─────────────────────────────────────────────────

const isWeight = (n: unknown): n is number => typeof n === 'number' && Number.isFinite(n) && n >= 0.02 && n <= 50;

/** The tree in a stored value: a valid tree that holds each of the three panes once, or null. */
export function cleanTree(value: unknown, depth = 0): Node | null {
  if (typeof value === 'string') return isMovable(value) ? value : null;
  if (typeof value !== 'object' || value === null || depth > 4) return null;
  const { dir, children, weights } = value as { dir?: unknown; children?: unknown; weights?: unknown };
  if ((dir !== 'row' && dir !== 'col') || !Array.isArray(children) || children.length < 2 || children.length > 3) return null;
  if (!Array.isArray(weights) || weights.length !== children.length || !weights.every(isWeight)) return null;
  const inner: Node[] = [];
  for (const child of children) {
    const clean = cleanTree(child, depth + 1);
    if (clean === null) return null;
    inner.push(clean);
  }
  return { dir, children: inner, weights: [...weights] as number[] };
}

/** Whether a tree has each of the three movable panes exactly once. */
export function holdsEachOnce(tree: Node): boolean {
  const all = leaves(tree);
  return all.length === MOVABLE.length && new Set(all).size === MOVABLE.length;
}
