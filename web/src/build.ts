// Building a schema from the bytes of the open file: a panel with the fields that the schema has and where each one is,
// and dialogs to add, rename, change, remove and group them. The schema is the text in the editor: every change is one
// call of the builder of the library (`draft`) with that text, and the text that comes back is put in the editor as one
// edit, so that the editor's own undo undoes it and the person can edit the text by hand between two changes.

import type { DraftKindSpec, DraftLayout, DraftOp, DraftRequest, DraftResponse, DraftSlot } from './doe.js';

/** What the builder needs of the page. */
export interface BuildHost {
  /** The text of the main schema. */
  schemaText(): string;
  /** The other schemas of the page, by path. */
  files(): Record<string, string>;
  /** The bytes of the open file, when the page has them (a file that a server reads is not). */
  bytes(): Uint8Array | null;
  /** Whether the main schema can be changed (what was fetched is shown, not edited). */
  editable(): boolean;
  /** The ids of the schemas that the schema can use the types of. */
  typeIds(): string[];
  /** The id for a schema that is begun: from the name of the file. */
  suggestedId(): string | undefined;
  /** Ask the library for one change. */
  call(request: DraftRequest): Promise<DraftResponse>;
  /** Put a text in the editor as the main schema, as one edit. */
  setSchema(text: string): void;
  /** Say something in the status line and to screen readers. */
  say(text: string): void;
  /** Show the bytes [first, end) as selected in the Bytes pane (null: none). */
  showBytes(range: { first: number; end: number } | null): void;
}

/** A field of the list: where it is, and which one. */
interface Row {
  at: string | null;
  index: number;
  slot: DraftSlot;
}

const hex = (n: number): string => `0x${n.toString(16)}`;

/** `0x4..0x8`, or `bits 8..11 (byte 0x1)` for a bit field. */
function rangeText(slot: DraftSlot): string {
  if (/^b\d+$/.test(slot.kind)) return `bits ${slot.start_bit}..${slot.end_bit}`;
  return `${hex(Math.floor(slot.start_bit / 8))}..${hex(Math.ceil(slot.end_bit / 8))}`;
}

/** A byte as hex digits: `7f`, `0x7f`, or nothing. */
function parseByte(text: string): number | null {
  const m = /^(?:0x)?([0-9a-f]{1,2})$/i.exec(text.trim());
  return m?.[1] ? Number.parseInt(m[1], 16) : null;
}

/** Bytes as hex digits, spaces allowed: `42 4c 49 50`, `0x424c`. */
function parseHexBytes(text: string): number[] | null {
  const digits = text.replace(/0x/gi, '').replace(/[\s,]+/g, '');
  if (digits === '' || digits.length % 2 !== 0 || !/^[0-9a-f]+$/i.test(digits)) return null;
  return (digits.match(/../g) ?? []).map((pair) => Number.parseInt(pair, 16));
}

function element<T extends HTMLElement>(id: string): T {
  const found = document.getElementById(id);
  if (!found) throw new Error(`missing #${id}`);
  return found as T;
}

export class BuildMode {
  /** Whether build mode is on. */
  on = false;
  private layout: DraftLayout | null = null;
  /** Whether the text is one that the builder can go on with. */
  private buildable = false;
  private selected: { at: string | null; index: number } | null = null;
  /** The bytes that a click in the Bytes pane chose for the next field: [first, end). */
  private pending: { first: number; end: number } | null = null;
  private serial = 0;
  /** What the dialog is for: a field is added, or the kind of one is changed. */
  private mode: { kind: 'add' } | { kind: 'retype'; row: Row } = { kind: 'add' };
  /** What the second dialog is for. */
  private ask: ((first: string, second: string) => Promise<string | null>) | null = null;
  private nameTouched = false;

  constructor(private readonly host: BuildHost) {
    element('build-toggle').addEventListener('click', () => void this.toggle());
    element('build-add').addEventListener('click', () => this.openAdd());
    element('build-rename').addEventListener('click', () => this.rename());
    element('build-retype').addEventListener('click', () => this.openRetype());
    element('build-remove').addEventListener('click', () => void this.remove());
    element('build-group').addEventListener('click', () => this.group());
    element('build-ungroup').addEventListener('click', () => void this.change({ op: 'ungroup', at: this.selected?.at ?? null, start: this.selected?.index ?? 0 }, 'took the type apart'));
    element('build-endian').addEventListener('click', () => void this.flipEndian());
    this.listWiring();
    this.dialogWiring();
  }

  // ── the mode ────────────────────────────────────────────────────────────────

  /** Whether an action of the builder can be done now. */
  get ready(): boolean {
    return this.on && this.buildable && this.host.editable();
  }

  async toggle(): Promise<void> {
    if (this.on) {
      this.on = false;
      this.pending = null;
      this.host.showBytes(null);
      this.show();
      this.host.say('Build mode is off');
      return;
    }
    if (this.host.bytes() === null) {
      this.host.say('Open a file first: the builder takes the fields from its bytes (a file that doe --ide reads on this machine cannot be built on here)');
      return;
    }
    this.on = true;
    this.show();
    await this.read();
    element('build-fields').focus();
  }

  /** Read the text in the editor again (it was changed, or the file was). */
  async read(): Promise<void> {
    if (!this.on) return;
    const mine = ++this.serial;
    const bytes = this.host.bytes();
    if (bytes === null) return;
    const answer = await this.host.call({ text: this.host.schemaText(), files: this.host.files(), id: this.host.suggestedId() });
    if (mine !== this.serial || !this.on) return;
    this.take(answer);
    this.show();
  }

  private take(answer: DraftResponse): void {
    this.buildable = answer.buildable;
    this.note = answer.buildable ? '' : (answer.error ?? '');
    this.layout = answer.layout;
    if (!answer.buildable) this.selected = null;
    else if (this.layout && !this.exists(this.selected)) this.selected = this.firstRow();
  }

  private exists(at: { at: string | null; index: number } | null): boolean {
    return at !== null && this.rows().some((row) => row.at === at.at && row.index === at.index);
  }

  private firstRow(): { at: string | null; index: number } | null {
    const row = this.rows()[0];
    return row ? { at: row.at, index: row.index } : null;
  }

  /** Every field, the schema's first and then each type's. */
  private rows(): Row[] {
    const layout = this.layout;
    if (!layout) return [];
    return [
      ...layout.fields.map((slot, index) => ({ at: null, index, slot })),
      ...layout.types.flatMap((type) => type.fields.map((slot, index) => ({ at: type.id, index, slot }))),
    ];
  }

  /** Where the next field starts: the end of the fields of the schema, in bytes. */
  private end(): number {
    const last = this.layout?.fields[this.layout.fields.length - 1];
    return last ? Math.ceil(last.end_bit / 8) : 0;
  }

  /** A click in the Bytes pane: the bytes from where the schema ends to the one clicked are for the next field. */
  pick(byte: number): boolean {
    if (!this.ready) return false;
    const end = this.end();
    if (byte < end) return false;
    this.pending = { first: end, end: byte + 1 };
    this.host.showBytes(this.pending);
    const n = byte + 1 - end;
    this.host.say(`${n} byte${n === 1 ? '' : 's'} chosen for the next field (${hex(end)}..${hex(byte + 1)}); A adds it`);
    this.show();
    return true;
  }

  // ── the panel ───────────────────────────────────────────────────────────────

  private show(): void {
    element('build-toggle').setAttribute('aria-pressed', String(this.on));
    const panel = element('build-panel');
    panel.hidden = !this.on;
    if (!this.on) return;
    const state = element('build-state');
    const list = element('build-fields');
    state.classList.remove('bad');
    const editable = this.host.editable();
    if (!editable) {
      state.textContent = 'This schema was fetched: it is shown, not changed. Copy it (or begin another) to build on it.';
      state.classList.add('bad');
    } else if (!this.buildable) {
      state.textContent = this.note || 'Reading the schema…';
      state.classList.add('bad');
    } else {
      const n = this.rows().length;
      const next = this.pending
        ? `${this.pending.end - this.pending.first} byte${this.pending.end - this.pending.first === 1 ? '' : 's'} chosen at ${hex(this.pending.first)}`
        : `the next field starts at ${hex(this.end())} (click a byte after it to choose how many bytes it has)`;
      state.textContent = `${n} field${n === 1 ? '' : 's'}; ${this.layout?.endian === 'be' ? 'big' : 'little'}-endian; ${next}`;
    }
    for (const id of ['build-add', 'build-rename', 'build-retype', 'build-remove', 'build-group', 'build-ungroup', 'build-endian']) {
      element<HTMLButtonElement>(id).disabled = !this.ready;
    }
    const some = this.selected !== null && this.exists(this.selected);
    for (const id of ['build-rename', 'build-retype', 'build-remove', 'build-group']) element<HTMLButtonElement>(id).disabled = !this.ready || !some;
    const row = this.selectedRow();
    element<HTMLButtonElement>('build-ungroup').disabled = !this.ready || !row || !this.definedTypes().has(row.slot.kind.replace(/\[\]$/, ''));
    list.replaceChildren();
    if (!this.layout || !this.buildable) return;
    const make = (row: Row): HTMLLIElement => {
      const item = document.createElement('li');
      item.id = `build-row-${row.at ?? 'top'}-${row.index}`;
      item.setAttribute('role', 'option');
      item.dataset.at = row.at ?? '';
      item.dataset.index = String(row.index);
      const isSelected = this.selected?.at === row.at && this.selected.index === row.index;
      item.setAttribute('aria-selected', String(isSelected));
      const range = document.createElement('span');
      range.className = 'range';
      range.textContent = rangeText(row.slot);
      const name = document.createElement('span');
      name.className = 'name';
      name.textContent = row.slot.id;
      const kind = document.createElement('span');
      kind.className = 'kind';
      kind.textContent = row.slot.kind;
      item.append(range, name, kind);
      if (isSelected) list.setAttribute('aria-activedescendant', item.id);
      return item;
    };
    for (const row of this.rows().filter((r) => r.at === null)) list.append(make(row));
    for (const type of this.layout.types) {
      const head = document.createElement('li');
      head.setAttribute('role', 'presentation');
      head.textContent = `type ${type.id}`;
      list.append(head);
      for (const row of this.rows().filter((r) => r.at === type.id)) list.append(make(row));
    }
    if (!some) list.removeAttribute('aria-activedescendant');
  }

  /** Why the text cannot be built on, as the library said it. */
  private note = '';

  private definedTypes(): Set<string> {
    return new Set(this.layout?.types.map((t) => t.id) ?? []);
  }

  private selectedRow(): Row | null {
    const selected = this.selected;
    return this.rows().find((row) => selected !== null && row.at === selected.at && row.index === selected.index) ?? null;
  }

  private listWiring(): void {
    const list = element('build-fields');
    list.addEventListener('click', (event) => {
      const item = (event.target as HTMLElement).closest<HTMLElement>('li[role="option"]');
      if (!item) return;
      this.select(item.dataset.at ? item.dataset.at : null, Number(item.dataset.index));
    });
    list.addEventListener('keydown', (event) => {
      if (!this.ready) return;
      const rows = this.rows();
      const at = rows.findIndex((r) => r.at === this.selected?.at && r.index === this.selected?.index);
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
        event.preventDefault();
        const next = rows[Math.max(0, Math.min(rows.length - 1, at + (event.key === 'ArrowDown' ? 1 : -1)))];
        if (next) this.select(next.at, next.index);
      } else if (event.key === 'Delete') {
        event.preventDefault();
        void this.remove();
      } else if (event.key === 'F2') {
        event.preventDefault();
        this.rename();
      }
    });
  }

  private select(at: string | null, index: number): void {
    this.selected = { at, index };
    const row = this.selectedRow();
    this.show();
    if (row) {
      this.host.say(`${row.slot.id}, ${row.slot.kind}, ${rangeText(row.slot)}`);
      // the bytes of a field of the schema itself are shown in the Bytes pane
      this.host.showBytes(row.at === null ? { first: Math.floor(row.slot.start_bit / 8), end: Math.ceil(row.slot.end_bit / 8) } : null);
    }
  }

  // ── changes ─────────────────────────────────────────────────────────────────

  /** Make one change; on success the text is in the editor, and on refusal the reason is said (and returned). */
  async change(op: DraftOp, done: string): Promise<string | null> {
    if (!this.ready) return 'the schema cannot be built on now';
    const answer = await this.host.call({ text: this.host.schemaText(), files: this.host.files(), id: this.host.suggestedId(), op });
    this.take(answer);
    if (!answer.ok) {
      this.show();
      this.host.say(answer.error ?? 'that change was not made');
      return answer.error ?? 'that change was not made';
    }
    if (answer.text !== this.host.schemaText()) this.host.setSchema(answer.text);
    this.pending = null;
    this.host.showBytes(null);
    this.show();
    this.host.say(`${done}${answer.error ? ` (${answer.error})` : ''}`);
    return null;
  }

  private async remove(): Promise<void> {
    const row = this.selectedRow();
    if (!row || !this.ready) return;
    const after = Math.max(0, row.index - 1);
    const refused = await this.change({ op: 'remove', at: row.at, index: row.index }, `removed ${row.slot.id}`);
    if (refused === null) this.selected = this.exists({ at: row.at, index: after }) ? { at: row.at, index: after } : this.firstRow();
    this.show();
  }

  private async flipEndian(): Promise<void> {
    const to = this.layout?.endian === 'be' ? 'le' : 'be';
    await this.change({ op: 'endian', to }, `the schema is ${to === 'be' ? 'big' : 'little'}-endian now`);
  }

  private rename(): void {
    const row = this.selectedRow();
    if (!row || !this.ready) return;
    this.asking(`Rename ${row.slot.id}`, [['New name', row.slot.id]], (answers) =>
      this.change({ op: 'rename', at: row.at, index: row.index, to: answers[0] ?? '' }, `renamed ${row.slot.id} to ${answers[0]}`),
    );
  }

  private group(): void {
    const row = this.selectedRow();
    if (!row || !this.ready) return;
    this.asking(
      `Make ${row.slot.id} and what follows it a type`,
      [
        ['Id of the new type', ''],
        ['How many fields (from the selected one)', '1'],
        ['Name of the field that has the type', ''],
      ],
      async (answers) => {
        const count = Number.parseInt(answers[1] ?? '', 10);
        if (!Number.isInteger(count) || count < 1) return 'how many fields: a whole number, 1 or more';
        return this.change(
          { op: 'group', at: row.at, start: row.index, count, type_id: answers[0] ?? '', field_id: answers[2] ?? '' },
          `${count} field${count === 1 ? '' : 's'} made the type ${answers[0]}`,
        );
      },
    );
  }

  // ── the question dialog (rename, group) ────────────────────────────────────

  /** What the dialog does with the answers: the reason that it cannot be done, or null (and the dialog closes). */
  private onAsk: ((answers: string[]) => Promise<string | null>) | null = null;

  /** Ask for one to three texts. */
  private asking(title: string, fields: [string, string][], then: (answers: string[]) => Promise<string | null>): void {
    element('build-ask-title').textContent = title;
    for (let i = 0; i < 3; i += 1) {
      const label = element(`build-ask-label-${i + 1}`);
      const field = fields[i];
      label.hidden = field === undefined;
      const span = label.querySelector('span');
      if (span && field) span.textContent = field[0];
      element<HTMLInputElement>(`build-ask-${i + 1}`).value = field?.[1] ?? '';
    }
    element('build-ask-error').hidden = true;
    this.onAsk = then;
    element<HTMLDialogElement>('build-ask-dialog').showModal();
    element<HTMLInputElement>('build-ask-1').select();
  }

  // ── the dialog that adds a field ───────────────────────────────────────────

  private dialogWiring(): void {
    const ask = element<HTMLDialogElement>('build-ask-dialog');
    element('build-ask-close').addEventListener('click', () => ask.close());
    element('build-ask-cancel').addEventListener('click', () => ask.close());
    element<HTMLFormElement>('build-ask-form').addEventListener('submit', (event) => {
      event.preventDefault();
      const answers = [1, 2, 3].map((n) => element<HTMLInputElement>(`build-ask-${n}`).value.trim());
      void this.onAsk?.(answers).then((refused) => {
        const error = element('build-ask-error');
        error.hidden = refused === null;
        error.textContent = refused ?? '';
        if (refused === null) {
          ask.close();
          element('build-fields').focus();
        }
      });
    });
    const dialog = element<HTMLDialogElement>('build-dialog');
    element('build-dialog-close').addEventListener('click', () => dialog.close());
    element('build-cancel').addEventListener('click', () => dialog.close());
    element<HTMLSelectElement>('build-kind').addEventListener('change', () => void this.kindChosen());
    element<HTMLSelectElement>('build-position').addEventListener('change', () => this.showWhere());
    element('build-type').addEventListener('input', () => void this.kindChosen());
    element('build-name').addEventListener('input', () => {
      this.nameTouched = true;
    });
    for (const id of ['build-bits', 'build-length', 'build-encoding', 'build-terminator', 'build-magic', 'build-type', 'build-size', 'build-repeat', 'build-name']) {
      element(id).addEventListener('input', () => {
        element('build-error').hidden = true;
      });
    }
    element<HTMLFormElement>('build-form').addEventListener('submit', (event) => {
      event.preventDefault();
      void this.apply();
    });
  }

  /** Open the dialog that adds a field. */
  openAdd(): void {
    if (!this.ready) return;
    this.mode = { kind: 'add' };
    element('build-title').textContent = 'Add a field';
    element('build-apply').textContent = 'Add';
    element('build-position-label').hidden = false;
    this.fillPositions();
    this.prepare(null);
    this.showWhere();
    void this.propose();
    element<HTMLDialogElement>('build-dialog').showModal();
    element('build-kind').focus();
  }

  /** The places a field can go: at the end, and before or after the selected one. */
  private fillPositions(): void {
    const row = this.selectedRow();
    const select = element<HTMLSelectElement>('build-position');
    const option = (value: string, text: string): HTMLOptionElement => {
      const item = document.createElement('option');
      item.value = value;
      item.textContent = text;
      return item;
    };
    select.replaceChildren(option('end', row?.at ? `At the end of the type ${row.at}` : 'After the last field'));
    if (row) {
      select.append(option('after', `After ${row.slot.id}`), option('before', `Before ${row.slot.id}`));
    }
    select.value = 'end';
  }

  /** Where the new field goes, as the library takes it, and the byte it starts at when that is known. */
  private insertion(): { at: string | null; index: number | undefined; first: number | null } {
    const row = this.selectedRow();
    const place = element<HTMLSelectElement>('build-position').value;
    if (row && place === 'after') return { at: row.at, index: row.index + 1, first: row.at === null ? Math.ceil(row.slot.end_bit / 8) : null };
    if (row && place === 'before') return { at: row.at, index: row.index, first: row.at === null ? Math.floor(row.slot.start_bit / 8) : null };
    const at = row?.at ?? null;
    return { at, index: undefined, first: at === null ? (this.pending?.first ?? this.end()) : null };
  }

  /** What the dialog says about where the field is read, and the signature bytes that are the file's there. */
  private showWhere(): void {
    const place = this.insertion();
    const chosen = place.index === undefined && place.at === null && this.pending ? this.pending.end - this.pending.first : null;
    element('build-where').textContent =
      place.at !== null
        ? `It goes in the type ${place.at}.`
        : place.first === null
          ? 'It is read after the last field.'
          : `It is read from byte ${hex(place.first)}${chosen !== null ? `; ${chosen} byte${chosen === 1 ? '' : 's'} are chosen` : ''}${place.index === undefined ? '' : ', and the fields after it move down'}.`;
    const bytes = this.host.bytes();
    if (this.mode.kind === 'add' && bytes && place.first !== null) {
      const length = chosen ?? 4;
      element<HTMLInputElement>('build-magic').value = [...bytes.subarray(place.first, place.first + Math.max(1, length))].map((b) => b.toString(16).padStart(2, '0')).join(' ');
    }
  }

  private openRetype(): void {
    const row = this.selectedRow();
    if (!row || !this.ready) return;
    this.mode = { kind: 'retype', row };
    element('build-title').textContent = `Change the kind of ${row.slot.id}`;
    element('build-apply').textContent = 'Change';
    element('build-position-label').hidden = true;
    element('build-where').textContent = `It is ${row.slot.kind} now (${rangeText(row.slot)}).`;
    this.prepare(row);
    this.showFields();
    element<HTMLDialogElement>('build-dialog').showModal();
    element('build-kind').focus();
  }

  /** Empty the form (or fill the names), and offer the types that can be used. */
  private prepare(row: Row | null): void {
    const set = (id: string, value: string): void => {
      element<HTMLInputElement>(id).value = value;
    };
    element('build-error').hidden = true;
    this.nameTouched = row !== null;
    set('build-name', row?.slot.id ?? '');
    element('build-name-label').hidden = row !== null;
    set('build-bits', '');
    set('build-length', '');
    set('build-encoding', 'ASCII');
    set('build-terminator', '00');
    set('build-size', '');
    set('build-repeat', '');
    set('build-type', '');
    set('build-magic', '');
    const known = this.pending ? this.pending.end - this.pending.first : null;
    if (known !== null) {
      set('build-length', String(known));
    }
    const list = element('build-types');
    list.replaceChildren(
      ...this.host.typeIds().map((id) => {
        const option = document.createElement('option');
        option.value = id;
        return option;
      }),
    );
    // the kind that the chosen bytes suggest comes from the library (`propose`); until it answers, a byte
    element<HTMLSelectElement>('build-kind').value = known !== null && [1, 2, 4, 8].includes(known) ? `u${known}` : known !== null ? 'bytes' : 'u1';
    this.showFields();
  }

  /** Which boxes the chosen kind needs. */
  private showFields(): void {
    const kind = element<HTMLSelectElement>('build-kind').value;
    const shows: Record<string, boolean> = {
      'build-bits': kind === 'bits',
      'build-length': kind === 'bytes' || kind === 'str',
      'build-encoding': kind === 'str' || kind === 'strz',
      'build-terminator': kind === 'strz',
      'build-magic': kind === 'magic',
      'build-type': kind === 'type',
      'build-size': kind === 'type',
      'build-repeat': kind !== 'magic' && kind !== 'bits' && kind !== 'strz',
    };
    for (const [id, visible] of Object.entries(shows)) {
      const label = element(id).closest('label');
      if (label) label.hidden = !visible;
    }
  }

  private async kindChosen(): Promise<void> {
    this.showFields();
    if (this.mode.kind === 'add') await this.propose();
  }

  /** What the library proposes: a name that no field has. */
  private async propose(): Promise<void> {
    if (this.nameTouched) return;
    const kind = this.kindWord();
    const type = element<HTMLInputElement>('build-type').value.trim();
    const answer = await this.host.call({
      text: this.host.schemaText(),
      files: this.host.files(),
      id: this.host.suggestedId(),
      op: { op: 'propose', kind: kind ?? 'u1', type_id: type },
    });
    // Not while the name is being typed: a proposal that lands between the selecting and the typing of the first
    // keystroke would leave the text typed after the proposed name.
    const field = element<HTMLInputElement>('build-name');
    if (answer.proposal && !this.nameTouched && document.activeElement !== field) field.value = answer.proposal.name;
  }

  /** The kind as the library spells it. */
  private kindWord(): string | null {
    const kind = element<HTMLSelectElement>('build-kind').value;
    if (kind !== 'bits') return kind;
    const bits = Number.parseInt(element<HTMLInputElement>('build-bits').value, 10);
    return Number.isInteger(bits) ? `b${bits}` : null;
  }

  /** What the form says, or why it cannot be a field. */
  private spec(): DraftKindSpec | string {
    const kind = this.kindWord();
    if (kind === null) return 'bits: a number from 1 to 64';
    const value = (id: string): string => element<HTMLInputElement>(id).value.trim();
    const spec: DraftKindSpec = { kind };
    if (kind === 'bytes' || kind === 'str') {
      const length = Number.parseInt(value('build-length'), 10);
      if (!Number.isInteger(length) || length < 1) return 'length: a whole number of bytes, 1 or more';
      spec.length = length;
    }
    if (kind === 'str' || kind === 'strz') spec.encoding = value('build-encoding');
    if (kind === 'strz') {
      const terminator = parseByte(value('build-terminator'));
      if (terminator === null) return 'terminator: one byte, in hex (00, 0a, 0x2c)';
      spec.terminator = terminator;
    }
    if (kind === 'magic') {
      const magic = parseHexBytes(value('build-magic'));
      if (magic === null) return 'bytes of the signature: hex digits in pairs (42 4c 49 50)';
      spec.magic = magic;
    }
    if (kind === 'type') {
      if (value('build-type') === '') return 'type: the id of an imported schema, or of one of its types (id::name)';
      spec.type_id = value('build-type');
      if (value('build-size') !== '') {
        const size = Number.parseInt(value('build-size'), 10);
        if (!Number.isInteger(size) || size < 1) return 'size: a whole number of bytes, 1 or more';
        spec.size = size;
      }
    }
    if (kind !== 'magic' && !/^b\d+$/.test(kind) && kind !== 'strz' && value('build-repeat') !== '') spec.repeat = value('build-repeat');
    return spec;
  }

  private async apply(): Promise<void> {
    const spec = this.spec();
    const error = element('build-error');
    if (typeof spec === 'string') {
      error.hidden = false;
      error.textContent = spec;
      return;
    }
    const name = element<HTMLInputElement>('build-name').value.trim();
    const mode = this.mode;
    const place = this.insertion();
    const op: DraftOp =
      mode.kind === 'add'
        ? { op: 'add', at: place.at, ...(place.index === undefined ? {} : { index: place.index }), ...(name ? { name } : {}), ...spec }
        : { op: 'retype', at: mode.row.at, index: mode.row.index, ...spec };
    const where = element<HTMLSelectElement>('build-position');
    const placed = where.value === 'end' ? '' : ` ${where.selectedOptions[0]?.textContent?.toLowerCase() ?? ''}`;
    const refused = await this.change(op, mode.kind === 'add' ? `added ${name || spec.kind}${placed}` : `${mode.row.slot.id} is ${spec.kind} now`);
    if (refused !== null) {
      error.hidden = false;
      error.textContent = refused;
      return;
    }
    element<HTMLDialogElement>('build-dialog').close();
    element('build-fields').focus();
  }
}
