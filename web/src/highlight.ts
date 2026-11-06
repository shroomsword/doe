// The colours of the YAML of a schema, from a theme: one style for the editor and for the viewer of the types, so that
// a schema reads the same in both. The viewer draws the text once with the parser of the editor (no editor state).

import { yaml } from '@codemirror/lang-yaml';
import { HighlightStyle } from '@codemirror/language';
import type { Parser } from '@lezer/common';
import { highlightTree, tags } from '@lezer/highlight';
import type { ThemeInfo } from './theme.js';

/** The style of the syntax tokens in the colours of a theme. */
export function highlightStyle(theme: ThemeInfo): HighlightStyle {
  const c = theme.colors;
  return HighlightStyle.define([
    { tag: [tags.comment, tags.lineComment], color: c.muted, fontStyle: 'italic' },
    { tag: [tags.propertyName, tags.definition(tags.propertyName)], color: c.type },
    { tag: [tags.string, tags.special(tags.string)], color: c.string },
    { tag: [tags.number, tags.integer, tags.float], color: c.number },
    { tag: [tags.bool, tags.null], color: c.boolean },
    { tag: [tags.keyword, tags.atom], color: c.enum },
    { tag: [tags.meta, tags.labelName, tags.typeName, tags.tagName, tags.className], color: c.accent },
    { tag: [tags.punctuation, tags.separator, tags.operator, tags.bracket], color: c.muted },
    { tag: tags.invalid, color: c.error },
    // what the languages of the preview of a file add: names of functions, attributes, regular expressions, markup
    { tag: [tags.function(tags.variableName), tags.function(tags.propertyName), tags.macroName], color: c.instance },
    { tag: [tags.attributeName, tags.namespace, tags.escape, tags.regexp], color: c.bytes },
    { tag: [tags.heading, tags.strong], color: c.accent, fontWeight: 'bold' },
    { tag: tags.emphasis, fontStyle: 'italic' },
    { tag: [tags.link, tags.url], color: c.offset, textDecoration: 'underline' },
    { tag: [tags.inserted], color: c.string },
    { tag: [tags.deleted], color: c.error },
    { tag: tags.blockComment, color: c.muted, fontStyle: 'italic' },
  ]);
}

/** The longest text that is highlighted: a longer one is shown plain (the parse is linear, but the page should not stall). */
export const HIGHLIGHT_LIMIT = 400_000;

/** The rules of a style are in a `<style>` of the page, kept for the style in use; the classes in the text are from it. */
function mount(style: HighlightStyle): void {
  let sheet = document.getElementById('doe-highlight');
  if (!sheet) {
    sheet = document.createElement('style');
    sheet.id = 'doe-highlight';
    document.head.append(sheet);
  }
  sheet.textContent = style.module?.getRules() ?? '';
}

/**
 * Show `text` in `target` with the colours of `style`: text nodes and `<span class>` made with DOM calls, never HTML
 * from the text, so that the text of the element is the text, unchanged. A text that is too long, or that does not parse,
 * is shown plain (whatever was highlighted before the parser stopped stays). `parser` is that of the language of the text
 * (YAML, the language of a schema, when it is not given).
 */
export function drawHighlighted(target: HTMLElement, text: string, style: HighlightStyle, parser?: Parser): void {
  target.replaceChildren();
  if (text.length > HIGHLIGHT_LIMIT) {
    target.append(document.createTextNode(text));
    return;
  }
  mount(style);
  const parts: Node[] = [];
  let at = 0;
  const plain = (to: number): void => {
    if (to > at) parts.push(document.createTextNode(text.slice(at, to)));
    at = Math.max(at, to);
  };
  try {
    const tree = (parser ?? yaml().language.parser).parse(text);
    highlightTree(tree, style, (from, to, classes) => {
      if (from < at) return;
      plain(from);
      const span = document.createElement('span');
      span.className = classes;
      span.textContent = text.slice(from, to);
      parts.push(span);
      at = to;
    });
  } catch {
    // what was drawn stays; the rest is plain
  }
  plain(text.length);
  target.append(...parts);
}
