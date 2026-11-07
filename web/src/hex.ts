// The bytes of the input: offset, hex and text, 16 to a row. Only the rows in
// view are in the page, so a file of hundreds of megabytes scrolls like a small
// one. Browsers limit how tall an element can be, so for a long file the
// scroll bar stands for the whole file but not row for row: a pixel of scroll
// can be many rows.

import type { ByteKinds } from './kinds.js';

/** The bytes in a row until the person sets another number. */
export const DEFAULT_BYTES_PER_ROW = 16;
const ROW_HEIGHT = 20; // px, as --hex-row in style.css
const MAX_SCROLL_HEIGHT = 8_000_000; // px, below what browsers allow
const OVERSCAN_ROWS = 4;

/** The bytes to mark: from `first` up to, not including, `end`; `startBit` and `endBit` are the bits they start and stop at. */
export interface HexSelection {
  first: number;
  end: number;
  startBit: number;
  endBit: number;
}

const hex = (value: number, width: number): string => value.toString(16).padStart(width, '0');

function printable(byte: number): string {
  if (byte < 0x20 || byte > 0x7e) return '·';
  const char = String.fromCharCode(byte);
  return char === '&' ? '&amp;' : char === '<' ? '&lt;' : char === '>' ? '&gt;' : char;
}

/** Bytes that the view reads; `want` says which it is about to show. */
export interface ByteSource {
  readonly length: number;
  /** The byte, or undefined when it is not here yet. */
  byteAt(index: number): number | undefined;
  /** The bytes from `from` up to `to` are about to be shown. */
  want(from: number, to: number): void;
}

class MemoryBytes implements ByteSource {
  constructor(private readonly bytes: Uint8Array) {}
  get length(): number {
    return this.bytes.length;
  }
  byteAt(index: number): number | undefined {
    return this.bytes[index];
  }
  want(): void {}
}

const CHUNK = 64 * 1024;
const CACHED_CHUNKS = 32;

/** Bytes of a file that stays where it is (on the machine of `doe --ide`) and comes in chunks. */
export class RemoteBytes implements ByteSource {
  private readonly chunks = new Map<number, Uint8Array>();
  private readonly loading = new Set<number>();

  constructor(
    readonly length: number,
    private readonly fetchRange: (offset: number, len: number) => Promise<Uint8Array>,
    private readonly onLoad: () => void,
  ) {}

  byteAt(index: number): number | undefined {
    return this.chunks.get(Math.floor(index / CHUNK))?.[index % CHUNK];
  }

  want(from: number, to: number): void {
    for (let chunk = Math.floor(from / CHUNK); chunk * CHUNK < Math.min(to, this.length); chunk += 1) {
      if (this.chunks.has(chunk) || this.loading.has(chunk)) continue;
      this.loading.add(chunk);
      this.fetchRange(chunk * CHUNK, CHUNK).then(
        (data) => {
          this.loading.delete(chunk);
          this.chunks.set(chunk, data);
          // Keep the newest few.
          while (this.chunks.size > CACHED_CHUNKS) this.chunks.delete(this.chunks.keys().next().value as number);
          this.onLoad();
        },
        () => this.loading.delete(chunk),
      );
    }
  }
}

export class HexView {
  private readonly scroller: HTMLElement;
  private readonly spacer: HTMLElement;
  private readonly rows: HTMLElement;
  private bytes: ByteSource = new MemoryBytes(new Uint8Array(0));
  private selection: HexSelection | null = null;
  /** Which kind of value owns each byte, when the bytes are coloured by their fields. */
  private kinds: ByteKinds | null = null;
  private coloured = true;
  private pending = false;
  private perRow = DEFAULT_BYTES_PER_ROW;

  constructor(
    parent: HTMLElement,
    private readonly onByte: (byte: number) => void,
  ) {
    this.scroller = document.createElement('div');
    this.scroller.className = 'hex';
    this.scroller.tabIndex = 0;
    this.scroller.setAttribute('role', 'group');
    this.scroller.setAttribute('aria-label', 'bytes of the input');
    this.spacer = document.createElement('div');
    this.spacer.className = 'hex-spacer';
    this.rows = document.createElement('div');
    this.rows.className = 'hex-rows';
    this.spacer.append(this.rows);
    this.scroller.append(this.spacer);
    parent.append(this.scroller);

    this.scroller.addEventListener('scroll', () => this.schedule());
    new ResizeObserver(() => this.schedule()).observe(this.scroller);
    this.scroller.addEventListener('click', (event) => {
      const target = (event.target as HTMLElement).closest<HTMLElement>('[data-i]');
      if (target?.dataset.i) this.onByte(Number(target.dataset.i));
    });
  }

  private get totalRows(): number {
    return Math.ceil(this.bytes.length / this.perRow);
  }

  private get viewRows(): number {
    return this.scroller.clientHeight / ROW_HEIGHT;
  }

  private get scrollHeight(): number {
    return Math.min(this.totalRows * ROW_HEIGHT, MAX_SCROLL_HEIGHT);
  }

  /** Replace the bytes; the view goes back to the top. */
  setBytes(bytes: Uint8Array | ByteSource | null): void {
    this.bytes = bytes instanceof Uint8Array ? new MemoryBytes(bytes) : (bytes ?? new MemoryBytes(new Uint8Array(0)));
    this.selection = null;
    this.spacer.style.height = `${this.scrollHeight}px`;
    this.scroller.scrollTop = 0;
    this.scroller.dataset.length = String(this.bytes.length);
    this.render();
  }

  /** The bytes in a row. */
  get columns(): number {
    return this.perRow;
  }

  /** Show `columns` bytes in a row; the first byte in view stays in view, and the selection stays. */
  setColumns(columns: number): void {
    if (columns === this.perRow || columns < 1) return;
    const topByte = Math.floor(this.firstRow()) * this.perRow;
    this.perRow = columns;
    this.spacer.style.height = `${this.scrollHeight}px`;
    const room = this.totalRows - this.viewRows;
    const maxScroll = this.scrollHeight - this.scroller.clientHeight;
    this.scroller.scrollTop = room <= 0 || maxScroll <= 0 ? 0 : Math.min(1, Math.floor(topByte / columns) / room) * maxScroll;
    this.scroller.dataset.columns = String(columns);
    this.render();
  }

  /** Give the bytes the focus. */
  focus(): void {
    this.scroller.focus();
  }

  /** The bytes that are shown. */
  get source(): ByteSource {
    return this.bytes;
  }

  /** Draw again, when bytes have arrived. */
  repaint(): void {
    this.render();
  }

  /** Colour the bytes by the kind of the field that owns them (null: no colours). */
  setKinds(kinds: ByteKinds | null): void {
    this.kinds = kinds;
    this.render();
  }

  /** Whether the bytes are coloured by their fields (the toggle of the pane). */
  setColoured(on: boolean): void {
    if (on === this.coloured) return;
    this.coloured = on;
    this.render();
  }

  setSelection(selection: HexSelection | null): void {
    this.selection = selection;
    this.render();
  }

  /** Scroll so that the byte is in view, in the middle when it was not. */
  reveal(byte: number): void {
    const row = Math.floor(byte / this.perRow);
    const top = this.firstRow();
    if (row >= top + 1 && row < top + this.viewRows - 2) return;
    const room = this.totalRows - this.viewRows;
    const target = Math.max(0, row - this.viewRows / 2);
    const maxScroll = this.scrollHeight - this.scroller.clientHeight;
    this.scroller.scrollTop = room <= 0 ? 0 : Math.min(1, target / room) * maxScroll;
    this.render();
  }

  /** The row at the top of the view, with the fraction of it that is scrolled past. */
  private firstRow(): number {
    const room = this.totalRows - this.viewRows;
    const maxScroll = this.scrollHeight - this.scroller.clientHeight;
    if (room <= 0 || maxScroll <= 0) return 0;
    return Math.min(1, this.scroller.scrollTop / maxScroll) * room;
  }

  private schedule(): void {
    if (this.pending) return;
    this.pending = true;
    requestAnimationFrame(() => {
      this.pending = false;
      this.render();
    });
  }

  private render(): void {
    this.spacer.style.height = `${this.scrollHeight}px`;
    const top = this.firstRow();
    const first = Math.max(0, Math.floor(top) - OVERSCAN_ROWS);
    const last = Math.min(this.totalRows, Math.floor(top) + Math.ceil(this.viewRows) + OVERSCAN_ROWS + 1);
    const selection = this.selection;
    this.bytes.want(first * this.perRow, last * this.perRow);
    const parts: string[] = [];
    for (let row = first; row < last; row += 1) {
      const from = row * this.perRow;
      const to = Math.min(from + this.perRow, this.bytes.length);
      let cells = '';
      let text = '';
      for (let i = from; i < to; i += 1) {
        const value = this.bytes.byteAt(i);
        let marked = '';
        if (selection && i >= selection.first && i < selection.end) {
          const whole = selection.startBit <= i * 8 && selection.endBit >= i * 8 + 8;
          marked = whole ? ' sel' : ' sel part';
        }
        const kind = this.coloured ? this.kinds?.kindAt(i) : null;
        if (kind) marked += ` k-${kind}`;
        cells += `<span class="b${marked}" data-i="${i}">${value === undefined ? '··' : hex(value, 2)}</span>`;
        text += `<span class="a${marked}" data-i="${i}">${value === undefined ? ' ' : printable(value)}</span>`;
      }
      parts.push(`<div class="hex-row"><span class="hex-off">${hex(from, 8)}</span><span class="hex-bytes">${cells}</span><span class="hex-text">${text}</span></div>`);
    }
    // The rows sit at the top of the view, wherever the scroll bar is.
    this.rows.style.transform = `translateY(${this.scroller.scrollTop - (top - first) * ROW_HEIGHT}px)`;
    this.rows.innerHTML = parts.join('');
  }
}
