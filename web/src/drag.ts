// Dragging a pane by its title bar to put it before or after another one. It uses pointer events, not the browser's
// drag and drop, so that it works with a mouse, a pen and a finger, shows what is being dragged and where it will go,
// scrolls the page near its edges, and can be cancelled with Esc. The page decides what a drop means (`drop`); this
// only finds the pane that is under the pointer and which half of it.

import type { PaneName } from './layout.js';

export interface DragPane {
  name: PaneName;
  section: HTMLElement;
  /** What the dragged copy says. */
  label: string;
}

export interface DragOptions {
  panes: readonly DragPane[];
  /** The panes that are on the screen (a closed pane cannot be dropped on). */
  shown(): readonly PaneName[];
  /** The pointer was released over `target`: before it or after it (`swap` with Shift held: take its place). */
  drop(name: PaneName, target: PaneName, after: boolean, swap: boolean): void;
}

/** The parts of a title bar that are controls: a press on them is not the start of a drag. */
const CONTROLS = 'button, input, select, textarea, label, a, .pane-search';
/** How far the pointer moves before a press is a drag. */
const THRESHOLD = 5;
/** How near the window's edge the page scrolls, and by how much a frame. */
const EDGE = 48;
const SPEED = 14;

export class PaneDrag {
  private active: { name: PaneName; pointer: number; x: number; y: number; started: boolean; ghost: HTMLElement | null } | null = null;
  private line: HTMLElement | null = null;
  private target: { name: PaneName; after: boolean } | null = null;
  private frame = 0;

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
    this.scroll(event.clientY);
  };

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
    const line = document.createElement('div');
    line.className = 'drop-line';
    line.setAttribute('aria-hidden', 'true');
    line.hidden = true;
    document.body.append(line);
    this.line = line;
  }

  /** The pane under the pointer, and the half of it: the one nearer its start (before) or its end (after). */
  private aim(x: number, y: number): void {
    const shown = this.options.shown();
    const hit = document.elementFromPoint(x, y)?.closest<HTMLElement>('main > section');
    const pane = hit ? this.options.panes.find((candidate) => candidate.section === hit) : undefined;
    if (!pane || !hit || pane.name === this.active?.name || !shown.includes(pane.name)) {
      this.target = null;
      if (this.line) this.line.hidden = true;
      return;
    }
    const box = hit.getBoundingClientRect();
    const main = hit.parentElement?.getBoundingClientRect();
    // a pane that spans the width (the low fourth place, or the third in two columns, or a one-column page) is
    // before or after another one up and down; the others side by side, left and right
    const across = main !== undefined && box.width > main.width * 0.85;
    const after = across ? y > box.top + box.height / 2 : x > box.left + box.width / 2;
    this.target = { name: pane.name, after };
    const line = this.line;
    if (!line) return;
    line.hidden = false;
    if (across) {
      line.style.left = `${box.left}px`;
      line.style.width = `${box.width}px`;
      line.style.height = '4px';
      line.style.top = `${(after ? box.bottom : box.top) - 2}px`;
    } else {
      line.style.top = `${box.top}px`;
      line.style.height = `${box.height}px`;
      line.style.width = '4px';
      line.style.left = `${(after ? box.right : box.left) - 2}px`;
    }
  }

  /** Scroll the page while the pointer is near the top or the bottom of the window (the one-column page is taller than it). */
  private scroll(y: number): void {
    cancelAnimationFrame(this.frame);
    const by = y < EDGE ? -SPEED : y > window.innerHeight - EDGE ? SPEED : 0;
    if (by === 0 || !this.active?.started) return;
    const step = (): void => {
      if (!this.active?.started) return;
      window.scrollBy(0, by);
      this.aim(this.active.x, this.active.y);
      this.frame = requestAnimationFrame(step);
    };
    this.frame = requestAnimationFrame(step);
  }

  private readonly up = (event: PointerEvent): void => {
    const drag = this.active;
    if (!drag || event.pointerId !== drag.pointer) return;
    const target = drag.started ? this.target : null;
    const name = drag.name;
    this.end();
    if (target) this.options.drop(name, target.name, target.after, event.shiftKey);
  };

  private readonly cancel = (): void => this.end();

  private readonly key = (event: KeyboardEvent): void => {
    if (event.key !== 'Escape' || !this.active) return;
    event.preventDefault();
    event.stopPropagation();
    this.end();
  };

  private end(): void {
    cancelAnimationFrame(this.frame);
    window.removeEventListener('pointermove', this.move);
    window.removeEventListener('pointerup', this.up);
    window.removeEventListener('pointercancel', this.cancel);
    window.removeEventListener('keydown', this.key, true);
    this.active?.ghost?.remove();
    this.line?.remove();
    this.line = null;
    this.target = null;
    this.active = null;
    document.body.classList.remove('pane-dragging');
    for (const pane of this.options.panes) pane.section.classList.remove('dragging');
  }
}
