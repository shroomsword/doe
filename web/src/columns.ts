// How many columns the rows of the Parsed pane flow in. A setting of the view, kept in the browser's local storage
// (like the radix and the layout): the default is one column, which is the tree as it always was. No DOM here.

export const MIN_COLUMNS = 1;
export const MAX_COLUMNS = 8;
export const COLUMNS_KEY = 'doe.tree-columns';

/** The width a column keeps at least (CSS `column-width`), in `rem`: a pane that is too narrow for the number gets fewer. */
export const COLUMN_WIDTH_REM = 20;

/** The number in a text (from the dialog's box or from the storage): a whole number from 1 to the largest, else null. */
export function parseColumns(text: string | null | undefined): number | null {
  if (text === null || text === undefined) return null;
  const trimmed = text.trim();
  if (!/^\d{1,3}$/.test(trimmed)) return null;
  const value = Number(trimmed);
  return value >= MIN_COLUMNS && value <= MAX_COLUMNS ? value : null;
}

/** Why a text is not a number of columns, for the dialog. */
export function columnsError(text: string): string | null {
  if (text.trim() === '') return 'enter a number of columns';
  if (!/^\d+$/.test(text.trim())) return 'a whole number, please';
  return parseColumns(text) === null ? `from ${MIN_COLUMNS} to ${MAX_COLUMNS} columns` : null;
}

/** The saved number, or one column. Storage can be missing, blocked or hold junk: then one column. */
export function loadColumns(): number {
  try {
    return parseColumns(window.localStorage.getItem(COLUMNS_KEY)) ?? MIN_COLUMNS;
  } catch {
    return MIN_COLUMNS;
  }
}

export function saveColumns(columns: number): void {
  try {
    if (columns === MIN_COLUMNS) window.localStorage.removeItem(COLUMNS_KEY);
    else window.localStorage.setItem(COLUMNS_KEY, String(columns));
  } catch {
    // no storage: the number lasts until the page is closed
  }
}

/**
 * How many columns a pane `width` pixels wide shows of the `wanted` number: the columns keep `COLUMN_WIDTH_REM` each
 * (with `gap` pixels between), so a narrow pane shows fewer. `rem` is the size of a rem in pixels.
 */
export function shownColumns(wanted: number, width: number, rem: number, gap: number): number {
  const each = COLUMN_WIDTH_REM * rem;
  return Math.max(1, Math.min(wanted, Math.floor((width + gap) / (each + gap))));
}
