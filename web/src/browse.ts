// The types that are available: those of the schemas that are open, of the type
// library of this machine (with `doe --ide`) and of the Kaitai Struct gallery,
// in one list with a filter. The list is made here without the page, and
// `TypeBrowser` shows it in a dialog.

import type { GalleryFormat } from './detect.js';
import type { DetectWhy, Magic } from './doe.js';
import type { LocalType } from './local.js';
import type { Row } from './schemas.js';

export type Source = 'open' | 'library' | 'gallery';

/** The heading of each source, in the order they are listed. */
export const SOURCE_LABEL: Record<Source, string> = {
  open: 'Open schemas',
  library: 'Type library of this machine',
  gallery: 'Kaitai Struct gallery',
};

const ORDER: Source[] = ['open', 'library', 'gallery'];

/** One type that can be used. */
export interface Entry {
  source: Source;
  /** The name to parse with, or the id of the format. */
  id: string;
  /** The file it is in. */
  path: string;
  /** The first line of its doc, or the title of the format. */
  doc: string | null;
  /** What it starts with, so that detection tries it on a file that starts with that; null until it is known. */
  magic: Magic[] | null;
  /** What it is called, in lower case: its id, the name of its file and its extensions. */
  names: string[];
  /** Why its schema could not be compiled, once that is known. */
  compileError: string | null;
  /** `real`: tried on a file written by its own tool; `crafted`: on a file built by hand. */
  verified: 'real' | 'crafted' | null;
  /** Whether it is the type that is in use. */
  current: boolean;
  /** An error that stops it being chosen, for an open schema that does not read. */
  error: string | null;
}

/** The types of the open schemas. `parseType` is the one chosen to parse with, null for the schema's own. */
export function openEntries(rows: readonly Row[], parseType: string | null): Entry[] {
  const out: Entry[] = [];
  for (const row of rows) {
    const own = row.main ? row.id : null;
    for (const type of row.types) {
      out.push({
        source: 'open',
        id: type,
        path: row.path,
        doc: null,
        magic: null,
        names: [type.toLowerCase()],
        compileError: null,
        verified: null,
        current: parseType === null ? row.main && type === own : parseType === type,
        error: row.error ? row.error.message : null,
      });
    }
  }
  return out;
}

/** The types of the library of this machine. */
export function libraryEntries(types: readonly LocalType[]): Entry[] {
  return types.map((type) => ({
    source: 'library',
    id: type.id,
    path: type.path,
    doc: firstLine(type.doc),
    magic: null,
    names: [type.id.toLowerCase()],
    compileError: null,
    verified: null,
    current: false,
    error: null,
  }));
}

/** The formats of the gallery. `currentPath` is the file of the main schema, to mark the format in use. */
export function galleryEntries(formats: readonly GalleryFormat[], currentPath: string | null): Entry[] {
  return formats.map((format) => ({
    source: 'gallery',
    id: format.id,
    path: format.path,
    doc: format.title,
    magic: format.magic ?? [],
    names: format.names ?? [format.id.toLowerCase()],
    compileError: null,
    verified: format.verified ?? null,
    current: currentPath === format.path,
    error: null,
  }));
}

/** Whether the entry starts with something that can be checked; null while that is not known. */
export function hasSignature(entry: Entry): boolean | null {
  return entry.magic === null ? null : entry.magic.length > 0;
}

/** A few words for why an entry is where the ranking puts it. */
export const WHY: Record<DetectWhy, string> = {
  signatureandname: 'the signature and the name fit',
  signature: 'the file starts with its signature',
  name: 'the name of the file fits',
  remaining: 'no signature to check',
};

/** What the ranking made of the entries: those that can be the type of the file, in the order to try them. */
export interface Ranking {
  order: { index: number; why: DetectWhy }[];
  /** How many were left out because the file does not start with their signature. */
  excluded: number;
}

/** The first line of a doc that has any text. */
export function firstLine(doc: string | null | undefined): string | null {
  const line = (doc ?? '').split('\n').map((l) => l.trim()).find((l) => l !== '');
  return line ?? null;
}

/** The entries whose id, path or doc has every word of `text`, in any case. */
export function filterEntries(entries: readonly Entry[], text: string): Entry[] {
  const words = text.toLowerCase().split(/\s+/).filter((word) => word !== '');
  if (words.length === 0) return [...entries];
  return entries.filter((entry) => {
    const haystack = `${entry.id} ${entry.path} ${entry.doc ?? ''}`.toLowerCase();
    return words.every((word) => haystack.includes(word));
  });
}

/** The entries by source, in the order of the sources, each source's own order kept. */
export function grouped(entries: readonly Entry[]): { source: Source; entries: Entry[] }[] {
  return ORDER.map((source) => ({ source, entries: entries.filter((e) => e.source === source) })).filter((g) => g.entries.length > 0);
}

export interface Handlers {
  /** Whether there is a file to rank the types by. */
  canRank(): boolean;
  /** Rank the entries by the opened file (their signatures are known by now), or null if that is not possible. */
  rank(entries: readonly Entry[]): Promise<Ranking | null>;
  /** The entry was chosen to parse with. */
  pick(entry: Entry): void;
  /** The text of the schema of the entry, for reading. */
  text(entry: Entry): Promise<string>;
}

function make<K extends keyof HTMLElementTagNameMap>(tag: K, text?: string, className?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
}

/** The dialog with the list. */
export class TypeBrowser {
  private entries: Entry[] = [];
  private ranking: Ranking | null = null;
  private readonly rankBox: HTMLInputElement;
  private readonly note: HTMLElement;
  private readonly filter: HTMLInputElement;
  private readonly count: HTMLElement;
  private readonly list: HTMLElement;
  private readonly reader: HTMLElement;
  private readonly readerTitle: HTMLElement;
  private readonly readerText: HTMLElement;

  constructor(
    private readonly dialog: HTMLDialogElement,
    private readonly handlers: Handlers,
  ) {
    const find = <T extends HTMLElement>(selector: string): T => {
      const found = dialog.querySelector<T>(selector);
      if (!found) throw new Error(`the type browser has no ${selector}`);
      return found;
    };
    this.filter = find('#browse-filter');
    this.rankBox = find('#browse-rank');
    this.note = find('#browse-note');
    this.count = find('#browse-count');
    this.list = find('#browse-list');
    this.reader = find('#browse-reader');
    this.readerTitle = find('#browse-reader-title');
    this.readerText = find('#browse-reader-text');
    this.filter.addEventListener('input', () => this.render());
    this.rankBox.addEventListener('change', () => void this.applyRanking());
    find('#browse-close').addEventListener('click', () => this.dialog.close());
    find('#browse-back').addEventListener('click', () => this.closeReader());
    // Esc leaves the text first, then the dialog
    this.dialog.addEventListener('cancel', (event) => {
      if (!this.reader.hidden) {
        event.preventDefault();
        this.closeReader();
      }
    });
  }

  get isOpen(): boolean {
    return this.dialog.open;
  }

  /** Show the dialog with these entries. */
  open(entries: Entry[]): void {
    this.entries = entries;
    this.ranking = null;
    this.rankBox.checked = false;
    this.rankBox.disabled = !this.handlers.canRank();
    this.rankBox.title = this.rankBox.disabled ? 'open a file first: the types are ranked by what it starts with' : 'show first the types that can be the type of the opened file';
    this.closeReader();
    this.render();
    if (!this.dialog.open) this.dialog.showModal();
    this.filter.focus();
    this.filter.select();
  }

  close(): void {
    if (this.dialog.open) this.dialog.close();
  }

  /** The entries again, with what has been found out about them since (their signatures). */
  refresh(entries: Entry[]): void {
    this.entries = entries;
    if (this.rankBox.checked) void this.applyRanking();
    else this.render();
  }

  private async applyRanking(): Promise<void> {
    if (!this.rankBox.checked) {
      this.ranking = null;
      this.render();
      return;
    }
    const entries = this.entries;
    // the ranking waits until every signature is known; it is made again when the next one comes
    if (entries.some((e) => e.magic === null)) {
      this.ranking = null;
      this.render();
      return;
    }
    this.note.textContent = 'ranking…';
    const ranking = await this.handlers.rank(entries);
    if (!this.rankBox.checked) return;
    this.ranking = ranking;
    this.render();
  }

  private closeReader(): void {
    this.reader.hidden = true;
    this.list.hidden = false;
    this.filter.disabled = false;
    this.filter.focus();
  }

  private render(): void {
    const ranked = this.ranking;
    const unknown = this.entries.filter((e) => e.magic === null).length;
    this.note.textContent = '';
    if (this.rankBox.checked && !ranked) this.note.textContent = unknown > 0 ? `reading the signatures: ${this.entries.length - unknown} of ${this.entries.length}…` : 'ranking…';
    else if (unknown > 0) this.note.textContent = `reading the signatures: ${this.entries.length - unknown} of ${this.entries.length}…`;
    const source = ranked ? ranked.order.map((o) => this.entries[o.index]).filter((e): e is Entry => e !== undefined) : this.entries;
    const kept = filterEntries(source, this.filter.value);
    this.count.textContent = `${kept.length} of ${this.entries.length}`;
    this.list.replaceChildren();
    if (ranked) {
      this.note.textContent = `ranked by the opened file; ${ranked.excluded} left out because it does not start with their signature, ${this.entries.length - ranked.order.length - ranked.excluded} more because their name does not fit`;
      if (kept.length > 0) {
        const items = make('ul');
        for (const entry of kept) {
          const why = ranked.order.find((o) => this.entries[o.index] === entry)?.why;
          items.append(this.item(entry, why ? WHY[why] : null));
        }
        const section = make('section');
        section.append(make('h4', `Ranked · ${kept.length}`), items);
        this.list.append(section);
        return;
      }
    }
    if (kept.length === 0) {
      this.list.append(make('p', this.entries.length === 0 ? 'There are no types to list.' : 'No type matches the filter.', 'browse-empty'));
      return;
    }
    for (const group of grouped(kept)) {
      const section = make('section');
      section.append(make('h4', `${SOURCE_LABEL[group.source]} · ${group.entries.length}`));
      const items = make('ul');
      for (const entry of group.entries) items.append(this.item(entry, null));
      section.append(items);
      this.list.append(section);
    }
  }

  private item(entry: Entry, why: string | null): HTMLElement {
    const item = make('li', undefined, entry.current ? 'current' : undefined);
    const head = make('div', undefined, 'browse-head');
    head.append(make('strong', entry.id));
    if (entry.current) head.append(make('span', ' in use', 'browse-tag'));
    if (hasSignature(entry) === true) head.append(make('span', ' signature', 'browse-tag'));
    if (entry.verified) head.append(make('span', entry.verified === 'real' ? ' ✓' : ' ○', 'browse-tag'));
    item.append(head);
    item.append(make('div', entry.path, 'browse-path'));
    if (why) item.append(make('div', why, 'browse-why'));
    if (entry.doc) item.append(make('div', entry.doc, 'browse-doc'));
    if (entry.compileError) item.append(make('div', `✗ ${entry.compileError}`, 'browse-error'));
    if (entry.error) item.append(make('div', `✗ ${entry.error}`, 'browse-error'));
    const buttons = make('div', undefined, 'browse-buttons');
    const use = make('button', 'Use', undefined);
    use.type = 'button';
    use.disabled = entry.error !== null;
    use.title = entry.error ? 'the schema does not read' : `parse with ${entry.id}`;
    use.addEventListener('click', () => {
      this.dialog.close();
      this.handlers.pick(entry);
    });
    const view = make('button', 'View', undefined);
    view.type = 'button';
    view.title = 'read the schema (read-only)';
    view.addEventListener('click', () => void this.view(entry));
    buttons.append(use, view);
    item.append(buttons);
    return item;
  }

  private async view(entry: Entry): Promise<void> {
    this.readerTitle.textContent = entry.path;
    this.readerText.textContent = 'reading…';
    this.reader.hidden = false;
    this.list.hidden = true;
    this.filter.disabled = true;
    try {
      this.readerText.textContent = await this.handlers.text(entry);
    } catch (error) {
      this.readerText.textContent = `could not read it: ${String(error)}`;
    }
  }
}
