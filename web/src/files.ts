// The Files view: an archive or file system as directories and files. The library lists the entries (`filesystem:` in a
// schema, `doe::filesystem`); this nests them by path, orders them, and draws the rows. The tree building and the text
// of the columns need no DOM and are tested alone; `FilesView` is the DOM part.

import type { FsEntry, FsKind, Listing } from './doe.js';
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

/** The most children of a directory drawn at first, and each time "more" is asked. */
export const CHUNK = 500;
const PREVIEW_BYTES = 4096;

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
}

export class FilesView {
  private readonly list: HTMLElement;
  private readonly preview: HTMLElement;
  private readonly previewName: HTMLElement;
  private readonly previewText: HTMLElement;
  private readonly saveButton: HTMLButtonElement;
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
    const mode = document.createElement('span');
    mode.className = 'file-mode';
    mode.textContent = entry ? modeText(entry.mode, entry.kind) : '';
    const time = document.createElement('span');
    time.className = 'file-time';
    time.textContent = timeText(entry?.mtime ?? null);
    time.title = 'UTC';
    el.append(size, mode, time);
    return el;
  }

  private showPreview(): void {
    const node = this.selectedNode();
    const entry = node?.entry ?? null;
    this.preview.hidden = entry === null;
    this.saveButton.hidden = true;
    this.previewText.textContent = '';
    if (!node || !entry) return;
    this.previewName.textContent = `${node.path.join('/')}${entry.link ? ` -> ${entry.link}` : ''}`;
    if (!entry.data) {
      this.previewText.textContent = entry.kind === 'file' ? '(no contents: the schema does not give them, or they are empty)' : '';
      return;
    }
    const [from, end] = entry.data;
    this.saveButton.hidden = false;
    const bytes = this.handlers.read(from, Math.min(end, from + PREVIEW_BYTES));
    if (!bytes) {
      this.previewText.textContent = '(the bytes are not loaded here: select the entry and look in the Bytes pane)';
      return;
    }
    const text = textOf(bytes);
    this.previewText.textContent = text === null ? `${end - from} bytes (not text)` : text + (end - from > PREVIEW_BYTES ? '\n…' : '');
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
