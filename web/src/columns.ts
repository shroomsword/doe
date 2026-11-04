// How many columns the Bytes pane has: the bytes in a row of the hex view. A setting of the view, kept in the browser's
// local storage (like the radix and the layout); the default is 16, which is what the view always had. No DOM here.

export const DEFAULT_COLUMNS = 16;
export const MIN_COLUMNS = 1;
export const MAX_COLUMNS = 64;
export const COLUMNS_KEY = 'doe.bytes-columns';

/** The number in a text (from the dialog's box or from the storage): a whole number from 1 to the largest, else null. */
export function parseColumns(text: string | null | undefined): number | null {
  if (text === null || text === undefined) return null;
  const trimmed = text.trim();
  if (!/^\d{1,4}$/.test(trimmed)) return null;
  const value = Number(trimmed);
  return value >= MIN_COLUMNS && value <= MAX_COLUMNS ? value : null;
}

/** Why a text is not a number of columns, for the dialog. */
export function columnsError(text: string): string | null {
  if (text.trim() === '') return 'enter a number of columns';
  if (!/^\d+$/.test(text.trim())) return 'a whole number, please';
  return parseColumns(text) === null ? `from ${MIN_COLUMNS} to ${MAX_COLUMNS} columns` : null;
}

/** The saved number, or the default. Storage can be missing, blocked or hold junk: then the default. */
export function loadColumns(): number {
  try {
    return parseColumns(window.localStorage.getItem(COLUMNS_KEY)) ?? DEFAULT_COLUMNS;
  } catch {
    return DEFAULT_COLUMNS;
  }
}

export function saveColumns(columns: number): void {
  try {
    if (columns === DEFAULT_COLUMNS) window.localStorage.removeItem(COLUMNS_KEY);
    else window.localStorage.setItem(COLUMNS_KEY, String(columns));
  } catch {
    // no storage: the number lasts until the page is closed
  }
}
