// Where the panes of the page are, and which are shown. The page has up to four places ("slots"): in a wide window
// the first three are the columns of the top row and the fourth is the row under them; in a medium one they are two
// columns, the third across, and the fourth across; in a narrow one they are one under the other. A layout is the
// order of the four panes and the set of those that are closed; the shown panes fill the places in that order. The
// Schema pane is closed like the others (the key x does not close it, the button does). The default is the order the panes are written in, all shown; the user can swap two
// and close the others, and it is kept in the browser's local storage. No DOM here except in `applyLayout`.

export type PaneName = 'schema' | 'parsed' | 'bytes' | 'status';

export const PANE_NAMES: readonly PaneName[] = ['schema', 'parsed', 'bytes', 'status'];
export const DEFAULT_ORDER: readonly PaneName[] = PANE_NAMES;

export interface Layout {
  /** All four panes, in the order of the places. */
  order: PaneName[];
  /** The panes that are closed. */
  hidden: PaneName[];
}

export const LAYOUT_KEY = 'doe.layout';
/** Version 1 had the order only; version 2 has the closed panes too. */
const VERSION = 2;

export const defaultLayout = (): Layout => ({ order: [...DEFAULT_ORDER], hidden: [] });

const isPane = (name: unknown): name is PaneName => PANE_NAMES.includes(name as PaneName);

/** The layout in a stored text, or null when it is not four different panes (junk, another version, nothing). */
export function parseLayout(text: string | null): Layout | null {
  if (text === null) return null;
  try {
    const value: unknown = JSON.parse(text);
    if (typeof value !== 'object' || value === null) return null;
    const { v, order, hidden } = value as { v?: unknown; order?: unknown; hidden?: unknown };
    if ((v !== 1 && v !== 2) || !Array.isArray(order) || order.length !== PANE_NAMES.length) return null;
    const names = order.filter(isPane);
    if (names.length !== PANE_NAMES.length || new Set(names).size !== PANE_NAMES.length) return null;
    // a record of version 1 has all the panes shown
    const closed = v === 1 || hidden === undefined ? [] : hidden;
    if (!Array.isArray(closed) || !closed.every(isPane) || new Set(closed).size !== closed.length) return null;
    return { order: names, hidden: closed };
  } catch {
    return null;
  }
}

export function layoutText(layout: Layout): string {
  return JSON.stringify({ v: VERSION, order: layout.order, hidden: layout.hidden });
}

/** The layout with two panes in each other's places. */
export function swapped(layout: Layout, a: PaneName, b: PaneName): Layout {
  return { ...layout, order: layout.order.map((name) => (name === a ? b : name === b ? a : name)) };
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
  return layout.hidden.length === 0 && layout.order.every((name, i) => name === DEFAULT_ORDER[i]);
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
  const inOrder = layout.order.every((name, i) => main.children[i]?.id === ids[name]);
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
