// The Files view: an archive or file system as directories and files. The library lists the entries (`filesystem:` in a
// schema, `doe::filesystem`); this nests them by path, orders them, and draws the rows. The tree building and the text
// of the columns need no DOM and are tested alone; `FilesView` is the DOM part.

import type { HighlightStyle } from '@codemirror/language';
import type { FsEntry, FsKind, Listing } from './doe.js';
import { drawHighlighted } from './highlight.js';
import { detectLanguage, LANGUAGES, loadParser } from './languages.js';
import type { Path } from './model.js';

export interface FsNode {
  /** The last component of the path. */
  name: string;
  /** All the components. */
  path: string[];
  /** The entry of the archive, or null for a directory that only the paths of other entries imply. */
  entry: FsEntry | null;
  children: FsNode[];
  /** Entries in this node and below, itself not counted. */
  count: number;
}

export const isDirectory = (node: FsNode): boolean => node.entry === null || node.entry.kind === 'directory';

/** The tree of the entries. A directory that is named by several entries (explicit and implied) is one node; an entry that repeats a file is a node of its own, flagged by the library. */
export function buildTree(entries: readonly FsEntry[]): FsNode {
  const root: FsNode = { name: '', path: [], entry: null, children: [], count: 0 };
  const directories = new Map<string, FsNode>([['', root]]);
  const key = (path: readonly string[]): string => path.join('/');
  /** The directory node for `path`, made (and its parents) when it is not there. */
  const directory = (path: string[]): FsNode => {
    const found = directories.get(key(path));
    if (found) return found;
    const parent = directory(path.slice(0, -1));
    const node: FsNode = { name: path[path.length - 1] ?? '', path, entry: null, children: [], count: 0 };
    parent.children.push(node);
    directories.set(key(path), node);
    return node;
  };
  for (const entry of entries) {
    const path = entry.path.length > 0 ? entry.path : [''];
    if (entry.kind === 'directory') {
      const node = directory(path);
      // the first entry that names a directory is its entry; a later one does not replace it
      node.entry ??= entry;
      continue;
    }
    const parent = directory(path.slice(0, -1));
    const node: FsNode = { name: path[path.length - 1] ?? '', path, entry, children: [], count: 0 };
    parent.children.push(node);
  }
  const total = (node: FsNode): number => {
    node.count = node.children.reduce((n, child) => n + 1 + total(child), 0);
    return node.count;
  };
  total(root);
  return root;
}

const collator = new Intl.Collator('en', { numeric: true, sensitivity: 'base' });

/** The children in the order to show: by name (case and accents do not matter, numbers by value), the directories first if asked. */
export function ordered(children: readonly FsNode[], directoriesFirst: boolean): FsNode[] {
  return [...children].sort((a, b) => {
    if (directoriesFirst && isDirectory(a) !== isDirectory(b)) return isDirectory(a) ? -1 : 1;
    return collator.compare(a.name, b.name) || (a.name < b.name ? -1 : a.name > b.name ? 1 : 0);
  });
}

/** The letter of `ls -l` for a kind. */
export const KIND_LETTER: Record<FsKind, string> = { file: '-', directory: 'd', symlink: 'l', hardlink: 'h', device: 'c', fifo: 'p', socket: 's' };

/** `drwxr-xr-x`: the kind and the nine permission bits; `-` for a mode the format does not give. */
export function modeText(mode: number | null, kind: FsKind): string {
  if (mode === null) return KIND_LETTER[kind];
  const bits = 'rwxrwxrwx';
  let out = KIND_LETTER[kind];
  for (let i = 0; i < 9; i += 1) out += mode & (1 << (8 - i)) ? bits.charAt(i) : '-';
  return out;
}

/** Bytes in the unit that reads best: `6 B`, `1.5 KiB`... */
export function sizeText(size: number | null): string {
  if (size === null) return '';
  if (size < 1024) return `${size} B`;
  const units = ['KiB', 'MiB', 'GiB', 'TiB'];
  let value = size / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

/** A time in seconds since 1970 as `2026-11-04 12:00` (UTC). */
export function timeText(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds)) return '';
  const date = new Date(seconds * 1000);
  if (Number.isNaN(date.getTime())) return '';
  return date.toISOString().slice(0, 16).replace('T', ' ');
}

/** The words for a flag of the library. */
export const FLAG_TEXT: Record<string, string> = {
  empty: 'the entry has no name',
  absolute: 'the path is absolute: extracting it would write outside the folder',
  parent: 'the path has a ..: extracting it would write outside the folder',
  repeated: 'an earlier entry has the same path (the later one replaces it)',
};

/** Whether bytes are text to show as a preview: valid UTF-8 without control characters other than white space. */
export function textOf(bytes: Uint8Array): string | null {
  try {
    const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    // eslint-disable-next-line no-control-regex
    return /[\u0000-\u0008\u000e-\u001f\u007f]/.test(text) ? null : text;
  } catch {
    return null;
  }
}

/** A checksum algorithm of the column: the name `doe::filesystem::Algorithm` and `md5sum`-like tools use, the label of the header and the width. */
export interface ChecksumAlgorithm {
  id: string;
  label: string;
  /** How many hex digits. */
  digits: number;
  /** Whether the header says it is for comparing files, not a security measure (MD5 and SHA-1 have known collisions). */
  weak: boolean;
}

/** The algorithms, in the order the menu lists them and `m` goes through them. */
export const ALGORITHMS: readonly ChecksumAlgorithm[] = [
  { id: 'md5', label: 'MD5', digits: 32, weak: true },
  { id: 'sha1', label: 'SHA-1', digits: 40, weak: true },
  { id: 'sha256', label: 'SHA-256', digits: 64, weak: false },
];

/** An algorithm with more digits than this is shown cut to `SHORT_DIGITS` in a pane too narrow for the row, with the whole in the tooltip. */
export const LONG_DIGITS = 40;
export const SHORT_DIGITS = 16;

/** The text of a digest in the column: the whole, or its first `SHORT_DIGITS` and an ellipsis when `compact` and the algorithm is a long one. */
export function shownDigest(digest: string, compact: boolean): string {
  return compact && digest.length > LONG_DIGITS ? `${digest.slice(0, SHORT_DIGITS)}…` : digest;
}

/** The algorithm that comes `step` places on from `current` (null: off) in the cycle off, MD5, SHA-1, SHA-256, off. */
export function cycleAlgorithm(current: string | null, step: 1 | -1): string | null {
  const at = current === null ? 0 : ALGORITHMS.findIndex((a) => a.id === current) + 1;
  const next = (at + step + ALGORITHMS.length + 1) % (ALGORITHMS.length + 1);
  return next === 0 ? null : (ALGORITHMS[next - 1]?.id ?? null);
}
/** Ranges are sent to be hashed in batches of about this many bytes (and at most `HASH_BATCH_ENTRIES` entries). */
export const HASH_BATCH_BYTES = 16 * 1024 * 1024;
export const HASH_BATCH_ENTRIES = 200;

/** The line of `md5sum` for a file: the digest, two spaces, the path; a name with a backslash or a new line is escaped as `md5sum` does. */
export function checksumLine(digest: string, path: string): string {
  if (!path.includes('\\') && !path.includes('\n')) return `${digest}  ${path}\n`;
  return `\\${digest}  ${path.replaceAll('\\', '\\\\').replaceAll('\n', '\\n')}\n`;
}

/** Cut `items` (with the bytes each one hashes) into batches of at most `bytes` bytes and `count` items; an item over the limit is a batch alone. */
export function batchesOf<T>(items: readonly T[], size: (item: T) => number, bytes = HASH_BATCH_BYTES, count = HASH_BATCH_ENTRIES): T[][] {
  const out: T[][] = [];
  let batch: T[] = [];
  let used = 0;
  for (const item of items) {
    const n = size(item);
    if (batch.length > 0 && (used + n > bytes || batch.length >= count)) {
      out.push(batch);
      batch = [];
      used = 0;
    }
    batch.push(item);
    used += n;
  }
  if (batch.length > 0) out.push(batch);
  return out;
}

/** The most children of a directory drawn at first, and each time "more" is asked. */
export const CHUNK = 500;
/** The most bytes of an entry that the preview reads: more is cut, with a mark. */
export const PREVIEW_BYTES = 100_000;
/** The longest line the preview draws (a longer one is cut, with a mark): a minified file is one line of megabytes. */
export const LINE_LIMIT = 2000;

/** The text of the bytes of a preview that was cut at `PREVIEW_BYTES`: the cut may be inside a character, which is dropped. */
export function textOfCut(bytes: Uint8Array): string | null {
  for (let drop = 0; drop <= 3 && drop < bytes.length; drop += 1) {
    const text = textOf(drop === 0 ? bytes : bytes.subarray(0, bytes.length - drop));
    if (text !== null) return text;
  }
  return null;
}

/** `text` with each line of more than `limit` characters cut to it and ended by `…`. */
export function cutLines(text: string, limit: number = LINE_LIMIT): string {
  if (text.length <= limit) return text;
  return text
    .split('\n')
    .map((line) => {
      if (line.length <= limit) return line;
      // not between the two halves of a character outside the BMP
      const high = line.charCodeAt(limit - 1) >= 0xd800 && line.charCodeAt(limit - 1) < 0xdc00;
      return `${line.slice(0, high ? limit - 1 : limit)}…`;
    })
    .join('\n');
}

interface Row {
  node: FsNode;
  depth: number;
  /** For a "N more" row: the directory it belongs to. */
  more?: { of: FsNode; hidden: number };
}

export interface FilesHandlers {
  /** An entry was chosen: its place in the trace. */
  select(path: Path | null): void;
  /** The bytes of the input from `from` to `to`, as far as the page has them; undefined if it has none (not loaded). */
  read(from: number, to: number): Uint8Array | undefined;
  /** Save the contents of an entry as a file. */
  save(entry: FsEntry): void;
  /**
   * The checksums of ranges of the input, or null when the page does not have the input in memory (a file that stays
   * on the machine of `doe --ide`): then the column says so. A rejection leaves `—` for the batch.
   */
  hash(algorithm: string, ranges: [number, number][]): Promise<(string | null)[]> | null;
  /** Save a text as a file of that name. */
  download(name: string, text: string): void;
}

export class FilesView {
  private readonly list: HTMLElement;
  private readonly preview: HTMLElement;
  private readonly previewName: HTMLElement;
  private readonly previewText: HTMLElement;
  private readonly saveButton: HTMLButtonElement;
  private readonly language: HTMLSelectElement;
  private readonly hashValue: HTMLElement;
  private readonly hashCopy: HTMLButtonElement;
  private readonly hashBar: HTMLElement;
  private readonly hashSave: HTMLButtonElement;
  private readonly hashHead: HTMLElement;
  /** Whether the list is too narrow for the whole of a long digest in its row (a pane the size of a phone's, a small window). */
  private compact = false;
  /** The algorithm of the checksum column, or null when the column is off. */
  private algorithm: string | null = null;
  /** The checksums by the algorithm and the range they are of (`md5:from-to`): valid for the input they were made from. */
  private readonly digests = new Map<string, string>();
  /** The ranges the worker could not give (the input does not hold them, or the call failed). */
  private readonly failed = new Set<string>();
  /** Whether the page has no input in memory to hash. */
  private noInput = false;
  /** Counts the runs of the hashing, so that one that was overtaken (the listing or the input changed) stops. */
  private hashRun = 0;
  private style: HighlightStyle | null = null;
  /** The language chosen in the badge for the entry that is shown (by its key); none when the guess is used. */
  private chosen: { entry: string; language: string } | null = null;
  /** Counts the previews, so that a parser that arrives late for an entry that is no longer shown is dropped. */
  private drawn = 0;
  private root: FsNode | null = null;
  private byTrace = new Map<string, FsNode>();
  private readonly open = new Set<string>();
  private readonly shown = new Map<string, number>();
  private selected: string | null = null;
  private rows: Row[] = [];
  directoriesFirst = true;

  constructor(
    private readonly parent: HTMLElement,
    private readonly handlers: FilesHandlers,
  ) {
    this.list = parent.querySelector<HTMLElement>('#files-list') ?? document.createElement('div');
    this.preview = parent.querySelector<HTMLElement>('#files-preview') ?? document.createElement('div');
    this.previewName = parent.querySelector<HTMLElement>('#files-preview-name') ?? document.createElement('span');
    this.previewText = parent.querySelector<HTMLElement>('#files-preview-text') ?? document.createElement('pre');
    this.saveButton = parent.querySelector<HTMLButtonElement>('#files-save') ?? document.createElement('button');
    this.language = parent.querySelector<HTMLSelectElement>('#files-language') ?? document.createElement('select');
    this.hashValue = parent.querySelector<HTMLElement>('#files-hash-value') ?? document.createElement('code');
    this.hashCopy = parent.querySelector<HTMLButtonElement>('#files-hash-copy') ?? document.createElement('button');
    this.hashBar = parent.querySelector<HTMLElement>('#files-hash-bar') ?? document.createElement('div');
    this.hashSave = parent.querySelector<HTMLButtonElement>('#files-hash-save') ?? document.createElement('button');
    this.hashHead = parent.querySelector<HTMLElement>('#files-hash-head') ?? document.createElement('span');
    this.hashCopy.addEventListener('click', () => {
      const digest = this.hashValue.dataset.digest ?? '';
      if (digest) void navigator.clipboard?.writeText(digest).catch(() => undefined);
    });
    this.hashSave.addEventListener('click', () => this.saveChecksums());
    if (typeof ResizeObserver !== 'undefined') new ResizeObserver(() => this.fit()).observe(this.list);
    this.language.addEventListener('change', () => {
      this.chosen = this.selected === null || this.language.value === '' ? null : { entry: this.selected, language: this.language.value };
      this.showPreview();
    });
    this.list.addEventListener('click', (event) => this.click(event));
    this.list.addEventListener('dblclick', (event) => {
      const row = this.rowOf(event.target);
      if (row && isDirectory(row.node)) this.toggle(row.node);
    });
    this.list.addEventListener('keydown', (event) => this.keydown(event));
    this.saveButton.addEventListener('click', () => {
      const node = this.selectedNode();
      if (node?.entry?.data) this.handlers.save(node.entry);
    });
  }

  /** Whether there is a listing. */
  get has(): boolean {
    return this.root !== null;
  }

  focus(): void {
    this.list.focus();
  }

  /** Show a listing (or none). What was open stays open for the same paths. */
  setListing(listing: Listing | null | undefined): void {
    if (!listing || listing.entries.length === 0) {
      this.root = null;
      this.byTrace.clear();
      this.open.clear();
      this.shown.clear();
      this.selected = null;
      this.render();
      return;
    }
    this.root = buildTree(listing.entries);
    this.byTrace = new Map();
    const index = (node: FsNode): void => {
      if (node.entry) this.byTrace.set(node.entry.trace_path.join(','), node);
      node.children.forEach(index);
    };
    index(this.root);
    // a first listing opens the top level; a later one keeps what was open
    if (this.open.size === 0) this.open.add('');
    this.render();
    if (this.algorithm !== null) void this.computeChecksums();
  }

  private selectedNode(): FsNode | null {
    return this.selected === null ? null : (this.rows.find((r) => key(r.node) === this.selected)?.node ?? null);
  }

  /** Select the entry that holds the node at `path` of the trace (the entry itself or a field inside it), opening its directories. */
  syncSelection(path: Path | null): void {
    if (!this.root || path === null) {
      this.selected = null;
      this.render();
      return;
    }
    for (let length = path.length; length > 0; length -= 1) {
      const node = this.byTrace.get(path.slice(0, length).join(','));
      if (node) {
        for (let i = 0; i < node.path.length; i += 1) this.open.add(node.path.slice(0, i).join('/'));
        this.selected = key(node);
        this.render();
        this.list.querySelector('.selected')?.scrollIntoView({ block: 'nearest' });
        return;
      }
    }
    // the node is not inside an entry (the header of the archive, say)
    this.selected = null;
    this.render();
  }

  private visible(node: FsNode, depth: number, out: Row[]): void {
    const children = ordered(node.children, this.directoriesFirst);
    const limit = this.shown.get(node.path.join('/')) ?? CHUNK;
    for (const child of children.slice(0, limit)) {
      out.push({ node: child, depth });
      if (isDirectory(child) && this.open.has(child.path.join('/'))) this.visible(child, depth + 1, out);
    }
    if (children.length > limit) out.push({ node, depth, more: { of: node, hidden: children.length - limit } });
  }

  private render(): void {
    this.list.replaceChildren();
    this.rows = [];
    if (this.root) this.visible(this.root, 0, this.rows);
    const fragment = document.createDocumentFragment();
    this.rows.forEach((row, i) => fragment.append(this.draw(row, i)));
    this.list.append(fragment);
    this.showPreview();
  }

  private draw(row: Row, index: number): HTMLElement {
    const el = document.createElement('div');
    el.className = 'file-row';
    el.dataset.row = String(index);
    el.setAttribute('role', 'treeitem');
    el.setAttribute('aria-level', String(row.depth + 1));
    if (row.more) {
      el.className = 'file-row file-more';
      el.textContent = `${row.more.hidden} more…`;
      return el;
    }
    const node = row.node;
    const entry = node.entry;
    const dir = isDirectory(node);
    const id = key(node);
    el.style.setProperty('--depth', String(row.depth));
    if (id === this.selected) {
      el.classList.add('selected');
      el.setAttribute('aria-selected', 'true');
    }
    if (dir) el.setAttribute('aria-expanded', String(this.open.has(node.path.join('/'))));
    const mark = document.createElement('span');
    mark.className = 'file-twisty';
    mark.textContent = dir ? (this.open.has(node.path.join('/')) ? '▾' : '▸') : ' ';
    mark.setAttribute('aria-hidden', 'true');
    const kind = entry?.kind ?? 'directory';
    const type = document.createElement('span');
    type.className = `file-kind kind-${kind}`;
    type.textContent = KIND_LETTER[kind];
    type.title = kind;
    const name = document.createElement('span');
    name.className = 'file-name';
    name.textContent = node.name === '' ? '(no name)' : node.name;
    if (entry?.link) {
      const link = document.createElement('span');
      link.className = 'file-link';
      link.textContent = ` -> ${entry.link}`;
      name.append(link);
    }
    if (dir) {
      const count = document.createElement('span');
      count.className = 'file-count';
      count.textContent = ` ${node.count}`;
      count.title = `${node.count} ${node.count === 1 ? 'entry' : 'entries'} inside`;
      name.append(count);
    }
    el.append(mark, type, name);
    if (entry?.flags.length) {
      const flag = document.createElement('span');
      flag.className = 'file-flag';
      flag.textContent = '⚠';
      flag.title = entry.flags.map((f) => FLAG_TEXT[f] ?? f).join('; ');
      flag.setAttribute('role', 'img');
      flag.setAttribute('aria-label', flag.title);
      el.append(flag);
    }
    const size = document.createElement('span');
    size.className = 'file-size';
    size.textContent = dir ? '' : sizeText(entry?.size ?? null);
    const cells: HTMLElement[] = [size];
    if (this.algorithm !== null) {
      const hash = document.createElement('span');
      hash.className = 'file-hash';
      hash.dataset.k = key(node);
      this.paintCell(hash, entry);
      cells.push(hash);
    }
    const mode = document.createElement('span');
    mode.className = 'file-mode';
    mode.textContent = entry ? modeText(entry.mode, entry.kind) : '';
    const time = document.createElement('span');
    time.className = 'file-time';
    time.textContent = timeText(entry?.mtime ?? null);
    time.title = 'UTC';
    el.append(...cells, mode, time);
    return el;
  }

  private showPreview(): void {
    const node = this.selectedNode();
    const entry = node?.entry ?? null;
    this.preview.hidden = entry === null;
    this.saveButton.hidden = true;
    this.language.hidden = true;
    this.hashValue.hidden = true;
    this.hashCopy.hidden = true;
    this.drawn += 1;
    this.previewText.textContent = '';
    delete this.previewText.dataset.language;
    if (!node || !entry) return;
    this.previewName.textContent = `${node.path.join('/')}${entry.link ? ` -> ${entry.link}` : ''}`;
    if (!entry.data) {
      this.previewText.textContent = entry.kind === 'file' ? '(no contents: the schema does not give them, or they are empty)' : '';
      return;
    }
    if (entry.data_is_file === false) {
      this.previewText.textContent = '(the stored bytes are compressed or otherwise changed, not the file: the schema says so, so there is no preview or Save of them; see them in the Bytes pane)';
      return;
    }
    const [from, end] = entry.data;
    this.saveButton.hidden = false;
    this.showDigest(entry);
    const cut = end - from > PREVIEW_BYTES;
    const bytes = this.handlers.read(from, Math.min(end, from + PREVIEW_BYTES));
    if (!bytes) {
      this.previewText.textContent = '(the bytes are not loaded here: select the entry and look in the Bytes pane)';
      return;
    }
    const text = cut ? textOfCut(bytes) : textOf(bytes);
    if (text === null) {
      this.previewText.textContent = `${end - from} bytes (not text)`;
      return;
    }
    const shown = cutLines(text) + (cut ? '\n…' : '');
    const guess = detectLanguage(node.name, text);
    this.drawLanguage(guess);
    const wanted = this.chosen?.entry === this.selected ? this.chosen.language : '';
    const id = wanted === '' ? guess : wanted === 'plain' ? null : wanted;
    this.previewText.textContent = shown;
    this.previewText.dataset.language = id ?? 'plain';
    if (id === null || !this.style) return;
    // the parser comes in its own chunk the first time: the text is on the page (plain) meanwhile
    const mine = ++this.drawn;
    const style = this.style;
    loadParser(id).then(
      (parser) => {
        if (mine === this.drawn) drawHighlighted(this.previewText, shown, style, parser);
      },
      () => {
        // the parser could not be loaded (offline, say): the text stays plain
      },
    );
  }

  /** Fill the badge: the guess first, then the choices; the choice for this entry stays selected. */
  private drawLanguage(guess: string | null): void {
    const name = (id: string | null): string => LANGUAGES.find((l) => l.id === id)?.name ?? 'plain';
    const auto = new Option(`Auto: ${name(guess)}`, '');
    const options = [auto, new Option('Plain', 'plain'), ...LANGUAGES.map((l) => new Option(l.name, l.id))];
    this.language.replaceChildren(...options);
    this.language.value = this.chosen?.entry === this.selected ? this.chosen.language : '';
    this.language.hidden = false;
  }

  /** The colours of the preview: those of a theme. */
  setTheme(style: HighlightStyle): void {
    this.style = style;
    if (this.root) this.showPreview();
  }

  // ── checksums ───────────────────────────────────────────────────────────────

  /** The algorithm of the checksum column, or null when it is off. */
  get checksumAlgorithm(): string | null {
    return this.algorithm;
  }

  private info(): ChecksumAlgorithm | null {
    return ALGORITHMS.find((a) => a.id === this.algorithm) ?? null;
  }

  /**
   * Show the column with the checksums of an algorithm, or hide it (null). The checksums computed for the other
   * algorithms are kept for this input, so that going back to one is at once; only the missing ones are computed.
   */
  setChecksums(id: string | null): void {
    const wanted = ALGORITHMS.some((a) => a.id === id) ? id : null;
    if (wanted === this.algorithm) return;
    this.algorithm = wanted;
    this.hashRun += 1;
    const info = this.info();
    this.hashBar.hidden = info === null;
    if (info) {
      this.fit();
      this.list.style.setProperty('--hash-width', `${this.compact && info.digits > LONG_DIGITS ? SHORT_DIGITS + 1 : info.digits}ch`);
      this.hashHead.textContent = info.label;
      this.hashHead.title = info.weak ? `${info.label} is a checksum for comparing files, not a security measure (it has known collisions)` : `${info.label} checksum of the contents, to compare with a published one`;
    }
    this.render();
    if (info) void this.computeChecksums();
  }

  /** The list changed size: a long digest is cut when the row has no room for all of it (the name keeps 20 characters). */
  private fit(): void {
    const info = this.info();
    const size = Number.parseFloat(getComputedStyle(this.list).fontSize) || 13;
    // size, mode, time, the gaps and a name, in characters of the monospace font (about 0.6 of its size)
    const compact = info !== null && info.digits > LONG_DIGITS && this.list.clientWidth < (info.digits + 62) * size * 0.6;
    if (compact === this.compact) return;
    this.compact = compact;
    if (info) this.list.style.setProperty('--hash-width', `${compact ? SHORT_DIGITS + 1 : info.digits}ch`);
    this.paintChecksums();
  }

  /** The input is another one: what was computed is of the old bytes. */
  resetChecksums(): void {
    this.digests.clear();
    this.failed.clear();
    this.noInput = false;
    this.hashRun += 1;
  }

  /** The key of a range in the cache. */
  private rangeKey(range: readonly [number, number]): string {
    return `${this.algorithm ?? ''}:${range[0]}-${range[1]}`;
  }

  /** The range a file entry's checksum is of: its data, or nothing for a file that has none and is empty. Null: no checksum. */
  private rangeOf(entry: FsEntry): [number, number] | null {
    if (entry.kind !== 'file' || entry.data_is_file === false) return null;
    if (entry.data) return [entry.data[0], entry.data[1]];
    return entry.size === 0 ? [0, 0] : null;
  }

  /** The text and the explanation of the cell of an entry. */
  private cellOf(entry: FsEntry | null): { text: string; title: string } {
    if (!entry || entry.kind !== 'file') return { text: '', title: '' };
    if (entry.data_is_file === false) return { text: '—', title: 'the schema says that the stored bytes are not the file (they are compressed): a checksum of them would not be that of the file' };
    const range = this.rangeOf(entry);
    if (!range) return { text: '—', title: 'the schema does not give the contents of this entry' };
    const digest = this.digests.get(this.rangeKey(range));
    if (digest) return { text: shownDigest(digest, this.compact), title: `${this.info()?.label ?? ''} ${digest}` };
    if (this.noInput) return { text: '—', title: 'the bytes of the file are not loaded in the page; use doe --ide with the file on that machine' };
    if (this.failed.has(this.rangeKey(range))) return { text: '—', title: 'the checksum could not be computed (the bytes are not all there)' };
    return { text: '…', title: 'being computed' };
  }

  private paintCell(cell: HTMLElement, entry: FsEntry | null): void {
    const { text, title } = this.cellOf(entry);
    cell.textContent = text;
    cell.title = title;
  }

  /** Write what is known into the cells that are on the page (not the whole list again). */
  private paintChecksums(): void {
    for (const cell of this.list.querySelectorAll<HTMLElement>('.file-hash')) {
      const row = this.rows.find((r) => !r.more && key(r.node) === cell.dataset.k);
      this.paintCell(cell, row?.node.entry ?? null);
    }
    const node = this.selectedNode();
    if (node?.entry) this.showDigest(node.entry);
    this.hashSave.disabled = !this.checksumsDone();
  }

  /** The digest of the selected entry in the preview bar, with the copy button. */
  private showDigest(entry: FsEntry): void {
    const range = this.algorithm !== null ? this.rangeOf(entry) : null;
    const digest = range ? this.digests.get(this.rangeKey(range)) : undefined;
    this.hashValue.hidden = !digest;
    this.hashCopy.hidden = !digest;
    this.hashValue.textContent = digest ? `${this.info()?.label ?? ''} ${digest}` : '';
    this.hashValue.dataset.digest = digest ?? '';
  }

  /** The entries of the listing that have a checksum to compute. */
  private hashable(): FsEntry[] {
    const out: FsEntry[] = [];
    const walk = (node: FsNode): void => {
      if (node.entry && this.rangeOf(node.entry)) out.push(node.entry);
      node.children.forEach(walk);
    };
    if (this.root) walk(this.root);
    return out;
  }

  /** Whether every entry that can have a checksum has its (or has failed). */
  private checksumsDone(): boolean {
    if (this.noInput) return false;
    return this.hashable().every((e) => {
      const k = this.rangeKey(this.rangeOf(e) ?? [0, 0]);
      return this.digests.has(k) || this.failed.has(k);
    });
  }

  /** Compute the checksums that are missing, a batch at a time; stops when the run is overtaken. */
  private async computeChecksums(): Promise<void> {
    const run = ++this.hashRun;
    const algorithm = this.algorithm;
    if (algorithm === null) return;
    const todo = new Map<string, [number, number]>();
    for (const entry of this.hashable()) {
      const range = this.rangeOf(entry);
      if (range && !this.digests.has(this.rangeKey(range)) && !this.failed.has(this.rangeKey(range))) todo.set(this.rangeKey(range), range);
    }
    this.paintChecksums();
    for (const batch of batchesOf([...todo.values()], (r) => r[1] - r[0])) {
      if (run !== this.hashRun) return;
      const pending = this.handlers.hash(algorithm, batch);
      if (pending === null) {
        this.noInput = true;
        this.paintChecksums();
        return;
      }
      let digests: (string | null)[];
      try {
        digests = await pending;
      } catch {
        digests = batch.map(() => null);
      }
      if (run !== this.hashRun) return;
      batch.forEach((range, i) => {
        const digest = digests[i];
        if (digest) this.digests.set(this.rangeKey(range), digest);
        else this.failed.add(this.rangeKey(range));
      });
      this.paintChecksums();
    }
  }

  /** The checksums of every file as `md5sum`, `sha1sum` and `sha256sum` write them: `<digits>  <path>` lines in the order of the archive. */
  private checksumText(): string {
    let text = '';
    const walk = (node: FsNode): void => {
      const entry = node.entry;
      const range = entry ? this.rangeOf(entry) : null;
      const digest = range ? this.digests.get(this.rangeKey(range)) : undefined;
      // an entry that cannot be placed safely is not written: `md5sum -c` would read outside the folder
      if (entry && digest && entry.flags.length === 0) text += checksumLine(digest, entry.path.join('/'));
      node.children.forEach(walk);
    };
    if (this.root) walk(this.root);
    return text;
  }

  /** Save the list (so that `md5sum -c`, `sha1sum -c` or `sha256sum -c` reads it in the folder the archive is extracted to). */
  private saveChecksums(): void {
    const text = this.checksumText();
    if (text) this.handlers.download(`checksums.${this.algorithm ?? 'md5'}`, text);
  }

  /** Put the list on the clipboard (the key S). What it says is for the status line; null: there is no list to copy. */
  async copyChecksums(): Promise<string | null> {
    const info = this.info();
    if (!info) return null;
    if (!this.checksumsDone()) return `the ${info.label} checksums are still being made`;
    const text = this.checksumText();
    if (!text) return `no ${info.label} checksum to copy`;
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      return 'the clipboard cannot be written here';
    }
    return `copied the ${info.label} checksums of ${text.split('\n').length - 1} files`;
  }

  private rowOf(target: EventTarget | null): Row | null {
    const el = (target as HTMLElement | null)?.closest<HTMLElement>('.file-row');
    return el ? (this.rows[Number(el.dataset.row)] ?? null) : null;
  }

  private choose(node: FsNode): void {
    this.selected = key(node);
    this.render();
    this.handlers.select(node.entry ? node.entry.trace_path : null);
  }

  private toggle(node: FsNode): void {
    const id = node.path.join('/');
    if (this.open.has(id)) this.open.delete(id);
    else this.open.add(id);
    this.render();
  }

  private click(event: MouseEvent): void {
    const row = this.rowOf(event.target);
    if (!row) return;
    if (row.more) {
      const id = row.more.of.path.join('/');
      this.shown.set(id, (this.shown.get(id) ?? CHUNK) + CHUNK);
      this.render();
      return;
    }
    if (isDirectory(row.node) && (event.target as HTMLElement).classList.contains('file-twisty')) this.toggle(row.node);
    this.choose(row.node);
  }

  private keydown(event: KeyboardEvent): void {
    if (event.ctrlKey || event.altKey || event.metaKey) return;
    const at = this.rows.findIndex((r) => !r.more && key(r.node) === this.selected);
    const move = (to: number): void => {
      const row = this.rows[Math.max(0, Math.min(this.rows.length - 1, to))];
      if (row && !row.more) this.choose(row.node);
      this.list.querySelector('.selected')?.scrollIntoView({ block: 'nearest' });
    };
    const current = at >= 0 ? this.rows[at] : undefined;
    switch (event.key) {
      case 'ArrowDown':
        move(at + 1);
        break;
      case 'ArrowUp':
        move(at <= 0 ? 0 : at - 1);
        break;
      case 'Home':
        move(0);
        break;
      case 'End':
        move(this.rows.length - 1);
        break;
      case 'ArrowRight':
        if (current && isDirectory(current.node) && !this.open.has(current.node.path.join('/'))) this.toggle(current.node);
        else move(at + 1);
        break;
      case 'ArrowLeft':
        if (current && isDirectory(current.node) && this.open.has(current.node.path.join('/'))) this.toggle(current.node);
        else if (current) {
          const parent = this.rows.findIndex((r) => !r.more && r.node.path.length === current.node.path.length - 1 && r.node.path.every((p, i) => p === current.node.path[i]));
          if (parent >= 0) move(parent);
        }
        break;
      case 'Enter':
      case ' ':
        if (current && isDirectory(current.node)) this.toggle(current.node);
        break;
      default:
        return;
    }
    event.preventDefault();
  }
}

function key(node: FsNode): string {
  // an entry is told from another with the same path by its place in the trace
  return `${node.path.join('/')}\u0000${node.entry ? node.entry.trace_path.join(',') : ''}`;
}
