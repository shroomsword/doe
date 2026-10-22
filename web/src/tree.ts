// The parsed value as a tree: each field with its type, its value and the bytes
// it was read from. Only the rows that are in view of the tree are in the page
// (the children of a node that is closed are not made), and a node with very
// many children shows a first part of them and a row that shows more.

import type { TraceNode } from './doe.js';
import { type Path, describeRange, nodeAt } from './model.js';

/** How many children of a node are shown at first, and added by each click on "more". */
const CHUNK = 200;

interface Row {
  path: Path;
  node: TraceNode;
  depth: number;
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

  constructor(
    parent: HTMLElement,
    private readonly onSelect: (path: Path) => void,
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
  setTrace(root: TraceNode | null): void {
    const previous = this.root;
    this.root = root;
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
    const walk = (node: TraceNode, path: Path, depth: number): void => {
      rows.push({ path, node, depth });
      if (!this.open.has(key(path))) return;
      const limit = this.shown.get(key(path)) ?? CHUNK;
      node.children.slice(0, limit).forEach((child, index) => walk(child, [...path, index], depth + 1));
      if (node.children.length > limit) rows.push({ path, node, depth: depth + 1, more: node.children.length - limit });
    };
    walk(root, [], 0);
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
        const classes = ['tree-row'];
        if (selected !== null && key(path) === selected) classes.push('selected');
        if (node.instance) classes.push('instance');
        if (node.failed) classes.push('failed');
        const folded = node.children.length > 0;
        const isOpen = this.open.has(key(path));
        const toggle = folded ? `<span class="toggle">${isOpen ? '▾' : '▸'}</span>` : '<span class="toggle"></span>';
        const range = describeRange(node);
        const parts = [
          `<span class="name">${escape(node.name)}</span>`,
          node.type ? `<span class="type">${escape(node.type)}</span>` : '',
          node.summary !== null ? `<span class="summary">${escape(node.summary)}</span>` : '',
          range ? `<span class="range">${escape(range)}</span>` : node.instance ? '<span class="range">computed</span>' : '',
          node.failed ? '<span class="flag">failed here</span>' : '',
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

  /** The node shown at a path, for the status line. */
  nodeAt(path: Path): TraceNode | null {
    return this.root ? nodeAt(this.root, path) : null;
  }
}
