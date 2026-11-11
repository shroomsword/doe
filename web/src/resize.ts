// Resizing the panes by dragging the separators between them (or with the keys of a window splitter). The grid of `main`
// is the style sheet's: this only sets custom properties on it (`--c1`.. for the widths of the columns, `--r1` and `--r2`
// for the two rows of the wide layout, `--h1`.. for the heights of the places in the layouts that scroll), and puts a
// separator in each gutter that has two neighbours. Which separators there are, and what a drag changes, depends on the
// mode the style sheet is in (`--mode` on the root: wide, medium or narrow) and on how many panes are shown, so it is read
// from the page each time. The sizes belong to the places, not to the panes (layout.ts), are proportions, and are kept
// by the page (`change`) when a drag ends.

import { DEFAULT_STATUS, type Movable, splitsOf, view, viewPanes, weightAt, withWeights } from './dock.js';
import { type Dock, type Layout, type Mode, type PaneName, type Sizes, applyDock, currentMode, shown, shownMovable } from './layout.js';

/** The least room a pane keeps, in rem: enough for its title bar and its buttons. */
const MIN_WIDTH = 12;
const MIN_HEIGHT = 6;
/** How far a key moves a separator, and with Shift, as a fraction of the room of the two panes (or of the window's height). */
const STEP = 0.05;
const BIG_STEP = 0.2;
/** The most a place in a layout that scrolls can be, as a fraction of the window's height. */
const MAX_HEIGHT = 4;

/** What a separator changes. `columns`: the widths of two columns that share a row; `rows`: how the wide layout's two rows
 * share the height; `height`: the height of one row of a layout that scrolls (the page gets longer or shorter). */
type Kind = 'columns' | 'rows' | 'height' | 'dock' | 'dock-status';

const isVertical = (split: Split): boolean => split.kind === 'columns' || (split.kind === 'dock' && split.dir === 'row');

interface Split {
  kind: Kind;
  /** `columns`: the column to the left; `height`: the index in `Sizes.rows` of the row above the separator. */
  index: number;
  /** The places on the two sides (1 to 4): the last one before the separator and the first one after it. */
  before: number;
  after: number;
  /** The places in the row above (`rows`, `height`): their bottom edge is where the separator is. */
  above: number[];
  /** How many columns the mode has (`columns`). */
  columns: number;
  mode: Mode;
  /** The dock (`dock`): the split in the tree (the child index at each step from the root), which way it goes, the two children
   * the separator is between (their indexes in the split, closed panes counted), and the panes of each side. */
  path?: number[];
  dir?: 'row' | 'col';
  first?: number;
  second?: number;
  beforePanes?: PaneName[];
  afterPanes?: PaneName[];
}

export interface SplitterOptions {
  main: HTMLElement;
  /** The element the separators are put in: `position: absolute` over `main`. */
  host: HTMLElement;
  layout(): Layout;
  /** The ids of the panes' elements: the dock is drawn on them. */
  ids: Record<PaneName, string>;
  /** The name of the pane in a place (1 to 4). */
  name(place: number): string;
  /** A drag or a key ended: the new sizes of the mode, or undefined for the default. */
  change(mode: Mode, sizes: Sizes | undefined, message: string): void;
}

const clamp = (n: number, low: number, high: number): number => Math.min(Math.max(n, low), Math.max(high, low));

export class Splitters {
  private readonly bars: HTMLElement[] = [];
  /** The animation frames that are waiting: to move the separators (after a resize), and to show a drag. */
  private frame = 0;
  private dragFrame = 0;
  /** A drag in progress: where it began and what to put back when it is cancelled. */
  private drag: { bar: HTMLElement; split: Split; pointer: number; start: number; room: number; first: number; sizes: Sizes | undefined } | null = null;

  constructor(private readonly options: SplitterOptions) {
    const refresh = (): void => this.schedule();
    new ResizeObserver(refresh).observe(options.main);
    window.addEventListener('resize', refresh);
  }

  /** The mode the style sheet is in. */
  mode(): Mode {
    return currentMode();
  }

  /** Apply the sizes of the layout for the mode the page is in, and put the separators where the panes are. */
  sync(): void {
    this.paint(this.options.layout().sizes[this.mode()]);
    this.place();
  }

  private schedule(): void {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.sync();
    });
  }

  // ── the custom properties ──────────────────────────────────────────────────

  /** Set the custom properties of `main` from the sizes of the mode (nothing for the default). */
  private paint(sizes: Sizes | undefined): void {
    const { main } = this.options;
    for (const name of ['--c1', '--c2', '--c3', '--r1', '--r2', '--h1', '--h2', '--h3', '--h4', '--max4']) main.style.removeProperty(name);
    // the dock, when the layout has one and the window is wide enough, is drawn instead of the places
    const layout = this.options.layout();
    if (applyDock(main, this.options.ids, layout, this.mode(), sizes?.dock ?? layout.dock)) return;
    if (!sizes) return;
    sizes.cols?.forEach((weight, i) => main.style.setProperty(`--c${i + 1}`, `${weight.toFixed(4)}fr`));
    const mode = this.mode();
    if (mode === 'wide') {
      if (sizes.rows) {
        main.style.setProperty('--r1', `${(sizes.rows[0] ?? 1).toFixed(4)}fr`);
        main.style.setProperty('--r2', `${(sizes.rows[1] ?? 1).toFixed(4)}fr`);
        // the low row is no longer held to its content
        main.style.setProperty('--max4', 'none');
      }
    } else {
      sizes.rows?.forEach((fraction, i) => {
        if (fraction !== null) main.style.setProperty(`--h${i + 1}`, `${(fraction * 100).toFixed(3)}dvh`);
      });
    }
  }

  // ── which separators there are ─────────────────────────────────────────────

  private splits(): Split[] {
    const count = Number(this.options.main.dataset.count ?? 0);
    const mode = this.mode();
    const out: Split[] = [];
    if (count < 2 || this.options.main.classList.contains('has-max')) return out;
    if (this.options.main.dataset.docked) return this.dockSplits(mode);
    const make = (kind: Kind, index: number, before: number, after: number, above: number[], columns: number): void => {
      out.push({ kind, index, before, after, above, columns, mode });
    };
    if (mode === 'wide') {
      const top = Math.min(count, 3);
      for (let i = 1; i < top; i += 1) make('columns', i - 1, i, i + 1, [], 3);
      if (count === 4) make('rows', 0, 3, 4, [1, 2, 3], 3);
    } else if (mode === 'medium') {
      make('columns', 0, 1, 2, [], 2);
      if (count >= 3) make('height', 0, 2, 3, [1, 2], 2);
      if (count === 4) make('height', 2, 3, 4, [3], 2);
    } else {
      for (let k = 1; k < count; k += 1) make('height', k - 1, k, k + 1, [k], 1);
    }
    return out;
  }

  /** The separators of the dock: between the children of each split of the panes that are shown, and between them and Status. */
  private dockSplits(mode: Mode): Split[] {
    const layout = this.options.layout();
    const dock = layout.dock;
    if (!dock) return [];
    const tree = view(dock.tree, shownMovable(layout));
    const open = shown(layout);
    const slot = (names: PaneName[]): number => open.indexOf(names[0] ?? 'schema') + 1;
    const out: Split[] = [];
    for (const split of splitsOf(tree)) {
      for (let i = 0; i + 1 < split.children.length; i += 1) {
        const before = viewPanes(split.children[i] as NonNullable<typeof tree>);
        const after = viewPanes(split.children[i + 1] as NonNullable<typeof tree>);
        out.push({ kind: 'dock', index: i, before: slot(before), after: slot(after), above: [], columns: 0, mode, path: split.path, dir: split.dir, first: split.from[i], second: split.from[i + 1], beforePanes: before, afterPanes: after });
      }
    }
    if (tree && !layout.hidden.includes('status')) {
      out.push({ kind: 'dock-status', index: 0, before: slot(viewPanes(tree)), after: open.indexOf('status') + 1, above: [], columns: 0, mode, beforePanes: viewPanes(tree), afterPanes: ['status'] });
    }
    return out;
  }

  /** The box, in pixels from the top left of `main`, that the panes `names` fill together. */
  private box(names: readonly PaneName[]): { left: number; top: number; right: number; bottom: number } | null {
    let box: { left: number; top: number; right: number; bottom: number } | null = null;
    for (const name of names) {
      const section = document.getElementById(this.options.ids[name]);
      if (!section || section.hidden) continue;
      const mine = { left: section.offsetLeft, top: section.offsetTop, right: section.offsetLeft + section.offsetWidth, bottom: section.offsetTop + section.offsetHeight };
      box = box ? { left: Math.min(box.left, mine.left), top: Math.min(box.top, mine.top), right: Math.max(box.right, mine.right), bottom: Math.max(box.bottom, mine.bottom) } : mine;
    }
    return box;
  }

  private section(place: number): HTMLElement | null {
    return this.options.main.querySelector<HTMLElement>(`:scope > section[data-slot="${place}"]:not([hidden])`);
  }

  private label(split: Split): string {
    return `Resize ${this.options.name(split.before)} and ${this.options.name(split.after)}`;
  }

  /** (Re)build the separators for the panes that are shown and put them in the gutters. */
  private place(): void {
    if (this.drag) return;
    const splits = this.splits();
    while (this.bars.length > splits.length) this.bars.pop()?.remove();
    while (this.bars.length < splits.length) this.bars.push(this.createBar());
    splits.forEach((split, i) => {
      const bar = this.bars[i];
      if (!bar) return;
      bar.dataset.split = JSON.stringify(split);
      bar.setAttribute('aria-label', this.label(split));
      this.position(bar, split);
    });
  }

  private createBar(): HTMLElement {
    const bar = document.createElement('div');
    bar.className = 'splitter';
    bar.setAttribute('role', 'separator');
    bar.tabIndex = 0;
    bar.addEventListener('pointerdown', (event) => this.press(bar, event));
    bar.addEventListener('keydown', (event) => this.key(bar, event));
    bar.addEventListener('dblclick', () => this.reset(bar));
    this.options.host.append(bar);
    return bar;
  }

  private splitOf(bar: HTMLElement): Split {
    return JSON.parse(bar.dataset.split ?? '{}') as Split;
  }

  /** Put the bar in its gutter, and say what it is at: the orientation and the share of the first side. */
  private position(bar: HTMLElement, split: Split): void {
    if (split.kind === 'dock' || split.kind === 'dock-status') {
      this.positionDock(bar, split);
      return;
    }
    const before = this.section(split.before);
    const after = this.section(split.after);
    if (!before || !after) return;
    const vertical = isVertical(split);
    bar.setAttribute('aria-orientation', vertical ? 'vertical' : 'horizontal');
    bar.dataset.orientation = vertical ? 'vertical' : 'horizontal';
    const thick = parseFloat(getComputedStyle(bar).getPropertyValue('--splitter')) || 16;
    if (vertical) {
      const gutter = after.offsetLeft - (before.offsetLeft + before.offsetWidth);
      const top = Math.min(before.offsetTop, after.offsetTop);
      const bottom = Math.max(before.offsetTop + before.offsetHeight, after.offsetTop + after.offsetHeight);
      Object.assign(bar.style, { left: `${before.offsetLeft + before.offsetWidth + gutter / 2 - thick / 2}px`, top: `${top}px`, width: `${thick}px`, height: `${bottom - top}px` });
    } else {
      const upper = split.above.map((place) => this.section(place)).filter((s): s is HTMLElement => s !== null);
      const bottom = Math.max(...upper.map((s) => s.offsetTop + s.offsetHeight));
      const gutter = after.offsetTop - bottom;
      const left = Math.min(...upper.map((s) => s.offsetLeft), after.offsetLeft);
      const right = Math.max(...upper.map((s) => s.offsetLeft + s.offsetWidth), after.offsetLeft + after.offsetWidth);
      Object.assign(bar.style, { top: `${bottom + gutter / 2 - thick / 2}px`, left: `${left}px`, height: `${thick}px`, width: `${right - left}px` });
    }
    this.values(bar, split);
  }

  private positionDock(bar: HTMLElement, split: Split): void {
    const before = this.box(split.beforePanes ?? []);
    const after = this.box(split.afterPanes ?? []);
    if (!before || !after) return;
    const vertical = isVertical(split);
    bar.setAttribute('aria-orientation', vertical ? 'vertical' : 'horizontal');
    bar.dataset.orientation = vertical ? 'vertical' : 'horizontal';
    const thick = parseFloat(getComputedStyle(bar).getPropertyValue('--splitter')) || 16;
    if (vertical) {
      const gutter = after.left - before.right;
      const top = Math.min(before.top, after.top);
      const bottom = Math.max(before.bottom, after.bottom);
      Object.assign(bar.style, { left: `${before.right + gutter / 2 - thick / 2}px`, top: `${top}px`, width: `${thick}px`, height: `${bottom - top}px` });
    } else {
      const gutter = after.top - before.bottom;
      const left = Math.min(before.left, after.left);
      const right = Math.max(before.right, after.right);
      Object.assign(bar.style, { top: `${before.bottom + gutter / 2 - thick / 2}px`, left: `${left}px`, height: `${thick}px`, width: `${right - left}px` });
    }
    this.values(bar, split);
  }

  // ── the sizes in pixels ────────────────────────────────────────────────────

  /** The size of the first side and the room of both sides, in pixels, and the least the first side can have. */
  private measure(split: Split): { first: number; room: number; min: number; max: number } {
    const rem = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
    if (split.kind === 'dock' || split.kind === 'dock-status') {
      const a = this.box(split.beforePanes ?? []);
      const b = this.box(split.afterPanes ?? []);
      if (!a || !b) return { first: 0, room: 0, min: 0, max: 0 };
      const wide = isVertical(split);
      const first = wide ? a.right - a.left : a.bottom - a.top;
      const room = first + (wide ? b.right - b.left : b.bottom - b.top);
      const min = Math.min((wide ? MIN_WIDTH : MIN_HEIGHT) * rem, room / 2);
      return { first, room, min, max: room - min };
    }
    const before = this.section(split.before);
    const after = this.section(split.after);
    if (!before || !after) return { first: 0, room: 0, min: 0, max: 0 };
    if (isVertical(split)) {
      const first = before.getBoundingClientRect().width;
      const room = first + after.getBoundingClientRect().width;
      const min = Math.min(MIN_WIDTH * rem, room / 2);
      return { first, room, min, max: room - min };
    }
    const upper = this.section(split.above[0] ?? split.before);
    if (split.kind === 'rows') {
      const first = (upper ?? before).getBoundingClientRect().height;
      const room = first + after.getBoundingClientRect().height;
      const min = Math.min(MIN_HEIGHT * rem, room / 2);
      return { first, room, min, max: room - min };
    }
    const first = (upper ?? before).getBoundingClientRect().height;
    return { first, room: window.innerHeight, min: MIN_HEIGHT * rem, max: MAX_HEIGHT * window.innerHeight };
  }

  /** The ARIA values of a bar: the share of the first side, in percent. */
  private values(bar: HTMLElement, split: Split): void {
    const { first, room, min, max } = this.measure(split);
    if (room <= 0) return;
    bar.setAttribute('aria-valuenow', String(Math.round((first / room) * 100)));
    bar.setAttribute('aria-valuemin', String(Math.round((min / room) * 100)));
    bar.setAttribute('aria-valuemax', String(Math.round((max / room) * 100)));
  }

  /** The sizes of the mode with the first side of `split` at `size` pixels, from the sizes `base`. */
  private resized(split: Split, size: number, base: Sizes | undefined): Sizes {
    const { room, min, max, first } = this.measure(split);
    const target = clamp(size, min, max);
    const out: Sizes = { ...base };
    if (split.kind === 'dock' || split.kind === 'dock-status') {
      const dock = this.options.layout().dock;
      if (dock) out.dock = this.resizedDock(split, dock, room > 0 ? target / room : first / room);
    } else if (split.kind === 'columns') {
      const weights = Array.from({ length: split.columns }, (_, i) => base?.cols?.[i] ?? 1);
      const pair = (weights[split.index] ?? 1) + (weights[split.index + 1] ?? 1);
      // `first` is where the pair is now: the weights keep their sum, so the other columns do not change
      const share = room > 0 ? target / room : first / room;
      weights[split.index] = pair * share;
      weights[split.index + 1] = pair - pair * share;
      out.cols = weights;
    } else if (split.kind === 'rows') {
      out.rows = [target / room, (room - target) / room];
    } else {
      const rows: (number | null)[] = Array.from({ length: 4 }, (_, i) => base?.rows?.[i] ?? null);
      rows[split.index] = target / window.innerHeight;
      out.rows = rows;
    }
    return out;
  }

  /** The dock with the first side of a dock separator at `share` of what the two sides have together. */
  private resizedDock(split: Split, dock: Dock, share: number): Dock {
    if (split.kind === 'dock-status') {
      // the panes are 1 high together, Status is `status`: the panes are `share` of the pair
      return { ...dock, status: Math.min(Math.max((1 - share) / Math.max(share, 0.01), 0.05), 20) };
    }
    const path = split.path ?? [];
    const first = split.first ?? 0;
    const second = split.second ?? 0;
    const pair = weightAt(dock.tree, path, first) + weightAt(dock.tree, path, second);
    return { ...dock, tree: withWeights(dock.tree, path, first, second, pair * share, pair - pair * share) };
  }

  // ── dragging ───────────────────────────────────────────────────────────────

  private press(bar: HTMLElement, event: PointerEvent): void {
    if (this.drag || event.button !== 0) return;
    const split = this.splitOf(bar);
    const { first, room } = this.measure(split);
    const vertical = isVertical(split);
    this.drag = { bar, split, pointer: event.pointerId, start: vertical ? event.clientX : event.clientY, room, first, sizes: this.options.layout().sizes[split.mode] };
    bar.setPointerCapture(event.pointerId);
    bar.classList.add('dragging');
    document.body.classList.add(vertical ? 'pane-resizing-col' : 'pane-resizing-row');
    // no text is selected by the drag (user-select is off for the page meanwhile); the selection that exists, such as the
    // caret of the editor, is left alone
    event.preventDefault();
    bar.addEventListener('pointermove', this.move);
    bar.addEventListener('pointerup', this.up);
    bar.addEventListener('pointercancel', this.cancel);
    window.addEventListener('keydown', this.escape, true);
  }

  private readonly move = (event: PointerEvent): void => {
    const drag = this.drag;
    if (!drag || event.pointerId !== drag.pointer) return;
    const point = isVertical(drag.split) ? event.clientX : event.clientY;
    this.preview({ size: drag.first + (point - drag.start), split: drag.split }, drag.sizes);
  };

  /** Show the sizes while the pointer moves (not kept), once for each frame. */
  private pending: { size: number; split: Split; base: Sizes | undefined } | null = null;
  private preview(change: { size: number; split: Split }, base: Sizes | undefined): void {
    this.pending = { ...change, base };
    if (this.dragFrame) return;
    this.dragFrame = requestAnimationFrame(() => {
      this.dragFrame = 0;
      const pending = this.pending;
      this.pending = null;
      if (!pending || !this.drag) return;
      this.paint(this.resized(pending.split, pending.size, pending.base));
      this.position(this.drag.bar, pending.split);
    });
  }

  private readonly up = (event: PointerEvent): void => {
    const drag = this.drag;
    if (!drag || event.pointerId !== drag.pointer) return;
    const point = isVertical(drag.split) ? event.clientX : event.clientY;
    const sizes = this.resized(drag.split, drag.first + (point - drag.start), drag.sizes);
    this.finish();
    this.commit(drag.split, sizes);
  };

  private readonly cancel = (): void => {
    const drag = this.drag;
    if (!drag) return;
    this.finish();
    this.paint(drag.sizes);
    this.place();
  };

  private readonly escape = (event: KeyboardEvent): void => {
    if (event.key !== 'Escape') return;
    event.preventDefault();
    event.stopPropagation();
    this.cancel();
  };

  private finish(): void {
    const drag = this.drag;
    if (!drag) return;
    if (this.dragFrame) {
      cancelAnimationFrame(this.dragFrame);
      this.dragFrame = 0;
    }
    this.pending = null;
    this.drag = null;
    drag.bar.classList.remove('dragging');
    drag.bar.removeEventListener('pointermove', this.move);
    drag.bar.removeEventListener('pointerup', this.up);
    drag.bar.removeEventListener('pointercancel', this.cancel);
    window.removeEventListener('keydown', this.escape, true);
    document.body.classList.remove('pane-resizing-col', 'pane-resizing-row');
  }

  /** Keep the sizes (the page saves them and says what they are), and put the separators where the panes ended up. */
  private commit(split: Split, sizes: Sizes | undefined): void {
    this.options.change(split.mode, sizes, this.describe(split, sizes));
    this.sync();
  }

  /** "Schema 40%, Parsed 60%": the share of the two sides after the change, or the height in a layout that scrolls. */
  private describe(split: Split, sizes: Sizes | undefined): string {
    this.paint(sizes);
    const { first, room } = this.measure(split);
    const names = [this.options.name(split.before), this.options.name(split.after)];
    if (split.kind === 'height') return `${names[0]} ${Math.round((first / room) * 100)}% of the window high`;
    const percent = Math.round((first / room) * 100);
    return `${names[0]} ${percent}%, ${names[1]} ${100 - percent}%`;
  }

  // ── keys ───────────────────────────────────────────────────────────────────

  private key(bar: HTMLElement, event: KeyboardEvent): void {
    if (event.ctrlKey || event.altKey || event.metaKey || this.drag) return;
    const split = this.splitOf(bar);
    const vertical = isVertical(split);
    const back = vertical ? 'ArrowLeft' : 'ArrowUp';
    const forward = vertical ? 'ArrowRight' : 'ArrowDown';
    const { first, room, min, max } = this.measure(split);
    let size: number | null = null;
    const amount = (event.shiftKey ? BIG_STEP : STEP) * room;
    if (event.key === back) size = first - amount;
    else if (event.key === forward) size = first + amount;
    else if (event.key === 'Home') size = min;
    else if (event.key === 'End') size = max;
    else if (event.key === 'Enter') {
      event.preventDefault();
      this.reset(bar);
      return;
    }
    if (size === null) return;
    event.preventDefault();
    event.stopPropagation();
    const base = this.options.layout().sizes[split.mode];
    this.commit(split, this.resized(split, size, base));
  }

  /** Put this split back to the default size (a double-click, or Enter). */
  private reset(bar: HTMLElement): void {
    const split = this.splitOf(bar);
    const base = this.options.layout().sizes[split.mode];
    let sizes: Sizes | undefined = base ? { ...base } : undefined;
    if (split.kind === 'dock' || split.kind === 'dock-status') {
      const dock = this.options.layout().dock;
      if (!dock) return;
      // the two sides share what they have equally again, and Status has its low share
      const next: Dock =
        split.kind === 'dock-status'
          ? { ...dock, status: DEFAULT_STATUS }
          : this.resizedDock(split, dock, 0.5);
      const sized: Sizes = { ...base, dock: next };
      this.paint(sized);
      this.options.change(split.mode, sized, `${this.options.name(split.before)} and ${this.options.name(split.after)}: the default sizes`);
      this.sync();
      return;
    } else if (sizes) {
      if (split.kind === 'columns' && sizes.cols) {
        const cols = [...sizes.cols];
        cols[split.index] = 1;
        cols[split.index + 1] = 1;
        sizes.cols = cols.every((weight) => weight === 1) ? undefined : cols;
      } else if (split.kind === 'rows') {
        sizes.rows = undefined;
      } else if (split.kind === 'height' && sizes.rows) {
        const rows = [...sizes.rows];
        rows[split.index] = null;
        sizes.rows = rows.every((fraction) => fraction === null) ? undefined : rows;
      }
      if (!sizes.cols && !sizes.rows) sizes = undefined;
    }
    if (sizes && !sizes.cols && !sizes.rows) sizes = undefined;
    this.paint(sizes);
    const message = `${this.options.name(split.before)} and ${this.options.name(split.after)}: the default sizes`;
    this.options.change(split.mode, sizes, message);
    this.sync();
  }
}
