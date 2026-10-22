// The worker: it owns the WebAssembly module and the input, so the page's own
// thread is never held by a parse. The messages are in client.ts.

import { type Doe, loadDoe } from './doe.js';
import type { FromWorker, ToWorker } from './client.js';

let doe: Doe | null = null;
let input = new Uint8Array(0);

const send = (message: FromWorker): void => postMessage(message);

self.onmessage = (event: MessageEvent<ToWorker>) => {
  const message = event.data;
  try {
    switch (message.op) {
      case 'init':
        void loadDoe(message.wasmUrl).then(
          (loaded) => {
            doe = loaded;
            send({ op: 'ready', version: loaded.version() });
          },
          (error: unknown) => send({ op: 'failed', id: 0, message: `could not load doe: ${String(error)}` }),
        );
        break;
      case 'input':
        input = new Uint8Array(message.bytes);
        break;
      case 'check':
        send({ op: 'checked', id: message.id, response: need().check(message.request) });
        break;
      case 'parse':
        send({ op: 'parsed', id: message.id, response: need().parse(message.request, input) });
        break;
    }
  } catch (error) {
    send({ op: 'failed', id: 'id' in message ? message.id : 0, message: String(error) });
  }
};

function need(): Doe {
  if (!doe) throw new Error('doe is not loaded');
  return doe;
}
