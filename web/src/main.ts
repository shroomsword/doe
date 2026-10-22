import { App } from './app.js';
import type { DoeClient } from './client.js';

declare global {
  interface Window {
    /** For the tests: set when the module is loaded. */
    doe?: DoeClient & { ready: true };
    /** For the tests. */
    doeApp?: App;
  }
}

const app = new App(new URL('./worker.js', import.meta.url), new URL('./doe_wasm_bg.wasm', import.meta.url));
// ?limit=<ms> sets how long a parse may take (for the tests).
const limit = Number(new URLSearchParams(location.search).get('limit'));
if (limit > 0) app.client.limitMs = limit;

app
  .start()
  .then((doe) => {
    window.doe = Object.assign(doe, { ready: true as const });
    window.doeApp = app;
  })
  .catch((error: unknown) => {
    const status = document.getElementById('status');
    if (status) status.textContent = `could not start: ${String(error)}`;
  });
