// The page: the schema, the input, and what doe makes of them, in four panes
// that stay in step. Everything runs here, in the browser; nothing is sent
// anywhere.

import { CallEnded, DoeClient } from './client.js';
import type { Diagnostic, ExportFormat, ExportResponse, ParseResponse, Request, TraceNode } from './doe.js';
import { SchemaEditor } from './editor.js';
import { EXAMPLE_BYTES, EXAMPLE_SCHEMA } from './example.js';
import { HexView, RemoteBytes } from './hex.js';
import { type LocalApi, detectLocal } from './local.js';
import { type Path, byteRange, checkSize, describeRange, findByte, nodeAt, pathName } from './model.js';
import { ProblemsView } from './problems.js';
import { MAX_LINK, type Work, codeOfHash, decodeWork, encodeWork, forget, recall, remember } from './share.js';
import { CUSTOM, SYSTEM, THEMES, Themes } from './theme.js';
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
  private schemaPath = 'main.ksy';
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

  constructor(workerUrl: URL, wasmUrl: URL) {
    this.client = new DoeClient(workerUrl, wasmUrl);
    this.editor = new SchemaEditor(element('editor'), EXAMPLE_SCHEMA, () => this.later());
    this.themes = new Themes((theme) => this.editor.setTheme(theme));
    this.tree = new TreeView(element('tree'), (path) => this.select(path, 'tree'));
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
    const work: Work = { schema: this.editor.text, files: Object.fromEntries(this.files), path: this.schemaPath };
    if (this.includePaths) work.include_paths = this.includePaths;
    return work;
  }

  private applyWork(work: Work): void {
    this.files.clear();
    for (const [name, text] of Object.entries(work.files)) this.files.set(name, text);
    this.showFiles();
    this.schemaPath = work.path;
    this.includePaths = work.include_paths;
    this.editor.setText(work.schema);
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

  private gallery: { repo: string; commit: string; formats: { path: string; id: string; title: string | null; imports: string[] }[] } | null = null;

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
        option.textContent = format.title ? `${format.id} — ${format.title}` : format.id;
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

  /** Fetch a format and the files it imports, at the commit the page was built with. */
  async openGallery(format: { path: string; imports: string[] }): Promise<void> {
    const gallery = this.gallery;
    if (!gallery) return;
    const status = element('status');
    status.textContent = `fetching ${format.path}…`;
    try {
      const get = async (path: string): Promise<string> => {
        const response = await fetch(`https://raw.githubusercontent.com/${gallery.repo}/${gallery.commit}/${path}`);
        if (!response.ok) throw new Error(`${path}: ${response.status}`);
        return response.text();
      };
      const [text, ...imports] = await Promise.all([format.path, ...format.imports].map(get));
      const files: Record<string, string> = {};
      for (const [index, path] of format.imports.entries()) files[path] = imports[index] ?? '';
      this.applyWork({ schema: text ?? '', files, path: format.path, include_paths: [''] });
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
    for (const [name, text] of Object.entries(schema.files)) this.files.set(name, text);
    this.showFiles();
    this.schemaPath = schema.path;
    this.includePaths = schema.include_paths;
    this.editor.setText(schema.text);
    this.refreshNow();
  }

  private useServerInput(local: LocalApi, input: { name: string; size: number }): void {
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
    element('cancel').addEventListener('click', () => {
      this.client.cancel();
      this.abort?.abort();
    });
    element('share').addEventListener('click', () => void this.share());
    element('export').addEventListener('click', () => void this.exportOutput());
    element('forget').addEventListener('click', () => {
      forget();
      element('status').textContent = 'the saved schema is forgotten';
    });
    element('example').addEventListener('click', () => this.loadExample());
    element('files').addEventListener('click', (event) => {
      const name = (event.target as HTMLElement).closest<HTMLElement>('[data-remove]')?.dataset.remove;
      if (name !== undefined) {
        this.files.delete(name);
        this.showFiles();
        this.refresh();
      }
    });

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
    this.editor.setText(await file.text());
    this.refreshNow();
  }

  async addFiles(files: File[]): Promise<void> {
    for (const file of files) this.files.set(file.name, await file.text());
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
    this.refreshNow();
  }

  loadExample(): void {
    this.files.clear();
    this.showFiles();
    this.schemaPath = 'main.ksy';
    this.notice = null;
    this.selection = null;
    this.setInput(EXAMPLE_BYTES, 'example.bin');
    this.editor.setText(EXAMPLE_SCHEMA);
    this.refreshNow();
  }

  private showFiles(): void {
    const list = element('files');
    list.innerHTML = [...this.files.keys()]
      .map((name) => {
        const safe = name.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/"/g, '&quot;');
        return `<li><code>${safe}</code> <button type="button" data-remove="${safe}" aria-label="remove ${safe}">×</button></li>`;
      })
      .join('');
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
    const request: Request = { schema: this.editor.text, files: Object.fromEntries(this.files), path: this.schemaPath };
    if (this.includePaths) request.include_paths = this.includePaths;
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
    this.editor.showDiagnostics(response.diagnostics);
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
  }

  private pickByte(byte: number): void {
    if (!this.trace) return;
    this.select(findByte(this.trace, byte), 'hex');
  }

  private pickProblem(item: Diagnostic): void {
    if (item.position) this.editor.reveal(item.position);
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
    parts.push(response.diagnostics.length > 0 ? 'the schema has problems' : response.ok ? `parsed as ${response.id ?? '?'}` : problems > 0 ? 'the parse failed' : 'ok');
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
