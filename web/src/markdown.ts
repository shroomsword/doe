// Markdown of an entry of an archive, rendered. The text is from an untrusted archive, so the page is only given DOM nodes
// made here with `createElement` and `textContent`, never HTML from the text: raw HTML in the Markdown is shown as text,
// a link is text with its target written out and no `href`, an image is its alt text and no `src` (nothing is fetched),
// and the nesting, the number of nodes and the cells of tables are limited so that a crafted file cannot stall the page.
//
// The parser is the Lezer Markdown one that the editor's Markdown language already loads (CommonMark with the GitHub
// extensions: tables, task lists, strikethrough, autolinks), so there is no second Markdown dependency.

import type { SyntaxNode } from '@lezer/common';
import { GFM, parser } from '@lezer/markdown';

const markdown = parser.configure(GFM);

/** The deepest nesting of blocks and inline spans that is built; what is deeper is shown as the text it is. */
export const MAX_DEPTH = 24;
/** The most DOM nodes built; past it the rendering stops and says so. */
export const MAX_NODES = 20_000;
/** The most cells of tables. */
export const MAX_CELLS = 4_000;

export interface RenderOptions {
  /** Colour the code of a block whose fence names a language (the id of `languages.ts`); the text is already in `target`. */
  highlight?: (target: HTMLElement, code: string, language: string) => void;
  /** Tell the id of a language from the first word of a fence. */
  languageOf?: (info: string) => string | null;
}

export interface Rendered {
  /** Whether the file was too large to render all of it. */
  truncated: boolean;
}

class Stop extends Error {}

/** Build the rendering of `text` into `target` (its children are replaced). */
export function renderMarkdown(text: string, target: HTMLElement, options: RenderOptions = {}): Rendered {
  target.replaceChildren();
  const tree = markdown.parse(text);
  let nodes = 0;
  let cells = 0;
  const make = <K extends keyof HTMLElementTagNameMap>(parent: Node, tag: K, className?: string): HTMLElementTagNameMap[K] => {
    nodes += 1;
    if (nodes > MAX_NODES) throw new Stop();
    const element = document.createElement(tag);
    if (className) element.className = className;
    parent.appendChild(element);
    return element;
  };
  const put = (parent: Node, value: string): void => {
    if (value === '') return;
    nodes += 1;
    if (nodes > MAX_NODES) throw new Stop();
    parent.appendChild(document.createTextNode(value));
  };

  // ── inline ────────────────────────────────────────────────────────────────

  /** Marks that are syntax and not text. */
  const MARKS = new Set(['EmphasisMark', 'CodeMark', 'StrikethroughMark', 'HeaderMark', 'TaskMarker', 'QuoteMark', 'ListMark', 'LinkMark', 'SuperscriptMark', 'SubscriptMark']);

  /** The text of a source slice, with a new line as a space (a paragraph is one line to the reader). */
  const flat = (from: number, to: number): string => text.slice(from, to).replace(/\s*\n\s*/g, ' ');

  /** The inline content of `node` between `from` and `to`, into `parent`. */
  const inline = (node: SyntaxNode, parent: Node, from: number, to: number, depth: number): void => {
    let at = from;
    for (let child = node.firstChild; child; child = child.nextSibling) {
      if (child.to <= from || child.from >= to) continue;
      if (child.from > at) put(parent, flat(at, child.from));
      at = Math.max(at, child.to);
      if (MARKS.has(child.name) && child.name !== 'LinkMark') continue;
      if (depth > MAX_DEPTH) {
        put(parent, flat(child.from, child.to));
        continue;
      }
      span(child, parent, depth + 1);
    }
    if (at < to) put(parent, flat(at, to));
  };

  const decode = (entity: string): string => {
    const named: Record<string, string> = { '&amp;': '&', '&lt;': '<', '&gt;': '>', '&quot;': '"', '&apos;': "'", '&nbsp;': '\u00a0' };
    const found = named[entity];
    if (found) return found;
    const number = /^&#(x[0-9a-f]+|\d+);$/i.exec(entity)?.[1];
    if (number) {
      const code = number.toLowerCase().startsWith('x') ? Number.parseInt(number.slice(1), 16) : Number.parseInt(number, 10);
      if (code > 0 && code <= 0x10ffff && !(code >= 0xd800 && code <= 0xdfff)) return String.fromCodePoint(code);
    }
    return entity;
  };

  const span = (node: SyntaxNode, parent: Node, depth: number): void => {
    switch (node.name) {
      case 'Emphasis':
        inline(node, make(parent, 'em'), node.from, node.to, depth);
        break;
      case 'StrongEmphasis':
        inline(node, make(parent, 'strong'), node.from, node.to, depth);
        break;
      case 'Strikethrough':
        inline(node, make(parent, 'del'), node.from, node.to, depth);
        break;
      case 'InlineCode': {
        const marks = node.getChildren('CodeMark');
        const from = marks[0]?.to ?? node.from;
        const to = marks[marks.length - 1]?.from ?? node.to;
        put(make(parent, 'code', 'md-code'), text.slice(from, to).replace(/\n/g, ' ').trim());
        break;
      }
      case 'Link':
      case 'Image': {
        const marks = node.getChildren('LinkMark');
        const url = node.getChild('URL');
        const target = url ? text.slice(url.from, url.to) : '';
        const open = marks[0];
        const close = marks.find((m, i) => i > 0 && text[m.from] === ']');
        const from = open ? open.to : node.from;
        const to = close ? close.from : node.to;
        if (node.name === 'Image') {
          // the alt text, and where it points to in the tooltip: nothing is loaded
          const holder = make(parent, 'span', 'md-image');
          holder.title = target;
          put(holder, `[image: ${flat(from, to).trim() || target}]`);
        } else {
          const link = make(parent, 'span', 'md-link');
          link.title = target;
          inline(node, link, from, to, depth);
          if (target) put(make(parent, 'span', 'md-url'), ` <${target}>`);
        }
        break;
      }
      case 'Autolink': {
        const url = node.getChild('URL');
        const target = url ? text.slice(url.from, url.to) : text.slice(node.from, node.to);
        const link = make(parent, 'span', 'md-link');
        link.title = target;
        put(link, target);
        break;
      }
      case 'URL':
        put(make(parent, 'span', 'md-link'), text.slice(node.from, node.to));
        break;
      case 'HardBreak':
        make(parent, 'br');
        break;
      case 'Escape':
        put(parent, text.slice(node.from + 1, node.to));
        break;
      case 'Entity':
        put(parent, decode(text.slice(node.from, node.to)));
        break;
      case 'HTMLTag':
      case 'Comment':
      case 'ProcessingInstruction':
        // raw HTML is text
        put(make(parent, 'span', 'md-html'), text.slice(node.from, node.to));
        break;
      default:
        // a node that is not known is the text it covers (and its children are not looked at)
        put(parent, flat(node.from, node.to));
    }
  };

  // ── blocks ────────────────────────────────────────────────────────────────

  /** The text of a code block's lines without the indent that makes it a block. */
  const dedent = (value: string): string => value.replace(/^( {1,4}|\t)/gm, '');

  const code = (node: SyntaxNode, parent: Node): void => {
    const pre = make(parent, 'pre', 'md-pre');
    const element = make(pre, 'code');
    let body = '';
    let info = '';
    if (node.name === 'FencedCode') {
      const infoNode = node.getChild('CodeInfo');
      info = infoNode ? text.slice(infoNode.from, infoNode.to).trim() : '';
      body = node
        .getChildren('CodeText')
        .map((c) => text.slice(c.from, c.to))
        .join('\n');
    } else {
      body = dedent(
        node
          .getChildren('CodeText')
          .map((c) => text.slice(c.from, c.to))
          .join('\n'),
      );
    }
    element.textContent = body;
    const language = info ? (options.languageOf?.(info.split(/\s+/)[0] ?? '') ?? null) : null;
    if (language && options.highlight) options.highlight(element, body, language);
  };

  const table = (node: SyntaxNode, parent: Node, depth: number): void => {
    const holder = make(parent, 'table', 'md-table');
    for (let row = node.firstChild; row; row = row.nextSibling) {
      if (row.name !== 'TableHeader' && row.name !== 'TableRow') continue;
      const tr = make(holder, 'tr');
      for (let cell = row.firstChild; cell; cell = cell.nextSibling) {
        if (cell.name !== 'TableCell') continue;
        cells += 1;
        if (cells > MAX_CELLS) throw new Stop();
        inline(cell, make(tr, row.name === 'TableHeader' ? 'th' : 'td'), cell.from, cell.to, depth);
      }
    }
  };

  const list = (node: SyntaxNode, parent: Node, depth: number): void => {
    const ordered = node.name === 'OrderedList';
    const holder = ordered ? make(parent, 'ol') : make(parent, 'ul');
    if (holder instanceof HTMLOListElement) {
      const first = node.firstChild?.getChild('ListMark');
      const start = first ? Number.parseInt(text.slice(first.from, first.to), 10) : 1;
      if (Number.isFinite(start) && start !== 1) holder.start = start;
    }
    for (let item = node.firstChild; item; item = item.nextSibling) {
      if (item.name !== 'ListItem') continue;
      const li = make(holder, 'li');
      for (let child = item.firstChild; child; child = child.nextSibling) {
        if (child.name === 'ListMark') continue;
        if (child.name === 'Task') {
          const marker = child.getChild('TaskMarker');
          const done = marker ? /x/i.test(text.slice(marker.from, marker.to)) : false;
          const box = make(li, 'span', 'md-task');
          put(box, done ? '☑ ' : '☐ ');
          inline(child, li, marker ? marker.to : child.from, child.to, depth);
        } else block(child, li, depth + 1);
      }
    }
  };

  const block = (node: SyntaxNode, parent: Node, depth: number): void => {
    if (depth > MAX_DEPTH) {
      // too deep to build: the text it covers
      put(make(parent, 'pre', 'md-pre'), text.slice(node.from, node.to));
      return;
    }
    const name = node.name;
    const heading = /^(?:ATX|Setext)Heading([1-6])$/.exec(name);
    if (heading) {
      const level = Number(heading[1]);
      const tag = (['h1', 'h2', 'h3', 'h4', 'h5', 'h6'] as const)[level - 1] ?? 'h6';
      const element = make(parent, tag);
      // a Setext heading's underline is a mark of the heading; its text is up to the first line end
      const end = name.startsWith('Setext') ? (text.indexOf('\n', node.from) < 0 ? node.to : Math.min(node.to, text.indexOf('\n', node.from))) : node.to;
      inline(node, element, node.from, end, depth);
      return;
    }
    switch (name) {
      case 'Paragraph':
        inline(node, make(parent, 'p'), node.from, node.to, depth);
        break;
      case 'BulletList':
      case 'OrderedList':
        list(node, parent, depth);
        break;
      case 'Blockquote': {
        const quote = make(parent, 'blockquote');
        for (let child = node.firstChild; child; child = child.nextSibling) if (child.name !== 'QuoteMark') block(child, quote, depth + 1);
        break;
      }
      case 'FencedCode':
      case 'CodeBlock':
        code(node, parent);
        break;
      case 'HorizontalRule':
        make(parent, 'hr');
        break;
      case 'Table':
        table(node, parent, depth);
        break;
      case 'LinkReference':
        // [label]: target — the definition is shown as the text it is
        put(make(parent, 'p', 'md-ref'), flat(node.from, node.to));
        break;
      default:
        // raw HTML blocks, comments and what is not known: the source, as text
        put(make(parent, 'pre', 'md-pre md-html'), text.slice(node.from, node.to));
    }
  };

  const root = tree.topNode;
  let truncated = false;
  try {
    for (let child = root.firstChild; child; child = child.nextSibling) block(child, target, 0);
  } catch (error) {
    if (!(error instanceof Stop)) throw error;
    truncated = true;
    const note = document.createElement('p');
    note.className = 'md-note';
    note.textContent = '… the rest is not shown: this file is too large to render (see the source)';
    target.appendChild(note);
  }
  return { truncated };
}
