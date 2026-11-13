// The parsed value as a tree: each field with its type, its value and the bytes
// it was read from. Only the rows that are in view of the tree are in the page
// (the children of a node that is closed are not made), and a node with very
// many children shows a first part of them and a row that shows more.

import type { Docs, TraceNode } from './doe.js';
import { type Path, describeRange, docOf, firstLine, nodeAt, valueKind } from './model.js';
import { type Radix, fieldKey, integerText } from './radix.js';

/** How many children of a node are shown at first, and added by each click on "more". */
const CHUNK = 200;

interface Row {
  path: Path;
  /** The name that the radix of the field is kept under. */
  key: string;
  node: TraceNode;
  depth: number;
  /** The description (`doc:`) of the field, whole; null when it has none. */
  doc?: string | null;
  /** For the row that shows more: how many are hidden. */
  more?: number;
}

const key = (path: Path): string => path.join('.');

function escape(text: string): string {
  return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

export class TreeView {
  private readonly element: HTMLElement;
  private root: TraceNode | null = null;
  private readonly open = new Set<string>();
  private readonly shown = new Map<string, number>();
  private selected: Path | null = null;
  private rows: Row[] = [];
  /** The descriptions of the fields of the result, and whether the rows show them. */
  private docs: Docs | undefined;
  private showDocs = true;

  constructor(
    parent: HTMLElement,
    private readonly onSelect: (path: Path) => void,
    private readonly radixOf: (key: string) => Radix = () => 'dec',
    /** The arrow of a field that points was clicked. */
    private readonly onFollow: (path: Path) => void = () => undefined,
  ) {
    this.element = document.createElement('div');
    this.element.className = 'tree';
    this.element.tabIndex = 0;
    this.element.setAttribute('role', 'tree');
    this.element.setAttribute('aria-label', 'parsed value');
    parent.append(this.element);
    this.element.addEventListener('click', (event) => this.click(event));
    this.element.addEventListener('keydown', (event) => this.keydown(event));
  }

  /** Show a new result. What was open stays open where the same names are still there. */
  setTrace(root: TraceNode | null, docs?: Docs): void {
    const previous = this.root;
    this.root = root;
    this.docs = docs;
    // What was open is kept for the same schema, and forgotten for another.
    if (previous && root && (previous.name !== root.name || previous.type !== root.type)) {
      this.open.clear();
      this.shown.clear();
    }
    if (!root) {
      this.open.clear();
      this.shown.clear();
      this.selected = null;
    } else {
      this.open.add('');
      if (!previous) this.shown.clear();
      // Open the way to the node where the parse failed.
      let path: number[] = [];
      let node = root;
      for (;;) {
        const index = node.children.findIndex((child) => child.failed || this.leadsToFailure(child));
        const next = node.children[index];
        if (index < 0 || !next) break;
        path = [...path, index];
        this.open.add(key(path.slice(0, -1)));
        node = next;
      }
    }
    this.render();
  }

  /** Show the descriptions of the fields after their values, or not. */
  setDocsShown(on: boolean): void {
    if (on === this.showDocs) return;
    this.showDocs = on;
    this.render();
  }

  /** The description of the node at `path`, whole, when the rows show descriptions. */
  docAt(path: Path): string | null {
    if (!this.showDocs || !this.root || path.length === 0) return null;
    const parent = nodeAt(this.root, path.slice(0, -1));
    const node = nodeAt(this.root, path);
    return parent && node ? docOf(this.docs, parent.type, node.name) : null;
  }

  private leadsToFailure(node: TraceNode): boolean {
    return node.failed || node.children.some((child) => this.leadsToFailure(child));
  }

  /** Mark a node as the selected one, opening the way to it. */
  select(path: Path | null, reveal: boolean): void {
    this.selected = path;
    if (path && this.root) {
      for (let length = 0; length < path.length; length += 1) this.open.add(key(path.slice(0, length)));
      // A node past the first part of its parent's children has to be made.
      const parent = path.slice(0, -1);
      const index = path[path.length - 1] ?? 0;
      if (index >= (this.shown.get(key(parent)) ?? CHUNK)) this.shown.set(key(parent), index + 1);
    }
    this.render();
    if (reveal) this.element.querySelector('.tree-row.selected')?.scrollIntoView({ block: 'nearest' });
  }

  private visibleRows(): Row[] {
    const rows: Row[] = [];
    const root = this.root;
    if (!root) return rows;
    const walk = (node: TraceNode, path: Path, depth: number, field: string, parentType: string | null): void => {
      rows.push({ path, node, depth, key: field, doc: this.showDocs ? docOf(this.docs, parentType, node.name) : null });
      if (!this.open.has(key(path))) return;
      const limit = this.shown.get(key(path)) ?? CHUNK;
      node.children.slice(0, limit).forEach((child, index) => walk(child, [...path, index], depth + 1, fieldKey(field, node, child), node.type));
      if (node.children.length > limit) rows.push({ path, node, depth: depth + 1, key: field, more: node.children.length - limit });
    };
    walk(root, [], 0, '', null);
    return rows;
  }

  private render(): void {
    this.rows = this.visibleRows();
    const selected = this.selected ? key(this.selected) : null;
    this.element.innerHTML = this.rows
      .map((row, index) => {
        const indent = `style="--depth:${row.depth}"`;
        if (row.more !== undefined) {
          return `<div class="tree-row more" ${indent} data-row="${index}" role="treeitem">${row.more} more…</div>`;
        }
        const { node, path } = row;
        const integer = integerText(node, this.radixOf(row.key));
        const text = integer ?? node.summary;
        const classes = ['tree-row'];
        if (selected !== null && key(path) === selected) classes.push('selected');
        if (node.instance) classes.push('instance');
        if (node.failed) classes.push('failed');
        if (node.problem) classes.push('invalid');
        const folded = node.children.length > 0;
        const isOpen = this.open.has(key(path));
        const toggle = folded ? `<span class="toggle">${isOpen ? '▾' : '▸'}</span>` : '<span class="toggle"></span>';
        const range = describeRange(node);
        const parts = [
          `<span class="name">${escape(node.name)}</span>`,
          node.type ? `<span class="type">${escape(node.type)}</span>` : '',
          text !== null ? `<span class="summary ${node.integer ? (node.integer.name === null ? 'number' : 'enum') : (valueKind(text) ?? '')}">${escape(text)}</span>` : '',
          row.doc ? `<span class="doc" title="${escape(row.doc)}">${escape(firstLine(row.doc))}</span>` : '',
          node.points_to ? `<span class="pointer" role="button" title="Jump to what this field points at (J)">→ 0x${Math.floor(node.points_to.start / 8).toString(16)}</span>` : '',
          range ? `<span class="range">${escape(range)}</span>` : node.instance ? '<span class="range">computed</span>' : '',
          node.failed ? '<span class="flag">failed here</span>' : '',
          node.problem ? `<span class="flag" title="${escape(node.problem)}">not valid text</span>` : '',
        ];
        const aria = `aria-selected="${selected !== null && key(path) === selected}"${folded ? ` aria-expanded="${isOpen}"` : ''}`;
        return `<div class="${classes.join(' ')}" ${indent} data-row="${index}" role="treeitem" ${aria}>${toggle}${parts.join('')}</div>`;
      })
      .join('');
  }

  private click(event: MouseEvent): void {
    const target = (event.target as HTMLElement).closest<HTMLElement>('[data-row]');
    const row = target?.dataset.row !== undefined ? this.rows[Number(target.dataset.row)] : undefined;
    if (!row) return;
    if (row.more !== undefined) {
      this.shown.set(key(row.path), (this.shown.get(key(row.path)) ?? CHUNK) + CHUNK);
      this.render();
      return;
    }
    if ((event.target as HTMLElement).closest('.pointer') !== null) {
      this.onSelect(row.path);
      this.onFollow(row.path);
      return;
    }
    const onToggle = (event.target as HTMLElement).closest('.toggle') !== null;
    if (onToggle && row.node.children.length > 0) {
      const id = key(row.path);
      if (this.open.has(id)) this.open.delete(id);
      else this.open.add(id);
      this.render();
      return;
    }
    this.onSelect(row.path);
  }

  private keydown(event: KeyboardEvent): void {
    // Alt with an arrow is the page's back and forward
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const index = this.rows.findIndex((row) => row.more === undefined && this.selected !== null && key(row.path) === key(this.selected));
    const row = this.rows[index];
    const move = (to: number): void => {
      const next = this.rows[Math.max(0, Math.min(this.rows.length - 1, to))];
      if (next && next.more === undefined) this.onSelect(next.path);
    };
    switch (event.key) {
      case 'ArrowDown':
        move(index < 0 ? 0 : index + 1);
        break;
      case 'ArrowUp':
        move(index < 0 ? 0 : index - 1);
        break;
      case 'ArrowRight':
        if (row && row.node.children.length > 0 && !this.open.has(key(row.path))) {
          this.open.add(key(row.path));
          this.render();
        } else move(index + 1);
        break;
      case 'ArrowLeft':
        if (row && this.open.has(key(row.path)) && row.path.length > 0) {
          this.open.delete(key(row.path));
          this.render();
        } else if (row && row.path.length > 0 && this.root) {
          this.onSelect(row.path.slice(0, -1));
        }
        break;
      default:
        return;
    }
    event.preventDefault();
  }

  /** Give the tree the focus. */
  focus(): void {
    this.element.focus();
  }

  /** The node that is selected, or null. */
  get current(): Path | null {
    return this.selected;
  }

  /** Draw the rows again (the radix of some integers changed). */
  refresh(): void {
    if (this.root) this.render();
  }

  /** The name under which the radix of the node at `path` is kept: null for the root. */
  fieldAt(path: Path): string | null {
    if (!this.root || path.length === 0) return null;
    let field = '';
    let node = this.root;
    for (const index of path) {
      const child = node.children[index];
      if (!child) return null;
      field = fieldKey(field, node, child);
      node = child;
    }
    return field;
  }

  /** The node shown at a path, for the status line. */
  nodeAt(path: Path): TraceNode | null {
    return this.root ? nodeAt(this.root, path) : null;
  }
}
