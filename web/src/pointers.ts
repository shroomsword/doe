// Fields that point: a number that the schema's `pos:` or `size:` reads places or measures another value, and the jump goes
// there. The library says which (`points_to` on the field); a number that is an offset with no such link is a guess.
// The places jumped from are kept to go back to, as the terminal viewer does (`history.rs`).

import type { TraceNode } from './doe.js';
import { type Path, nodeAt } from './model.js';

/** The most places kept; the oldest go first. */
export const HISTORY_LIMIT = 100;

export interface Place {
  /** The byte the place is at. */
  offset: number;
  /** The selected field. */
  path: Path;
}

/** The places, with the one the person is at: a jump records where it came from; going back and jumping again drops what was ahead. */
export class History {
  private places: Place[] = [];
  private at = 0;

  get length(): number {
    return this.places.length;
  }

  /** A jump is about to leave `here`. */
  leave(here: Place): void {
    if (this.places.length === 0) {
      this.places = [here];
      this.at = 0;
      return;
    }
    this.places[this.at] = here;
    this.places.length = this.at + 1;
  }

  /** The jump landed on `there`; within `screen` bytes of the place left it replaces that place, and a hop from the very first place that stays near is no jump. */
  arrive(there: Place, screen: number): void {
    const last = this.places[this.at];
    const near = last !== undefined && Math.abs(last.offset - there.offset) < screen;
    if (near && this.at > 0) {
      this.places[this.at] = there;
      return;
    }
    if (near) {
      this.places = [];
      this.at = 0;
      return;
    }
    this.places.push(there);
    if (this.places.length > HISTORY_LIMIT) this.places.shift();
    this.at = this.places.length - 1;
  }

  /** Note where the person is before walking, so that coming back lands here. */
  remember(here: Place): void {
    if (this.places[this.at]) this.places[this.at] = here;
  }

  /** One place back (or forward), or null at the oldest (or the newest). */
  walk(forward: boolean): Place | null {
    const next = this.at + (forward ? 1 : -1);
    const place = this.places[next];
    if (next < 0 || !place) return null;
    this.at = next;
    return place;
  }

  /** Drop the current place (it no longer exists), staying on the one before it. */
  dropCurrent(): void {
    if (this.at >= this.places.length) return;
    this.places.splice(this.at, 1);
    this.at = Math.min(Math.max(this.at - 1, 0), Math.max(this.places.length - 1, 0));
  }
}

/** The byte a node starts at. */
export function startByte(node: TraceNode): number | null {
  return node.start === null ? null : Math.floor(node.start / 8);
}

/** The outermost node that starts at the byte `offset`, as a path; a guess at what a number that is an offset means. Not the root. */
export function startingAt(root: TraceNode, offset: number): Path | null {
  const want = offset * 8;
  let best: number[] | null = null;
  const stack: Array<{ path: number[]; node: TraceNode }> = [{ path: [], node: root }];
  while (stack.length > 0) {
    const item = stack.pop();
    if (!item || item.node.processed) continue;
    if (item.path.length > 0 && item.node.start === want && (best === null || item.path.length < best.length || (item.path.length === best.length && lexicographicallyBefore(item.path, best)))) best = item.path;
    item.node.children.forEach((child, index) => stack.push({ path: [...item.path, index], node: child }));
  }
  return best;
}

function lexicographicallyBefore(a: readonly number[], b: readonly number[]): boolean {
  for (let i = 0; i < Math.min(a.length, b.length); i += 1) {
    if (a[i] !== b[i]) return (a[i] ?? 0) < (b[i] ?? 0);
  }
  return false;
}

export type Followed = { path: Path; offset: number; more: number; guess: boolean } | { error: string };

/** Where the field at `path` points: what its value places or sizes, or (a guess) the outermost value that starts at its number. */
export function follow(root: TraceNode, path: Path, size: number): Followed {
  const node = nodeAt(root, path);
  if (!node) return { error: 'select a field first' };
  let target: Path | null = null;
  let more = 0;
  let guess = false;
  if (node.points_to) {
    target = node.points_to.path;
    more = node.points_to.more;
  } else {
    if (!node.integer) return { error: 'this field does not point at anything' };
    const number = Number(node.integer.value);
    if (!Number.isSafeInteger(number) || number < 0) return { error: `${node.integer.value} is not an offset` };
    if (number >= size) return { error: `0x${number.toString(16)} is past the end of the file (${size} bytes)` };
    target = startingAt(root, number);
    guess = true;
    if (!target || target.join() === path.join()) return { error: `nothing starts at 0x${number.toString(16)}: the schema does not say that this field points anywhere` };
  }
  const found = nodeAt(root, target);
  const offset = found ? startByte(found) : null;
  if (offset === null) return { error: 'what it points at is not in the file' };
  if (offset >= size) return { error: `0x${offset.toString(16)} is past the end of the file (${size} bytes)` };
  return { path: target, offset, more, guess };
}
