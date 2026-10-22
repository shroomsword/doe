// The page: the schema, the input, and what doe makes of them, in four panes
// that stay in step. Everything runs here, in the browser; nothing is sent
// anywhere.

import { CallEnded, DoeClient } from './client.js';
import type { Diagnostic, ParseResponse, Request, TraceNode } from './doe.js';
import { SchemaEditor } from './editor.js';
import { EXAMPLE_BYTES, EXAMPLE_SCHEMA } from './example.js';
import { HexView, RemoteBytes } from './hex.js';
import { type LocalApi, detectLocal } from './local.js';
import { type Path, byteRange, checkSize, describeRange, findByte, nodeAt, pathName } from './model.js';
import { ProblemsView } from './problems.js';
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

  constructor(workerUrl: URL, wasmUrl: URL) {
    this.client = new DoeClient(workerUrl, wasmUrl);
    this.editor = new SchemaEditor(element('editor'), EXAMPLE_SCHEMA, () => this.later());
    this.tree = new TreeView(element('tree'), (path) => this.select(path, 'tree'));
    this.hex = new HexView(element('hex'), (byte) => this.pickByte(byte));
    this.problems = new ProblemsView(element('problems'), (item) => this.pickProblem(item));
    this.wire();
  }

  /** Load the module and show the example. */
  async start(): Promise<DoeClient> {
    await this.client.start();
    element('version').textContent = this.client.version;
    this.local = await detectLocal();
    if (this.local) await this.showLocal(this.local);
    this.setInput(EXAMPLE_BYTES, 'example.bin');
    this.refresh();
    return this.client;
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
    element('cancel').addEventListener('click', () => {
      this.client.cancel();
      this.abort?.abort();
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
