// The page: the schema, the input, and what doe makes of them, in four panes
// that stay in step. Everything runs here, in the browser; nothing is sent
// anywhere.

import { CallEnded, DoeClient } from './client.js';
import type { Diagnostic, ExportFormat, ExportResponse, InspectResponse, ParseResponse, Request, TraceNode } from './doe.js';
import { SchemaEditor } from './editor.js';
import { Help } from './help.js';
import { type Action, type Needs, KEYS, byteQuery, findPattern, isField, keyName, keyOf } from './keys.js';
import { PaneSearch, type SearchHandler } from './search.js';
import { HexView, RemoteBytes } from './hex.js';
import { type LocalApi, type LocalType, detectLocal } from './local.js';
import { type Entry, type Ranking, TypeBrowser, galleryEntries, libraryEntries, openEntries } from './browse.js';
import { type Path, byteRange, checkSize, describeRange, findByte, findNode, nodeAt, pathName, pathOfProblem } from './model.js';
import { ProblemsView } from './problems.js';
import { type Origin, SchemaList, editable, isKnownType, listing, parsedAs } from './schemas.js';
import { type Work, codeOfHash, decodeWork, recall, remember } from './share.js';
import { CUSTOM, SYSTEM, THEMES, Themes } from './theme.js';
import { RADIXES, Radixes, isRadix } from './radix.js';
import { type Attempt, type GalleryFormat, describe, detectGallery, galleryOrigin, isBundled } from './detect.js';
import { TreeView } from './tree.js';
import { type Layout, type PaneName, PANE_NAMES, applyLayout, defaultLayout, isDefault, loadLayout, saveLayout, shown, swapped, withVisible } from './layout.js';

/** How long the schema is left alone before it is checked again. */
const PAUSE_MS = 250;

/** The part of a file handle that saving uses (File System Access API, in the browsers that have it). */
interface SaveHandle {
  name: string;
  createWritable(): Promise<{ write(data: string): Promise<void>; close(): Promise<void> }>;
}
type WindowWithPicker = Window & {
  showSaveFilePicker?: (options: { suggestedName: string; types: { description: string; accept: Record<string, string[]> }[] }) => Promise<SaveHandle>;
};

const SCHEMA_EXTENSIONS = ['.ksy', '.yaml', '.yml'];

/** The panes that the keys move between, in order. */
const PANES: readonly PaneName[] = PANE_NAMES;
/** What the panes are called, for the messages. */
const NAMES: Record<PaneName, string> = { schema: 'Schema', parsed: 'Parsed', bytes: 'Bytes', status: 'Status' };
const PANE_IDS: Record<PaneName, string> = { schema: 'pane-editor', parsed: 'pane-tree', bytes: 'pane-hex', status: 'pane-status' };

function element<T extends HTMLElement>(id: string): T {
  const found = document.getElementById(id);
  if (!found) throw new Error(`the page has no #${id}`);
  return found as T;
}

const isSchemaFile = (name: string): boolean => SCHEMA_EXTENSIONS.some((ext) => name.toLowerCase().endsWith(ext));

export class App {
  readonly client: DoeClient;
  private generation = 0;
  private local: LocalApi | null = null;
  /** The file named on the command line, when it is the input: it stays on the machine of the server. */
  private serverInput: { name: string; size: number } | null = null;
  private abort: AbortController | null = null;
  private includePaths: string[] | undefined;
  /** The file that the remembered work was last used with, to say which one to open again. */
  private lastInputName: string | null = null;
  private readonly files = new Map<string, string>();
  /** Where each imported file came from (`added` when it is not here). */
  private readonly origins = new Map<string, Origin>();
  private schemaPath = 'main.ksy';
  /** The text of the main schema; the editor shows it, or an imported file. */
  private mainText = '';
  private mainOrigin: Origin = 'typed';
  /** The imported file the editor shows; null: the main schema. */
  private active: string | null = null;
  /** The type to parse with; null: the schema's own. */
  private parseType: string | null = null;
  /** The schemas the last request is made of. */
  private found: InspectResponse | null = null;
  private diagnostics: Diagnostic[] = [];
  /** The id of the type that the last parse said it was parsing as. */
  private lastId: string | null = null;
  /** The editor is being given another text: that is not an edit. */
  private switching = false;
  private bytes: Uint8Array | null = null;
  private inputName: string | null = null;
  private notice: Diagnostic | null = null;
  private trace: TraceNode | null = null;
  private selection: Path | null = null;
  private timer: ReturnType<typeof setTimeout> | undefined;

  private readonly editor: SchemaEditor;
  private readonly tree: TreeView;
  private readonly hex: HexView;
  private readonly problems: ProblemsView;
  private readonly themes: Themes;
  private readonly radixes: Radixes;
  private readonly schemas: SchemaList;
  private readonly browser: TypeBrowser;
  private readonly help: Help;
  /** The types of this machine, when the page is served by `doe --ide`. */
  private localTypes: LocalType[] = [];

  constructor(workerUrl: URL, wasmUrl: URL) {
    this.client = new DoeClient(workerUrl, wasmUrl);
    this.editor = new SchemaEditor(element('editor'), '', () => this.edited());
    this.schemas = new SchemaList(element('schemas'), element<HTMLSelectElement>('parse-type'), element('edit-note'), {
      select: (path) => this.showFile(path),
      remove: (path) => this.removeFile(path),
      type: (type) => this.chooseType(type),
      copy: (path) => this.copyFile(path),
    });
    this.browser = new TypeBrowser(element<HTMLDialogElement>('browse-dialog'), {
      canRank: () => this.canDetect(),
      rank: (entries) => this.rankEntries(entries),
      pick: (entry) => void this.pickEntry(entry),
      text: (entry) => this.entryText(entry),
    });
    this.help = new Help(element<HTMLDialogElement>('help-dialog'), element('help-body'), element('help'), element('help-close'));
    this.themes = new Themes((theme) => this.editor.setTheme(theme));
    this.radixes = new Radixes(() => {
      this.tree.refresh();
      this.showFieldRadix();
    });
    this.tree = new TreeView(
      element('tree'),
      (path) => this.select(path, 'tree'),
      (key) => this.radixes.of(key),
    );
    element<HTMLSelectElement>('radix').value = this.radixes.global;
    this.hex = new HexView(element('hex'), (byte) => this.pickByte(byte));
    this.problems = new ProblemsView(element('problems'), (item) => this.pickProblem(item));
    this.wire();
    this.keys();
    this.showThemes();
    this.themes.apply();
  }

  // ── the theme ───────────────────────────────────────────────────────────────

  /** Fill the picker: the system's setting, the built-in themes by variant, and the one that was loaded. */
  private showThemes(): void {
    const picker = element<HTMLSelectElement>('theme');
    const option = (value: string, text: string): HTMLOptionElement => {
      const item = document.createElement('option');
      item.value = value;
      item.textContent = text;
      return item;
    };
    picker.replaceChildren(option(SYSTEM, 'System (light or dark)'));
    for (const [label, variant] of [
      ['Dark', 'dark'],
      ['Light', 'light'],
    ] as const) {
      const group = document.createElement('optgroup');
      group.label = label;
      for (const theme of THEMES.filter((t) => t.variant === variant)) group.append(option(theme.id, theme.name));
      picker.append(group);
    }
    const loaded = this.themes.loaded;
    if (loaded) picker.append(option(CUSTOM, `${loaded.name} (loaded file)`));
    picker.value = this.themes.chosen;
  }

  private async openTheme(file: File | undefined): Promise<void> {
    if (!file) return;
    try {
      const theme = this.themes.load(await file.text());
      this.themeSaid(`theme ${theme.name} loaded from ${file.name}`);
      this.showThemes();
    } catch (error) {
      this.themeSaid(`${file.name} is not a theme: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  /** Say what happened to a theme file, in the status and in the theme dialog (which is over the status). */
  private themeSaid(text: string): void {
    element('status').textContent = text;
    element('theme-note').textContent = text;
  }

  /** Load the module, and bring back the saved work or the work of a link; a first visit has nothing in the editor. */
  async start(): Promise<DoeClient> {
    await this.client.start();
    element('version').textContent = this.client.version;
    this.local = await detectLocal();
    if (this.local) await this.showLocal(this.local);
    await this.loadGallery();
    await this.restore();
    this.refresh();
    return this.client;
  }

  // ── saved work, links, the gallery, export ─────────────────────────────────

  private work(): Work {
    const work: Work = { schema: this.mainText, files: Object.fromEntries(this.files), path: this.schemaPath };
    if (this.includePaths) work.include_paths = this.includePaths;
    if (this.parseType) work.type = this.parseType;
    if (this.mainOrigin !== 'typed') work.origin = this.mainOrigin;
    if (this.origins.size > 0) work.origins = Object.fromEntries(this.origins);
    if (this.radixes.count > 0) work.radixes = this.radixes.toJSON();
    return work;
  }

  private applyWork(work: Work): void {
    this.files.clear();
    this.origins.clear();
    for (const [name, text] of Object.entries(work.files)) this.files.set(name, text);
    for (const [name, origin] of Object.entries(work.origins ?? {})) if (this.files.has(name)) this.origins.set(name, origin);
    this.schemaPath = work.path;
    this.includePaths = work.include_paths;
    this.mainOrigin = work.origin ?? 'typed';
    this.parseType = work.type ?? null;
    this.radixes.replace(work.radixes);
    this.found = null;
    this.setMain(work.schema);
  }

  /** Show the main schema, with this text, in the editor. */
  private setMain(text: string): void {
    this.mainText = text;
    this.active = null;
    this.switching = true;
    this.editor.setText(text);
    this.switching = false;
    this.editor.setReadOnly(false);
    this.showFiles();
  }

  /** Open the work of a link (`#s=…`, made by an earlier version), or else the work this browser remembers. No file: the person opens it. */
  private async restore(): Promise<boolean> {
    const code = codeOfHash(location.hash);
    const shared = code ? await decodeWork(code) : null;
    if (code && !shared) {
      this.notice = { severity: 'error', kind: 'request', message: 'the link does not hold a schema', position: null };
    }
    const saved = shared ? null : recall();
    const work = shared ?? saved?.work;
    if (!work) return false;
    this.applyWork(work);
    this.lastInputName = saved?.inputName ?? null;
    this.bytes = null;
    this.inputName = null;
    this.hex.setBytes(null);
    return true;
  }

  /** Remember the schema and its imports in this browser. */
  private persist(): void {
    remember(this.work(), this.inputName ?? this.lastInputName);
  }

  private gallery: { repo: string; commit: string; formats: GalleryFormat[] } | null = null;

  /** The list of the formats of the Kaitai Struct gallery and of doe's own library; their definitions are fetched when one is used. */
  private async loadGallery(): Promise<void> {
    try {
      const response = await fetch('gallery.json');
      if (!response.ok) return;
      const gallery = (await response.json()) as NonNullable<App['gallery']>;
      if (!Array.isArray(gallery.formats) || gallery.formats.length === 0) return;
      this.gallery = gallery;
    } catch {
      // no gallery.json (a build without the submodule): the dialog has no gallery
    }
  }

  /** A file of the gallery, at the commit the page was built with; one of doe's own library is served with the page. */
  private async fetchGallery(path: string): Promise<string> {
    const gallery = this.gallery;
    if (!gallery) throw new Error('there is no gallery');
    const url = isBundled(path) ? path : `https://raw.githubusercontent.com/${gallery.repo}/${gallery.commit}/${path}`;
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${path}: ${response.status}`);
    return response.text();
  }

  // ── finding the schema of a file ───────────────────────────────────────────

  private detection: { cancelled: boolean } | null = null;
  /** Look for the schema if the one in the editor does not parse the file that was just opened. */
  private detectIfItFails = false;

  private autoDetect(): boolean {
    try {
      return localStorage.getItem('doe.detect') === '1';
    } catch {
      return false;
    }
  }

  /** Whether there is a file to look for a schema of. */
  private canDetect(): boolean {
    return this.bytes !== null || this.serverInput !== null;
  }

  private showDetectReport(title: string, lines: { text: string; fit: boolean }[], offerAll: boolean): void {
    this.reveal('status');
    const box = element('detect-report');
    box.hidden = false;
    box.replaceChildren();
    const head = document.createElement('div');
    head.textContent = title;
    box.append(head);
    const list = document.createElement('ul');
    for (const line of lines) {
      const item = document.createElement('li');
      item.textContent = line.text;
      if (line.fit) item.className = 'fit';
      list.append(item);
    }
    box.append(list);
    if (offerAll) {
      const button = document.createElement('button');
      button.type = 'button';
      button.id = 'detect-all';
      button.textContent = 'Try them all';
      button.title = 'Also try the formats that have no signature and whose name the file does not carry (slow)';
      button.addEventListener('click', () => void this.detect(true));
      box.append(button);
    }
  }

  /**
   * Find the schema of the opened file: the formats are put in an order by the
   * library (by signature, then by name) and the first that parses the file is
   * opened. `all` also tries the formats that have neither.
   */
  async detect(all = false): Promise<void> {
    if (!this.canDetect()) {
      element('status').textContent = 'open a file first: the schema is looked for from the file';
      return;
    }
    this.detectIfItFails = false;
    const run = { cancelled: false };
    this.detection = run;
    const status = element('status');
    status.textContent = 'looking for the schema…';
    const cancel = element('cancel');
    cancel.hidden = false;
    try {
      if (this.serverInput && this.local) {
        const found = await this.local.detect(all);
        const lines = found.attempts.map((a) => ({ text: `${a.name} (${a.why}): ${a.fits ? 'parsed' : a.outcome}`, fit: a.fits }));
        if (found.chosen) {
          await this.openType(this.local, found.chosen.path);
          status.textContent = `detected ${found.chosen.name} (${found.attempts.length} tried)`;
          this.showDetectReport(`${found.chosen.name} parses the file`, lines, false);
        } else {
          status.textContent = 'no schema of this machine parses the file';
          lines.push({ text: `${found.known} types known, ${found.excluded} left out because the file does not start with their signature`, fit: false });
          this.showDetectReport('no schema fits', lines, !all);
        }
        return;
      }
      if (!this.gallery) {
        status.textContent = 'there is no gallery to look in';
        return;
      }
      const found = await detectGallery(
        {
          formats: this.gallery.formats,
          order: (sources, name, every) => this.client.order(sources, name, every),
          fetchText: (path) => this.fetchGallery(path),
          parse: (request) => this.client.parse(request),
          progress: (text) => {
            status.textContent = text;
          },
          cancelled: () => run.cancelled,
        },
        this.inputName,
        all,
      );
      const lines = found.attempts.map((a: Attempt) => ({ text: describe(a), fit: a.result === 'fit' }));
      if (found.chosen) {
        const { format, text, files } = found.chosen;
        this.applyWork({
          schema: text,
          files,
          path: format.path,
          include_paths: [''],
          origin: galleryOrigin(format.path),
          origins: Object.fromEntries(Object.keys(files).map((path) => [path, galleryOrigin(path)])),
        });
        this.refreshNow();
        status.textContent = `detected ${format.id} (${found.attempts.length} tried)`;
        this.showDetectReport(`${format.id} parses the file`, lines, false);
      } else if (run.cancelled) {
        status.textContent = 'detection cancelled';
      } else {
        status.textContent = 'no gallery format parses the file';
        lines.push({ text: `${found.known} formats known, ${found.excluded} left out because the file does not start with their signature`, fit: false });
        this.showDetectReport('no format fits', lines, !all);
      }
    } catch (error) {
      status.textContent = `detection stopped: ${error instanceof Error ? error.message : String(error)}`;
    } finally {
      cancel.hidden = this.detection !== run;
      if (this.detection === run) this.detection = null;
    }
  }

  /** Fetch a format and the files it imports, at the commit the page was built with. */
  async openGallery(format: { path: string; imports: string[] }): Promise<void> {
    const gallery = this.gallery;
    if (!gallery) return;
    const status = element('status');
    status.textContent = `fetching ${format.path}…`;
    try {
      const get = (path: string): Promise<string> => this.fetchGallery(path);
      const [text, ...imports] = await Promise.all([format.path, ...format.imports].map(get));
      const files: Record<string, string> = {};
      for (const [index, path] of format.imports.entries()) files[path] = imports[index] ?? '';
      this.applyWork({
        schema: text ?? '',
        files,
        path: format.path,
        include_paths: [''],
        origin: galleryOrigin(format.path),
        origins: Object.fromEntries(Object.keys(files).map((path) => [path, galleryOrigin(path)])),
      });
      this.refreshNow();
    } catch (error) {
      this.problems.show([{ severity: 'error', kind: 'request', message: `could not fetch the format: ${String(error)}`, position: null }]);
      status.textContent = 'could not fetch the format';
    }
  }

  // ── saving the parsed value ────────────────────────────────────────────────

  /** The handle of the file that *Browse...* chose, where the browser lets a page choose one. */
  private saveHandle: SaveHandle | null = null;
  /** The name the dialog offered for the format, to tell whether the person changed it. */
  private saveDefault = '';

  /** The format of the dialog: `text`, or `json` (indented, as `--pretty`). */
  private saveFormat(): 'text' | 'json' {
    const chosen = document.querySelector<HTMLInputElement>('input[name="save-format"]:checked');
    return chosen?.value === 'json' ? 'json' : 'text';
  }

  private defaultName(format: 'text' | 'json'): string {
    return `${this.inputName ?? 'output'}.${format === 'text' ? 'txt' : 'json'}`;
  }

  private saveNote(text: string, bad = false): void {
    const note = element('save-note');
    note.textContent = text;
    note.classList.toggle('bad', bad);
  }

  private openSave(): void {
    if (element<HTMLButtonElement>('save').disabled) return;
    this.saveHandle = null;
    this.saveDefault = this.defaultName(this.saveFormat());
    element<HTMLInputElement>('save-name').value = this.saveDefault;
    element('save-place').textContent = '';
    const picker = typeof (window as WindowWithPicker).showSaveFilePicker === 'function';
    element('save-browse').hidden = !picker;
    this.saveNote(picker ? '' : 'This browser cannot choose where to save: the file goes where its download setting says, or it asks.');
    element<HTMLDialogElement>('save-dialog').showModal();
  }

  /** The format changed: the name follows it unless the person wrote their own, and a place chosen for the other format is dropped. */
  private saveFormatChanged(): void {
    const name = element<HTMLInputElement>('save-name');
    if (name.value === this.saveDefault) {
      this.saveDefault = this.defaultName(this.saveFormat());
      name.value = this.saveDefault;
    }
    this.saveHandle = null;
    element('save-place').textContent = '';
  }

  /** *Browse...*: the system's save dialog, in the browsers that let a page open one. */
  private async browseSave(): Promise<void> {
    const pick = (window as WindowWithPicker).showSaveFilePicker;
    if (typeof pick !== 'function') return;
    const format = this.saveFormat();
    try {
      const handle = await pick.call(window, {
        suggestedName: element<HTMLInputElement>('save-name').value || this.defaultName(format),
        types: [format === 'text' ? { description: 'Text', accept: { 'text/plain': ['.txt'] } } : { description: 'JSON', accept: { 'application/json': ['.json'] } }],
      });
      this.saveHandle = handle;
      element<HTMLInputElement>('save-name').value = handle.name;
      element('save-place').textContent = `will be written to ${handle.name}, where you chose`;
      this.saveNote('');
    } catch (error) {
      // the person closed the dialog: nothing changes
      if (!(error instanceof DOMException && error.name === 'AbortError')) this.saveNote(`could not choose the file: ${String(error)}`, true);
    }
  }

  /** The whole value, rendered as the command line prints it; null (with the reason shown) when it cannot be made. */
  private async makeOutput(format: ExportFormat): Promise<string | null> {
    const request = this.request();
    let answer: ExportResponse;
    try {
      if (this.serverInput && this.local) answer = await this.local.export(request, format, new AbortController().signal);
      else if (this.bytes) answer = await this.client.export(request, format);
      else return null;
    } catch (error) {
      this.ended(error);
      this.saveNote('the value could not be made', true);
      return null;
    }
    if (!answer.ok || answer.output === null) {
      const problems = [...answer.diagnostics];
      if (answer.error) problems.push(answer.error);
      this.problems.show(problems);
      this.saveNote('there is nothing to save: the parse did not work', true);
      return null;
    }
    return answer.output;
  }

  /** *Save*: write to the chosen file, or download under the name in the dialog. */
  private async save(): Promise<void> {
    const format = this.saveFormat();
    const output = await this.makeOutput(format === 'text' ? 'text' : 'pretty');
    if (output === null) return;
    const typed = element<HTMLInputElement>('save-name').value.trim().replace(/[\\/]/g, '_');
    const name = typed === '' ? this.defaultName(format) : typed;
    try {
      if (this.saveHandle) {
        const writable = await this.saveHandle.createWritable();
        await writable.write(output);
        await writable.close();
      } else {
        const link = document.createElement('a');
        link.href = URL.createObjectURL(new Blob([output], { type: format === 'text' ? 'text/plain;charset=utf-8' : 'application/json;charset=utf-8' }));
        link.download = name;
        link.click();
        setTimeout(() => URL.revokeObjectURL(link.href), 10_000);
      }
    } catch (error) {
      this.saveNote(`could not save: ${String(error)}`, true);
      return;
    }
    element<HTMLDialogElement>('save-dialog').close();
    element('status').textContent = `saved ${this.saveHandle?.name ?? name}`;
  }

  /** The page is served by `doe --ide`: offer its types and the file named on the command line. */
  private async showLocal(local: LocalApi): Promise<void> {
    const input = local.info.input;
    if (input) {
      const button = element('use-input');
      button.hidden = false;
      button.textContent = `Use ${input.name}`;
      button.title = `${input.size} bytes, read on this machine`;
      button.addEventListener('click', () => this.useServerInput(local, input));
    }
    try {
      const types = await local.types();
      this.localTypes = types;
      if (types.length === 0) return;
      const select = element<HTMLSelectElement>('types');
      select.innerHTML = '<option value="">choose…</option>';
      for (const type of types) {
        const option = document.createElement('option');
        option.value = type.path;
        option.textContent = type.doc ? `${type.id} — ${type.doc}` : type.id;
        select.append(option);
      }
      element('types-box').hidden = false;
      select.addEventListener('change', () => {
        if (select.value) void this.openType(local, select.value);
      });
    } catch (error) {
      this.problems.show([{ severity: 'error', kind: 'request', message: `could not list the types: ${String(error)}`, position: null }]);
    }
  }

  /** Put a type of this machine, with the files it imports, in the editor. */
  async openType(local: LocalApi, path: string): Promise<void> {
    const schema = await local.schema(path);
    this.files.clear();
    this.origins.clear();
    for (const [name, text] of Object.entries(schema.files)) {
      this.files.set(name, text);
      this.origins.set(name, 'library');
    }
    this.schemaPath = schema.path;
    this.includePaths = schema.include_paths;
    this.mainOrigin = 'library';
    this.parseType = null;
    this.found = null;
    this.setMain(schema.text);
    this.refreshNow();
  }

  private useServerInput(local: LocalApi, input: { name: string; size: number }): void {
    element<HTMLButtonElement>('detect').disabled = false;
    this.serverInput = input;
    this.bytes = null;
    this.inputName = input.name;
    this.selection = null;
    this.hex.setBytes(new RemoteBytes(input.size, (offset, len) => local.range(offset, len), () => this.hex.repaint()));
    this.refreshNow();
  }

  private setInput(bytes: Uint8Array, name: string): void {
    this.serverInput = null;
    this.lastInputName = null;
    this.bytes = bytes;
    this.inputName = name;
    this.client.setInput(bytes);
    this.hex.setBytes(bytes);
    element<HTMLButtonElement>('detect').disabled = false;
  }

  // ── the keys ───────────────────────────────────────────────────────────────

  private lastPane: PaneName = 'parsed';
  /** The order of the panes in the places of the page, and the ones that are closed (layout.ts). */
  private layout: Layout = loadLayout();
  /** Where the last search of the bytes found its match. */
  private byteFound: number | null = null;
  private searches: Record<'parsed' | 'bytes' | 'status', PaneSearch> | null = null;

  /** Set up the search bars, the keys and what the buttons say about their keys. */
  private keys(): void {
    const bar = (id: string): HTMLElement => element(id).querySelector<HTMLElement>('.pane-title') ?? element(id);
    this.searches = {
      parsed: new PaneSearch(bar('pane-tree'), 'the parsed value', this.parsedSearch(), () => this.focusPane('parsed')),
      bytes: new PaneSearch(bar('pane-hex'), 'the bytes', this.bytesSearch(), () => this.focusPane('bytes')),
      status: new PaneSearch(bar('pane-status'), 'the status', this.statusSearch(), () => this.focusPane('status')),
    };
    for (const name of PANES) {
      element(PANE_IDS[name]).addEventListener('focusin', () => {
        this.lastPane = name;
      });
    }
    this.layoutSetup();
    const buttons: Record<string, Action> = {
      'open-input': 'open-file',
      'open-schema': 'open-schema',
      'add-files': 'imports',
      browse: 'browse',
      detect: 'detect',
      save: 'save',
      help: 'help',
      'theme-button': 'theme',
      cancel: 'cancel',
      'detect-auto': 'auto',
      radix: 'radix',
    };
    for (const [id, action] of Object.entries(buttons)) {
      const key = keyOf(action);
      if (!key) continue;
      const control = element(id);
      const target = control.closest('label') ?? control;
      target.title = target.title ? `${target.title} (${key})` : `${target.textContent?.trim() || id} (${key})`;
      control.setAttribute('aria-keyshortcuts', key);
    }
    element('theme-button').addEventListener('click', () => this.perform('theme'));
    element('theme-close').addEventListener('click', () => element<HTMLDialogElement>('theme-dialog').close());
    element('keys-close').addEventListener('click', () => element<HTMLDialogElement>('keys-dialog').close());
    document.addEventListener('keydown', (event) => this.onKey(event));
  }

  private onKey(event: KeyboardEvent): void {
    if (event.defaultPrevented || event.isComposing || event.repeat) return;
    if (event.ctrlKey || event.metaKey) return;
    // Alt works only with Shift and a letter: the keys that move a pane
    if (event.altKey && !(event.shiftKey && keyName(event).startsWith('Alt+Shift+'))) return;
    if (document.querySelector('dialog[open]')) return;
    if (event.key === 'Escape') {
      // out of the editor, to its pane, so that the keys work again
      if ((event.target as Element | null)?.closest('.cm-editor')) this.focusPane('schema', true);
      else this.restoreMaximized();
      return;
    }
    const name = keyName(event);
    const entry = KEYS.find((candidate) => candidate.key === name);
    if (!entry) return;
    if (isField(event.target) && !entry.inFields) return;
    if ((name === 'Tab' || name === 'Shift+Tab') && !this.onPane(event.target)) return;
    if (entry.needs && !this.available(entry.needs)) return;
    event.preventDefault();
    this.perform(entry.action);
  }

  /** Whether the focus is on a pane or its main part (not on a button or a field of it), where Tab goes on to the next pane. */
  private onPane(target: EventTarget | null): boolean {
    return target === document.body || (target instanceof Element && target.matches('section, .tree, .hex'));
  }

  private available(needs: Needs): boolean {
    if (needs === 'file') return this.canDetect();
    if (needs === 'parsed') return !element<HTMLButtonElement>('save').disabled;
    return !element('cancel').hidden;
  }

  private currentPane(): PaneName {
    const active = document.activeElement;
    for (const name of PANES) if (active && element(PANE_IDS[name]).contains(active)) return name;
    return this.isShown(this.lastPane) ? this.lastPane : (shown(this.layout)[0] ?? 'schema');
  }

  /** Put the focus in a pane. `plain`: on the pane itself, not on what is in it (out of the editor). */
  private focusPane(name: PaneName, plain = false): void {
    this.lastPane = name;
    const section = element(PANE_IDS[name]);
    if (plain) section.focus();
    else if (name === 'schema') this.editor.focus();
    else if (name === 'parsed') this.tree.focus();
    else if (name === 'bytes') this.hex.focus();
    else section.focus();
  }

  private cyclePane(step: 1 | -1): void {
    const open = shown(this.layout);
    const at = open.indexOf(this.currentPane());
    this.focusPane(open[(at + step + open.length) % open.length] ?? 'schema');
  }

  /**
   * Focus the pane that lies in a direction from the current one, as the page is laid out now (one column, two or
   * three): the nearest pane whose box is entirely past this one's edge, preferring the one that shares the most of
   * its width or height. At the edge nothing happens.
   */
  private movePane(dx: number, dy: number): void {
    const target = this.neighbour(dx, dy);
    if (target) this.focusPane(target);
  }

  /** The pane that lies in a direction from the current one (see `movePane`), or null at the edge. */
  private neighbour(dx: number, dy: number): PaneName | null {
    const here = this.currentPane();
    const from = element(PANE_IDS[here]).getBoundingClientRect();
    let best: { name: PaneName; gap: number; overlap: number } | null = null;
    for (const name of PANES) {
      if (name === here) continue;
      const box = element(PANE_IDS[name]).getBoundingClientRect();
      if (box.width === 0 || box.height === 0) continue;
      const gap = dx > 0 ? box.left - from.right : dx < 0 ? from.left - box.right : dy > 0 ? box.top - from.bottom : from.top - box.bottom;
      if (gap < -1) continue;
      const overlap = dx !== 0 ? Math.min(from.bottom, box.bottom) - Math.max(from.top, box.top) : Math.min(from.right, box.right) - Math.max(from.left, box.left);
      if (overlap <= 0) continue;
      if (!best || gap < best.gap - 1 || (Math.abs(gap - best.gap) <= 1 && overlap > best.overlap)) best = { name, gap, overlap };
    }
    return best ? best.name : null;
  }

  // ── the layout ─────────────────────────────────────────────────────────────

  /** Put the panes where they were left, and make them draggable by their titles. */
  private layoutSetup(): void {
    applyLayout(document.querySelector('main') as HTMLElement, PANE_IDS, this.layout);
    this.layoutChanged(false);
    element('layout-reset').addEventListener('click', () => this.resetLayout());
    element('panes-button').addEventListener('click', () => this.perform('panes'));
    element('panes-close').addEventListener('click', () => element<HTMLDialogElement>('panes-dialog').close());
    // every pane has a button at the top right of its title bar that closes it
    for (const name of PANES) {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'pane-close icon';
      button.setAttribute('aria-label', `Close the ${NAMES[name]} pane`);
      button.title = `Close the ${NAMES[name]} pane (x)`;
      button.innerHTML = '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true" focusable="false"><path fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" d="M5 5l14 14M19 5L5 19"/></svg>';
      button.addEventListener('click', () => this.setVisible(name, false));
      const title = element(PANE_IDS[name]).querySelector<HTMLElement>('.pane-title');
      // the maximize button comes before the close button, and the title bar maximizes with a double-click
      const maximize = document.createElement('button');
      maximize.type = 'button';
      maximize.className = 'pane-maximize icon';
      maximize.dataset.pane = name;
      maximize.addEventListener('click', () => this.toggleMaximized(name));
      title?.append(maximize, button);
      title?.addEventListener('dblclick', (event) => {
        // the empty part of the bar and its heading, not a button, a box or the search bar
        if (event.target instanceof Element && event.target.closest('button, input, select, textarea, label, a, .pane-search')) return;
        window.getSelection()?.removeAllRanges();
        this.toggleMaximized(name);
      });
      this.maximizeButton(name);
    }
    let dragged: PaneName | null = null;
    for (const name of PANES) {
      const section = element(PANE_IDS[name]);
      const handle = section.querySelector<HTMLElement>('.pane-title h2');
      if (!handle) continue;
      handle.draggable = true;
      handle.title = 'Drag onto another pane to swap their places (or Alt+Shift+h, j, k, l)';
      handle.addEventListener('dragstart', (event) => {
        dragged = name;
        section.classList.add('dragging');
        event.dataTransfer?.setData('text/plain', name);
        if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move';
      });
      handle.addEventListener('dragend', () => {
        dragged = null;
        for (const other of PANES) element(PANE_IDS[other]).classList.remove('dragging', 'drop-target');
      });
      section.addEventListener('dragover', (event) => {
        if (dragged === null || dragged === name) return;
        event.preventDefault();
        section.classList.add('drop-target');
      });
      section.addEventListener('dragleave', (event) => {
        if (!(event.relatedTarget instanceof Node) || !section.contains(event.relatedTarget)) section.classList.remove('drop-target');
      });
      section.addEventListener('drop', (event) => {
        event.preventDefault();
        section.classList.remove('drop-target');
        if (dragged !== null && dragged !== name) this.swapPanes(dragged, name);
      });
    }
  }

  /** Use a new layout: put the panes in it, keep it, say what happened, and give the focus back (moving elements drops it). */
  private commit(layout: Layout, message: string, focus?: PaneName): void {
    const had = document.activeElement;
    this.layout = layout;
    applyLayout(document.querySelector('main') as HTMLElement, PANE_IDS, layout);
    saveLayout(layout);
    this.layoutChanged(true, message);
    // moving elements in the page drops the focus: give it back to what had it, or to the pane that was asked for
    if (focus) this.focusPane(focus);
    else if (had instanceof HTMLElement && had !== document.activeElement && document.contains(had) && !had.closest('[hidden]')) had.focus();
    this.panesList();
  }

  private swapPanes(a: PaneName, b: PaneName): void {
    this.commit(swapped(this.layout, a, b), `${NAMES[a]} and ${NAMES[b]} swapped places`);
  }

  /** The pane moved one place in a direction: it takes the place of the pane that is there. */
  private moveBy(dx: number, dy: number): void {
    const here = this.currentPane();
    const target = this.neighbour(dx, dy);
    if (target) this.swapPanes(here, target);
  }

  private resetLayout(): void {
    if (isDefault(this.layout)) return;
    this.commit(defaultLayout(), 'The panes are back in their places');
  }

  /** Show the reset button when the layout is not the default, and say what happened to a screen reader. */
  private layoutChanged(announce: boolean, message = ''): void {
    element('layout-reset').hidden = isDefault(this.layout);
    if (announce) element('layout-live').textContent = message;
  }

  private isShown(name: PaneName): boolean {
    return !this.layout.hidden.includes(name);
  }

  /** Close or show a pane. Schema stays; when the pane that has the focus is closed, the focus goes to the next one that is shown. */
  private setVisible(name: PaneName, visible: boolean, focusIt = false): void {
    if (this.isShown(name) === visible) {
      if (visible && focusIt) this.focusPane(name);
      return;
    }
    if (!visible && this.maximized === name) this.restoreMaximized();
    const next = withVisible(this.layout, name, visible);
    let focus: PaneName | undefined;
    // the pane that has the focus (or had it last) is closed: the focus goes to the next one that is shown
    if (!visible && (element(PANE_IDS[name]).contains(document.activeElement) || (this.lastPane === name && document.activeElement === document.body))) {
      const open = shown(this.layout);
      focus = open[(open.indexOf(name) + 1) % open.length];
      // the last pane that was shown: the focus goes to the button that opens the list of the panes
      if (focus === name || focus === undefined) {
        focus = undefined;
        element('panes-button').focus();
      }
    }
    if (visible && focusIt) focus = name;
    this.commit(next, `The ${NAMES[name]} pane is ${visible ? 'shown' : 'closed'}`, focus);
  }

  /** Show a pane if it was closed (a message must not be lost), without taking the focus. */
  private reveal(name: PaneName): void {
    if (!this.isShown(name)) this.setVisible(name, true);
  }

  /** The pane that fills the page, if one does. It is a state of the view, not of the layout: it is not kept. */
  private maximized: PaneName | null = null;

  /** Draw the button of a pane: the icon and the name say maximize, or restore when the pane is maximized. */
  private maximizeButton(name: PaneName): void {
    const button = element(PANE_IDS[name]).querySelector<HTMLButtonElement>('.pane-maximize');
    if (!button) return;
    const on = this.maximized === name;
    const label = `${on ? 'Restore' : 'Maximize'} the ${NAMES[name]} pane`;
    button.setAttribute('aria-label', label);
    button.title = `${label} (f)`;
    button.setAttribute('aria-pressed', String(on));
    // the conventional icons: one square to maximize, two overlapping squares to restore
    button.innerHTML = on
      ? '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true" focusable="false"><path fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round" d="M8 8V4h12v12h-4M4 8h12v12H4z"/></svg>'
      : '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true" focusable="false"><rect x="4" y="4" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round"/></svg>';
  }

  /** Make a pane fill the page, or put the page back. The other panes are hidden by the style sheet, not closed. */
  private toggleMaximized(name: PaneName): void {
    if (!this.isShown(name)) return;
    const was = this.maximized;
    this.maximized = was === name ? null : name;
    const main = document.querySelector('main') as HTMLElement;
    main.classList.toggle('has-max', this.maximized !== null);
    for (const pane of PANES) {
      element(PANE_IDS[pane]).classList.toggle('maximized', this.maximized === pane);
      this.maximizeButton(pane);
    }
    element('layout-live').textContent = this.maximized ? `${NAMES[name]} maximized` : `${NAMES[name]} restored`;
    // the focus stays in the pane, which is the one that is on the screen
    this.focusPane(name);
    element(PANE_IDS[name]).scrollIntoView({ block: 'nearest' });
  }

  /** Put the page back after a maximized pane, when something needs the others. */
  private restoreMaximized(): void {
    if (this.maximized) this.toggleMaximized(this.maximized);
  }

  /** The key x: it closes the focused pane, but not Schema (its button does), so that the key cannot close the editor by mistake. */
  private closeFocusedPane(): void {
    const pane = this.currentPane();
    if (pane === 'schema') {
      element('layout-live').textContent = 'x does not close the Schema pane: use its X button, or v';
      return;
    }
    this.setVisible(pane, false);
  }

  /** The list in the dialog of the panes: a check box for each, the state in words. */
  private panesList(): void {
    const list = element('panes-list');
    const rows = PANES.map((name) => {
      const item = document.createElement('li');
      const label = document.createElement('label');
      const box = document.createElement('input');
      box.type = 'checkbox';
      box.checked = this.isShown(name);
      box.dataset.pane = name;
      box.addEventListener('change', () => this.setVisible(name, box.checked));
      const state = document.createElement('span');
      state.className = 'state';
      state.textContent = this.isShown(name) ? 'shown' : 'closed';
      label.append(box, document.createTextNode(NAMES[name]), state);
      item.append(label);
      return item;
    });
    // keep the focus on the box that was changed
    const active = document.activeElement instanceof HTMLInputElement ? document.activeElement.dataset.pane : undefined;
    list.replaceChildren(...rows);
    if (active) list.querySelector<HTMLInputElement>(`input[data-pane="${active}"]`)?.focus();
  }

  private perform(action: Action): void {
    const click = (id: string): void => element(id).click();
    // while a pane is maximized the keys that go to the other panes do nothing, except the ones that name a pane: they
    // put the page back first (f and Esc put it back too)
    if (this.maximized && ['next-pane', 'previous-pane', 'pane-left', 'pane-down', 'pane-up', 'pane-right', 'move-left', 'move-down', 'move-up', 'move-right'].includes(action)) {
      element('layout-live').textContent = 'A pane is maximized: press f or Esc to restore the others';
      return;
    }
    const named: Partial<Record<Action, PaneName>> = { 'pane-schema': 'schema', 'pane-parsed': 'parsed', 'pane-bytes': 'bytes', 'pane-status': 'status' };
    if (this.maximized && named[action] && named[action] !== this.maximized) this.restoreMaximized();
    switch (action) {
      case 'help':
        void this.help.show();
        break;
      case 'keys':
        this.showKeys();
        break;
      case 'theme':
        element('theme-note').textContent = '';
        element<HTMLDialogElement>('theme-dialog').showModal();
        break;
      case 'open-file':
        click('open-input');
        break;
      case 'open-schema':
        click('open-schema');
        break;
      case 'browse':
        click('browse');
        break;
      case 'imports':
        click('add-files');
        break;
      case 'detect':
        if (!element<HTMLButtonElement>('detect').disabled) click('detect');
        break;
      case 'save':
        this.openSave();
        break;
      case 'next-pane':
        this.cyclePane(1);
        break;
      case 'previous-pane':
        this.cyclePane(-1);
        break;
      case 'pane-left':
        this.movePane(-1, 0);
        break;
      case 'pane-down':
        this.movePane(0, 1);
        break;
      case 'pane-up':
        this.movePane(0, -1);
        break;
      case 'pane-right':
        this.movePane(1, 0);
        break;
      case 'move-left':
        this.moveBy(-1, 0);
        break;
      case 'move-down':
        this.moveBy(0, 1);
        break;
      case 'move-up':
        this.moveBy(0, -1);
        break;
      case 'move-right':
        this.moveBy(1, 0);
        break;
      case 'reset-layout':
        this.resetLayout();
        break;
      case 'search':
        this.searchPane();
        break;
      case 'goto':
        this.reveal('bytes');
        this.searches?.bytes.open('@');
        break;
      case 'auto':
        click('detect-auto');
        break;
      case 'radix': {
        const at = RADIXES.findIndex((radix) => radix.id === this.radixes.global);
        const next = RADIXES[(at + 1) % RADIXES.length]?.id;
        if (next && isRadix(next)) {
          this.radixes.setGlobal(next);
          element<HTMLSelectElement>('radix').value = next;
        }
        break;
      }
      case 'cancel':
        click('cancel');
        break;
      case 'pane-schema':
        this.focusPane('schema');
        break;
      case 'pane-parsed':
        this.setVisible('parsed', true, true);
        this.focusPane('parsed');
        break;
      case 'pane-bytes':
        this.setVisible('bytes', true, true);
        this.focusPane('bytes');
        break;
      case 'pane-status':
        this.setVisible('status', true, true);
        this.focusPane('status');
        break;
      case 'maximize':
        this.toggleMaximized(this.currentPane());
        break;
      case 'close-pane':
        this.closeFocusedPane();
        break;
      case 'panes':
        this.panesList();
        element<HTMLDialogElement>('panes-dialog').showModal();
        break;
    }
  }

  private searchPane(): void {
    const pane = this.currentPane();
    if (pane === 'schema') this.editor.search();
    else this.searches?.[pane].open();
  }

  /** The list of the keys, with the ones that need something that is not there marked. */
  private showKeys(): void {
    const needs: Record<Needs, string> = { file: 'needs an open file', parsed: 'needs a parsed value', running: 'needs a parse that is running' };
    const rows = KEYS.map((entry) => {
      const missing = entry.needs && !this.available(entry.needs) ? needs[entry.needs] : '';
      const row = document.createElement('tr');
      if (missing) row.className = 'unavailable';
      const head = document.createElement('th');
      head.scope = 'row';
      const kbd = document.createElement('kbd');
      kbd.textContent = entry.key;
      head.append(kbd);
      const label = document.createElement('td');
      label.textContent = entry.label;
      const why = document.createElement('td');
      why.className = 'why';
      why.textContent = missing ? `not now: ${missing}` : '';
      row.append(head, label, why);
      return row;
    });
    element('keys-table').replaceChildren(...rows);
    element<HTMLDialogElement>('keys-dialog').showModal();
  }

  private parsedSearch(): SearchHandler {
    return {
      placeholder: 'name, type or value',
      find: (query, direction) => {
        if (!this.trace) return 'nothing is parsed';
        const found = findNode(this.trace, query, direction === 0 ? null : this.tree.current, direction === 0 ? 1 : direction);
        if (!found) return 'no match';
        this.select(found.path, 'tree');
        this.tree.select(found.path, true);
        return `${found.count} ${found.count === 1 ? 'match' : 'matches'}`;
      },
      closed: () => undefined,
    };
  }

  private bytesSearch(): SearchHandler {
    return {
      placeholder: 'hex (de ad), text, or @offset (@#500 for decimal)',
      find: (query, direction) => {
        const source = this.hex.source;
        if (source.length === 0) return 'no file is open';
        const parsed = byteQuery(query);
        if (!parsed) return 'not an offset, hexadecimal bytes or text';
        const mark = (start: number, length: number): void => {
          this.byteFound = start;
          this.hex.setSelection({ first: start, end: start + length, startBit: start * 8, endBit: (start + length) * 8 });
          this.hex.reveal(start);
        };
        if (parsed.kind === 'offset') {
          if (parsed.offset >= source.length) return `past the end (${source.length.toString(16)} bytes)`;
          mark(parsed.offset, 1);
          return `offset ${parsed.offset.toString(16)}`;
        }
        const from = direction === 0 ? -1 : (this.byteFound ?? -1);
        const at = findPattern(source, parsed.pattern, from, direction === 0 ? 1 : direction);
        if (at === null) return 'not found';
        mark(at, parsed.pattern.length);
        return `at ${at.toString(16)}`;
      },
      closed: () => {
        this.byteFound = null;
      },
    };
  }

  private statusSearch(): SearchHandler {
    const lines = (): HTMLElement[] => [...document.querySelectorAll<HTMLElement>('#pane-status .problem, #pane-status #detect-report li')];
    return {
      placeholder: 'in the problems and the report',
      find: (query) => {
        const wanted = query.trim().toLowerCase();
        let shown = 0;
        for (const line of lines()) {
          const hit = line.textContent?.toLowerCase().includes(wanted) ?? false;
          line.classList.toggle('search-miss', !hit);
          if (hit) shown += 1;
        }
        return `${shown} of ${lines().length} shown`;
      },
      closed: () => {
        for (const line of lines()) line.classList.remove('search-miss');
      },
    };
  }

  // ── the controls ───────────────────────────────────────────────────────────

  private wire(): void {
    const input = (id: string, handler: (files: File[]) => void): void => {
      const control = element<HTMLInputElement>(id);
      control.addEventListener('change', () => {
        handler([...(control.files ?? [])]);
        control.value = '';
      });
    };
    element('browse').addEventListener('click', () => this.browseTypes());
    input('open-schema', (files) => void this.openSchema(files[0]));
    input('add-files', (files) => void this.addFiles(files));
    input('open-input', (files) => void this.openInput(files[0]));
    input('open-theme', (files) => void this.openTheme(files[0]));
    element<HTMLSelectElement>('theme').addEventListener('change', (event) => {
      this.themes.choose((event.target as HTMLSelectElement).value);
    });
    element<HTMLSelectElement>('radix').addEventListener('change', (event) => {
      const radix = (event.target as HTMLSelectElement).value;
      if (isRadix(radix)) this.radixes.setGlobal(radix);
    });
    element<HTMLSelectElement>('field-radix').addEventListener('change', (event) => {
      const key = this.selectedField();
      if (key === null) return;
      const radix = (event.target as HTMLSelectElement).value;
      this.radixes.setField(key, isRadix(radix) ? radix : null);
      this.persist();
    });
    element('cancel').addEventListener('click', () => {
      if (this.detection) this.detection.cancelled = true;
      this.client.cancel();
      this.abort?.abort();
    });
    element('detect').addEventListener('click', () => void this.detect(false));
    const auto = element<HTMLInputElement>('detect-auto');
    auto.checked = this.autoDetect();
    auto.addEventListener('change', () => {
      try {
        localStorage.setItem('doe.detect', auto.checked ? '1' : '0');
      } catch {
        // no storage: the choice is not remembered
      }
    });
    element('save').addEventListener('click', () => this.openSave());
    element('save-go').addEventListener('click', () => void this.save());
    element('save-cancel').addEventListener('click', () => element<HTMLDialogElement>('save-dialog').close());
    element('save-browse').addEventListener('click', () => void this.browseSave());
    for (const radio of document.querySelectorAll<HTMLInputElement>('input[name="save-format"]')) radio.addEventListener('change', () => this.saveFormatChanged());

    window.addEventListener('dragover', (event) => event.preventDefault());
    window.addEventListener('drop', (event) => {
      event.preventDefault();
      void this.dropped([...(event.dataTransfer?.files ?? [])]);
    });
  }

  /** Schema files that are dropped are imports; anything else is the input. */
  private async dropped(files: File[]): Promise<void> {
    const schemas = files.filter((file) => isSchemaFile(file.name));
    const others = files.filter((file) => !isSchemaFile(file.name));
    if (schemas.length > 0) await this.addFiles(schemas);
    if (others[0]) await this.openInput(others[0]);
  }

  async openSchema(file: File | undefined): Promise<void> {
    if (!file) return;
    this.schemaPath = file.name;
    this.mainOrigin = 'file';
    this.parseType = null;
    this.found = null;
    this.setMain(await file.text());
    this.refreshNow();
  }

  async addFiles(files: File[]): Promise<void> {
    for (const file of files) {
      this.files.set(file.name, await file.text());
      this.origins.set(file.name, 'added');
    }
    this.showFiles();
    this.refreshNow();
  }

  async openInput(file: File | undefined): Promise<void> {
    if (!file) return;
    const refusal = checkSize(file.size);
    if (refusal) {
      this.notice = { severity: 'error', kind: 'input', message: refusal, position: null };
      this.refreshNow();
      return;
    }
    this.notice = null;
    this.selection = null;
    this.setInput(new Uint8Array(await file.arrayBuffer()), file.name);
    if (this.autoDetect() && this.noSchema()) {
      // nothing is in the editor: no use trying it
      void this.detect(false);
      return;
    }
    this.detectIfItFails = this.autoDetect();
    this.refreshNow();
  }

  /** Whether there is no schema yet: the editor is empty and nothing is imported. */
  private noSchema(): boolean {
    return this.mainText.trim() === '' && this.files.size === 0;
  }

  // ── the browser of types ───────────────────────────────────────────────────

  /** Open the list of every type that can be used: the open schemas, this machine's library, doe's own library and the gallery. */
  private browseTypes(): void {
    const rows = listing(this.found, this.schemaPath, this.mainOrigin, this.files, this.origins, this.diagnostics.length).rows;
    const current = this.mainOrigin === 'gallery' || this.mainOrigin === 'bundled' ? this.schemaPath : null;
    const entries = [
      ...openEntries(rows, this.parseType),
      ...libraryEntries(this.localTypes).map((entry) => ({ ...entry, current: this.mainOrigin === 'library' && entry.path === this.schemaPath })),
      ...galleryEntries(this.gallery?.formats ?? [], current),
    ];
    this.browser.open(entries);
    void this.readSignatures(entries);
  }

  /** Find out what the open and library types start with, a few at a time, and show it as it comes. */
  private async readSignatures(entries: Entry[]): Promise<void> {
    const changed = (): void => {
      if (this.browser.isOpen) this.browser.refresh(entries);
    };
    const library = entries.filter((e) => e.source === 'library');
    const fromLibrary = this.local && library.length > 0 ? this.local.signatures().catch(() => []) : Promise.resolve([]);
    for (const entry of entries.filter((e) => e.source === 'open')) {
      if (!this.browser.isOpen) return;
      try {
        const answer = await this.client.signature({ ...this.request(), type: entry.id });
        entry.magic = answer.ok ? answer.magic : [];
        entry.names = [...new Set([...entry.names, ...answer.names])];
        entry.compileError = answer.ok ? null : (answer.diagnostics[0]?.message ?? 'the schema does not compile');
      } catch {
        entry.magic = [];
      }
      changed();
    }
    const signatures = await fromLibrary;
    for (const entry of library) {
      const known = signatures.find((s) => s.path === entry.path);
      entry.magic = known?.magic ?? [];
      entry.names = known?.names ?? entry.names;
      entry.compileError = known?.error ?? null;
    }
    changed();
  }

  /** Put the entries in the order in which the library would try them on the opened file. */
  private async rankEntries(entries: readonly Entry[]): Promise<Ranking | null> {
    if (!this.canDetect()) return null;
    try {
      if (this.serverInput && this.local) this.client.setInput(await this.local.range(0, 64 * 1024));
      const sources = entries.map((e) => ({ name: e.id, magic: e.magic ?? [], names: e.names }));
      const answer = await this.client.order(sources, this.inputName, false);
      return answer.ok ? { order: answer.order, excluded: answer.excluded } : null;
    } catch {
      return null;
    }
  }

  /** A type was chosen in the browser. */
  private async pickEntry(entry: Entry): Promise<void> {
    if (entry.source === 'open') {
      this.chooseType(entry.id);
    } else if (entry.source === 'library' && this.local) {
      await this.openType(this.local, entry.path);
    } else if (entry.source === 'gallery' || entry.source === 'bundled') {
      const format = this.gallery?.formats.find((f) => f.path === entry.path);
      if (format) await this.openGallery(format);
    }
  }

  /** The text of the schema of an entry, to read. */
  private async entryText(entry: Entry): Promise<string> {
    if (entry.source === 'open') {
      const text = entry.path === this.schemaPath ? (this.active === null ? this.editor.text : this.mainText) : this.files.get(entry.path);
      if (text === undefined) throw new Error('the schema is not open any more');
      return text;
    }
    if (entry.source === 'library') {
      if (!this.local) throw new Error('there is no library on this machine');
      return (await this.local.schema(entry.path)).text;
    }
    return this.fetchGallery(entry.path);
  }

  /** The list of the open schemas and the choice of the type. */
  private showFiles(): void {
    const found = listing(this.found, this.schemaPath, this.mainOrigin, this.files, this.origins, this.diagnostics.length);
    this.schemas.show(found, this.active ?? this.schemaPath, this.parseType);
    element('editing').textContent = this.active === null ? this.schemaPath : `${this.active} (an import)`;
    const id = parsedAs(this.parseType, found.rows, this.lastId, !this.noSchema());
    const parsed = element('parsed-as');
    parsed.textContent = id;
    parsed.title = id ? `the file is parsed as ${id}` : '';
  }

  /** The editor changed: the text belongs to the file that is shown. */
  private edited(): void {
    if (this.switching) return;
    const text = this.editor.text;
    if (this.active === null) this.mainText = text;
    else this.files.set(this.active, text);
    this.later();
  }

  /** Show a schema of the list in the editor. */
  private showFile(path: string): void {
    const main = path === this.schemaPath;
    const text = main ? this.mainText : this.files.get(path);
    if (text === undefined) return;
    this.active = main ? null : path;
    const origin = main ? this.mainOrigin : (this.origins.get(path) ?? 'added');
    this.switching = true;
    this.editor.setText(text);
    this.switching = false;
    this.editor.setReadOnly(!editable(origin, main));
    this.showFiles();
    this.showMarks();
  }

  /** Underline the problems of the schema that is shown. */
  private showMarks(): void {
    if (this.active === null) this.editor.showDiagnostics(this.diagnostics);
    else {
      const error = this.found?.schemas.find((info) => info.path === this.active)?.error;
      this.editor.showDiagnostics(error ? [error] : []);
    }
  }

  private removeFile(path: string): void {
    if (this.active === path) this.showFile(this.schemaPath);
    this.files.delete(path);
    this.origins.delete(path);
    this.showFiles();
    this.refresh();
  }

  /** A schema that was fetched becomes the person's own, to edit. */
  private copyFile(path: string): void {
    this.origins.set(path, 'added');
    this.showFile(path);
    this.persist();
  }

  private chooseType(type: string): void {
    this.parseType = type === '' ? null : type;
    this.refreshNow();
  }

  /** Ask which schemas the request is made of, and show them. A newer run replaces this one's answer. */
  private inspect(request: Request, mine: number): void {
    this.client.inspect(request).then(
      (found) => {
        if (mine !== this.generation) return;
        this.found = found;
        // a type that is no longer there is dropped, once, and the run is repeated
        const rows = listing(found, this.schemaPath, this.mainOrigin, this.files, this.origins, 0).rows;
        const readable = found.ok && found.schemas.every((info) => !info.error);
        if (this.parseType && readable && !isKnownType(rows, this.parseType)) {
          this.parseType = null;
          this.refreshNow();
          return;
        }
        this.showFiles();
        this.showMarks();
      },
      () => {
        // no list this time: the page keeps the last one
      },
    );
  }

  // ── the work ───────────────────────────────────────────────────────────────

  /** Check again once the schema has been left alone for a moment. */
  private later(): void {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.refresh(), PAUSE_MS);
  }

  private refreshNow(): void {
    clearTimeout(this.timer);
    this.refresh();
  }

  private request(): Request {
    const request: Request = { schema: this.mainText, files: Object.fromEntries(this.files), path: this.schemaPath };
    if (this.includePaths) request.include_paths = this.includePaths;
    if (this.parseType) request.type = this.parseType;
    return request;
  }

  /** Run the schema on the input. Only the newest run shows its result. */
  private refresh(): void {
    const mine = ++this.generation;
    if (this.noSchema()) {
      this.refreshEmpty();
      return;
    }
    const cancel = element('cancel');
    cancel.hidden = false;
    element('status').textContent = 'parsing…';
    const request = this.request();
    this.persist();
    element<HTMLButtonElement>('save').disabled = true;
    this.abort?.abort();
    this.abort = null;
    let run: Promise<ParseResponse>;
    if (this.serverInput && this.local) {
      // The file is read by the server, as large as it is. A new run drops
      // this one's answer; the server finishes it, and answers "busy" to
      // anything that comes meanwhile, which is said and retried below.
      const abort = new AbortController();
      this.abort = abort;
      this.client.cancel();
      run = this.local.parse(request, abort.signal);
    } else if (this.bytes) run = this.client.parse(request);
    else run = this.client.check(request).then((checked) => ({ ...checked, value: null, trace: null, error: null, problems: [] }));
    run.then(
      (response) => {
        if (mine !== this.generation) return;
        cancel.hidden = true;
        element<HTMLButtonElement>('save').disabled = !(response.ok && (this.bytes || this.serverInput));
        this.show(response);
        this.inspect(request, mine);
      },
      (error: unknown) => {
        // Ended by a newer run: that one will show.
        if (mine !== this.generation) return;
        if (error instanceof Error && error.message.includes('already running')) {
          // The server is still on an earlier run of the same file.
          setTimeout(() => mine === this.generation && this.refresh(), 500);
          return;
        }
        cancel.hidden = true;
        this.ended(error);
      },
    );
  }

  /** Nothing to run: there is no schema. That is not a mistake, so there is no problem to show. */
  private refreshEmpty(): void {
    element('cancel').hidden = true;
    this.persist();
    element<HTMLButtonElement>('save').disabled = true;
    this.abort?.abort();
    this.abort = null;
    this.client.cancel();
    this.found = null;
    this.show({ ok: true, id: '', diagnostics: [], value: null, trace: null, error: null, problems: [] });
    const size = this.serverInput?.size ?? this.bytes?.length;
    const file = this.inputName && size !== undefined ? `${this.inputName}, ${size} bytes` : 'no file opened';
    element('status').textContent = `${file} · type a schema, open one, or pick a type`;
  }

  /** A run that gave no answer: the page keeps what it showed, and says why. */
  private ended(error: unknown): void {
    const aborted = error instanceof DOMException && error.name === 'AbortError';
    const message = aborted ? 'the parse was cancelled' : error instanceof CallEnded ? (error.reason === 'timeout' ? `Stopped: ${error.message}.` : error.message) : `doe stopped: ${String(error)}`;
    this.problems.show([{ severity: 'error', kind: 'input', message, position: null }]);
    element('status').textContent = aborted || (error instanceof CallEnded && error.reason === 'cancelled') ? 'cancelled' : 'stopped';
  }

  private show(response: ParseResponse): void {
    const problems: Diagnostic[] = [...response.diagnostics];
    if (response.error) problems.push(response.error);
    problems.push(...response.problems);
    if (this.notice) problems.push(this.notice);
    this.diagnostics = response.diagnostics;
    this.lastId = response.id;
    this.problems.show(problems);
    // a parse that failed must not go unseen because the Status pane is closed
    if (!response.ok || response.error) this.reveal('status');
    this.trace = response.trace;
    this.tree.setTrace(response.trace);
    // The selection stays on a node that is still there with the same name.
    if (this.selection && this.trace) {
      const same = pathName(this.trace, this.selection) === this.selectionName;
      if (!same || !nodeAt(this.trace, this.selection)) this.selection = null;
    } else this.selection = null;
    this.select(this.selection, 'none');
    this.status(response, problems.length);
    this.showFiles();
    this.showMarks();
    if (this.detectIfItFails && (!response.ok || response.error)) void this.detect(false);
    this.detectIfItFails = false;
  }

  private selectionName = '';

  private select(path: Path | null, from: 'tree' | 'hex' | 'none'): void {
    this.selection = path;
    const node = path && this.trace ? nodeAt(this.trace, path) : null;
    this.selectionName = path && this.trace ? pathName(this.trace, path) : '';
    this.tree.select(node ? path : null, from !== 'tree');
    const range = node && !node.processed ? byteRange(node) : null;
    if (node && range && node.start !== null && node.end !== null) {
      this.hex.setSelection({ first: range.first, end: range.end, startBit: node.start, endBit: node.end });
      if (from === 'tree') this.hex.reveal(range.first);
    } else this.hex.setSelection(null);
    this.showSelection(node);
    this.showFieldRadix();
  }

  /** The name under which the radix of the selected field is kept, when it is an integer. */
  private selectedField(): string | null {
    const node = this.selection && this.trace ? nodeAt(this.trace, this.selection) : null;
    return node?.integer && this.selection ? this.tree.fieldAt(this.selection) : null;
  }

  /** Show the choice of radix for the selected field, only when it is an integer. */
  private showFieldRadix(): void {
    const key = this.selectedField();
    element('field-radix-box').hidden = key === null;
    if (key !== null) element<HTMLSelectElement>('field-radix').value = this.radixes.own(key) ?? '';
  }

  private pickByte(byte: number): void {
    if (!this.trace) return;
    this.select(findByte(this.trace, byte), 'hex');
  }

  private pickProblem(item: Diagnostic): void {
    if (item.position) {
      // the position is in the main schema
      if (this.active !== null) this.showFile(this.schemaPath);
      this.editor.reveal(item.position);
    }
    else if (item.kind === 'text' && this.trace) {
      const found = pathOfProblem(this.trace, item.message);
      if (found) this.select(found, 'tree');
    } else if (item.kind === 'parse' && this.trace) {
      const failed = this.pathToFailure(this.trace);
      if (failed) this.select(failed, 'tree');
    }
  }

  private pathToFailure(node: TraceNode, path: number[] = []): Path | null {
    if (node.failed) return path;
    for (const [index, child] of node.children.entries()) {
      const found = this.pathToFailure(child, [...path, index]);
      if (found) return found;
    }
    return null;
  }

  private status(response: ParseResponse, problems: number): void {
    const parts: string[] = [];
    if (!this.inputName) parts.push(this.lastInputName ? `open ${this.lastInputName} again` : 'no file opened');
    const size = this.serverInput?.size ?? this.bytes?.length;
    if (this.inputName && size !== undefined) parts.push(`${this.inputName}, ${size} bytes${this.serverInput ? ' (on this machine)' : ''}`);
    parts.push(response.diagnostics.length > 0 ? 'the schema has problems' : response.ok ? `parsed as ${this.parseType ?? response.id ?? '?'}` : problems > 0 ? 'the parse failed' : 'ok');
    element('status').textContent = parts.join(' · ');
  }

  private showSelection(node: TraceNode | null): void {
    const target = element('selection');
    if (!node) {
      target.textContent = '';
      return;
    }
    const range = describeRange(node);
    const note = node.processed ? ' (in transformed bytes, not the input)' : '';
    target.textContent = `${this.selectionName}${range ? ` · ${range}` : ''}${note}`;
  }
}
