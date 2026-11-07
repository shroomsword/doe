// The languages of the preview of a file of an archive: which one a file is in (by its name, and by a `#!` first line),
// and the parser of each, loaded when the first file of that language is shown (the build splits them into chunks of
// their own, so the page does not carry the parsers of languages that are not looked at).

import type { Parser } from '@lezer/common';

/** A language of the preview: the id is what the badge shows and the picker lists. */
export interface Lang {
  id: string;
  /** The name for the person. */
  name: string;
}

/** The languages, in the order of the picker. */
export const LANGUAGES: readonly Lang[] = [
  { id: 'c', name: 'C' },
  { id: 'cpp', name: 'C++' },
  { id: 'css', name: 'CSS' },
  { id: 'diff', name: 'Diff' },
  { id: 'dockerfile', name: 'Dockerfile' },
  { id: 'html', name: 'HTML' },
  { id: 'ini', name: 'INI' },
  { id: 'javascript', name: 'JavaScript' },
  { id: 'json', name: 'JSON' },
  { id: 'markdown', name: 'Markdown' },
  { id: 'python', name: 'Python' },
  { id: 'rust', name: 'Rust' },
  { id: 'shell', name: 'Shell' },
  { id: 'sql', name: 'SQL' },
  { id: 'toml', name: 'TOML' },
  { id: 'typescript', name: 'TypeScript' },
  { id: 'xml', name: 'XML' },
  { id: 'yaml', name: 'YAML' },
];

/** The id of the language of each extension (lower case, without the dot). */
export const EXTENSIONS: Record<string, string> = {
  c: 'c',
  h: 'c',
  cc: 'cpp',
  cpp: 'cpp',
  cxx: 'cpp',
  hh: 'cpp',
  hpp: 'cpp',
  hxx: 'cpp',
  css: 'css',
  diff: 'diff',
  patch: 'diff',
  htm: 'html',
  html: 'html',
  xhtml: 'html',
  cfg: 'ini',
  conf: 'ini',
  ini: 'ini',
  cjs: 'javascript',
  js: 'javascript',
  jsx: 'javascript',
  mjs: 'javascript',
  json: 'json',
  geojson: 'json',
  md: 'markdown',
  markdown: 'markdown',
  py: 'python',
  pyi: 'python',
  rs: 'rust',
  bash: 'shell',
  sh: 'shell',
  zsh: 'shell',
  sql: 'sql',
  toml: 'toml',
  cts: 'typescript',
  mts: 'typescript',
  ts: 'typescript',
  tsx: 'typescript',
  plist: 'xml',
  svg: 'xml',
  xml: 'xml',
  xsd: 'xml',
  xsl: 'xml',
  ksy: 'yaml',
  yaml: 'yaml',
  yml: 'yaml',
};

/** The id of the language of some whole file names (lower case). */
export const NAMES: Record<string, string> = {
  '.bashrc': 'shell',
  '.profile': 'shell',
  '.zshrc': 'shell',
  dockerfile: 'dockerfile',
  containerfile: 'dockerfile',
  gnumakefile: 'shell',
  makefile: 'shell',
  pkgbuild: 'shell',
  'cargo.lock': 'toml',
  'meson.build': 'python',
  'sconstruct': 'python',
};

/** The id of the language of the interpreter a `#!` line names (`python3`, `bash`...). */
const INTERPRETERS: [RegExp, string][] = [
  [/^(?:ba|z|da|k|a)?sh$/, 'shell'],
  [/^python[0-9.]*$/, 'python'],
  [/^(?:node|nodejs|deno|bun)$/, 'javascript'],
];

/** The language a `#!` first line names, or null: `#!/bin/sh`, `#!/usr/bin/env python3`, `#!/usr/bin/env -S node --flag`. */
export function fromShebang(text: string): string | null {
  const line = /^#![ \t]*([^\r\n]*)/.exec(text)?.[1];
  if (line === undefined) return null;
  const words = line.trim().split(/[ \t]+/);
  let program = words[0] ?? '';
  if (program.split('/').pop() === 'env') {
    // `env` takes options and NAME=value words before the program
    const next = words.slice(1).find((w) => !w.startsWith('-') && !w.includes('='));
    program = next ?? '';
  }
  const base = program.split('/').pop() ?? '';
  for (const [pattern, id] of INTERPRETERS) if (pattern.test(base)) return id;
  return null;
}

/**
 * The language of a file from its name (the last part of its path) and, when the name does not say, from a `#!` first
 * line of `text`. Null when neither tells: the file stays plain.
 */
export function detectLanguage(name: string, text: string): string | null {
  const lower = name.toLowerCase();
  const byName = NAMES[lower];
  if (byName) return byName;
  if (lower.startsWith('dockerfile.') || lower.endsWith('.dockerfile')) return 'dockerfile';
  const dot = lower.lastIndexOf('.');
  const byExtension = dot > 0 ? EXTENSIONS[lower.slice(dot + 1)] : undefined;
  if (byExtension) return byExtension;
  return fromShebang(text);
}

/** The id of the language that the first word of a Markdown code fence names (`rust`, `js`, `sh`, `yml`...), or null. */
export function languageOfFence(info: string): string | null {
  const word = info.toLowerCase().replace(/^\./, '');
  if (word === '') return null;
  if (LANGUAGES.some((l) => l.id === word)) return word;
  return EXTENSIONS[word] ?? null;
}

const parsers = new Map<string, Promise<Parser>>();

/** The parser of a language, loaded the first time it is asked for. Rejects for an id that is not a language. */
export function loadParser(id: string): Promise<Parser> {
  let found = parsers.get(id);
  if (!found) {
    found = make(id);
    parsers.set(id, found);
    // a failed load is tried again next time
    found.catch(() => parsers.delete(id));
  }
  return found;
}

async function make(id: string): Promise<Parser> {
  const legacy = async (stream: unknown): Promise<Parser> => {
    const { StreamLanguage } = await import('@codemirror/language');
    return StreamLanguage.define(stream as Parameters<typeof StreamLanguage.define>[0]).parser;
  };
  switch (id) {
    case 'c':
    case 'cpp': {
      const { cpp } = await import('@codemirror/lang-cpp');
      return cpp().language.parser;
    }
    case 'css': {
      const { css } = await import('@codemirror/lang-css');
      return css().language.parser;
    }
    case 'diff':
      return legacy((await import('@codemirror/legacy-modes/mode/diff')).diff);
    case 'dockerfile':
      return legacy((await import('@codemirror/legacy-modes/mode/dockerfile')).dockerFile);
    case 'html': {
      const { html } = await import('@codemirror/lang-html');
      return html().language.parser;
    }
    case 'ini':
      return legacy((await import('@codemirror/legacy-modes/mode/properties')).properties);
    case 'javascript': {
      const { javascript } = await import('@codemirror/lang-javascript');
      return javascript({ jsx: true }).language.parser;
    }
    case 'json': {
      const { json } = await import('@codemirror/lang-json');
      return json().language.parser;
    }
    case 'markdown': {
      const { markdown } = await import('@codemirror/lang-markdown');
      return markdown().language.parser;
    }
    case 'python': {
      const { python } = await import('@codemirror/lang-python');
      return python().language.parser;
    }
    case 'rust': {
      const { rust } = await import('@codemirror/lang-rust');
      return rust().language.parser;
    }
    case 'shell':
      return legacy((await import('@codemirror/legacy-modes/mode/shell')).shell);
    case 'sql': {
      const { StandardSQL } = await import('@codemirror/lang-sql');
      return StandardSQL.language.parser;
    }
    case 'toml':
      return legacy((await import('@codemirror/legacy-modes/mode/toml')).toml);
    case 'typescript': {
      const { javascript } = await import('@codemirror/lang-javascript');
      return javascript({ jsx: true, typescript: true }).language.parser;
    }
    case 'xml': {
      const { xml } = await import('@codemirror/lang-xml');
      return xml().language.parser;
    }
    case 'yaml': {
      const { yaml } = await import('@codemirror/lang-yaml');
      return yaml().language.parser;
    }
    default:
      throw new Error(`no language ${id}`);
  }
}
