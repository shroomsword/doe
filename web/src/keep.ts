// What a change of the layout must not lose. Putting a pane in another place takes its element out of the page and
// puts it back, and an element that was out of the page, or is hidden (`display: none`), has lost its scroll offsets
// (and the focus, and the selection of a text box). `Positions` remembers the offsets of everything that is scrolled
// in the panes, before the change, and puts them back after it, also for a pane that was hidden and is shown again
// later: its offsets wait until it is laid out. No state of the page is in here; it works on the elements.

/** Whether the element is laid out (it is in the page and not hidden, nor inside what is hidden). */
function isLaidOut(element: Element): boolean {
  return element.isConnected && element.getClientRects().length > 0;
}

/** How many frames the offsets that were put back are kept in place (a slow machine measures the editor late). */
const REAPPLY_FRAMES = 20;
/** What ends that: the person has taken over. */
const INPUT_EVENTS = ['wheel', 'keydown', 'pointerdown', 'touchstart'] as const;

export class Positions {
  private readonly saved = new Map<Element, { top: number; left: number }>();

  /**
   * Remember the scroll offset of every element in `roots` (and of the roots) that is scrolled. A root that is not laid
   * out now keeps what was remembered for it earlier, since its offsets read as 0 while it is hidden; an element that
   * is laid out and is not scrolled any more is forgotten.
   */
  remember(roots: Iterable<Element>): void {
    for (const root of roots) {
      if (!isLaidOut(root)) continue;
      for (const element of [root, ...root.querySelectorAll('*')]) {
        if (element.scrollTop > 0 || element.scrollLeft > 0) this.saved.set(element, { top: element.scrollTop, left: element.scrollLeft });
        else this.saved.delete(element);
      }
    }
  }

  /** Put the offsets back now, and keep them remembered (for a second `restore` after the focus has been given back). */
  apply(): void {
    for (const [element, at] of this.saved) {
      if (!element.isConnected || !isLaidOut(element)) continue;
      element.scrollTop = at.top;
      element.scrollLeft = at.left;
    }
  }

  /**
   * Put back the offsets of the elements that are laid out now; the others wait for the time they are. An editor that
   * has the focus measures itself again when its size changes, and scrolls its caret into view when it has done, a
   * moment after the change: so for the next frames (until the person scrolls, types or presses something) an
   * element that is not where it was put back is moved there again.
   */
  restore(): void {
    const applied: [Element, { top: number; left: number }][] = [];
    for (const [element, at] of this.saved) {
      if (!element.isConnected) {
        this.saved.delete(element);
        continue;
      }
      if (!isLaidOut(element)) continue;
      element.scrollTop = at.top;
      element.scrollLeft = at.left;
      applied.push([element, at]);
      this.saved.delete(element);
    }
    if (applied.length === 0) return;
    // for a few frames, until the person scrolls or types: nothing else should move what was just put back
    let frames = 0;
    const stop = (): void => {
      frames = Infinity;
      for (const type of INPUT_EVENTS) window.removeEventListener(type, stop, true);
    };
    for (const type of INPUT_EVENTS) window.addEventListener(type, stop, { capture: true, passive: true });
    const again = (): void => {
      frames += 1;
      if (frames > REAPPLY_FRAMES) {
        stop();
        return;
      }
      for (const [element, at] of applied) {
        if (!isLaidOut(element) || (element.scrollTop === at.top && element.scrollLeft === at.left)) continue;
        element.scrollTop = at.top;
        element.scrollLeft = at.left;
      }
      requestAnimationFrame(again);
    };
    requestAnimationFrame(again);
  }

  /** How many offsets are remembered (for tests). */
  get size(): number {
    return this.saved.size;
  }
}

/** The selection of a text box, to put back after the focus was lost and returned. */
export interface BoxSelection {
  start: number | null;
  end: number | null;
  direction: 'forward' | 'backward' | 'none' | null;
}

export function boxSelection(element: Element | null): BoxSelection | null {
  if (!(element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement)) return null;
  try {
    return { start: element.selectionStart, end: element.selectionEnd, direction: element.selectionDirection };
  } catch {
    // some kinds of input (a number, a file) have no selection
    return null;
  }
}

export function putBackSelection(element: Element, selection: BoxSelection | null): void {
  if (!selection || !(element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement)) return;
  if (selection.start === null || selection.end === null) return;
  try {
    element.setSelectionRange(selection.start, selection.end, selection.direction ?? undefined);
  } catch {
    // as above
  }
}
