// The page's side of the worker: calls that answer later, and that end: a call
// that takes longer than the limit, or that is cancelled, ends the worker and
// starts a new one, which is the only way to stop WebAssembly that is running.

import type { CheckResponse, ParseResponse, Request } from './doe.js';

export type ToWorker =
  | { op: 'init'; wasmUrl: string }
  | { op: 'input'; bytes: ArrayBuffer }
  | { op: 'check'; id: number; request: Request }
  | { op: 'parse'; id: number; request: Request };

export type FromWorker =
  | { op: 'ready'; version: string }
  | { op: 'checked'; id: number; response: CheckResponse }
  | { op: 'parsed'; id: number; response: ParseResponse }
  | { op: 'failed'; id: number; message: string };

/** Why a call did not give an answer. */
export class CallEnded extends Error {
  constructor(
    readonly reason: 'timeout' | 'cancelled' | 'failed',
    message: string,
  ) {
    super(message);
  }
}

export interface Pending {
  id: number;
  resolve: (response: never) => void;
  reject: (error: CallEnded) => void;
  timer: ReturnType<typeof setTimeout>;
}

/** The longest a call may take, by default. */
export const DEFAULT_LIMIT_MS = 5000;

export class DoeClient {
  limitMs = DEFAULT_LIMIT_MS;
  version = '';
  private worker: Worker | null = null;
  private starting: Promise<void> | null = null;
  private pending: Pending | null = null;
  private input: Uint8Array = new Uint8Array(0);
  private inputSent = false;
  private next = 1;
  private newest = 0;

  constructor(
    private readonly workerUrl: URL,
    private readonly wasmUrl: URL,
  ) {}

  /** Start the worker and wait for the module to be loaded. */
  start(): Promise<void> {
    this.starting ??= new Promise<void>((resolve, reject) => {
      const worker = new Worker(this.workerUrl, { type: 'module' });
      this.worker = worker;
      this.inputSent = false;
      worker.onmessage = (event: MessageEvent<FromWorker>) => {
        const message = event.data;
        if (message.op === 'ready') {
          this.version = message.version;
          resolve();
        } else if (message.op === 'failed' && message.id === 0) {
          this.worker = null;
          this.starting = null;
          reject(new Error(message.message));
        } else this.answer(message);
      };
      worker.onerror = (event) => {
        const text = event.message || 'the worker stopped';
        this.worker = null;
        this.starting = null;
        reject(new Error(text));
        this.end(new CallEnded('failed', `doe stopped: ${text}. It may have run out of memory.`));
      };
      worker.postMessage({ op: 'init', wasmUrl: this.wasmUrl.href } satisfies ToWorker);
    });
    return this.starting;
  }

  /** The input that `parse` takes; it stays in the worker until it is replaced. */
  setInput(bytes: Uint8Array): void {
    this.input = bytes;
    this.inputSent = false;
  }

  check(request: Request): Promise<CheckResponse> {
    return this.call((id) => ({ op: 'check', id, request }));
  }

  parse(request: Request): Promise<ParseResponse> {
    return this.call((id) => ({ op: 'parse', id, request }));
  }

  /** End the call that is running, if there is one. */
  cancel(): void {
    this.end(new CallEnded('cancelled', 'the parse was cancelled'));
  }

  private async call<R>(make: (id: number) => ToWorker): Promise<R> {
    // A newer call ends an older one: only the newest is of use.
    this.cancel();
    const id = this.next++;
    this.newest = id;
    await this.start();
    if (this.newest !== id) throw new CallEnded('cancelled', 'a newer call replaced this one');
    const worker = this.worker;
    if (!worker) throw new CallEnded('failed', 'doe is not running');
    const message = make(id);
    return new Promise<R>((resolve, reject) => {
      const timer = setTimeout(
        () => this.end(new CallEnded('timeout', `it took longer than ${this.limitMs / 1000} s: the schema or the file is too much for this page`), id),
        this.limitMs,
      );
      this.pending = { id, resolve: resolve as (response: never) => void, reject, timer };
      if (message.op === 'parse' && !this.inputSent) {
        // A copy: the page keeps its own bytes for the hex view.
        const copy = this.input.slice().buffer;
        worker.postMessage({ op: 'input', bytes: copy } satisfies ToWorker, [copy]);
        this.inputSent = true;
      }
      worker.postMessage(message);
    });
  }

  private answer(message: FromWorker): void {
    const pending = this.pending;
    if (!pending || message.op === 'ready' || message.id !== pending.id) return;
    clearTimeout(pending.timer);
    this.pending = null;
    if (message.op === 'failed') pending.reject(new CallEnded('failed', `doe stopped: ${message.message}`));
    else pending.resolve(message.response as never);
  }

  /** Reject the running call (the one `only` when given) and replace the worker. */
  private end(error: CallEnded, only?: number): void {
    const pending = this.pending;
    if (!pending || (only !== undefined && pending.id !== only)) return;
    clearTimeout(pending.timer);
    this.pending = null;
    this.worker?.terminate();
    this.worker = null;
    this.starting = null;
    pending.reject(error);
  }
}
