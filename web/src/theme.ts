// Colour themes: the built-in ones (themes.generated.ts, made from the one table
// that the terminal viewer reads too), a theme file that a person loads, and
// what is chosen, which the browser remembers. A theme is applied as CSS custom
// properties on the page, and tells the editor to change its own colours.

import { THEMES, type ThemeColors, type ThemeInfo } from './themes.generated.js';

export type { ThemeColors, ThemeInfo };
export { THEMES };

/** "Follow the system": the theme is doe Light or doe Dark by the setting of the system. */
export const SYSTEM = 'system';
/** The theme file that the person loaded. */
export const CUSTOM = 'custom';

const CHOICE_KEY = 'doe.theme';
const CUSTOM_KEY = 'doe.theme.custom';

/** The names of the colours, in the order of the table. */
export const COLOR_NAMES = [
  'background',
  'foreground',
  'muted',
  'line',
  'panel',
  'accent',
  'selection',
  'selection_part',
  'error',
  'warning',
  'number',
  'string',
  'bytes',
  'enum',
  'boolean',
  'offset',
  'type',
  'instance',
] as const satisfies readonly (keyof ThemeColors)[];

/** The CSS custom property of each colour. */
const PROPERTIES: Record<keyof ThemeColors, string> = {
  background: '--bg',
  foreground: '--fg',
  muted: '--muted',
  line: '--line',
  panel: '--panel',
  accent: '--accent',
  selection: '--select',
  selection_part: '--select-part',
  error: '--bad',
  warning: '--warn',
  number: '--number',
  string: '--string',
  bytes: '--bytes',
  enum: '--enum',
  boolean: '--boolean',
  offset: '--offset',
  type: '--type',
  instance: '--instance',
};

const HEX = /^#[0-9a-fA-F]{6}$/;

/** Read a string in a theme file: between double quotes (with the escapes `\"` and `\\`) or single quotes. */
function readString(text: string, line: number): string {
  const quote = text[0];
  if (quote === "'") {
    const end = text.indexOf("'", 1);
    if (end < 0) throw new Error(`line ${line}: the string is not closed`);
    return text.slice(1, end);
  }
  let out = '';
  for (let i = 1; i < text.length; i += 1) {
    const c = text[i];
    if (c === '\\') {
      const next = text[i + 1];
      if (next === '"' || next === '\\') out += next;
      else throw new Error(`line ${line}: \\${next ?? ''} is not understood in a string`);
      i += 1;
    } else if (c === '"') {
      const rest = text.slice(i + 1).trim();
      if (rest !== '' && !rest.startsWith('#')) throw new Error(`line ${line}: there is text after the string`);
      return out;
    } else out += c;
  }
  throw new Error(`line ${line}: the string is not closed`);
}

/**
 * Read a theme file: the TOML of the terminal viewer's themes, which is
 * `name`, optionally `variant` (`dark`, the default, or `light`), `author`,
 * `license`, `source` and `note` (text in quotes), and a `[colors]` table with
 * every colour of the table as "#rrggbb". Throws an Error that says what is wrong.
 */
export function parseThemeFile(text: string): ThemeInfo {
  const top: Record<string, string> = {};
  const colors: Record<string, string> = {};
  let into = top;
  let inColors = false;
  text.split(/\r?\n/).forEach((raw, index) => {
    const line = index + 1;
    const trimmed = raw.trim();
    if (trimmed === '' || trimmed.startsWith('#')) return;
    if (trimmed.startsWith('[')) {
      if (trimmed.replace(/\s*#.*$/, '') !== '[colors]') throw new Error(`line ${line}: only [colors] is a table of a theme`);
      if (inColors) throw new Error(`line ${line}: [colors] twice`);
      inColors = true;
      into = colors;
      return;
    }
    const equals = trimmed.indexOf('=');
    if (equals < 0) throw new Error(`line ${line}: expected key = "value"`);
    const key = trimmed.slice(0, equals).trim();
    const value = trimmed.slice(equals + 1).trim();
    if (!/^[A-Za-z_][A-Za-z0-9_-]*$/.test(key)) throw new Error(`line ${line}: ${key} is not a key`);
    if (value[0] !== '"' && value[0] !== "'") throw new Error(`line ${line}: the value of ${key} is text in quotes`);
    if (key in into) throw new Error(`line ${line}: ${key} twice`);
    into[key] = readString(value, line);
  });

  for (const key of Object.keys(top)) {
    if (!['name', 'variant', 'author', 'license', 'source', 'note', 'id'].includes(key)) throw new Error(`${key} is not a key of a theme`);
  }
  const name = top.name;
  if (!name) throw new Error('a theme needs a name');
  const variant = top.variant ?? 'dark';
  if (variant !== 'dark' && variant !== 'light') throw new Error(`variant is "dark" or "light", not "${variant}"`);
  for (const key of Object.keys(colors)) {
    if (!(COLOR_NAMES as readonly string[]).includes(key)) throw new Error(`${key} is not a colour of a theme`);
  }
  const found: Partial<ThemeColors> = {};
  for (const colorName of COLOR_NAMES) {
    const value = colors[colorName];
    if (value === undefined) throw new Error(`the colour ${colorName} is missing from [colors]`);
    if (!HEX.test(value)) throw new Error(`${colorName} = "${value}" is not a colour: write it as "#rrggbb"`);
    found[colorName] = value.toLowerCase();
  }
  return {
    id: CUSTOM,
    name,
    variant,
    author: top.author ?? '',
    license: top.license ?? '',
    source: top.source ?? '',
    ...(top.note ? { note: top.note } : {}),
    colors: found as ThemeColors,
  };
}

function lookup(id: string): ThemeInfo | undefined {
  return THEMES.find((theme) => theme.id === id);
}

/** The theme for "follow the system". */
export function systemTheme(dark: boolean): ThemeInfo {
  const theme = lookup(dark ? 'doe-dark' : 'doe-light');
  if (!theme) throw new Error('the built-in themes have no doe Light and doe Dark');
  return theme;
}

/** Set the colours of the page to a theme's. */
export function applyTheme(theme: ThemeInfo, root: HTMLElement = document.documentElement): void {
  for (const name of COLOR_NAMES) root.style.setProperty(PROPERTIES[name], theme.colors[name]);
  // form controls, scrollbars and the like follow the variant
  root.style.setProperty('color-scheme', theme.variant);
  root.dataset.theme = theme.id;
}

/** The storage of the browser, or nothing where it is not allowed. */
function storage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

function remembered(key: string): string | null {
  try {
    return storage()?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

function remember(key: string, value: string | null): void {
  try {
    if (value === null) storage()?.removeItem(key);
    else storage()?.setItem(key, value);
  } catch {
    // the choice lasts as long as the page, then
  }
}

/** What is chosen and what is shown, kept in step with the system's setting and the browser's storage. */
export class Themes {
  private choice = SYSTEM;
  private custom: ThemeInfo | null = null;
  private readonly query = window.matchMedia('(prefers-color-scheme: dark)');

  constructor(private readonly onApply: (theme: ThemeInfo) => void) {
    const text = remembered(CUSTOM_KEY);
    if (text !== null) {
      try {
        this.custom = parseThemeFile(text);
      } catch {
        remember(CUSTOM_KEY, null);
      }
    }
    const saved = remembered(CHOICE_KEY);
    if (saved !== null && (saved === SYSTEM || (saved === CUSTOM && this.custom) || lookup(saved))) this.choice = saved;
    this.query.addEventListener('change', () => {
      if (this.choice === SYSTEM) this.apply();
    });
  }

  /** What is chosen: `system`, `custom` or the id of a built-in theme. */
  get chosen(): string {
    return this.choice;
  }

  /** The theme that was loaded from a file, if there is one. */
  get loaded(): ThemeInfo | null {
    return this.custom;
  }

  /** The theme that is shown now. */
  get current(): ThemeInfo {
    if (this.choice === CUSTOM && this.custom) return this.custom;
    return lookup(this.choice) ?? systemTheme(this.query.matches);
  }

  /** Choose a theme and show it. */
  choose(choice: string): void {
    if (choice === CUSTOM ? !this.custom : choice !== SYSTEM && !lookup(choice)) return;
    this.choice = choice;
    remember(CHOICE_KEY, choice);
    this.apply();
  }

  /** Read a theme file, keep it in the browser, and show it. Throws what is wrong with it. */
  load(text: string): ThemeInfo {
    const theme = parseThemeFile(text);
    this.custom = theme;
    remember(CUSTOM_KEY, text);
    this.choose(CUSTOM);
    return theme;
  }

  /** Show the theme that is chosen. */
  apply(): void {
    const theme = this.current;
    applyTheme(theme);
    this.onApply(theme);
  }
}
