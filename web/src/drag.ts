// Dragging a pane by its title bar to dock it. It uses pointer events, not the browser's drag and drop, so that it works with
// a mouse, a pen and a finger, shows what is being dragged and where it will go, and can be cancelled with Esc. While a pane
// is dragged the panes area shows the eight positions of a drop (the four corners and the four sides); the one under the
// pointer lights up and the outline of the layout that dropping there makes is drawn. A drop on the middle of another pane
// swaps the two. The page decides what a drop means (`drop`, `swap`).

import { type Movable, ZONES, type Zone, ZONE_NAMES, dockTo, isCorner, rects, swapIn } from './dock.js';
import type { Dock, PaneName } from './layout.js';

export interface DragPane {
  name: Movable;
  section: HTMLElement;
  /** What the dragged copy says. */
  label: string;
}

export interface DragOptions {
  panes: readonly DragPane[];
  /** The panes of the dock that are on the screen: a closed pane is not in the area and cannot be dropped on. */
  shown(): ReadonlySet<Movable>;
  /** The dock now, or what the wide window has, to start from. */
  dock(): Dock;
  /** Whether the window is too narrow to dock: the panes are one under the other and there is nothing to drop on. */
  narrow(): boolean;
  /** Whether a pane is in a place where it can be dragged (it is not while another pane is maximized). */
  enabled(): boolean;
  /** The pointer was released over a position. */
  drop(name: Movable, zone: Zone): void;
  /** The pointer was released over another pane: the two take each other's places. */
  swap(name: Movable, target: Movable): void;
}

/** The parts of a title bar that are controls: a press on them is not the start of a drag. */
const CONTROLS = 'button, input, select, textarea, label, a, .pane-search';
/** How far the pointer moves before a press is a drag. */
const THRESHOLD = 5;

/** Where each position is in the overlay, as percentages of its width and height. */
const BOXES: Record<Zone, { x: number; y: number; w: number; h: number }> = {
  tl: { x: 0, y: 0, w: 26, h: 26 },
  t: { x: 26, y: 0, w: 48, h: 16 },
  tr: { x: 74, y: 0, w: 26, h: 26 },
  l: { x: 0, y: 26, w: 16, h: 48 },
  r: { x: 84, y: 26, w: 16, h: 48 },
  bl: { x: 0, y: 74, w: 26, h: 26 },
  b: { x: 26, y: 84, w: 48, h: 16 },
  br: { x: 74, y: 74, w: 26, h: 26 },
};

/** The positions that a drop can be on: the corners need all three panes shown, the sides two. */
export function offered(shown: number): Zone[] {
  if (shown < 2) return [];
  return ZONES.filter((zone) => shown >= 3 || !isCorner(zone));
}

export class PaneDrag {
  private active: { name: Movable; pointer: number; x: number; y: number; started: boolean; ghost: HTMLElement | null } | null = null;
  private overlay: HTMLElement | null = null;
  private zones = new Map<Zone, HTMLElement>();
  private outline: HTMLElement | null = null;
  private target: { zone: Zone } | { swap: Movable } | null = null;

  constructor(private readonly options: DragOptions) {
    for (const pane of options.panes) {
      const title = pane.section.querySelector<HTMLElement>('.pane-title');
      title?.addEventListener('pointerdown', (event) => this.press(pane, event));
    }
  }

  private press(pane: DragPane, event: PointerEvent): void {
    if (this.active || event.button !== 0 || !(event.target instanceof Element)) return;
    if (event.target.closest(CONTROLS)) return;
    // a finger drags from the grip only, so that the rest of the title bar still scrolls the page
    if (event.pointerType !== 'mouse' && !event.target.closest('.pane-grip')) return;
    if (this.options.narrow() || !this.options.enabled() || offered(this.options.shown().size).length === 0) return;
    this.active = { name: pane.name, pointer: event.pointerId, x: event.clientX, y: event.clientY, started: false, ghost: null };
    window.addEventListener('pointermove', this.move);
    window.addEventListener('pointerup', this.up);
    window.addEventListener('pointercancel', this.cancel);
    window.addEventListener('keydown', this.key, true);
  }

  private readonly move = (event: PointerEvent): void => {
    const drag = this.active;
    if (!drag || event.pointerId !== drag.pointer) return;
    if (!drag.started) {
      if (Math.hypot(event.clientX - drag.x, event.clientY - drag.y) < THRESHOLD) return;
      this.begin(drag);
    }
    event.preventDefault();
    if (drag.ghost) {
      drag.ghost.style.left = `${event.clientX + 12}px`;
      drag.ghost.style.top = `${event.clientY + 12}px`;
    }
    drag.x = event.clientX;
    drag.y = event.clientY;
    this.aim(event.clientX, event.clientY);
  };

  /** The area that the panes above Status fill, in the window. */
  private area(): DOMRect | null {
    let box: DOMRect | null = null;
    for (const pane of this.options.panes) {
      if (!this.options.shown().has(pane.name) || pane.section.hidden) continue;
      const mine = pane.section.getBoundingClientRect();
      box = box
        ? new DOMRect(Math.min(box.left, mine.left), Math.min(box.top, mine.top), Math.max(box.right, mine.right) - Math.min(box.left, mine.left), Math.max(box.bottom, mine.bottom) - Math.min(box.top, mine.top))
        : mine;
    }
    return box;
  }

  private begin(drag: NonNullable<PaneDrag['active']>): void {
    drag.started = true;
    const pane = this.options.panes.find((candidate) => candidate.name === drag.name);
    pane?.section.classList.add('dragging');
    window.getSelection()?.removeAllRanges();
    document.body.classList.add('pane-dragging');
    const ghost = document.createElement('div');
    ghost.className = 'drag-ghost';
    ghost.setAttribute('aria-hidden', 'true');
    ghost.textContent = pane?.label ?? '';
    document.body.append(ghost);
    drag.ghost = ghost;
    const area = this.area();
    if (!area) return;
    const overlay = document.createElement('div');
    overlay.className = 'dock-overlay';
    overlay.setAttribute('aria-hidden', 'true');
    Object.assign(overlay.style, { left: `${area.left}px`, top: `${area.top}px`, width: `${area.width}px`, height: `${area.height}px` });
    const outline = document.createElement('div');
    outline.className = 'dock-outline';
    overlay.append(outline);
    this.outline = outline;
    for (const zone of offered(this.options.shown().size)) {
      const box = BOXES[zone];
      const el = document.createElement('div');
      el.className = 'dock-zone';
      el.dataset.zone = zone;
      el.textContent = ZONE_NAMES[zone];
      Object.assign(el.style, { left: `${box.x}%`, top: `${box.y}%`, width: `${box.w}%`, height: `${box.h}%` });
      overlay.append(el);
      this.zones.set(zone, el);
    }
    document.body.append(overlay);
    this.overlay = overlay;
  }

  /** What is under the pointer: a position of the area, the middle of another pane (to swap), or nothing. */
  private aim(x: number, y: number): void {
    const drag = this.active;
    const area = this.overlay?.getBoundingClientRect();
    this.target = null;
    if (!drag || !area || x < area.left || x > area.right || y < area.top || y > area.bottom) return this.show();
    const fx = ((x - area.left) / area.width) * 100;
    const fy = ((y - area.top) / area.height) * 100;
    for (const zone of this.zones.keys()) {
      const box = BOXES[zone];
      if (fx >= box.x && fx <= box.x + box.w && fy >= box.y && fy <= box.y + box.h) {
        this.target = { zone };
        return this.show();
      }
    }
    // the middle: the pane that is there, if it is another one that is shown
    const hit = document.elementFromPoint(x, y)?.closest<HTMLElement>('main > section');
    const other = this.options.panes.find((pane) => pane.section === hit);
    if (other && other.name !== drag.name && this.options.shown().has(other.name)) this.target = { swap: other.name };
    this.show();
  }

  /** Light the position that is aimed at and draw the layout that a drop there makes. */
  private show(): void {
    const drag = this.active;
    for (const [zone, el] of this.zones) el.classList.toggle('hot', this.target !== null && 'zone' in this.target && this.target.zone === zone);
    const outline = this.outline;
    if (!outline || !drag) return;
    outline.replaceChildren();
    const target = this.target;
    if (!target) return;
    const dock = this.options.dock();
    const tree = 'zone' in target ? dockTo(dock.tree, drag.name, target.zone) : swapIn(dock.tree, drag.name, target.swap);
    if (!tree) return;
    const names: Record<PaneName, string> = { schema: 'Schema', parsed: 'Parsed', bytes: 'Bytes', status: 'Status' };
    for (const [pane, box] of rects(tree, this.options.shown())) {
      const el = document.createElement('div');
      el.className = `dock-ghost${pane === drag.name ? ' moving' : ''}`;
      Object.assign(el.style, { left: `${box.x0 * 100}%`, top: `${box.y0 * 100}%`, width: `${(box.x1 - box.x0) * 100}%`, height: `${(box.y1 - box.y0) * 100}%` });
      el.textContent = names[pane];
      outline.append(el);
    }
  }

  private readonly up = (event: PointerEvent): void => {
    const drag = this.active;
    if (!drag || event.pointerId !== drag.pointer) return;
    const target = drag.started ? this.target : null;
    const name = drag.name;
    this.end();
    if (!target) return;
    if ('zone' in target) this.options.drop(name, target.zone);
    else this.options.swap(name, target.swap);
  };

  private readonly cancel = (): void => this.end();

  private readonly key = (event: KeyboardEvent): void => {
    if (event.key !== 'Escape' || !this.active) return;
    event.preventDefault();
    event.stopPropagation();
    this.end();
  };

  private end(): void {
    window.removeEventListener('pointermove', this.move);
    window.removeEventListener('pointerup', this.up);
    window.removeEventListener('pointercancel', this.cancel);
    window.removeEventListener('keydown', this.key, true);
    this.active?.ghost?.remove();
    this.overlay?.remove();
    this.overlay = null;
    this.outline = null;
    this.zones.clear();
    this.target = null;
    this.active = null;
    document.body.classList.remove('pane-dragging');
    for (const pane of this.options.panes) pane.section.classList.remove('dragging');
  }
}
