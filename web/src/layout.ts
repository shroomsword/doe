// Where the panes of the page are, and which are shown. The page has up to four places ("slots"): in a wide window
// the first three are the columns of the top row and the fourth is the row under them; in a medium one they are two
// columns, the third across, and the fourth across; in a narrow one they are one under the other. A layout is the
// order of the four panes and the set of those that are closed; the shown panes fill the places in that order. The
// Schema pane is closed like the others (the key x does not close it, the button does). The default is the order the panes are written in, all shown; the user can swap two
// and close the others, and it is kept in the browser's local storage. No DOM here except in `applyLayout`.

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
  /** Tiles (`Layout.tiles`): the weights of the rows, one for each row of the tiling (not only those that are shown). */
  tileRows?: number[];
  /** Tiles: the weights of the panes of each row, in the order of the row. */
  tileCols?: number[][];
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
   * The tiles: rows of panes, from the top, each row from the left; every pane is in exactly one row, and `order` is the
   * rows one after the other. Missing means the arrangement that the width of the window gives (the places above).
   * In a narrow window the tiles are not used: the panes are one under the other, in `order`.
   */
  tiles?: PaneName[][];
}

export const LAYOUT_KEY = 'doe.layout';
/** Version 1 had the order only; version 2 has the closed panes too; version 3 has the sizes; version 4 has the tiles. */
const VERSION = 4;

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
  const { tileRows, tileCols } = value as { tileRows?: unknown; tileCols?: unknown };
  if (Array.isArray(tileRows) && tileRows.length > 0 && tileRows.length <= 4 && tileRows.every(isWeight)) out.tileRows = [...tileRows];
  if (Array.isArray(tileCols) && tileCols.length > 0 && tileCols.length <= 4 && tileCols.every((row) => Array.isArray(row) && row.length > 0 && row.length <= 4 && row.every(isWeight))) {
    out.tileCols = tileCols.map((row: number[]) => [...row]);
  }
  return out.cols || out.rows || out.tileRows || out.tileCols ? out : undefined;
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

/** Rows of panes that hold each pane once, in rows that are not empty: the tiles in a stored value, or null. */
export function cleanTiles(value: unknown): PaneName[][] | null {
  if (!Array.isArray(value) || value.length === 0 || value.length > PANE_NAMES.length) return null;
  const rows: PaneName[][] = [];
  const seen = new Set<string>();
  for (const row of value) {
    if (!Array.isArray(row) || row.length === 0) return null;
    for (const name of row) {
      if (!isPane(name) || seen.has(name)) return null;
      seen.add(name);
    }
    rows.push([...(row as PaneName[])]);
  }
  return seen.size === PANE_NAMES.length ? rows : null;
}

/** The layout in a stored text, or null when it is not four different panes (junk, another version, nothing). */
export function parseLayout(text: string | null): Layout | null {
  if (text === null) return null;
  try {
    const value: unknown = JSON.parse(text);
    if (typeof value !== 'object' || value === null) return null;
    const { v, order, hidden, sizes, tiles } = value as { v?: unknown; order?: unknown; hidden?: unknown; sizes?: unknown; tiles?: unknown };
    if (typeof v !== 'number' || !Number.isInteger(v) || v < 1 || !Array.isArray(order) || order.length !== PANE_NAMES.length) return null;
    const names = order.filter(isPane);
    if (names.length !== PANE_NAMES.length || new Set(names).size !== PANE_NAMES.length) return null;
    // a record of version 1 has all the panes shown
    const closed = v === 1 || hidden === undefined ? [] : hidden;
    if (!Array.isArray(closed) || !closed.every(isPane) || new Set(closed).size !== closed.length) return null;
    // a newer version is read as far as it is understood: the order and the closed panes, and sizes and tiles if they are valid
    const rows = v >= 4 ? cleanTiles(tiles) : null;
    // the order is the rows one after the other: junk in the tiles loses the tiles, not the order
    const layout: Layout = { order: rows ? rows.flat() : names, hidden: closed, sizes: v >= 3 ? parseSizes(sizes) : {} };
    if (rows) layout.tiles = rows;
    return layout;
  } catch {
    return null;
  }
}

export function layoutText(layout: Layout): string {
  return JSON.stringify({ v: VERSION, order: layout.order, hidden: layout.hidden, sizes: layout.sizes, ...(layout.tiles ? { tiles: layout.tiles } : {}) });
}

/** The layout with two panes in each other's places. */
export function swapped(layout: Layout, a: PaneName, b: PaneName): Layout {
  const swap = (name: PaneName): PaneName => (name === a ? b : name === b ? a : name);
  const next: Layout = { ...layout, order: layout.order.map(swap) };
  if (layout.tiles) next.tiles = layout.tiles.map((row) => row.map(swap));
  return next;
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
  return layout.hidden.length === 0 && layout.tiles === undefined && layout.order.every((name, i) => name === DEFAULT_ORDER[i]) && Object.keys(layout.sizes).length === 0;
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

/** The layout with `pane` taken out and put before `target`, or after it. With tiles, it goes into the row of `target`. */
export function moved(layout: Layout, pane: PaneName, target: PaneName, after: boolean): Layout {
  if (pane === target) return layout;
  if (layout.tiles) {
    const rows = layout.tiles.map((row) => row.filter((name) => name !== pane)).filter((row) => row.length > 0);
    const into = rows.find((row) => row.includes(target));
    if (!into) return layout;
    into.splice(into.indexOf(target) + (after ? 1 : 0), 0, pane);
    return withTiles(layout, rows);
  }
  const rest = layout.order.filter((name) => name !== pane);
  const at = rest.indexOf(target) + (after ? 1 : 0);
  return { ...layout, order: [...rest.slice(0, at), pane, ...rest.slice(at)] };
}

// ── the tiles ─────────────────────────────────────────────────────────────────

/** The layout with these tiles (undefined for the arrangement of the width of the window). The sizes of the tiles are dropped. */
export function withTiles(layout: Layout, rows: PaneName[][] | undefined): Layout {
  const sizes: SizeMap = {};
  for (const mode of MODES) {
    const kept = layout.sizes[mode];
    if (kept && (kept.cols || kept.rows)) sizes[mode] = { ...(kept.cols ? { cols: kept.cols } : {}), ...(kept.rows ? { rows: kept.rows } : {}) };
  }
  const next: Layout = { order: rows ? rows.flat() : layout.order, hidden: layout.hidden, sizes };
  if (rows) next.tiles = rows.map((row) => [...row]);
  return next;
}

/** The tiles, or the arrangement the wide window has (three columns and a row) made explicit, to start changing it from. */
export function tilesOf(layout: Layout): PaneName[][] {
  if (layout.tiles) return layout.tiles.map((row) => [...row]);
  const [a, b, c, d] = layout.order;
  return [[a, b, c], [d]].filter((row): row is PaneName[] => row.every((name) => name !== undefined)) as PaneName[][];
}

/** Presets: the arrangements to start from. */
export const TILE_PRESETS: readonly { id: string; label: string; rows: PaneName[][] | null }[] = [
  { id: 'window', label: 'By the width of the window', rows: null },
  { id: 'rows', label: 'Schema on top, Parsed and Bytes beside each other, Status at the bottom', rows: [['schema'], ['parsed', 'bytes'], ['status']] },
  { id: 'three', label: 'Schema, Parsed and Bytes in a row, Status under them', rows: [['schema', 'parsed', 'bytes'], ['status']] },
  { id: 'side', label: 'Schema on the left; Parsed and Bytes above Status on the right', rows: [['schema', 'parsed'], ['bytes', 'status']] },
  { id: 'stack', label: 'One under the other', rows: [['schema'], ['parsed'], ['bytes'], ['status']] },
  { id: 'line', label: 'All in one row', rows: [['schema', 'parsed', 'bytes', 'status']] },
];

/** The tiles with a pane one place to the left or the right in its row (nothing at the end of the row). */
export function movedInRow(rows: PaneName[][], pane: PaneName, step: -1 | 1): PaneName[][] | null {
  const next = rows.map((row) => [...row]);
  const row = next.find((r) => r.includes(pane));
  if (!row) return null;
  const at = row.indexOf(pane);
  const to = at + step;
  if (to < 0 || to >= row.length) return null;
  [row[at], row[to]] = [row[to] as PaneName, row[at] as PaneName];
  return next;
}

/**
 * The tiles with a pane in the row above or under: at the end of that row, or in a row of its own at the top or the bottom
 * when it is already in the first (or last) row and shares it. Nothing when it is alone in the first or last row.
 */
export function movedAcross(rows: PaneName[][], pane: PaneName, step: -1 | 1): PaneName[][] | null {
  const index = rows.findIndex((r) => r.includes(pane));
  if (index < 0) return null;
  const from = rows[index] as PaneName[];
  const target = index + step;
  const rest = rows.map((row) => row.filter((name) => name !== pane));
  if (target < 0 || target >= rows.length) {
    if (from.length === 1) return null;
    if (step < 0) rest.unshift([pane]);
    else rest.push([pane]);
  } else {
    (rest[target] as PaneName[]).push(pane);
  }
  return rest.filter((row) => row.length > 0);
}

/** A row of the tiling as it is drawn: the panes that are shown in it, and where they are in the row of the tiles. */
export interface TileRow {
  panes: PaneName[];
  /** The index of the row in `Layout.tiles`. */
  row: number;
  /** The index of each shown pane in that row. */
  members: number[];
}

/** The rows of the tiling that have a pane shown (closed panes leave their rows and the rows with none are gone). */
export function tileRows(layout: Layout): TileRow[] {
  const out: TileRow[] = [];
  (layout.tiles ?? []).forEach((row, index) => {
    const members = row.map((_, i) => i).filter((i) => !layout.hidden.includes(row[i] as PaneName));
    if (members.length > 0) out.push({ panes: members.map((i) => row[i] as PaneName), row: index, members });
  });
  return out;
}

/** The weight of a row that the user has not sized: a row that holds only the Status pane is low. */
export const defaultRowWeight = (panes: readonly PaneName[]): number => (panes.length === 1 && panes[0] === 'status' ? 0.4 : 1);

/** The rows' weights, for every row of the tiles (the size set by the user where there is a valid one). */
export function rowWeights(layout: Layout, sizes: Sizes | undefined): number[] {
  const rows = layout.tiles ?? [];
  const set = sizes?.tileRows;
  return rows.map((row, i) => (set && set.length === rows.length ? (set[i] as number) : defaultRowWeight(row.filter((name) => !layout.hidden.includes(name)))));
}

/** The panes' weights in every row of the tiles. */
export function columnWeights(layout: Layout, sizes: Sizes | undefined): number[][] {
  const rows = layout.tiles ?? [];
  const set = sizes?.tileCols;
  return rows.map((row, i) => {
    const mine = set && set.length === rows.length ? set[i] : undefined;
    return row.map((_, j) => (mine && mine.length === row.length ? (mine[j] as number) : 1));
  });
}

/** Where the tiles are on a grid: the columns and the rows as CSS track lists, and the lines of each pane. */
export interface TileGrid {
  columns: string;
  rows: string;
  place: Partial<Record<PaneName, { column: string; row: string }>>;
}

/**
 * The grid for the tiling: every row has its own widths, so the columns of the grid are the stretches between all the
 * places where a pane ends in any row, and a pane spans the stretches between its edges.
 */
export function tileGrid(layout: Layout, sizes: Sizes | undefined): TileGrid | null {
  const rows = tileRows(layout);
  if (rows.length === 0) return null;
  const heights = rowWeights(layout, sizes);
  const widths = columnWeights(layout, sizes);
  const key = (x: number): number => Math.round(x * 1e6);
  const edges = new Set<number>([0, key(1)]);
  const spans = rows.map((r) => {
    const weights = r.members.map((i) => widths[r.row]?.[i] ?? 1);
    const total = weights.reduce((a, b) => a + b, 0);
    let at = 0;
    return weights.map((w) => {
      const start = key(at / total);
      at += w;
      const end = key(at / total);
      edges.add(start);
      edges.add(end);
      return [start, end] as const;
    });
  });
  const lines = [...edges].sort((a, b) => a - b);
  const column = new Map(lines.map((edge, i) => [edge, i + 1]));
  const place: TileGrid['place'] = {};
  rows.forEach((r, ri) => {
    r.panes.forEach((name, pi) => {
      const span = spans[ri]?.[pi] ?? [0, key(1)];
      place[name] = { column: `${column.get(span[0])} / ${column.get(span[1])}`, row: `${ri + 1} / ${ri + 2}` };
    });
  });
  return {
    columns: lines.slice(1).map((edge, i) => `minmax(0, ${Math.max(edge - (lines[i] as number), 1)}fr)`).join(' '),
    rows: rows.map((r) => `minmax(0, ${(heights[r.row] ?? 1).toFixed(4)}fr)`).join(' '),
    place,
  };
}

/** The mode the style sheet is in. */
export function currentMode(): Mode {
  const value = getComputedStyle(document.documentElement).getPropertyValue('--mode').trim();
  return value === 'wide' || value === 'medium' ? value : 'narrow';
}

/**
 * Draw the tiles when the layout has them and the window is wide enough (`data-tiled` on `main`, the grid in custom
 * properties, the place of each pane in the pane's own); otherwise take them away. Returns whether the tiles are drawn.
 */
export function applyTiles(main: HTMLElement, ids: Record<PaneName, string>, layout: Layout, mode: Mode, sizes: Sizes | undefined): boolean {
  const grid = layout.tiles && mode !== 'narrow' ? tileGrid(layout, sizes) : null;
  for (const name of PANE_NAMES) {
    const section = document.getElementById(ids[name]);
    const at = grid?.place[name];
    if (!section) continue;
    if (at) {
      section.style.setProperty('--tile-column', at.column);
      section.style.setProperty('--tile-row', at.row);
    } else {
      section.style.removeProperty('--tile-column');
      section.style.removeProperty('--tile-row');
    }
  }
  if (!grid) {
    delete main.dataset.tiled;
    main.style.removeProperty('--tile-columns');
    main.style.removeProperty('--tile-rows');
    return false;
  }
  main.dataset.tiled = '1';
  main.style.setProperty('--tile-columns', grid.columns);
  main.style.setProperty('--tile-rows', grid.rows);
  return true;
}
