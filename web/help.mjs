// The documentation of the page, made from README.md when the page is built:
// the web IDE and the schema sections, as HTML with a table of contents.
// (The README is our own, so its HTML is trusted.)
import { Marked } from 'marked';

/** The sections of the README that the page documents, by the exact heading, in the order of the table of contents. */
export const SECTIONS = ['Web IDE', 'Schema format', 'Kaitai Struct compatibility'];

const REPOSITORY = 'https://github.com/shroomsword/doe';

const escape = (text) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** The anchor that GitHub gives a heading (the README's own links use it). */
function slug(text) {
  return text
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s_-]/gu, '')
    .trim()
    .replace(/\s/g, '-');
}

/** The text of a heading as it is read: without the Markdown marks of code and emphasis. */
const plainText = (heading) => heading.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/[`*_]/g, '');

/** The headings (`#` to `######`, outside code) of a Markdown text: [{ level, text }]. */
function headingsOf(markdown) {
  const found = [];
  let fence = false;
  for (const line of markdown.split(/\r?\n/)) {
    if (/^(```|~~~)/.test(line)) fence = !fence;
    const heading = !fence && /^(#{1,6}) (.+?)\s*#*\s*$/.exec(line);
    if (heading) found.push({ level: heading[1].length, text: plainText(heading[2]) });
  }
  return found;
}

/** The part of the README under `## title`, up to the next `## `, with its heading line. Null when there is none. */
function section(markdown, title) {
  const lines = markdown.split(/\r?\n/);
  let fence = false;
  let start = -1;
  for (let i = 0; i < lines.length; i += 1) {
    if (/^(```|~~~)/.test(lines[i])) fence = !fence;
    if (fence || !/^## /.test(lines[i])) continue;
    if (start >= 0) return lines.slice(start, i).join('\n');
    if (lines[i] === `## ${title}`) start = i;
  }
  return start >= 0 ? lines.slice(start).join('\n') : null;
}

/**
 * The help for a README: `{ html, headings }`, where `html` is a table of
 * contents and the sections, and `headings` lists the `##` and `###` headings
 * ({ level, id, text }). Throws when a section is not in the README, so that a
 * heading that was renamed is noticed by the build.
 */
export function buildHelp(readme, sections = SECTIONS) {
  const parts = sections.map((title) => {
    const text = section(readme, title);
    if (text === null) throw new Error(`help: README.md has no section "## ${title}" (the sections of the help are listed in web/help.mjs)`);
    return text;
  });
  const markdown = parts.join('\n\n');

  // the anchors of the headings, the way GitHub makes them (a repeated heading gets -1, -2…)
  const headings = [];
  const used = new Map();
  for (const { level, text } of headingsOf(markdown)) {
    const base = slug(text);
    const count = used.get(base) ?? 0;
    used.set(base, count + 1);
    headings.push({ level, id: count === 0 ? base : `${base}-${count}`, text });
  }
  const included = new Set(headings.map((heading) => heading.id));

  let next = 0;
  const marked = new Marked({
    renderer: {
      heading({ tokens, depth }) {
        const heading = headings[next];
        next += 1;
        return `<h${depth} id="help-${heading?.id ?? ''}">${this.parser.parseInline(tokens)}</h${depth}>\n`;
      },
      link({ href, title, tokens }) {
        const text = this.parser.parseInline(tokens);
        const hint = title ? ` title="${escape(title)}"` : '';
        if (href.startsWith('#')) {
          const target = href.slice(1);
          // a section that is not in the help is shown as text, not as a link to nowhere
          return included.has(target) ? `<a href="#help-${escape(target)}" data-help="${escape(target)}"${hint}>${text}</a>` : `<span class="help-gone">${text}</span>`;
        }
        // another site, or a file of the repository: in a new tab
        const url = /^[a-z][a-z0-9+.-]*:/i.test(href) ? href : `${REPOSITORY}/blob/main/${href.replace(/^\.?\//, '')}`;
        return `<a href="${escape(url)}"${hint} target="_blank" rel="noopener noreferrer">${text}</a>`;
      },
    },
  });
  const body = marked
    .parse(markdown, { async: false })
    .replace(/<table>/g, '<div class="help-table"><table>')
    .replace(/<\/table>/g, '</table></div>');

  const toc = headings
    .filter((heading) => heading.level <= 3)
    .map((heading) => `<li class="level-${heading.level}"><a href="#help-${escape(heading.id)}" data-help="${escape(heading.id)}">${escape(heading.text)}</a></li>`)
    .join('\n');
  return {
    html: `<nav class="help-toc" aria-label="Contents"><ul>\n${toc}\n</ul></nav>\n<article class="help-text">\n${body}</article>\n`,
    headings: headings.filter((heading) => heading.level <= 3),
  };
}
