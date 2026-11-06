// The keys of the page: one table that both acts on a key and draws the list of
// them (the dialog of `K`), so that the list cannot get out of step with what
// the keys do. No DOM here, except where a function takes the target of an event.

/** What a key needs before it does anything: a file to look at, a parsed value, or a parse that is running. */
export type Needs = 'file' | 'parsed' | 'running' | 'filesystem';

export type Action =
  | 'help'
  | 'keys'
  | 'theme'
  | 'open-file'
  | 'open-schema'
  | 'browse'
  | 'imports'
  | 'detect'
  | 'save'
  | 'files'
  | 'checksums'
  | 'next-pane'
  | 'previous-pane'
  | 'pane-left'
  | 'pane-down'
  | 'pane-up'
  | 'pane-right'
  | 'move-left'
  | 'move-down'
  | 'move-up'
  | 'move-right'
  | 'reset-layout'
  | 'close-pane'
  | 'panes'
  | 'maximize'
  | 'search'
  | 'goto'
  | 'auto'
  | 'radix'
  | 'cancel'
  | 'columns'
  | 'pane-schema'
  | 'pane-parsed'
  | 'pane-bytes'
  | 'pane-status';

export interface Key {
  /** The name of the key as the page reads it: the character, or `Tab`, `Shift+Tab`. */
  key: string;
  action: Action;
  /** What it does, for the list. */
  label: string;
  needs?: Needs;
  /** The key works where the focus is in a text field too (it is not a letter that could be typed). */
  inFields?: boolean;
}

export const KEYS: readonly Key[] = [
  { key: '?', action: 'help', label: 'Help' },
  { key: 'K', action: 'keys', label: 'Show these keys' },
  { key: 'T', action: 'theme', label: 'Choose a theme' },
  { key: 'o', action: 'open-file', label: 'Open a file to parse' },
  { key: 'O', action: 'open-schema', label: 'Open a schema file' },
  { key: 't', action: 'browse', label: 'Browse the types and schemas, and load one' },
  { key: 'i', action: 'imports', label: 'Add imports' },
  { key: 'd', action: 'detect', label: 'Detect the schema of the open file', needs: 'file' },
  { key: 's', action: 'save', label: 'Save the parsed value', needs: 'parsed' },
  { key: 'F', action: 'files', label: 'Switch the Parsed pane between the files of an archive and the tree of fields', needs: 'filesystem' },
  { key: 'm', action: 'checksums', label: 'Show or hide the MD5 of each file in the Files view', needs: 'filesystem' },
  { key: 'Tab', action: 'next-pane', label: 'Next pane (from a pane, not from a button or a field)' },
  { key: 'Shift+Tab', action: 'previous-pane', label: 'Previous pane' },
  { key: 'h', action: 'pane-left', label: 'Focus the pane on the left' },
  { key: 'j', action: 'pane-down', label: 'Focus the pane below' },
  { key: 'k', action: 'pane-up', label: 'Focus the pane above' },
  { key: 'l', action: 'pane-right', label: 'Focus the pane on the right' },
  { key: 'Alt+Shift+h', action: 'move-left', label: 'Move the pane to the left (it swaps places with that one)', inFields: true },
  { key: 'Alt+Shift+j', action: 'move-down', label: 'Move the pane down', inFields: true },
  { key: 'Alt+Shift+k', action: 'move-up', label: 'Move the pane up', inFields: true },
  { key: 'Alt+Shift+l', action: 'move-right', label: 'Move the pane to the right', inFields: true },
  { key: 'R', action: 'reset-layout', label: 'Put the panes back in their places, all shown, at their default sizes' },
  { key: 'x', action: 'close-pane', label: 'Close the focused pane (the Schema pane has its X button)' },
  { key: 'p', action: 'panes', label: 'Choose the panes that are shown' },
  { key: 'f', action: 'maximize', label: 'Maximize the focused pane, or restore it (Esc restores too)' },
  { key: '/', action: 'search', label: 'Search in the focused pane, or in the help when it is open' },
  { key: 'g', action: 'goto', label: 'Go to an offset in the bytes', needs: 'file' },
  { key: 'a', action: 'auto', label: 'Turn automatic detection on or off' },
  { key: 'r', action: 'radix', label: 'Change how integers are shown (decimal, hexadecimal, binary, octal)' },
  { key: 'c', action: 'columns', label: 'Set the number of columns of the Bytes pane' },
  { key: 'C', action: 'cancel', label: 'Cancel the parse that is running', needs: 'running' },
  { key: '1', action: 'pane-schema', label: 'Focus the Schema pane' },
  { key: '2', action: 'pane-parsed', label: 'Focus the Parsed pane (shows it if it is closed)' },
  { key: '3', action: 'pane-bytes', label: 'Focus the Bytes pane (shows it if it is closed)' },
  { key: '4', action: 'pane-status', label: 'Focus the Status pane (shows it if it is closed)' },
];

/** The name of the key of an event, as the table has it: a character as it is typed, other keys with `Shift+` when Shift is held. */
export function keyName(event: Pick<KeyboardEvent, 'key' | 'shiftKey'> & Partial<Pick<KeyboardEvent, 'altKey' | 'code'>>): string {
  // Alt+Shift and a letter: the letter that the key is (on some keyboards Alt makes `event.key` another character)
  if (event.altKey && event.shiftKey && event.code?.startsWith('Key')) return `Alt+Shift+${event.code.slice(3).toLowerCase()}`;
  return event.key.length > 1 && event.shiftKey ? `Shift+${event.key}` : event.key;
}

/** Whether the target of a key is somewhere that text is typed. */
export function isField(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  // a file chooser, a check box or a radio button takes no text: after one is used the focus can stay on it
  return target.closest('input:not([type="file"], [type="checkbox"], [type="radio"], [type="button"]), select, textarea, [contenteditable="true"], [contenteditable=""], .cm-editor') !== null;
}

/** The key (by its name) for a button's tooltip: "Open (o)". */
export function keyOf(action: Action): string | undefined {
  return KEYS.find((entry) => entry.action === action)?.key;
}

// ── search in the bytes ─────────────────────────────────────────────────────

export type ByteQuery = { kind: 'offset'; offset: number } | { kind: 'bytes'; pattern: number[] };

/**
 * What a search of the bytes means: `@1f4` or `@0x1f4` is an offset (hexadecimal; `@#500` is decimal),
 * hexadecimal digits in pairs (`de ad be ef`, `0xdeadbeef`) are bytes, and any other text is looked for as it is.
 * Null when the query is empty or is an offset that cannot be read.
 */
export function byteQuery(query: string): ByteQuery | null {
  const text = query.trim();
  if (text === '') return null;
  if (text.startsWith('@')) {
    const rest = text.slice(1).trim();
    const decimal = rest.startsWith('#');
    const digits = (decimal ? rest.slice(1) : rest.replace(/^0x/i, '')).trim();
    if (!(decimal ? /^\d+$/ : /^[0-9a-f]+$/i).test(digits)) return null;
    const offset = Number.parseInt(digits, decimal ? 10 : 16);
    return Number.isSafeInteger(offset) ? { kind: 'offset', offset } : null;
  }
  const compact = text.replace(/^0x/i, '').replace(/[\s,:-]/g, '');
  if (compact.length >= 2 && compact.length % 2 === 0 && /^[0-9a-f]+$/i.test(compact) && /^(0x)?[0-9a-f\s,:-]+$/i.test(text)) {
    const pattern: number[] = [];
    for (let i = 0; i < compact.length; i += 2) pattern.push(Number.parseInt(compact.slice(i, i + 2), 16));
    return { kind: 'bytes', pattern };
  }
  return { kind: 'bytes', pattern: [...new TextEncoder().encode(text)] };
}

/**
 * The offset of the next place that `pattern` is, going `direction` from `from` (not counting `from` itself), and
 * wrapping around; null when it is not there. A byte that has not arrived (undefined) matches nothing.
 */
export function findPattern(bytes: { length: number; byteAt(index: number): number | undefined }, pattern: readonly number[], from: number, direction: 1 | -1): number | null {
  const last = bytes.length - pattern.length;
  if (pattern.length === 0 || last < 0) return null;
  const at = (start: number): boolean => pattern.every((value, i) => bytes.byteAt(start + i) === value);
  const total = last + 1;
  for (let step = 1; step <= total; step += 1) {
    const start = (((from + direction * step) % total) + total) % total;
    if (at(start)) return start;
  }
  return null;
}
