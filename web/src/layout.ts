// Where the panes of the page are. The page has four places ("slots"): in a wide window the first three are the
// columns of the top row and the fourth is the row under them; in a medium one they are two columns, the third
// across, and the fourth across; in a narrow one they are one under the other. A layout is the order of the four
// panes in the slots; the default is the order they are written in, and the user can swap two. It is kept in the
// browser's local storage. No DOM here except in `applyLayout`.

export type PaneName = 'schema' | 'parsed' | 'bytes' | 'status';

export const PANE_NAMES: readonly PaneName[] = ['schema', 'parsed', 'bytes', 'status'];
export const DEFAULT_LAYOUT: readonly PaneName[] = PANE_NAMES;

export const LAYOUT_KEY = 'doe.layout';
const VERSION = 1;

/** The layout in a stored text, or null when it is not four different panes (junk, another version, nothing). */
export function parseLayout(text: string | null): PaneName[] | null {
  if (text === null) return null;
  try {
    const value: unknown = JSON.parse(text);
    if (typeof value !== 'object' || value === null) return null;
    const { v, order } = value as { v?: unknown; order?: unknown };
    if (v !== VERSION || !Array.isArray(order) || order.length !== PANE_NAMES.length) return null;
    const names = order.filter((name): name is PaneName => PANE_NAMES.includes(name as PaneName));
    return new Set(names).size === PANE_NAMES.length && names.length === PANE_NAMES.length ? names : null;
  } catch {
    return null;
  }
}

export function layoutText(order: readonly PaneName[]): string {
  return JSON.stringify({ v: VERSION, order });
}

/** The layout with two panes in each other's places. */
export function swapped(order: readonly PaneName[], a: PaneName, b: PaneName): PaneName[] {
  return order.map((name) => (name === a ? b : name === b ? a : name));
}

export function isDefault(order: readonly PaneName[]): boolean {
  return order.every((name, i) => name === DEFAULT_LAYOUT[i]);
}

/** The saved layout, or the default one. Storage can be missing or blocked: then the default. */
export function loadLayout(): PaneName[] {
  try {
    return parseLayout(window.localStorage.getItem(LAYOUT_KEY)) ?? [...DEFAULT_LAYOUT];
  } catch {
    return [...DEFAULT_LAYOUT];
  }
}

export function saveLayout(order: readonly PaneName[]): void {
  try {
    if (isDefault(order)) window.localStorage.removeItem(LAYOUT_KEY);
    else window.localStorage.setItem(LAYOUT_KEY, layoutText(order));
  } catch {
    // no storage: the layout lasts until the page is closed
  }
}

/**
 * Put the panes of `main` in the order, with the number of the slot in `data-slot` (the style sheet puts a slot in its
 * place). The elements are moved in the page too, so that the order of the keyboard and of a screen reader is the
 * order on the screen.
 */
export function applyLayout(main: HTMLElement, ids: Record<PaneName, string>, order: readonly PaneName[]): void {
  order.forEach((name, i) => {
    const section = document.getElementById(ids[name]);
    if (!section) return;
    section.dataset.slot = String(i + 1);
    main.append(section);
  });
}
