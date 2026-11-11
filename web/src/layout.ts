// Where the panes of the page are, and which are shown. The page has up to four places ("slots"): in a wide window
// the first three are the columns of the top row and the fourth is the row under them; in a medium one they are two
// columns, the third across, and the fourth across; in a narrow one they are one under the other. A layout is the
// order of the four panes and the set of those that are closed; the shown panes fill the places in that order. The
// Schema pane is closed like the others (the key x does not close it, the button does). The default is the order the panes are written in, all shown; the user can swap two
// and close the others, and it is kept in the browser's local storage. No DOM here except in `applyLayout`.

import { DEFAULT_STATUS, type Movable, type Node, MOVABLE, cleanTree, dockGrid, defaultTree, holdsEachOnce, isMovable, leaves, swapIn, tidy } from './dock.js';

export type PaneName = 'schema' | 'parsed' | 'bytes' | 'status';

export const PANE_NAMES: readonly PaneName[] = ['schema', 'parsed', 'bytes', 'status'];
export const DEFAULT_ORDER: readonly PaneName[] = PANE_NAMES;

/** The three layouts the style sheet has (see `--mode` there): wide (three columns and a row), medium (two columns) and narrow (one). */
export type Mode = 'wide' | 'medium' | 'narrow';
export const MODES: readonly Mode[] = ['wide', 'medium', 'narrow'];

/**
 * The sizes of the places in one mode. They belong to the places (the columns and rows of the grid), not to the panes: the
 * pane in the second place has the second column's width whichever pane that is, and a closed pane's share goes to the
 * ones that are left. Missing is the default size.
 */
export interface Sizes {
  /** The weights of the columns (1 is the default): up to three in the wide mode, two in the medium one. */
  cols?: number[];
  /**
   * Wide: the weights of the top row and of the row under it. Medium and narrow: the heights of the places 1 to 4 (the
   * rows in the medium mode are those of places 1, 3 and 4), as a fraction of the height of the window; null is the default.
   */
  rows?: (number | null)[];
  /** Not kept with the sizes: the dock that a separator of it made, on its way to the layout (resize.ts). */
  dock?: Dock;
}

export type SizeMap = Partial<Record<Mode, Sizes>>;

export interface Layout {
  /** All four panes, in the order of the places. */
  order: PaneName[];
  /** The panes that are closed. */
  hidden: PaneName[];
  /** The sizes the user set, for each mode that has some. */
  sizes: SizeMap;
  /**
   * The panes docked by the user: Schema, Parsed and Bytes as a tree of splits above the Status pane, which is fixed along the
   * bottom. Missing means the arrangement that the width of the window gives (the places above). `order` is then the panes of
   * the tree in reading order, and Status last. In a narrow window the dock is not used: the panes are one under the other, in `order`.
   */
  dock?: Dock;
}

/** The arrangement of the three movable panes, and how much of the height Status has beside them (1). */
export interface Dock {
  tree: Node;
  /** Status beside the panes (1) when set with a separator; null is its own size, low (as in the arrangements of the window). */
  status: number | null;
}

export const LAYOUT_KEY = 'doe.layout';
/** Version 1 had the order only; version 2 has the closed panes too; version 3 has the sizes; version 4 had the tiles (rows of panes, which version 5 reads as a tree); version 5 has the dock. */
const VERSION = 5;

export const defaultLayout = (): Layout => ({ order: [...DEFAULT_ORDER], hidden: [], sizes: {} });

const isWeight = (n: unknown): n is number => typeof n === 'number' && Number.isFinite(n) && n >= 0.05 && n <= 20;
const isFraction = (n: unknown): n is number => typeof n === 'number' && Number.isFinite(n) && n >= 0.05 && n <= 5;

/** The sizes of a mode in a stored value: what is valid, and nothing for what is not (junk loses only the sizes). */
export function cleanSizes(value: unknown): Sizes | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const { cols, rows } = value as { cols?: unknown; rows?: unknown };
  const out: Sizes = {};
  if (Array.isArray(cols) && cols.length > 0 && cols.length <= 3 && cols.every(isWeight)) out.cols = [...cols];
  if (Array.isArray(rows) && rows.length > 0 && rows.length <= 4 && rows.every((n) => n === null || isFraction(n)) && rows.some((n) => n !== null)) {
    out.rows = [...rows] as (number | null)[];
  }
  return out.cols || out.rows ? out : undefined;
}

/** The sizes in a stored value. */
function parseSizes(value: unknown): SizeMap {
  const out: SizeMap = {};
  if (typeof value !== 'object' || value === null) return out;
  for (const mode of MODES) {
    const sizes = cleanSizes((value as Record<string, unknown>)[mode]);
    if (sizes) out[mode] = sizes;
  }
  return out;
}

/** The layout with the sizes of one mode set (undefined puts the default back). */
export function withSizes(layout: Layout, mode: Mode, sizes: Sizes | undefined): Layout {
  const next: SizeMap = { ...layout.sizes };
  const clean = cleanSizes(sizes);
  if (clean) next[mode] = clean;
  else delete next[mode];
  return { ...layout, sizes: next };
}

const isPane = (name: unknown): name is PaneName => PANE_NAMES.includes(name as PaneName);

/** Rows of panes (the tiles of version 4) as the tree that is docked now: Status leaves them, it is fixed along the bottom. */
function treeOfRows(value: unknown): Node | null {
  if (!Array.isArray(value) || value.length === 0 || value.length > PANE_NAMES.length) return null;
  const rows: Movable[][] = [];
  const seen = new Set<string>();
  for (const row of value) {
    if (!Array.isArray(row) || row.length === 0) return null;
    for (const name of row) {
      if (!isPane(name) || seen.has(name)) return null;
      seen.add(name);
    }
    const movable = (row as PaneName[]).filter(isMovable);
    if (movable.length > 0) rows.push(movable);
  }
  if (seen.size !== PANE_NAMES.length || rows.length === 0) return null;
  const rowNodes: Node[] = rows.map((r) => (r.length === 1 ? (r[0] as Movable) : { dir: 'row', children: r, weights: r.map(() => 1) }));
  const tree = tidy(rowNodes.length === 1 ? (rowNodes[0] as Node) : { dir: 'col', children: rowNodes, weights: rowNodes.map(() => 1) });
  return holdsEachOnce(tree) ? tree : null;
}

/** The order of the places that a dock gives: the panes of the tree in reading order, and Status. */
export const dockedOrder = (dock: Dock): PaneName[] => [...leaves(dock.tree), 'status'];

/** The layout in a stored text, or null when it is not four different panes (junk, another version, nothing). */
export function parseLayout(text: string | null): Layout | null {
  if (text === null) return null;
  try {
    const value: unknown = JSON.parse(text);
    if (typeof value !== 'object' || value === null) return null;
    const { v, order, hidden, sizes, dock, tiles } = value as { v?: unknown; order?: unknown; hidden?: unknown; sizes?: unknown; dock?: unknown; tiles?: unknown };
    if (typeof v !== 'number' || !Number.isInteger(v) || v < 1 || !Array.isArray(order) || order.length !== PANE_NAMES.length) return null;
    const names = order.filter(isPane);
    if (names.length !== PANE_NAMES.length || new Set(names).size !== PANE_NAMES.length) return null;
    // a record of version 1 has all the panes shown
    const closed = v === 1 || hidden === undefined ? [] : hidden;
    if (!Array.isArray(closed) || !closed.every(isPane) || new Set(closed).size !== closed.length) return null;
    // a newer version is read as far as it is understood: the order and the closed panes, and sizes and the dock if they are valid
    let docked: Dock | null = null;
    if (v >= 5 && typeof dock === 'object' && dock !== null) {
      const { tree, status } = dock as { tree?: unknown; status?: unknown };
      const clean = cleanTree(tree);
      if (clean && holdsEachOnce(clean) && (status === null || isWeight(status))) docked = { tree: clean, status };
    } else if (v === 4) {
      // the rows of version 4 are the nearest tree
      const tree = treeOfRows(tiles);
      if (tree) docked = { tree, status: DEFAULT_STATUS };
    }
    // the order is the panes of the dock in reading order: junk in the dock loses the dock, not the order
    const layout: Layout = { order: docked ? dockedOrder(docked) : names, hidden: closed, sizes: v >= 3 ? parseSizes(sizes) : {} };
    if (docked) layout.dock = docked;
    return layout;
  } catch {
    return null;
  }
}

export function layoutText(layout: Layout): string {
  return JSON.stringify({ v: VERSION, order: layout.order, hidden: layout.hidden, sizes: layout.sizes, ...(layout.dock ? { dock: layout.dock } : {}) });
}

/** The layout with two panes in each other's places. */
export function swapped(layout: Layout, a: PaneName, b: PaneName): Layout {
  // Status is fixed along the bottom
  if (!isMovable(a) || !isMovable(b)) return layout;
  if (layout.dock) {
    const dock = { ...layout.dock, tree: swapIn(layout.dock.tree, a, b) };
    return { ...layout, order: dockedOrder(dock), dock };
  }
  const swap = (name: PaneName): PaneName => (name === a ? b : name === b ? a : name);
  return { ...layout, order: layout.order.map(swap) };
}

/** The panes that are shown, in the order of the places. */
export function shown(layout: Layout): PaneName[] {
  return layout.order.filter((name) => !layout.hidden.includes(name));
}

/** The layout with a pane shown or closed. */
export function withVisible(layout: Layout, pane: PaneName, visible: boolean): Layout {
  const hidden = layout.hidden.filter((name) => name !== pane);
  if (!visible) hidden.push(pane);
  return { ...layout, hidden: PANE_NAMES.filter((name) => hidden.includes(name)) };
}

export function isDefault(layout: Layout): boolean {
  return layout.hidden.length === 0 && layout.dock === undefined && layout.order.every((name, i) => name === DEFAULT_ORDER[i]) && Object.keys(layout.sizes).length === 0;
}

/** The saved layout, or the default one. Storage can be missing or blocked: then the default. */
export function loadLayout(): Layout {
  try {
    return parseLayout(window.localStorage.getItem(LAYOUT_KEY)) ?? defaultLayout();
  } catch {
    return defaultLayout();
  }
}

export function saveLayout(layout: Layout): void {
  try {
    if (isDefault(layout)) window.localStorage.removeItem(LAYOUT_KEY);
    else window.localStorage.setItem(LAYOUT_KEY, layoutText(layout));
  } catch {
    // no storage: the layout lasts until the page is closed
  }
}

/**
 * Put the panes of `main` in the order, the shown ones with the number of their place in `data-slot` (the style sheet
 * puts a place in its place) and the closed ones hidden, and the number of shown panes in `main`'s `data-count`. The
 * elements are moved in the page too, so that the order of the keyboard and of a screen reader is the order on the
 * screen.
 */
export function applyLayout(main: HTMLElement, ids: Record<PaneName, string>, layout: Layout): void {
  const open = shown(layout);
  main.dataset.count = String(open.length);
  // moving an element takes the focus out of it, so only the order that changed is applied
  const sections = [...main.children].filter((child) => child.tagName === 'SECTION');
  const inOrder = layout.order.every((name, i) => sections[i]?.id === ids[name]);
  layout.order.forEach((name) => {
    const section = document.getElementById(ids[name]);
    if (!section) return;
    const place = open.indexOf(name);
    section.hidden = place < 0;
    if (place < 0) delete section.dataset.slot;
    else section.dataset.slot = String(place + 1);
    if (!inOrder) main.append(section);
  });
}

// ── the dock ─────────────────────────────────────────────────────────────────

/** The dock of a layout, or the arrangement of the wide window made explicit, to start docking from. */
export function dockOf(layout: Layout): Dock {
  if (layout.dock) return layout.dock;
  const order = layout.order.filter(isMovable);
  return { tree: defaultTree(order), status: DEFAULT_STATUS };
}

/** The layout with this dock (undefined for the arrangement of the width of the window). */
export function withDock(layout: Layout, dock: Dock | undefined): Layout {
  const next: Layout = { ...layout };
  if (dock) {
    next.dock = dock;
    next.order = dockedOrder(dock);
  } else {
    delete next.dock;
  }
  return next;
}

/** The panes of the dock that are shown (closed panes leave the tree, and Status is not in it). */
export const shownMovable = (layout: Layout): Set<Movable> => new Set(MOVABLE.filter((name) => !layout.hidden.includes(name)));

/** The mode the style sheet is in. */
export function currentMode(): Mode {
  const value = getComputedStyle(document.documentElement).getPropertyValue('--mode').trim();
  return value === 'wide' || value === 'medium' ? value : 'narrow';
}

/**
 * Draw the dock when the layout has one and the window is wide enough (`data-docked` on `main`, the grid in custom properties,
 * the place of each pane in the pane's own); otherwise take it away. `dock` is what to draw when it is not the layout's own
 * (the sizes while a separator is being dragged). Returns whether the dock is drawn.
 */
export function applyDock(main: HTMLElement, ids: Record<PaneName, string>, layout: Layout, mode: Mode, dock: Dock | undefined = layout.dock): boolean {
  const grid = dock && mode !== 'narrow' ? dockGrid(dock.tree, shownMovable(layout), !layout.hidden.includes('status'), dock.status) : null;
  for (const name of PANE_NAMES) {
    const section = document.getElementById(ids[name]);
    const at = grid?.place[name];
    if (!section) continue;
    if (at) {
      section.style.setProperty('--dock-column', at.column);
      section.style.setProperty('--dock-row', at.row);
    } else {
      section.style.removeProperty('--dock-column');
      section.style.removeProperty('--dock-row');
    }
  }
  if (!grid) {
    delete main.dataset.docked;
    main.style.removeProperty('--dock-columns');
    main.style.removeProperty('--dock-rows');
    main.style.removeProperty('--dock-status-max');
    return false;
  }
  main.dataset.docked = '1';
  // Status is as high as it is, up to a limit, until a separator has given it a share
  main.style.setProperty('--dock-status-max', dock?.status === null ? '14rem' : 'none');
  main.style.setProperty('--dock-columns', grid.columns);
  main.style.setProperty('--dock-rows', grid.rows);
  return true;
}
