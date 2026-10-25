// The page: the schema, the input, and what doe makes of them, in four panes
// that stay in step. Everything runs here, in the browser; nothing is sent
// anywhere.

import { CallEnded, DoeClient } from './client.js';
import type { Diagnostic, ExportFormat, ExportResponse, InspectResponse, ParseResponse, Request, TraceNode } from './doe.js';
import { SchemaEditor } from './editor.js';
import { EXAMPLE_BYTES, EXAMPLE_SCHEMA } from './example.js';
import { HexView, RemoteBytes } from './hex.js';
import { type LocalApi, detectLocal } from './local.js';
import { type Path, byteRange, checkSize, describeRange, findByte, nodeAt, pathName } from './model.js';
import { ProblemsView } from './problems.js';
import { type Origin, SchemaList, editable, isKnownType, listing } from './schemas.js';
import { MAX_LINK, type Work, codeOfHash, decodeWork, encodeWork, forget, recall, remember } from './share.js';
import { CUSTOM, SYSTEM, THEMES, Themes } from './theme.js';
import { Radixes, isRadix } from './radix.js';
import { type Attempt, type GalleryFormat, describe, detectGallery } from './detect.js';
import { TreeView } from './tree.js';

/** How long the schema is left alone before it is checked again. */
const PAUSE_MS = 250;

const SCHEMA_EXTENSIONS = ['.ksy', '.yaml', '.yml'];

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
  private mainText = EXAMPLE_SCHEMA;
  private mainOrigin: Origin = 'typed';
  /** The imported file the editor shows; null: the main schema. */
  private active: string | null = null;
  /** The type to parse with; null: the schema's own. */
  private parseType: string | null = null;
  /** The schemas the last request is made of. */
  private found: InspectResponse | null = null;
  private diagnostics: Diagnostic[] = [];
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

  constructor(workerUrl: URL, wasmUrl: URL) {
    this.client = new DoeClient(workerUrl, wasmUrl);
    this.editor = new SchemaEditor(element('editor'), EXAMPLE_SCHEMA, () => this.edited());
    this.schemas = new SchemaList(element('schemas'), element<HTMLSelectElement>('parse-type'), element('edit-note'), {
      select: (path) => this.showFile(path),
      remove: (path) => this.removeFile(path),
      type: (type) => this.chooseType(type),
      copy: (path) => this.copyFile(path),
    });
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
      element('status').textContent = `theme ${theme.name} loaded from ${file.name}`;
      this.showThemes();
    } catch (error) {
      element('status').textContent = `${file.name} is not a theme: ${error instanceof Error ? error.message : String(error)}`;
    }
  }

  /** Load the module and show the example. */
  async start(): Promise<DoeClient> {
    await this.client.start();
    element('version').textContent = this.client.version;
    this.local = await detectLocal();
    if (this.local) await this.showLocal(this.local);
    await this.showGallery();
    if (!(await this.restore())) this.setInput(EXAMPLE_BYTES, 'example.bin');
    this.refresh();
    return this.client;
  }

  // ── share links, saved work, the gallery, export ───────────────────────────

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

  /** Open the work of a share link, or else the work this browser remembers. No file: the person opens it. */
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

  private async share(): Promise<void> {
    const code = await encodeWork(this.work());
    const link = `${location.origin}${location.pathname}#s=${code}`;
    const status = element('status');
    if (link.length > MAX_LINK) {
      status.textContent = `the link would be ${link.length} characters: too long for a link; export or save the schema instead`;
      return;
    }
    history.replaceState(null, '', `#s=${code}`);
    try {
      await navigator.clipboard.writeText(link);
      status.textContent = `link copied (${link.length} characters; it holds the schema and imports, not the file)`;
    } catch {
      status.textContent = `the link is in the address bar (${link.length} characters; it holds the schema and imports, not the file)`;
    }
  }

  private gallery: { repo: string; commit: string; formats: GalleryFormat[] } | null = null;

  /** The formats of the Kaitai Struct gallery; their definitions are fetched when one is picked. */
  private async showGallery(): Promise<void> {
    try {
      const response = await fetch('gallery.json');
      if (!response.ok) return;
      const gallery = (await response.json()) as NonNullable<App['gallery']>;
      if (!Array.isArray(gallery.formats) || gallery.formats.length === 0 || !gallery.commit) return;
      this.gallery = gallery;
      const select = element<HTMLSelectElement>('gallery');
      select.innerHTML = '<option value="">choose…</option>';
      const groups = new Map<string, HTMLOptGroupElement>();
      for (const [index, format] of gallery.formats.entries()) {
        const category = format.path.split('/')[0] ?? '';
        let group = groups.get(category);
        if (!group) {
          group = document.createElement('optgroup');
          group.label = category;
          groups.set(category, group);
          select.append(group);
        }
        const option = document.createElement('option');
        option.value = String(index);
        const label = format.title ? `${format.id} — ${format.title}` : format.id;
        // ✓ a file written by the format's own tool is parsed in the tests; ○ a file built by hand is
        const mark = format.verified === 'real' ? ' ✓' : format.verified === 'crafted' ? ' ○' : '';
        option.textContent = label + mark;
        if (format.verified) option.title = format.verified === 'real' ? 'tried on a file written by the tool that makes the format' : 'tried on a valid file built by hand';
        group.append(option);
      }
      element('gallery-box').hidden = false;
      select.addEventListener('change', () => {
        const format = gallery.formats[Number(select.value)];
        if (select.value && format) void this.openGallery(format);
        select.value = '';
      });
    } catch {
      // no gallery.json (a build without the submodule): no picker
    }
  }

  /** A file of the gallery, at the commit the page was built with. */
  private async fetchGallery(path: string): Promise<string> {
    const gallery = this.gallery;
    if (!gallery) throw new Error('there is no gallery');
    const response = await fetch(`https://raw.githubusercontent.com/${gallery.repo}/${gallery.commit}/${path}`);
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
          origin: 'gallery',
          origins: Object.fromEntries(Object.keys(files).map((path) => [path, 'gallery' as const])),
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
        origin: 'gallery',
        origins: Object.fromEntries(Object.keys(files).map((path) => [path, 'gallery' as const])),
      });
      this.refreshNow();
    } catch (error) {
      this.problems.show([{ severity: 'error', kind: 'request', message: `could not fetch the format: ${String(error)}`, position: null }]);
      status.textContent = 'could not fetch the format';
    }
  }

  /** Save the whole value, rendered as the command line prints it. */
  private async exportOutput(): Promise<void> {
    const format = element<HTMLSelectElement>('export-format').value as ExportFormat;
    const request = this.request();
    let answer: ExportResponse;
    try {
      if (this.serverInput && this.local) answer = await this.local.export(request, format, new AbortController().signal);
      else if (this.bytes) answer = await this.client.export(request, format);
      else return;
    } catch (error) {
      this.ended(error);
      return;
    }
    if (!answer.ok || answer.output === null) {
      const problems = [...answer.diagnostics];
      if (answer.error) problems.push(answer.error);
      this.problems.show(problems);
      return;
    }
    const extension = format === 'text' ? 'txt' : 'json';
    const link = document.createElement('a');
    link.href = URL.createObjectURL(new Blob([answer.output], { type: 'text/plain;charset=utf-8' }));
    link.download = `${this.inputName ?? 'output'}.${extension}`;
    link.click();
    setTimeout(() => URL.revokeObjectURL(link.href), 10_000);
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

  // ── the controls ───────────────────────────────────────────────────────────

  private wire(): void {
    const input = (id: string, handler: (files: File[]) => void): void => {
      const control = element<HTMLInputElement>(id);
      control.addEventListener('change', () => {
        handler([...(control.files ?? [])]);
        control.value = '';
      });
    };
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
    element('share').addEventListener('click', () => void this.share());
    element('export').addEventListener('click', () => void this.exportOutput());
    element('forget').addEventListener('click', () => {
      forget();
      element('status').textContent = 'the saved schema is forgotten';
    });
    element('example').addEventListener('click', () => this.loadExample());

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
    if (this.autoDetect() && this.mainText === EXAMPLE_SCHEMA) {
      // nothing of the person's own is in the editor: no use trying the example
      void this.detect(false);
      return;
    }
    this.detectIfItFails = this.autoDetect();
    this.refreshNow();
  }

  loadExample(): void {
    this.files.clear();
    this.origins.clear();
    this.schemaPath = 'main.ksy';
    this.mainOrigin = 'typed';
    this.parseType = null;
    this.found = null;
    this.notice = null;
    this.selection = null;
    this.setInput(EXAMPLE_BYTES, 'example.bin');
    this.setMain(EXAMPLE_SCHEMA);
    this.refreshNow();
  }

  /** The list of the open schemas and the choice of the type. */
  private showFiles(): void {
    const found = listing(this.found, this.schemaPath, this.mainOrigin, this.files, this.origins, this.diagnostics.length);
    this.schemas.show(found, this.active ?? this.schemaPath, this.parseType);
    element('editing').textContent = this.active === null ? this.schemaPath : `${this.active} (an import)`;
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
    const cancel = element('cancel');
    cancel.hidden = false;
    element('status').textContent = 'parsing…';
    const request = this.request();
    this.persist();
    element<HTMLButtonElement>('export').disabled = true;
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
    else run = this.client.check(request).then((checked) => ({ ...checked, value: null, trace: null, error: null }));
    run.then(
      (response) => {
        if (mine !== this.generation) return;
        cancel.hidden = true;
        element<HTMLButtonElement>('export').disabled = !(response.ok && (this.bytes || this.serverInput));
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
    if (this.notice) problems.push(this.notice);
    this.diagnostics = response.diagnostics;
    this.problems.show(problems);
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
    else if (item.kind === 'parse' && this.trace) {
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
