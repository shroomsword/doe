// The schemas that are open: the main one and the files it imports, where each
// came from, whether it has errors, the imports that are not there, and the
// types any of them defines (one of which can be the type to parse with).

import type { Diagnostic, InspectResponse } from './doe.js';
import type { Origin } from './share.js';

export type { Origin };

/** What a person is told about where a schema came from. */
export const ORIGIN_LABEL: Record<Origin, string> = {
  typed: 'typed here',
  file: 'opened from a file',
  added: 'added file',
  gallery: 'Kaitai Struct gallery',
  library: 'type library of this machine',
};

/** Whether the text can be changed here: the main schema, and what the person added, can; what was fetched cannot, until a copy is made. */
export function editable(origin: Origin, main: boolean): boolean {
  return main || origin === 'typed' || origin === 'file' || origin === 'added';
}

/** One entry of the list. */
export interface Row {
  path: string;
  main: boolean;
  origin: Origin;
  id: string | null;
  /** The types the file defines, as `type` takes them. */
  types: string[];
  /** Why the file could not be read. */
  error: Diagnostic | null;
  /** The problems in it: for the main schema those of the problems pane, for an import the error of reading it. */
  errors: number;
  editable: boolean;
}

export interface Missing {
  name: string;
  from: string;
  message: string;
}

export interface Listing {
  rows: Row[];
  missing: Missing[];
}

/**
 * The list for what `inspect` found. A file that is in the request but that
 * nothing imports (it was added and is not named) is listed after the others,
 * so that it can be seen and removed. `problems` is the number of problems with
 * the main schema.
 */
export function listing(
  found: InspectResponse | null,
  mainPath: string,
  mainOrigin: Origin,
  files: ReadonlyMap<string, string>,
  origins: ReadonlyMap<string, Origin>,
  problems: number,
): Listing {
  const rows: Row[] = [];
  const seen = new Set<string>();
  const add = (path: string, info: InspectResponse['schemas'][number] | undefined): void => {
    const main = path === mainPath;
    const origin = main ? mainOrigin : (origins.get(path) ?? 'added');
    seen.add(path);
    rows.push({
      path,
      main,
      origin,
      id: info?.id ?? null,
      types: info?.types ?? [],
      error: info?.error ?? null,
      errors: main ? problems : info?.error ? 1 : 0,
      editable: editable(origin, main),
    });
  };
  const byPath = new Map((found?.schemas ?? []).map((info) => [info.path, info]));
  add(mainPath, byPath.get(mainPath));
  for (const info of found?.schemas ?? []) if (!seen.has(info.path)) add(info.path, info);
  for (const path of files.keys()) if (!seen.has(path)) add(path, undefined);
  return { rows, missing: found?.missing ?? [] };
}

/** The types of all the open schemas, by the file that defines them. */
export function typeChoices(rows: readonly Row[]): { path: string; id: string | null; types: string[] }[] {
  return rows.filter((row) => row.types.length > 0).map((row) => ({ path: row.path, id: row.id, types: row.types }));
}

/** Whether `type` is one that an open schema defines. */
export function isKnownType(rows: readonly Row[], type: string): boolean {
  return rows.some((row) => row.types.includes(type));
}

function escape(text: string): string {
  return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

export interface Handlers {
  /** A schema was picked to be shown (its path). */
  select(path: string): void;
  /** An import was taken out of the list. */
  remove(path: string): void;
  /** The type to parse with was changed (empty: the schema's own). */
  type(type: string): void;
  /** A copy of a schema that cannot be edited was asked for. */
  copy(path: string): void;
}

/** The list in the page and the choice of the type. */
export class SchemaList {
  private readonly list: HTMLElement;
  private readonly picker: HTMLSelectElement;
  private readonly note: HTMLElement;

  constructor(
    list: HTMLElement,
    picker: HTMLSelectElement,
    note: HTMLElement,
    private readonly handlers: Handlers,
  ) {
    this.list = list;
    this.picker = picker;
    this.note = note;
    list.addEventListener('click', (event) => {
      const target = event.target as HTMLElement;
      const remove = target.closest<HTMLElement>('[data-remove]')?.dataset.remove;
      if (remove !== undefined) {
        handlers.remove(remove);
        return;
      }
      const select = target.closest<HTMLElement>('[data-select]')?.dataset.select;
      if (select !== undefined) handlers.select(select);
    });
    picker.addEventListener('change', () => handlers.type(picker.value));
    note.addEventListener('click', (event) => {
      const path = (event.target as HTMLElement).closest<HTMLElement>('[data-copy]')?.dataset.copy;
      if (path !== undefined) handlers.copy(path);
    });
  }

  show(found: Listing, active: string, type: string | null): void {
    this.list.innerHTML =
      found.rows
        .map((row) => {
          const classes = ['schema'];
          if (row.main) classes.push('main');
          if (row.path === active) classes.push('active');
          if (row.errors > 0) classes.push('bad');
          const mark = row.errors > 0 ? `<span class="count" title="${escape(row.error?.message ?? 'problems in this schema')}">✗ ${row.errors}</span>` : '';
          const id = row.id ? `<span class="id">${escape(row.id)}</span>` : '';
          const remove = row.main ? '' : ` <button type="button" data-remove="${escape(row.path)}" aria-label="remove ${escape(row.path)}">×</button>`;
          return (
            `<li class="${classes.join(' ')}" data-origin="${row.origin}">` +
            `<button type="button" class="pick" data-select="${escape(row.path)}" aria-current="${row.path === active}">` +
            `<code>${escape(row.path)}</code>${id}<span class="origin">${row.main ? 'main schema · ' : ''}${ORIGIN_LABEL[row.origin]}${row.editable ? '' : ' · read-only'}</span>${mark}` +
            `</button>${remove}</li>`
          );
        })
        .join('') +
      found.missing
        .map(
          (m) =>
            `<li class="schema missing" title="${escape(m.message)}"><span class="count">✗ not found</span> <code>${escape(m.name)}</code> <span class="origin">imported by ${escape(m.from)}</span></li>`,
        )
        .join('');

    // the type to parse with
    const options = ['<option value="">the schema’s own</option>'];
    for (const choice of typeChoices(found.rows)) {
      const label = choice.id ? `${choice.path} (${choice.id})` : choice.path;
      options.push(
        `<optgroup label="${escape(label)}">${choice.types.map((t) => `<option value="${escape(t)}">${escape(t)}</option>`).join('')}</optgroup>`,
      );
    }
    // a type that was chosen is shown at once (a saved one, when the page opens), before the list says whether it is there
    if (type && !found.rows.some((row) => row.types.includes(type))) options.push(`<option value="${escape(type)}">${escape(type)}</option>`);
    this.picker.innerHTML = options.join('');
    this.picker.value = type ?? '';
    if (this.picker.value !== (type ?? '')) this.picker.value = '';

    // read-only
    const shown = found.rows.find((row) => row.path === active);
    if (shown && !shown.editable) {
      this.note.hidden = false;
      this.note.innerHTML = `<code>${escape(shown.path)}</code> is read-only (${ORIGIN_LABEL[shown.origin]}). <button type="button" data-copy="${escape(shown.path)}">Edit a copy</button>`;
    } else {
      this.note.hidden = true;
      this.note.textContent = '';
    }
  }
}
