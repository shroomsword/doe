// The keys of the page: one table that both acts on a key and draws the list of
// them (the dialog of `k`), so that the list cannot get out of step with what
// the keys do. No DOM here, except where a function takes the target of an event.

/** What a key needs before it does anything: a file to look at, a parsed value, or a parse that is running. */
export type Needs = 'file' | 'parsed' | 'running';

export type Action =
  | 'help'
  | 'keys'
  | 'open-file'
  | 'open-schema'
  | 'browse'
  | 'imports'
  | 'detect'
  | 'save'
  | 'next-pane'
  | 'previous-pane'
  | 'search'
  | 'goto'
  | 'auto'
  | 'radix'
  | 'cancel'
  | 'pane-schema'
  | 'pane-parsed'
  | 'pane-bytes'
  | 'pane-status';

export interface Key {
  /** The name of the key as the page reads it: the character, or `Tab`, `Shift+Tab`, `F6`. */
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
  { key: 'k', action: 'keys', label: 'Show these keys' },
  { key: 'o', action: 'open-file', label: 'Open a file to parse' },
  { key: 'O', action: 'open-schema', label: 'Open a schema file' },
  { key: 't', action: 'browse', label: 'Browse the types and schemas, and load one' },
  { key: 'i', action: 'imports', label: 'Add imports' },
  { key: 'd', action: 'detect', label: 'Detect the schema of the open file', needs: 'file' },
  { key: 's', action: 'save', label: 'Save the parsed value', needs: 'parsed' },
  { key: 'Tab', action: 'next-pane', label: 'Next pane (from a pane, not from a button or a field)' },
  { key: 'Shift+Tab', action: 'previous-pane', label: 'Previous pane' },
  { key: 'F6', action: 'next-pane', label: 'Next pane, from anywhere', inFields: true },
  { key: 'Shift+F6', action: 'previous-pane', label: 'Previous pane, from anywhere', inFields: true },
  { key: '/', action: 'search', label: 'Search in the focused pane' },
  { key: 'g', action: 'goto', label: 'Go to an offset in the bytes', needs: 'file' },
  { key: 'a', action: 'auto', label: 'Turn automatic detection on or off' },
  { key: 'r', action: 'radix', label: 'Change how integers are shown (decimal, hexadecimal, binary, octal)' },
  { key: 'c', action: 'cancel', label: 'Cancel the parse that is running', needs: 'running' },
  { key: '1', action: 'pane-schema', label: 'Focus the Schema pane' },
  { key: '2', action: 'pane-parsed', label: 'Focus the Parsed pane' },
  { key: '3', action: 'pane-bytes', label: 'Focus the Bytes pane' },
  { key: '4', action: 'pane-status', label: 'Focus the Status pane' },
];

/** The name of the key of an event, as the table has it: a character as it is typed, other keys with `Shift+` when Shift is held. */
export function keyName(event: Pick<KeyboardEvent, 'key' | 'shiftKey'>): string {
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
